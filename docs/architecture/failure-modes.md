# Failure modes and recovery

Status: implemented failures; accepted target foundations
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

## Accepted target failure handling

The protocol, browser baseline, snapshot, cursor, count, frozen-filter, browser
restart, and broker-restart rows below are implemented foundations from ADR
0005. Mutation and page rows remain required future behavior from ADRs 0005 and
0006.

| Failure | Risk | Required behavior |
| --- | --- | --- |
| Protocol v2 and v3 components are mixed, or v3 ABI revisions conflict | Capability, dispatch, or artifact fields could be misread | Fail the handshake visibly with no downgrade; broker and extension upgrade together |
| Complete browser baseline exceeds retention bounds | A partial baseline could look authoritative | Reject the snapshot as too large; never fall back to a live or incomplete baseline |
| Snapshot, cursor, or plan reaches fixed TTL | A stale handle could prolong sensitive state | Return handle-expired; reads never refresh TTL |
| A retained class or the aggregate store reaches its object or byte bound | New retained state cannot fit | Evict the oldest eligible class record or globally oldest aggregate record and clean up its dependent handles |
| Caller tampers with or invents a cursor | Pagination authority is untrusted | Accept only random server-side cursor handles tied to retained immutable state |
| Counts call completes | Counts could become an unintended retained inventory | Return counts without retaining a snapshot, cursor, or count record |
| `frozen` filter is requested where Chrome cannot report it | Missing could be mistaken for false | Return capability-unavailable or unsupported rather than an incorrect match set |
| A read waiter leaves or its deadline expires while queued | Work could dispatch after its caller no longer observes it | Remove the waiter and have the writer skip the unclaimed frame; validate any bounded late response by its retained method policy |
| User changes relevant target state after preview | A mutation could affect unintended state | Recheck exact incarnation and operation-specific preconditions before each operation |
| Tab disappears before mutation | Chrome returns missing tab | Return typed not-found; never reuse the old ID |
| Chrome restarts and reuses numeric IDs | A stale reference could target a new object | Use random incarnations and broker refs; reject every pre-restart public handle |
| One operation in a plan fails | Earlier Chrome operations may already have committed | Return ordered partial, failed, and skipped outcomes; do not claim atomic rollback |
| Mutation response is lost after dispatch | Retrying could duplicate a side effect | Return unknown when certainty is lost and require a fresh snapshot before retry decisions |
| Broker restarts | Public refs and deduplication state disappear while extension toggles survive | Expire all broker handles and plans; reconnect and acquire fresh snapshots |
| Global page permission is revoked | Discovery or an in-flight call may be stale | Update the complete protocol v3 capability state and recheck in the extension before dispatch |
| Visual target is not already active in its window | Capture could return the wrong tab | Fail before capture or action preflight; never activate implicitly |
| Activation, document, viewport, or scroll changes during capture | Image attribution becomes uncertain | Reject the image and require a fresh inspection |
| Action succeeds but `inspectAfter` fails | Reporting the action as failed invites a duplicate effect | Preserve action success and return a compact inspection error |
| Isolated evaluation times out after dispatch | Page code may still run or take effect | Return unknown; do not claim cancellation or retry automatically |
| Side-panel message has no compatible harness | Nothing can consume it | Keep bounded pending state or return unsupported; do not silently launch arbitrary commands |
| Incognito requested without access | Partial inventory | Report capability state and keep normal/incognito routing separate |

## Safety defaults

The implemented default is simple: only read tools are available. Current and
future implementation under the accepted ADRs must preserve these requirements:

- Mutation requests use exact process-local public references backed by browser
  and object incarnations, not public Chrome numeric IDs.
- Bulk/destructive operations use immutable preview plans.
- Timeouts do not imply cancellation succeeded; late results carry request IDs.
- Reconnection always begins with fresh broker references and snapshots.
- Extension capability toggles persist, but broker references, snapshots,
  cursors, and plans never do.
- Page reads do not activate, focus, scroll, reload, attach a debugger, or wake
  tabs implicitly.
