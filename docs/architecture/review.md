# Review of the initial architecture proposal

Status: reviewed
Last updated: 2026-07-30

Historical note: this review explains the initial stdio-proxy transport spike.
ADR 0004 later selected the simpler broker-hosted Streamable HTTP option. The
Native Messaging, lifetime, permissions, and session analysis remains useful.

## Overall assessment

The proposal's foundation is sound: a Manifest V3 extension should own Chrome
API access, Native Messaging is the standard extension-to-local-process
channel, and MCP should face the harnesses. The design becomes coherent after
separating the Chrome-owned Native Messaging stream from harness-owned MCP
streams and after treating harness launching as optional integration work.

## What survives unchanged

- Chrome must be running to control its real session.
- A pure extension cannot expose normal MCP stdio or a stable listening server.
- Native Messaging is the standard Chrome-to-native bridge.
- Chrome should start the native component on demand.
- Windows, tabs, groups, and discarded state come from Chrome extension APIs.
- Lightweight persisted routing state is useful.
- A side panel is a reasonable status and command surface.

## Where the original proposal breaks

| Original assumption | Problem | Correction |
| --- | --- | --- |
| The Native Messaging host is also a normal MCP stdio server | Chrome owns the host's stdin/stdout and uses length-prefixed Native Messaging frames. MCP stdio uses harness-owned stdin/stdout and newline-delimited JSON-RPC. One stream cannot be both. | Make the Chrome-launched process a broker. Give each stdio harness a small MCP proxy connected over local IPC. |
| The host can be started and stopped entirely by either side without coordination | Chrome starts one host process for a Native Messaging port. Multiple Chrome profiles or reconnects can create multiple processes and endpoint conflicts. | Use an installation ID handshake, single-instance/registry rules, and per-browser-instance routing. |
| “MCP is bidirectional,” so the side panel can send a chat turn to any harness | MCP supports server-to-client requests and notifications only through negotiated capabilities. It does not standardize injecting arbitrary prompts into a host application's conversation. | Treat side-panel-to-harness messaging and harness launching as harness-specific adapters. Queue only messages for adapters that explicitly support them. |
| Multiple harnesses are isolated by tab groups | A tab group is mutable Chrome UI state, scoped to a window, and not an authorization boundary. | Store ownership and routing in the broker. Tab groups may visualize a workspace but never enforce isolation. |
| A session ID is one universal concept | MCP transport sessions, harness conversations, browser instances, and user workspaces have different lifetimes. | Model these as separate identifiers with explicit mappings. |
| Include `windows`, `scripting`, and CDP in minimal permissions | There is no `windows` permission. `scripting` also needs host access and is unnecessary for metadata. CDP requires the powerful `debugger` permission. | Start with `tabs`, `tabGroups`, `storage`, and `nativeMessaging`; add features and permissions separately. |
| Use WebSocket or Native Messaging interchangeably | They have different installation, lifetime, authentication, and directionality properties. Supporting both immediately doubles the connection state machine. | Use Native Messaging for extension-to-broker. Use local IPC for proxies. Consider Streamable HTTP later for MCP clients. |
| The native host can execute arbitrary configured launch commands | A browser message that reaches an unrestricted shell launcher becomes a local code-execution boundary. | Use named, user-approved launcher adapters with fixed executable and argument schemas. Keep launching out of the core milestone. |

## Process-lifetime correction

A long-lived `runtime.connectNative()` port is useful here: Chrome documents
that it keeps both the native host and, since Chrome 105, the extension service
worker alive. The extension must reconnect from `onDisconnect` because a host
crash closes the port.

This supports the intended browser-owned lifetime without pretending MV3
service workers are ordinary persistent background pages.

## Multi-harness correction

MCP stdio normally serves one client process. Multi-harness support therefore
means either:

1. One stdio proxy per harness, all connected to one broker; or
2. A broker-hosted MCP Streamable HTTP endpoint supporting multiple protocol
   sessions.

The first path is the compatibility-first choice. The second is simpler at
runtime for clients that support it, but client support must be verified.

## Native Messaging constraints to design around

- Host-to-Chrome messages have a 1 MiB limit.
- Chrome-to-host messages have a 64 MiB limit.
- Native Messaging uses 32-bit length-prefixed JSON, not MCP framing.
- The host manifest is installed in operating-system-specific locations.
- The manifest's `allowed_origins` must use a stable extension ID.
- stdout is protocol-only; logs go to stderr or a file.

Large browser inventories should support filtering and paging. Page content
must never be smuggled through an unbounded inventory response.

## Recommended scope reduction

The first useful release does not need a chat box, harness launcher, CDP, or
durable conversational sessions. It needs:

1. Browser-instance discovery.
2. Read-only window/group/tab inventory.
3. A working extension → native broker → MCP proxy path.
4. A small set of carefully defined mutation tools.
5. Event subscriptions or snapshot revision numbers.

The higher-level session and launcher experience can be added after this path
is reliable.
