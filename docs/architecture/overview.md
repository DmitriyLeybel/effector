# Architecture overview

Status: implemented read-only topology
Last updated: 2026-08-03

## Purpose

Effector lets MCP-capable agent harnesses inspect a user's real Chrome session:
browser instances, windows, tab groups, and tabs. It preserves Chrome as the
owner of that session and complements page-oriented automation instead of
launching a separate automation browser. Controlled mutations are future work.

## Goals

- Use documented Chrome extension APIs.
- Read tab-strip metadata without activating or waking discarded tabs.
- Support standard MCP clients over authenticated Streamable HTTP.
- Allow more than one harness connection without mixing browser ownership.
- Avoid a manually managed always-on daemon.
- Recover cleanly when Chrome, the extension, native host, or harness exits.
- Keep page-content access and powerful debugging permissions out of the core.

## Non-goals for the first implementation

- Launching or controlling every possible harness.
- ACP agent supervision or browser-to-harness conversation UI.
- Treating Chrome Tab Groups as a security boundary.
- Reading arbitrary page content.
- Chrome DevTools Protocol integration.
- Cross-browser private APIs.
- Durable identity for a Chrome tab across browser restarts.

## Implemented topology

```text
Chrome
┌──────────────────────────────────────────────────────────────┐
│ Popup ── runtime messaging ── MV3 service worker             │
│                                  │                           │
│                          Native Messaging                    │
└──────────────────────────────────│───────────────────────────┘
                                   ▼
                       Native broker and MCP server
                           (Chrome-owned lifetime)
                                   │
                authenticated Streamable HTTP on loopback
                           ┌───────┴────────┐
                           ▼                ▼
                      Harness A        Harness B
```

The current implementation uses one Chrome-owned Rust process with
`native-host`, `install`, and `doctor` modes. ACP remains a later broker module
and does not participate in the MCP-first runtime.

## Components

### Chrome extension

The extension owns all current Chrome API calls:

- `chrome.windows` for window inventory.
- `chrome.tabs` for tab metadata.
- `chrome.tabGroups` for standard Chrome group metadata.
- `chrome.runtime.connectNative()` for the persistent broker connection.
- `chrome.storage` for a persistent installation identity.

It does not subscribe to browser events, mutate browser state, implement MCP,
or listen on a network port.

### Native broker host

Chrome starts this process through Native Messaging and keeps it alive while
the extension's native port remains connected. The broker:

- Translates between extension JSON messages and MCP tool calls.
- Maintains the connected browser instance and MCP session routing state. A
  multi-instance registry is later work.
- Owns lightweight routing state and bounded pending queues.
- Exposes authenticated MCP Streamable HTTP on a fixed loopback endpoint.
- Validates bearer authentication, Host, Origin, protocol, and MCP sessions.

The broker's Native Messaging `stdin/stdout` belongs exclusively to Chrome.
It must never emit logs or MCP messages on that stdout stream.

### MCP Streamable HTTP endpoint

The broker serves `http://127.0.0.1:37654/mcp` by default. The installer creates
a persistent random bearer token and prints the client configuration. Multiple
clients receive independent MCP sessions while sharing one Chrome connection.
Simple tool operations return JSON HTTP responses; the SDK retains SSE support
for future server notifications.

### Future side panel and ACP

The side panel is a browser control and observability surface. It may create an
Effector workspace, display connections, assign tabs, or communicate with an
ACP-compatible agent through the broker's ACP client module.

ACP is the preferred future front channel for prompts, streamed messages,
tool-call presentation, plans, permissions, and agent-session controls. MCP
remains the separate HTTP channel through which that agent invokes Effector's
browser tools. Generic MCP still cannot inject a chat turn into a harness.

See
[`../research/acp-harness-communication.md`](../research/acp-harness-communication.md)
for the proposed process topology and display model.

This is future scope. The current extension popup reports broker status and can
read Chrome inventory, while harness communication is MCP-only.

## Browser data model

```text
BrowserInstance
└── Window
    ├── TabGroup
    │   └── Tab
    └── ungrouped Tab
```

Every response includes a browser-instance ID. Chrome's numeric window, group,
and tab IDs are valid only within a running browser/profile context and must
not be treated as durable identifiers across restarts.

## Core request flow

1. Chrome launches the broker after the extension calls `connectNative()`.
2. A harness connects to the authenticated loopback MCP endpoint.
3. The harness initializes and invokes a tool such as `tabs.list`.
4. The in-process handler sends a correlated request through Native Messaging.
5. The extension calls documented Chrome APIs and returns a bounded result.
6. The broker correlates the response and returns it over MCP HTTP.

If Chrome is not running, the endpoint is not listening. If the extension
disconnects during a call, the broker returns a browser-disconnected result and
shuts down. Effector never launches a different browser session silently.

## Permissions strategy

Core inventory initially requires:

```json
{
  "permissions": [
    "tabs",
    "tabGroups",
    "storage",
    "nativeMessaging"
  ]
}
```

`chrome.windows` does not require a `"windows"` manifest permission. The
`"tabs"` permission is needed for sensitive fields such as tab URL and title.
`"sidePanel"` is added only when the side panel exists.

`"scripting"` is not a core permission. It requires host access or
`"activeTab"` and belongs to a later, explicitly enabled page-content feature.
`"debugger"` is similarly deferred because it is powerful and unnecessary for
tab-strip management.

## Security boundaries

- The native host manifest allowlists the exact extension ID supplied during
  development installation.
- MCP HTTP binds only to loopback and requires a persistent random bearer token
  stored in the user's protected Effector state directory.
- HTTP Host and Origin validation defend against DNS rebinding and cross-origin
  browser requests.
- Native Messaging frame sizes, protocol versions, request IDs, and required
  success fields are validated before results cross the MCP boundary.
- Browser-originated requests cannot execute arbitrary shell commands.
- Incognito access is denied by the extension manifest.
- Page content is excluded unless the user grants a separate capability.

Future harness launching, mutations, and page-content capabilities require
separate security designs. They are not part of the implemented boundary.

## Authoritative references

- [Chrome Native Messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging)
- [Chrome extension service-worker lifecycle](https://developer.chrome.com/docs/extensions/develop/concepts/service-workers/lifecycle)
- [Chrome extension permissions](https://developer.chrome.com/docs/extensions/reference/permissions-list)
- [MCP architecture](https://modelcontextprotocol.io/docs/learn/architecture)
- [MCP transports](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports)

## Open decisions

- Fixed-port ownership and routing for multiple simultaneous Chrome profiles.
- Client-specific provisioning of the HTTP bearer credential.
- Expansion beyond the initial `browser.list` and `tabs.list` tool surface.
- Persistence format for workspace metadata.
- Which ACP agent to use for the first end-to-end integration spike.
