# Process and transport model

Status: implemented spike
Last updated: 2026-08-02

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
- Request IDs, protocol versions, deadlines, and structured errors.
- Reconnect with bounded exponential backoff after `onDisconnect`.

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

The broker uses a bounded Native Messaging writer queue and a 30-second browser
request deadline. Inventory tools page results to avoid oversized Native
Messaging and MCP payloads. Further mutation work must add explicit concurrency
limits, snapshot revisions, and cancellation-aware operation handling.
