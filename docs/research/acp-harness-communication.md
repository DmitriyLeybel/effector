# ACP harness communication research

Status: recommended direction, pending ACP spike
Last updated: 2026-08-02

## Decision summary

Effector should use the Agent Client Protocol (ACP) as the preferred channel
between its Chrome side panel and supported agent harnesses.

- **ACP is the front channel:** user prompts, streamed agent messages, plans,
  tool-call presentation, permissions, session controls, and authentication.
- **MCP is the tool channel:** the agent discovers and invokes Effector's Chrome
  tools.
- **Native Messaging is the browser channel:** the extension asks the native
  broker to start and supervise agents, and the broker relays UI events.

The native broker may own both the ACP client module and the MCP broker module,
but ACP, MCP, and Chrome Native Messaging must use separate protocol streams.
They cannot share one `stdin/stdout` pair.

The first implementation should target stable ACP v1 over local stdio. ACP v2
is a draft as of 2026-07-30 and should not be the production baseline yet.

## What ACP changes for Effector

Without ACP, Effector would need a different control adapter for every harness:
one format for starting it, another for submitting prompts, and another for
parsing its terminal output. ACP standardizes this client-to-agent boundary in
the same way MCP standardizes the agent-to-tool boundary.

The current [ACP Registry](https://agentclientprotocol.com/get-started/registry)
already lists agents and adapters across multiple tool ecosystems. Effector
therefore needs one ACP client implementation, plus launch metadata for
compatible agents. For an unsupported harness, the preferred integration is a
small harness-to-ACP shim, not a new Effector-only conversation protocol.

ACP does not make every arbitrary process attachable. Effector can start a new
ACP agent process and can load or resume a session when the agent advertises
those capabilities. It cannot take ownership of an unrelated interactive CLI
that is already running in another terminal unless that CLI exposes its own
control/attach API.

## Protocol responsibilities

| Boundary | Protocol | Purpose | Process that owns the connection |
| --- | --- | --- | --- |
| Side panel ↔ service worker | Chrome runtime messaging | UI commands and rendered event data | Chrome extension |
| Service worker ↔ native broker | Native Messaging | Browser commands, ACP UI relay, and status | Chrome and broker |
| Broker ↔ ACP agent | ACP v1 over stdio | Prompts, responses, tools as UI events, permissions, and agent sessions | Broker launches agent |
| ACP agent ↔ native broker | Authenticated MCP Streamable HTTP | Chrome tool discovery and invocation | Agent connects to broker endpoint |

ACP supports this arrangement: its client passes MCP server configuration in
`session/new`, and the agent connects to that server. ACP remains on the child
process's stdio while MCP uses the broker's separate loopback HTTP listener.

## Recommended process topology

```text
Chrome
┌─────────────────────────────────────────────────────────────────────┐
│ Side panel                                                          │
│   prompt composer, transcript, tool cards, approvals, sessions      │
│        │ Chrome runtime messaging                                   │
│        ▼                                                            │
│ MV3 service worker ───── Native Messaging port ───────────────┐     │
└───────────────────────────────────────────────────────────────│─────┘
                                                                ▼
                                                Effector native broker
                                      ┌─────────────────┴──────────────┐
                                      │                                │
                              ACP client/supervisor             Browser/MCP server
                                      │                                ▲
                     separate child stdin/stdout                       │ authenticated
                                      ▼                                │ HTTP
                              ACP agent or adapter ─────────────────────┘
```

The broker serves MCP in-process. The ACP agent is a separate supervised child
with its own pipes and connects back to the broker's HTTP endpoint.

### Why ACP can run in the same broker

Chrome owns the native host's main `stdin/stdout` and uses length-prefixed JSON
there. The broker can still launch an ACP agent because every child process
gets a separate `stdin`, `stdout`, and `stderr`:

```text
broker stdin/stdout       = Chrome Native Messaging only
ACP child stdin/stdout    = newline-delimited ACP JSON-RPC only
broker loopback HTTP      = authenticated MCP for all agents and harnesses
```

Putting the ACP client in the existing broker is the recommended v1 path. It
reuses process supervision, local configuration, routing state, and the single
Native Messaging connection. The ACP module should remain an internal boundary
so it can become a helper process later if crash isolation or packaging makes
that worthwhile.

## End-to-end interaction

### Starting a conversation

1. The user selects an installed agent profile in the side panel.
2. The side panel sends a structured `agent.start` request. It never supplies
   an arbitrary shell command.
3. The broker resolves an allowlisted command and launches the ACP agent.
4. The broker sends ACP `initialize`, including only the client capabilities
   Effector actually implements.
5. If the agent advertises authentication methods, the side panel presents
   them and the broker invokes `authenticate` for the user's choice.
6. The broker calls `session/new` with a validated absolute working directory
   and the authenticated Effector Streamable HTTP MCP entry.
7. The broker stores the returned opaque ACP session ID in the Effector
   workspace binding.

Conceptually, the MCP configuration passed to the ACP agent is:

```json
{
  "name": "effector",
  "url": "http://127.0.0.1:37654/mcp",
  "headers": {
    "Authorization": "Bearer <installation-token>"
  }
}
```

The broker supplies this credential directly to the child session. It must
never enter extension storage, side-panel state, or the transcript.

### Sending a prompt and using Chrome tools

```text
user submits prompt in side panel
  → extension sends agent.prompt to broker
  → broker sends ACP session/prompt to agent
  → agent streams ACP session/update events
  → agent discovers/invokes Effector tools through MCP
  → broker routes the in-process MCP request to the extension
  → extension calls chrome.tabs / tabGroups / windows
  → MCP result returns to agent
  → ACP tool-call updates and final message return to side panel
```

The ACP event stream is the authoritative source for the conversation UI.
Effector should separately maintain a browser-operation audit log. If an agent
reports an Effector MCP call through ACP, the UI should correlate it with the
audit record where possible instead of rendering duplicate tool cards. Because
not every agent preserves arbitrary correlation metadata, correlation is an
optimization rather than a protocol assumption.

### Reconnecting and resuming

The side panel is ephemeral. The broker keeps a bounded event cache and active
session registry, so reopening the panel can restore its view without
restarting the agent.

ACP session features are capability-gated:

- Baseline: `session/new`, `session/prompt`, `session/update`, and
  `session/cancel`.
- Optional: list, load with history replay, resume without replay, close,
  delete, modes, configuration options, and slash commands.

Effector must check the initialization response before showing or calling an
optional feature. Persisting an ACP session ID does not guarantee that a given
agent can resume it after its process exits.

## Display layer

### Recommended: a structured transcript with terminal aesthetics

ACP streams semantic events, not a byte-for-byte copy of an agent's native CLI
screen. Effector can make the transcript compact and terminal-like while still
using the structure to provide better controls:

```text
┌ Agent: Codex  • session name • working/idle • context usage ┐
│ user                                                        │
│ List my Chrome tab groups and archive the stale research.   │
│                                                             │
│ agent                                                       │
│ I’ll inspect the tab hierarchy first.                       │
│                                                             │
│ ▾ Tool: effector.tabs.list                         completed │
│   4 windows · 12 groups · 83 tabs                            │
│                                                             │
│ ▸ Plan  1/3 complete                                        │
│                                                             │
│ ┌ Permission required ────────────────────────────────────┐  │
│ │ Close 8 tabs?                 [Reject] [Allow once]     │  │
│ └─────────────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│ Type a message…                                [Stop] [Send] │
└─────────────────────────────────────────────────────────────┘
```

### ACP-to-UI mapping

| ACP data or method | Side-panel representation |
| --- | --- |
| `agent_message_chunk` / `user_message_chunk` | Streaming Markdown message rows |
| `agent_thought_chunk` | Collapsed reasoning/status section when provided |
| `tool_call` / `tool_call_update` | Expandable card with title, state, input, output, and locations |
| `plan` | Checklist or compact plan drawer |
| `session/request_permission` | Blocking approval card using the agent-provided choices |
| `elicitation/create` | Validated form or external-URL prompt, when enabled |
| `available_commands_update` | Slash-command palette |
| mode/config updates | Capability-gated selectors |
| `session_info_update` | Session title and metadata |
| `usage_update` | Context usage and optional reported cost |
| terminal content attached to a tool call | Expandable command-output block |
| prompt response stop reason | Turn state: complete, cancelled, refused, or limited |

The message renderer should support safe Markdown, code blocks, resource links,
and only the rich content types negotiated during initialization. It should
sanitize links and rendered content, never render agent-supplied raw HTML, and
virtualize long histories.

### Important terminal distinction

ACP's `terminal/*` methods let an **agent ask its client to run shell
commands** and let the client return captured output. They do not transport the
interactive TUI of the ACP agent itself. Advertising ACP terminal support would
also make Effector responsible for executing arbitrary commands on behalf of
the agent.

Effector should not advertise ACP filesystem-write or terminal capabilities in
the first release. They are unnecessary for Chrome control and would enlarge
the extension's trust boundary from browser management to general host access.
The chosen agent can continue to use its own sandbox and native tools.

## Exact CLI terminal mode

If the product later needs the exact ANSI colors, cursor movement, prompts, and
full-screen behavior of an unmodified CLI, ACP is not sufficient by itself.
The broker would need to run that CLI under a pseudoterminal (PTY on Unix,
ConPTY on Windows) and the side panel would need a terminal emulator renderer.

| Approach | Compatibility | Semantic tool UI | Exact CLI fidelity | Maintenance |
| --- | --- | --- | --- | --- |
| ACP structured transcript | High for registry agents | High | No | Lowest long-term |
| PTY/ConPTY terminal stream | Nearly any CLI | Low | Yes | Medium/high |
| Harness-specific JSON/SDK adapter | Harness-specific | Potentially high | No | Highest per harness |

PTY mode is a useful fallback for unsupported CLIs, but it should be labeled as
a terminal session rather than normalized into ACP. Parsing ANSI text to infer
tool calls, permissions, or session state would be brittle. If a custom adapter
is worth maintaining, it should translate the harness's structured API into
ACP so the side panel still has one event model.

## Requirements

### Required for the first ACP spike

- Launch one named, allowlisted ACP agent profile from the broker.
- Keep ACP stdout protocol-only and capture the agent's stderr separately.
- Negotiate ACP v1 and reject incompatible protocol versions cleanly.
- Complete authentication when the selected agent requires it.
- Create a session with a fixed working directory and Effector's authenticated
  MCP HTTP endpoint.
- Submit text prompts and stream Markdown responses.
- Render ACP tool calls, updates, plans, and permission requests.
- Cancel an active prompt and enforce a process-kill timeout as a fallback.
- Restore active-session UI after the side panel closes and reopens.
- Shut down child processes and process groups without leaving orphans.
- Bound queues, transcript cache size, message size, and history replay.
- Surface browser-disconnected, agent-crashed, authentication-required, and
  unsupported-capability states explicitly.

### Required before general availability

- Cross-platform process supervision and native-host packaging.
- Agent-profile installation, version, and executable verification.
- Session list/load/resume/close UI when supported by each agent.
- Multiple Chrome profile and multiple active agent routing without ID leaks.
- Permission UX that distinguishes ACP agent approval from Effector's browser
  mutation policy.
- Safe Markdown/resource rendering and secret redaction in logs.
- Audit records for agent start/stop, browser mutations, approvals, and errors.
- Crash/reconnect tests for Chrome, service worker, broker, ACP agent, MCP HTTP
  session, and side panel independently.

### Later capabilities

- Registry-backed agent discovery and opt-in installation.
- Images or embedded resources when agent capabilities justify them.
- ACP elicitation forms and URL flows.
- Optional exact-terminal PTY mode for non-ACP CLIs.
- Remote ACP transport after the protocol stabilizes it.
- ACP v2 behind negotiation and feature flags while retaining v1 support.
- MCP-over-ACP if and when its current draft proposal stabilizes and agent
  support warrants replacing the direct HTTP tool channel.

## Session and process model

Effector should keep these identifiers distinct:

```text
BrowserInstanceId  identifies one connected Chrome profile/runtime epoch
WorkspaceId        associates browser resources and one or more agent bindings
AgentProcessId     identifies a supervised ACP subprocess
AgentSessionId     is the opaque session ID returned by that ACP agent
McpConnectionId    identifies the agent's Effector MCP HTTP session
```

An initial implementation should prefer one supervised ACP process per agent
profile and one active session per process. ACP permits several sessions on one
connection, but beginning with a one-to-one mapping simplifies cancellation,
failure isolation, and lifecycle testing. Multiplex sessions only after the
selected agents demonstrate reliable concurrent-session behavior.

The broker owns ACP child lifetime while Chrome owns the broker's native-host
lifetime. For v1:

- Closing only the side panel does not stop an active agent.
- Closing Chrome causes the broker to cancel active work, request session close
  when supported, terminate its children after a grace period, then exit.
- Persist only the metadata needed to offer a later load/resume. Do not pretend
  an in-memory process survived.
- A detached always-on agent supervisor is a separate future architecture and
  would violate the current browser-owned-lifetime goal.

## Security model

- The browser may select a launch profile by ID; it may not send a raw command
  line, executable path, shell expression, environment map, or arbitrary cwd.
- A profile resolves to a fixed executable and structured arguments stored by
  the native installation.
- Working directories and additional roots are explicit, absolute, and
  user-approved. Effector does not silently use the home directory.
- Agent authentication remains owned by the agent where possible. Provider
  tokens do not enter extension storage or conversation events.
- ACP client filesystem and terminal capabilities default to absent.
- Agent stderr and broker logs are bounded and redacted before being exposed to
  the side panel.
- Native Messaging messages remain below Chrome's size limits; events and
  history are chunked or paginated rather than sent as one transcript dump.
- Browser mutation authorization and ACP tool-call permission are related user
  decisions but separate enforcement layers. A permissive ACP choice must not
  bypass an Effector policy.
- Incognito browser instances, if ever supported, receive separate bindings and
  explicit opt-in.

## Failure behavior

| Failure | Expected behavior |
| --- | --- |
| Side panel closes | Keep session in broker; replay bounded cached state when reopened |
| Extension service worker restarts | Reattach to native port and resynchronize active session summaries |
| ACP agent exits | Mark binding crashed, preserve diagnostics, offer restart or supported resume |
| Agent writes non-ACP data to stdout | Treat as protocol violation; keep stderr diagnostics separate |
| MCP HTTP session disconnects | Agent remains visible, but Chrome tools report unavailable until reconnected |
| Chrome disconnects | Agent may explain browser unavailability; broker begins graceful shutdown policy |
| Prompt cancellation stalls | Send ACP cancel, wait a bounded grace period, then terminate the process group |
| UI stops reading | Apply backpressure, coalesce state, and drop only documented replayable events |
| Optional ACP method is unsupported | Hide/disable the control; never probe by sending an invalid call |

## Golden path

### Phase A: ACP-only vertical slice

Use an official ACP SDK to implement the existing Rust broker as a minimal
client for one registry agent. Validate initialize, authentication, new session,
prompt, streamed updates, permission response, cancellation, stderr capture,
panel reconnect, and process cleanup. Earlier research considered a TypeScript
spike, but ADR 0003 subsequently selected Rust for the native executable.

### Phase B: close the ACP/MCP loop

Pass Effector's authenticated HTTP endpoint in `session/new`, make the agent
call one read-only `tabs.list` tool, and render the ACP tool event and final
result. Add one controlled browser mutation to validate the two permission
layers and audit correlation.

### Phase C: session UX

Add capability-gated list/load/resume/close, titles, modes, configuration,
slash commands, bounded replay, and explicit browser/agent status.

### Phase D: more agents

Add registry-derived launch profiles and verify behavior against several
agents. Compatibility data belongs in profile metadata; the core side-panel
event model remains ACP.

### Phase E: fallback and future protocols

Add PTY/ConPTY terminal mode only for important non-ACP harnesses. Experiment
with ACP v2 and MCP-over-ACP behind feature flags; do not make either draft a
first-release dependency.

## Risks and unresolved choices

- Which Rust ACP SDK or protocol implementation best fits the existing broker.
- Which ACP agent provides the smallest reliable first integration test.
- Whether one agent process per session is acceptable at scale or whether a
  connection should multiplex sessions.
- How agent installation is verified without turning the extension into a
  general package manager.
- How much conversation history the broker caches versus asking supported
  agents to replay it through `session/load`.
- Whether browser-owned lifetime remains desirable once long-running background
  agent tasks are in scope. ACP v2 explicitly improves background-work
  semantics, but it is still draft.
- How reliably ACP tool-call identifiers can be correlated with MCP requests
  across third-party agent adapters.

## Authoritative sources

- [ACP introduction](https://agentclientprotocol.com/get-started/introduction)
- [ACP architecture and MCP relationship](https://agentclientprotocol.com/get-started/architecture)
- [ACP Registry](https://agentclientprotocol.com/get-started/registry)
- [ACP v1 initialization and capabilities](https://agentclientprotocol.com/protocol/v1/initialization)
- [ACP v1 transports](https://agentclientprotocol.com/protocol/v1/transports)
- [ACP v1 session setup](https://agentclientprotocol.com/protocol/v1/session-setup)
- [ACP v1 prompt lifecycle](https://agentclientprotocol.com/protocol/v1/prompt-turn)
- [ACP v1 tool calls and permissions](https://agentclientprotocol.com/protocol/v1/tool-calls)
- [ACP v1 terminal methods](https://agentclientprotocol.com/protocol/v1/terminals)
- [ACP v1 authentication](https://agentclientprotocol.com/protocol/v1/authentication)
- [ACP v2 draft announcement](https://agentclientprotocol.com/announcements/acp-v2-draft)
- [Draft MCP-over-ACP proposal](https://agentclientprotocol.com/rfds/mcp-over-acp)
