# Process and transport model

Status: implemented transport and protocol v3 read foundation
Last updated: 2026-08-03

## Process roles

| Role | Started by | Lifetime | Protocol-facing streams |
| --- | --- | --- | --- |
| Extension service worker | Chrome | Browser/profile lifetime while the native port is connected | Chrome runtime and Native Messaging port |
| Native broker and MCP server | Chrome through `connectNative()` | Until the native port closes | Native Messaging on stdin/stdout; MCP Streamable HTTP on loopback |
| Future ACP agent or adapter | Broker after explicit user action | One or more ACP sessions | ACP on separate child stdin/stdout; logs on child stderr |

One executable implements the broker, installer, and diagnostics modes. MCP
clients do not launch another Effector process.

## Transport boundaries

### Extension to broker

- Chrome Native Messaging only.
- Length-prefixed JSON messages.
- Long-lived `runtime.connectNative()` port.
- Implemented protocol v3 request IDs, strict handshake/version/ABI checks,
  implementation intersection, browser response identity, capability revisions,
  typed errors and dispatch state, read deadlines, and correlated responses.
- Reconnect with bounded exponential backoff after `onDisconnect`.

Protocol v3 preserves `ready`, `ready_ack`, `request`, `response`, and
`requestId`. It adds an exact envelope ABI, method/branch implementation
manifests, rich complete capability state, typed dispatch metadata, typed PNG
artifacts, and allowlisted request-class deadline metadata. Rust and extension
versions change together; old/new mismatches fail visibly rather than
downgrading. The current three read methods permit no artifacts or side effects.

### Harness to broker

- MCP Streamable HTTP at `http://127.0.0.1:37654/mcp` by default.
- Bearer authentication with a persistent random installation token.
- Host and Origin validation to resist DNS rebinding and cross-origin access.
- In-memory MCP sessions for legacy protocol versions and direct JSON responses
  for request-response operations.
- Multiple MCP clients share the one browser connection.

The address may be overridden with `EFFECTOR_MCP_ADDRESS` for testing. Effector
rejects non-loopback addresses. The token may be overridden with
`EFFECTOR_MCP_TOKEN`; production uses the per-user `mcp-token` state file.

### Broker to ACP agent

This boundary is reserved for later work and is not implemented in the
MCP-first release.

- Stable ACP over the child's dedicated stdin/stdout.
- The broker launches and supervises a named, allowlisted agent profile.
- The ACP session receives Effector's authenticated Streamable HTTP endpoint as
  its MCP browser-tool server.
- Remote ACP transports and draft MCP-over-ACP remain deferred.

## Startup sequence

1. Chrome starts the extension service worker.
2. The worker calls `connectNative("com.effector.browser")`.
3. Chrome reads the registered Native Messaging manifest and launches Effector.
4. Effector binds the fixed loopback MCP endpoint.
5. Extension and broker exchange protocol and browser metadata.
6. The broker advertises the endpoint to the extension popup.
7. Configured MCP clients connect, authenticate, initialize, and list tools.

If a harness starts before Chrome, its HTTP connection fails because no broker
owns the endpoint. The harness must reconnect after Chrome starts. Effector does
not launch Chrome implicitly.

## Request flow

1. A harness POSTs an MCP tool call to `/mcp`.
2. The broker validates authentication, Host, Origin, protocol, and session.
3. The in-process MCP tool handler creates an internal browser request ID.
4. The broker writes the request to the extension through Native Messaging.
5. The extension calls documented Chrome APIs and returns a typed result.
6. The broker correlates the result and returns the MCP HTTP response.

No internal proxy protocol or broker discovery file participates in this path.

## Retained-state flow

Steps 1 through 5 are implemented for `browser.snapshot`. Mutation preview and
apply in step 6 remain future work.

1. The extension reconciles Chrome events and queries into one complete
   normalized baseline with internal object-incarnation keys.
2. The broker validates and retains the complete bounded baseline in memory.
3. The broker derives filters and immutable compact/full snapshot pages from
   that retained state. The focused window comes first; other windows use
   ascending runtime Chrome window ID order.
4. Random server-side cursors point to the retained snapshot and next position.
   Cursor reads do not query Chrome or refresh expiry.
5. Counts are computed without retaining a snapshot or cursor. Unsupported
   filters such as `frozen` on an older Chrome fail instead of treating the
   missing field as false.
6. Preview resolves public references against one retained snapshot and creates
   an exact memory-only plan. Apply rechecks relevant incarnation and property
   preconditions in the extension and executes non-atomically.

Browser/page snapshot, cursor, plan, and artifact stores use bounded FIFO
eviction and fixed creation-time TTLs. The extension persists global toggle
choices; broker references, baselines, snapshots, cursors, plans, artifacts,
and deduplication results do not survive broker restart.

## Accepted page flow

This is also target behavior, not a currently available page-tool surface.
Protocol v3 capability facts represent implementation, desired setting, granted
Chrome permission, API/probe support, effective state, and one safe reason.
Capability changes can alter MCP discovery, while extension dispatch always
rechecks authority.

Page semantics and structured actions use a fixed isolated-world page agent.
Visual inspection captures only the current viewport of a tab already active in
its own window and verifies activation, document, viewport, and scroll identity
around capture. `inspectAfter` reuses that pipeline after one action. Advanced
evaluation is separately enabled and uses exact-document isolated
`userScripts` execution on supported Chrome versions. The accepted flow does not
attach a debugger, activate targets implicitly, or provide full-page capture.

## Multiple Chrome profiles

Each profile can ask Chrome to launch a separate native host, but only one
process can own the fixed MCP port. The current implementation therefore
supports one active profile. Profile selection, broker handoff, or a separate
always-on singleton service is required before simultaneous profiles are
supported.

## Shutdown and recovery

- Closing Chrome or disconnecting the extension closes Native Messaging stdin.
- The broker treats EOF as the authoritative shutdown signal.
- MCP sessions are cancelled and the HTTP listener receives graceful shutdown.
- Shutdown is bounded; the HTTP task is aborted after five seconds if needed.
- Pending browser requests are released and the native process exits.
- If the host crashes, the extension reconnects with bounded backoff and Chrome
  launches a fresh process.
- MCP clients must reconnect and initialize a new session after broker restart.

## Backpressure and limits

The broker uses a bounded Native Messaging writer queue, a 30-second broker read
deadline, and a 29-second maximum extension read budget. It rejects requests over
the product limit before lifecycle insertion or writer queueing. The writer
recomputes the extension budget when dequeuing, commits dispatch before the
first frame byte, acknowledges successful flush, and skips queued requests whose
waiter left, deadline elapsed, or broker began shutdown. Bounded terminal read
records validate late responses without retaining browser result data. Protocol
v3 owns and validates one bounded PNG artifact shape, but current methods reject
artifacts.
Browser snapshots use complete bounded baselines and one broker-owned,
class-aware aggregate retention store. An injected monotonic clock, per-class
count/byte policy, global FIFO pressure, fixed non-refreshing TTLs, and owned
eviction cleanup preserve exact accounting as future retained kinds are added.
Keyed mutation-task ownership, multi-waiter result sharing, overlap lanes, and
cancellation-aware non-atomic outcome handling remain unimplemented.
