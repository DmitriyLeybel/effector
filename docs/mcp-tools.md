# MCP tool reference

Status: implemented
Last updated: 2026-08-03

Effector currently exposes two read-only tools. They inspect Chrome metadata and
never activate discarded tabs. No current tool creates, activates, reloads,
moves, groups, discards, or closes browser state.

Incognito access is disabled by the extension manifest, so these tools expose
normal-profile metadata only.

Tool results can contain tab titles, URLs, and other browser metadata. Review
results before publishing logs or support reports.

## `browser.list`

Lists the connected Chrome instance and inventory counts. It takes no
parameters.

The result contains:

| Field | Meaning |
| --- | --- |
| `browserInstanceId` | Identity for this extension connection and browser runtime |
| `connectedAt` | Time the extension connected to the native broker |
| `extensionId` | Connected Chrome extension ID |
| `extensionVersion` | Connected extension version |
| `summary.windowCount` | Number of Chrome windows |
| `summary.groupCount` | Number of Tab Groups |
| `summary.tabCount` | Number of tabs |
| `summary.discardedTabCount` | Number of discarded tabs |

## `tabs.list`

Lists a bounded page of tab metadata together with the windows and Tab Groups
needed to interpret that page.

| Parameter | Type | Behavior |
| --- | --- | --- |
| `windowId` | integer | Return tabs in this window only |
| `groupId` | integer | Return tabs in this Tab Group only; use `-1` for ungrouped tabs |
| `activeOnly` | boolean | When `true`, return active tabs only; `false` applies no filter |
| `discardedOnly` | boolean | When `true`, return discarded tabs only; `false` applies no filter |
| `limit` | non-negative integer | Page size; defaults to 100, `0` becomes 1, and values above 250 become 250 |
| `cursor` | non-negative integer | Zero-based offset; use the preceding response's `nextCursor` |

All supplied filters are combined. Filtering happens before pagination.

The response contains:

| Field | Meaning |
| --- | --- |
| `browserInstanceId` | Identity for this extension connection and browser runtime |
| `capturedAt` | Time this page was captured |
| `totalMatched` | Total tabs after filtering and before pagination |
| `cursor` | Offset used for this page |
| `limit` | Effective page size |
| `nextCursor` | Offset for the next page, or `null` when this is the last page |
| `tabs` | Tab metadata for this page |
| `windows` | Windows referenced by tabs on this page |
| `groups` | Tab Groups referenced by tabs on this page |

`windows` and `groups` describe only the current page, not every tab counted by
`totalMatched`. Call the tool again with `cursor` set to `nextCursor` until
`nextCursor` is `null`.

Each call reads current Chrome state; pages do not share a frozen snapshot. Until
snapshot revisions are implemented, tabs changing between calls can cause
offset pagination to skip or repeat entries.

Tab metadata includes IDs, title, URL, window and group placement, active and
highlighted state, pin and mute state, audible state, loading status, discarded,
frozen, and auto-discardable state, incognito state, pending URL, last-accessed
time, opener ID, and favicon URL when Chrome provides those values.

Effector returns the result object as MCP structured content and as equivalent
pretty-printed text for clients that do not consume structured content.

Window objects contain ID, focus, position, dimensions, incognito flag, type,
state, and always-on-top state. Tab Group objects contain ID, window ID, title,
color, collapsed state, and shared state when Chrome provides it.

Example arguments for the first 50 discarded tabs in one window:

```json
{
  "windowId": 123,
  "discardedOnly": true,
  "limit": 50
}
```
