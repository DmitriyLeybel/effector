# Failure modes and recovery

Status: implementation draft
Last updated: 2026-08-03

| Failure | Observable result | Required recovery |
| --- | --- | --- |
| Chrome is closed | The Chrome-owned MCP endpoint is absent | Harness reconnects after Chrome starts; Effector never launches Chrome implicitly |
| Extension is disabled or missing | Broker endpoint is absent | Enable or install the extension, then reload it or restart Chrome |
| Native host is not registered | Popup reports the `connectNative()` failure | Register the exact extension ID, then reload the extension |
| Native host crashes | Native port disconnects | Extension reconnects with bounded exponential backoff |
| Extension service worker restarts | In-memory connection state is lost | The restarted worker opens a new native connection and browser instance |
| Harness starts before Chrome | MCP HTTP connection or tool discovery fails | Start Chrome, confirm the popup connection, then reconnect or restart the harness |
| WSL2 uses NAT while the broker runs on Windows | WSL cannot reach the Windows loopback endpoint | Enable WSL mirrored networking, run `wsl --shutdown`, then restart WSL and the harness |
| Bearer token is stale or incorrect | MCP returns an unauthorized response | Replace the client header with the installation token and restart the client |
| Harness exits | Its MCP session closes or expires | The broker remains available to other clients while Chrome stays connected |
| Native message exceeds its transport limit | The message is rejected or the native connection closes | Keep requests bounded; `tabs.list` currently caps each page at 250 tabs |
| Multiple Chrome profiles connect | Only one broker can own the fixed endpoint | The additional broker fails visibly; close the other profile before retrying |
| MCP port is already owned | Another profile or process owns the fixed endpoint | Fail visibly; do not attach to or terminate an unknown process |

## Planned failure handling

The following rows describe requirements for mutation, workspace, side-panel,
and snapshot features that are not implemented yet.

| Failure | Risk | Required future behavior |
| --- | --- | --- |
| User changes tabs during a command | Snapshot revision becomes stale | Commands accept expected revision where needed and return conflict information |
| Tab disappears before mutation | Chrome returns missing tab | Return a typed `not_found`; never reuse the old ID |
| Chrome restarts and reuses numeric IDs | Stale references could target the wrong tab | Namespace IDs with browser runtime epoch and reject stale epochs |
| Side-panel message has no compatible harness | Nothing can consume it | Keep bounded pending state or return unsupported; do not silently launch arbitrary commands |
| Incognito requested without access | Partial inventory | Report capability state and keep normal/incognito routing separate |

## Safety defaults

The implemented default is simple: only read tools are available. Future
mutation work must preserve these requirements:

- Mutation requests identify exact browser instance and tab/group/window IDs.
- Bulk/destructive operations support dry-run previews or confirmations.
- Timeouts do not imply cancellation succeeded; late results carry request IDs.
- Reconnection always begins with a fresh inventory revision.
