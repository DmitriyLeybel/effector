# MCP tool reference

Status: `browser.snapshot` and legacy tools implemented; remaining V1 proposed
Last updated: 2026-08-03

Effector currently implements `browser.snapshot` alongside the migration-only
`browser.list` and `tabs.list` tools. The remaining four tools in the five-tool
replacement are proposed and are not implemented yet.

Implementation sequencing is tracked in
[`mcp-tool-surface-plan.md`](mcp-tool-surface-plan.md) and the code-grounded
[`Part 2 continuation`](mcp-tool-surface-plan-part-2.md). The
[`Part 3 workflow overlay`](mcp-tool-surface-plan-part-3.md) defines measurement
and decision gates for proposed efficiency refinements; it does not change this
contract until a gate is accepted and the contract is updated explicitly.

## Version-one surface

| Tool | Purpose | Availability |
| --- | --- | --- |
| `browser.snapshot` | See Chrome windows, groups, and tabs | Core |
| `browser.change` | Preview or apply browser organization changes | Core, changes globally enabled by the user |
| `page.inspect` | Read page semantics or an active viewport image | Page tools globally enabled by the user |
| `page.act` | Perform one structured page action | Page tools globally enabled by the user |
| `page.evaluate` | Run arbitrary isolated JavaScript | Advanced evaluation globally enabled by the user |

The five names are the complete model-facing concept set: see the browser,
change the browser, inspect a page, act on a page, and use code as an advanced
escape hatch. Version one does not create separate tools for windows, groups,
tabs, screenshots, queries, clicks, filling, scrolling, navigation, or waiting.

## Design rules

### Optimize the common call

- An empty `browser.snapshot` call is useful.
- Defaults return compact data. Detail is requested only when it is needed.
- Results do not echo arguments or repeat context already expressed by nesting
  or opaque references.
- Default and false fields are omitted where absence has a documented meaning.
- Read tools guarantee no browser disturbance, so they do not return an object
  full of `false` effect flags.
- Tool descriptions stay short. Argument descriptions explain constraints and
  defaults without restating the whole tool.
- Structured content is canonical. Text fallback is a short summary rather than
  a pretty-printed duplicate of the complete JSON result.

### One trust boundary

Possession of the installation bearer token authorizes every capability the user
has globally enabled in the Effector extension. All authenticated MCP sessions
have the same authority. Version one has no per-client, per-tab, per-window, or
per-origin policy layer.

The extension exposes three global controls:

1. Browser changes
2. Page tools
3. Advanced evaluation

Browser changes and page tools are disabled until the user enables them once in
extension UI. Advanced evaluation is a separate, stronger toggle and remains
disabled by default. Effector still enforces normal Chrome restrictions,
incognito denial, exact references, stale-document checks, and bounded results.

### Opaque references

Version one uses these references:

| Reference | Meaning |
| --- | --- |
| `windowRef` | One Chrome window |
| `groupRef` | One live Tab Group |
| `tabRef` | One tab container across navigations |
| `documentRef` | The particular document currently loaded in a tab |
| `browserSnapshotRef` | One retained immutable browser snapshot |
| `pageSnapshotRef` | One retained page inspection |
| `elementRef` | One element from one page inspection and document |
| `planRef` | One retained browser-change preview |

References are opaque, typed, and local to one Chrome-owned broker process.
`tabRef` survives navigation until its tab closes. A closed browser object
returns `NOT_FOUND`; navigation invalidates `documentRef` and `elementRef`;
retention or process loss expires snapshots, cursors, and plans. Clients return
references exactly as issued and never parse or construct them.

`documentRef` includes the tab identity, so `page.act` and `page.evaluate` do not
also require `tabRef`. `elementRef` includes its document and inspection
identity, so actions do not repeat those fields either.

### Stale state

Effector fails closed when a referenced tab, document, or element changed. It
does not silently target a newer document or a similar element. Browser-change
plans store exact target preconditions from a retained browser snapshot;
unrelated browser activity does not invalidate them.

### Compact JSON

In compact records:

- Array order represents tab-strip order, so tab indexes are omitted.
- Window and group nesting represents placement, so tabs do not repeat those
  references.
- Boolean state is present only when `true`, unless the distinction between
  false and unknown is required.
- Optional Chrome fields are omitted when unavailable.
- Empty arrays and empty strings are omitted unless they carry meaning.

## `browser.snapshot`

**Tool description:** Return a compact, stable page of the connected Chrome
window, Tab Group, and tab hierarchy without activating or waking tabs.

### Arguments

| Argument | Type | Default | Description |
| --- | --- | --- | --- |
| `windowRef` | opaque reference | none | Restrict results to one exact window. |
| `groupRef` | opaque reference | none | Restrict results to one exact Tab Group. |
| `tabRefs` | array of opaque references | none | Restrict results to these exact tabs. |
| `active` | boolean | either | Match tabs whose active state equals this value. |
| `pinned` | boolean | either | Match tabs whose pinned state equals this value. |
| `discarded` | boolean | either | Match tabs whose discarded state equals this value without waking them. |
| `frozen` | boolean | either | Match tabs whose frozen state equals this value without unfreezing them. |
| `query` | non-empty string, at most 4096 UTF-8 bytes | none | Case-insensitive substring match against tab title or URL metadata. |
| `detail` | `counts`, `compact`, or `full` | `compact` | Return only counts, the normal identification fields, or secondary metadata. |
| `limit` | integer, 1 through 250 | `100` | Return at most this many matching tabs. |
| `cursor` | opaque string | none | Continue the exact retained snapshot and query represented by this cursor. |

`windowRef`, `groupRef`, and `tabRefs` are mutually exclusive. All supplied
filters are combined. A cursor call contains only `cursor`; changing scope,
filters, detail, or limit starts a new snapshot.

`counts` returns no window, group, tab, title, or URL records and accepts no
cursor or limit. It is appropriate for health checks.

```json
{
  "windowCount": 3,
  "groupCount": 8,
  "tabCount": 124,
  "discardedTabCount": 37
}
```

### Compact result

```json
{
  "browserSnapshotRef": "bs_91LQ",
  "capturedAt": "2026-08-03T12:00:00Z",
  "totalMatched": 2,
  "windows": [
    {
      "ref": "win_4",
      "focused": true,
      "items": [
        {
          "group": {
            "ref": "grp_8",
            "title": "Effector",
            "color": "green"
          },
          "tabs": [
            {
              "ref": "tab_7K2F",
              "title": "Effector MCP tools",
              "url": "https://example.com/effector",
              "active": true
            }
          ]
        },
        {
          "ref": "tab_9P3A",
          "title": "Chrome extensions",
          "url": "https://developer.chrome.com/docs/extensions"
        }
      ]
    }
  ],
  "nextCursor": null
}
```

Each window's `items` array preserves tab-strip order. An ungrouped tab appears
directly as a tab record; a group item contains group metadata and its ordered
tabs. Windows use a stable snapshot order with the focused window first.
Filtered or paginated windows and groups may include `partial: true`.

`full` adds window bounds/type/state; group collapsed/shared state; and tab
pending URL, highlighted, audible/muted, loading, auto-discardable,
last-accessed, opener, and favicon fields when Chrome provides them. It does not
inspect document content.

Each cursor page reads the same retained snapshot, not live state, so pages do
not skip or repeat tabs when Chrome changes. Retained snapshots have bounded
record and byte limits. If the complete matching snapshot cannot be retained,
Effector returns `RESULT_TOO_LARGE` and the caller narrows the query.

`browser.snapshot` never activates, focuses, reloads, attaches to, or wakes a
tab. Discarded and frozen state is metadata only.

## `browser.change`

**Tool description:** Preview or apply a bounded batch of exact tab, window, and
Tab Group changes selected from a browser snapshot.

Version one has two modes. Preview never changes Chrome. Apply accepts only a
server-issued plan reference, so the model cannot preview one operation set and
apply another accidentally.

### Preview

```json
{
  "mode": "preview",
  "browserSnapshotRef": "bs_91LQ",
  "operations": [
    {
      "type": "group.create",
      "tabRefs": ["tab_1", "tab_2", "tab_3"],
      "title": "Effector Research",
      "color": "green"
    },
    {
      "type": "tab.close",
      "tabRefs": ["tab_9", "tab_10"]
    }
  ]
}
```

| Argument | Type | Default | Description |
| --- | --- | --- | --- |
| `mode` | `preview` | required | Validate and summarize operations without changing Chrome. |
| `browserSnapshotRef` | opaque reference | required | Use this retained snapshot to resolve exact targets and stale-state preconditions. |
| `operations` | array, 1 through 50 | required | Ordered typed changes containing at most 100 total target references. |
| `stopOnError` | boolean | `true` | During apply, stop after the first failed operation instead of continuing with independent later operations. |

Preview returns only new information:

```json
{
  "planRef": "plan_72C",
  "summary": "Create 1 group with 3 tabs; close 2 tabs",
  "destructive": true,
  "warnings": ["Closing tabs cannot be undone reliably"],
  "expiresAt": "2026-08-03T12:05:00Z"
}
```

`destructive` is present only when true and is set for `tab.close`,
`tab.discard`, and `window.close`. Summary wording and warning order are
deterministic and warnings are deduplicated. Other disruptive operations such as
reload, navigation, focus, and moving the final tab retain operation-specific
warnings without being labeled destructive.

It does not echo normalized operations. The extension's global **Browser
changes** toggle is the V1 authorization decision. Effector does not add custom
per-operation confirmation receipts; an MCP client or harness may still present
its normal tool confirmation UI.

### Apply

```json
{
  "mode": "apply",
  "planRef": "plan_72C"
}
```

| Argument | Type | Description |
| --- | --- | --- |
| `mode` | `apply` | Apply exactly one retained preview. |
| `planRef` | opaque reference | Identify the exact operations, targets, preconditions, order, and failure policy to apply. |

Apply rechecks only target and destination state that matters to the plan. A
plan is single-use; retrying it in the same broker process returns the retained
result instead of running it twice.

```json
{
  "status": "applied",
  "results": [
    { "index": 0, "status": "applied", "createdRef": "grp_N4" },
    { "index": 1, "status": "applied" }
  ],
  "browserSnapshotRef": "bs_92MZ"
}
```

Overall status is `applied`, `partial`, or `unknown`. Per-operation status is
`applied`, `partial`, `failed`, `skipped`, or `unknown`. Successful entries
contain only new references or other information the caller could not infer.
Partial and failed entries add a compact error and any known created references.

Chrome has no multi-operation transaction. Earlier operations may succeed before
a later one fails, and a timeout does not prove that Chrome cancelled work. An
`unknown` result requires a new `browser.snapshot` before any retry.

Undo is deferred from version one. Reliable generic rollback is not available
for close, discard, navigation, or arbitrary page state, and implementing a
partial undo protocol would add substantial schema and retained-state cost.

### Operation types

| Type | Fields | Behavior |
| --- | --- | --- |
| `tab.open` | `url`; optional `windowRef`, `groupRef`, `index`; `active=false`, `pinned=false` | Open one tab, in the background by default. |
| `tab.close` | non-empty `tabRefs` | Close exact tabs; preview marks this destructive. |
| `tab.move` | non-empty `tabRefs`; optional `windowRef`, `groupRef`, `index` | Move tabs in caller order; `groupRef:null` explicitly ungroups them. |
| `tab.update` | non-empty `tabRefs`; one or more of `pinned`, `muted`, `autoDiscardable` | Set only supplied properties. |
| `tab.activate` | `tabRef`; `focusWindow=false` | Activate the tab and optionally focus its window. |
| `tab.duplicate` | `tabRef` | Duplicate one tab using Chrome's normal behavior. |
| `tab.discard` | non-empty `tabRefs` | Discard inactive tabs; active tabs are rejected. |
| `tab.reload` | non-empty `tabRefs`; `bypassCache=false` | Reload tabs and replace their document references. |
| `group.create` | non-empty same-window `tabRefs`; optional `title`, `color`, `collapsed` | Create one group containing the tabs. |
| `group.update` | `groupRef`; one or more of `title`, `color`, `collapsed` | Set only supplied group properties. |
| `group.move` | `groupRef`, `windowRef`, `index` | Move one group to a normal window and index. |
| `window.create` | optional non-empty `tabRefs`; `focused=false` | Create a normal window, optionally moving existing tabs into it. |
| `window.update` | `windowRef`; supported bounds, `state`, `focused`, or `drawAttention` | Set only supplied window properties. |
| `window.close` | `windowRef` | Close the window and every tab it still contains. |

Indexes are non-negative tab-strip indexes. Omitted indexes mean the end of the
destination. `tab.open` without a window or group uses the last-focused normal
window. `tab.move` requires at least one of `windowRef`, `groupRef`, or explicit
`groupRef:null`; a group reference determines its window, and a conflicting
window is rejected. Version-one URL inputs allow `https:`, `http:`, and exact
`about:blank`; other schemes are rejected.

## `page.inspect`

**Tool description:** Return bounded page semantics, an active viewport image,
or both without changing tab or window activation.

### Arguments

| Argument | Type | Default | Description |
| --- | --- | --- | --- |
| `tabRef` | opaque reference | none | Inspect whichever live document is currently loaded in this exact tab. |
| `documentRef` | opaque reference | none | Inspect this exact known document and fail if its tab navigated. |
| `view` | `semantic`, `visual`, or `both` | `semantic` | Return structured page meaning, the current rendered viewport, or both. |
| `scope` | `auto`, `viewport`, or `document` | `auto` | Prefer useful bounded content, limit content to the viewport, or inspect the whole document subject to output bounds. |
| `elementRef` | opaque reference | none | Inspect this exact previously returned element and its immediate semantic context. |
| `detail` | `compact` or `full` | `compact` | Return normal semantic content or add geometry and secondary structure. |
| `frames` | `main` or `all` | `main` | Inspect the main frame or every normal web frame available to the globally enabled page capability. |
| `cursor` | opaque string | none | Continue the exact retained page inspection represented by this cursor. |

A call supplies exactly one of `tabRef`, `documentRef`, `elementRef`, or
`cursor`. Each reference already carries the narrower identities, so no target
fields are repeated.

For `visual`, the target is `tabRef` or `documentRef`, scope must be `auto` or
`viewport`, and semantic detail, frames, and cursors do not apply. For `both`,
semantic scope is the current viewport and capture requires its scroll position
to remain unchanged.

### Compact result

```json
{
  "pageSnapshotRef": "ps_R31D",
  "documentRef": "doc_B91C",
  "url": "https://example.com/settings",
  "title": "Settings",
  "text": "Choose how your account appears.",
  "headings": ["Account settings"],
  "elements": [
    { "ref": "el_42", "role": "button", "name": "Save changes" },
    { "ref": "el_43", "role": "textbox", "name": "Display name" }
  ],
  "nextCursor": null
}
```

Compact output prioritizes meaningful visible prose, headings, links, and
interactive controls. Text already represented by a heading or control name is
not duplicated in `text`. Default state such as visible and enabled is omitted;
exceptional state appears as fields such as `disabled`, `checked`, `selected`,
or `expanded`. Non-password controls include their current value when useful;
select controls include option value/label pairs. Password and file-input values
are always omitted.

`full` adds landmarks, form structure, focused/selected state, relevant
attributes, and element geometry. It remains a semantic representation, not a
raw DOM dump or Chrome's canonical accessibility tree.

### Visual result

`visual` returns one MCP image content block and compact structured metadata:

```json
{
  "pageSnapshotRef": "ps_R31D",
  "documentRef": "doc_B91C",
  "url": "https://example.com/settings",
  "width": 1440,
  "height": 900,
  "scrollX": 0,
  "scrollY": 1820,
  "mimeType": "image/png"
}
```

The encoded image is not duplicated in structured JSON or text fallback.
Dimensions and encoded bytes are capped. `both` returns semantic fields at the
requested detail within the viewport plus image metadata and the same single
image block. The semantic read and image capture are sequential, not atomic,
but both are rejected if the document identity or scroll position changes.

Chrome's normal capture API captures the active tab in a specified window. That
window need not be focused, but the requested tab must already be selected in
it. Effector tracks activation and navigation events around capture, verifies
tab and document identity, and discards the image if either changed. It never
activates or focuses implicitly; a non-active target returns
`ACTIVATION_REQUIRED`, and the agent may explicitly use `browser.change` first.
When the caller supplied `documentRef`, the error adds
`recovery: {"tabRef":"..."}` because the caller otherwise lacks that reference.
A `tabRef` caller receives no duplicate recovery field. Recovery never creates
or applies a snapshot or plan: the caller explicitly snapshots, previews
`tab.activate`, applies the returned `planRef`, and retries inspection.

Page cursors are immutable slices of one retained inspection. Page and element
references fail after navigation. Discarded and frozen tabs return an error
without waking or activating; the agent can explicitly activate or reload them
through `browser.change` first.

Full-page capture, scrolling/stitching, element crops, visual labels, and
debugger-backed background capture are deferred. They add disturbance, identity,
or artifact complexity that the active-viewport path avoids.

## `page.act`

**Tool description:** Perform one structured action against an exact inspected
document and optionally return a compact post-action semantic or visual view.

```json
{
  "documentRef": "doc_B91C",
  "action": {
    "type": "click",
    "elementRef": "el_42"
  }
}
```

### Arguments

| Argument | Type | Default | Description |
| --- | --- | --- | --- |
| `documentRef` | opaque reference | required | Require this exact document and identify its tab without activating it. |
| `action` | action union | required | Perform exactly one supported structured action. |
| `wait` | `auto` or `none` | `auto` | Observe bounded navigation readiness or 250 milliseconds of target-document DOM quiet, or return after dispatch bookkeeping. |
| `timeoutMs` | integer, 1 through 30000 | `10000` | Bound target resolution, action dispatch, and automatic waiting. |
| `inspectAfter` | `semantic`, `visual`, or `both` | none | After the action and automatic wait, return a compact viewport inspection in the same call. |

Element actions accept one `elementRef` from `page.inspect`. They do not accept
fallback locators or coordinates.

For example, scroll and capture can be one explicit mutating call:

```json
{
  "documentRef": "doc_B91C",
  "action": {
    "type": "scroll",
    "direction": "down",
    "amount": 700
  },
  "inspectAfter": "visual"
}
```

When `inspectAfter` includes visual output, Effector verifies before dispatch
that the document's tab is already active in its window. It fails before acting
if capture would be unavailable. The post-action inspection reuses
`page.inspect` result shapes and image bounds.

For `wait="auto"`, Effector registers target-document mutation and navigation
observation before dispatch. Observed navigation waits for the relevant
replacement document agent and `document.readyState` of `interactive` or
`complete`. Without navigation, it requires 250 milliseconds without a
target-document subtree `childList`, `attributes`, or `characterData` mutation.
The aggregate `timeoutMs` never extends because activity continues. DOM quiet is
a best-effort settling heuristic, not network idle or proof that delayed
application work finished. `wait="none"` skips final settling while retaining
identity and dispatch bookkeeping. It drains navigation already observed before
action completion and establishes the exact current document before ordinary
success. If that identity cannot be established within the aggregate deadline,
the result is `status="unknown"`, and the caller inspects before retrying.

| Type | Fields | Behavior |
| --- | --- | --- |
| `click` | `elementRef`; `button="primary"`, `clickCount=1` | Invoke the exact visible, enabled element through DOM interaction. |
| `fill` | `elementRef`, `text`; `mode="replace"` or `append` | Change one editable control. |
| `select` | `elementRef`, non-empty `values` | Select exact option values. |
| `check` | `elementRef`, `checked` | Set a checkbox or radio-like control to the requested state. |
| `scroll` | either `elementRef`, or `direction` plus positive pixel `amount` | Scroll an element into view or scroll the document. |
| `focus` | `elementRef` | Focus one element in the document. |
| `navigate` | `url` | Navigate the tab to an allowed URL. |
| `back` | none | Traverse one history entry back. |
| `forward` | none | Traverse one history entry forward. |
| `reload` | `bypassCache=false` | Reload the document. |

Before an element action, Effector verifies document identity, element identity,
visibility, enabled state, and basic action compatibility. Version-one DOM
actions are not described as trusted physical input and do not cover hover, key
presses, drag and drop, uploads, native dialogs, or browser UI.

The tool does not wake or activate implicitly. If foreground state is required,
it returns `ACTIVATION_REQUIRED`; the agent may use `browser.change` explicitly.
Visual `inspectAfter` preflight uses the same compact recovery shape as visual
`page.inspect` and performs no action when preflight fails.

Without `inspectAfter`, the compact success result is the current document
reference:

```json
{ "documentRef": "doc_C02F" }
```

It changes after navigation or reload and otherwise normally remains the same.
The action itself already explains what was attempted, so success does not echo
the target, text, or state. If completion is uncertain, the caller inspects the
page before deciding whether to retry.

With `inspectAfter`, the result also contains `inspection` and, for visual modes,
one MCP image content block. This avoids a second ordinary verification call
without making the read-only `page.inspect` tool scroll or mutate.

Known action success followed only by inspection failure returns normal success
with compact `inspectionError`. Known action success followed only by an
automatic-settling timeout returns normal success with compact `waitError` and
omits the requested inspection. `waitError` means the action must not be retried
blindly; the caller inspects current state. An uncertain action outcome remains
`status="unknown"` instead.

If dispatch may have occurred but completion cannot be established, the normal
result is compact and explicit:

```json
{ "status": "unknown", "documentRef": "doc_B91C" }
```

The caller inspects before retrying.

The global **Page tools** toggle is the V1 authorization decision. Effector does
not attempt to infer whether a page control purchases, publishes, deletes, or
causes another high-impact application effect. The configured MCP client is
trusted with the enabled capability and may provide its own confirmation UI.

## `page.evaluate`

**Tool description:** Run size-limited JavaScript in an exact loaded document
when the global advanced-evaluation capability is enabled.

```json
{
  "documentRef": "doc_B91C",
  "code": "return document.querySelectorAll('article').length;"
}
```

### Arguments

| Argument | Type | Default | Description |
| --- | --- | --- | --- |
| `documentRef` | opaque reference | required | Run only in this exact document and identify its tab without activating it. |
| `code` | non-empty string | required | Body of an async function invoked as `async (...args) => { code }`. |
| `args` | JSON array | `[]` | Pass serializable values separately from source code. |
| `timeoutMs` | integer, 1 through 10000 | `2000` | Bound how long Effector waits; timeout cannot undo effects or always stop synchronous code. |

Version one runs only in an isolated user-script world and caps source,
argument, and result bytes internally. Main-world execution and caller-selected
result limits are deferred. One-shot exact-document evaluation requires Chrome
135 or later; earlier versions report `CAPABILITY_UNAVAILABLE` and do not expose
the tool.

The result is simply:

```json
{ "value": 12 }
```

A timeout after evaluation may have begun instead returns
`{ "status": "unknown" }`; it does not imply the script was cancelled.

The value must be JSON-serializable. Arbitrary code is not read-only: it can
change the DOM or storage, navigate, send requests, expose rendered data, or
consume renderer resources. The tool is absent from discovery until the user
enables **Advanced evaluation** globally. Chrome's `userScripts` permission and
user-controlled setting are required; `scripting.executeScript()` is not used
to accept runtime source strings.

Like other page tools, evaluation never wakes or activates a tab and is
unavailable on restricted browser pages.

## Errors

Errors are compact and typed:

```json
{
  "code": "STALE_DOCUMENT",
  "message": "The tab navigated after this document was inspected."
}
```

Only recovery data the caller does not already have is added, such as a current
`documentRef` when safe and useful. Errors do not echo arguments, page content,
inventory, or internal error chains.

| Code | Meaning |
| --- | --- |
| `CAPABILITY_DISABLED` | The required global capability is not enabled. |
| `CAPABILITY_UNAVAILABLE` | The Chrome version or target does not support the request. |
| `HANDLE_EXPIRED` | A process-local snapshot, cursor, plan, or reference is no longer retained. |
| `NOT_FOUND` | The exact browser object or page element no longer exists. |
| `STALE_SNAPSHOT` | Relevant browser-change target state diverged from the preview snapshot. |
| `STALE_DOCUMENT` | The tab no longer contains the expected document. |
| `STALE_ELEMENT` | The element no longer resolves in its document. |
| `TAB_NOT_LOADED` | The tab is discarded or frozen and was not woken. |
| `ACTIVATION_REQUIRED` | The operation cannot run in the background. |
| `RATE_LIMITED` | Chrome's capture or operation rate limit was reached; retry later. |
| `RESTRICTED_PAGE` | Chrome does not permit page access on this URL. |
| `INVALID_ARGUMENT` | Arguments are out of range, contradictory, or invalid for the operation. |
| `NOT_ACTIONABLE` | The element is hidden, disabled, obscured, or incompatible with the action. |
| `TIMEOUT` | The deadline elapsed; side-effecting completion may be uncertain. |
| `RESULT_TOO_LARGE` | The caller must narrow scope, detail, or filters. |
| `PARTIAL_FAILURE` | Some browser-change operations succeeded and others failed. |

Input/schema errors and failures before dispatch set MCP `isError=true`. Partial
or uncertain side-effecting results use the normal result schema so the caller
receives operation outcomes.

## Permissions

### Core browser tools

`browser.snapshot` and `browser.change` use the existing `tabs`, `tabGroups`,
`storage`, and `nativeMessaging` permissions. `chrome.windows` needs no manifest
permission. The extension's global Browser changes toggle controls apply, not a
new per-operation authorization system.

### Page tools

Enabling Page tools once in extension UI requests optional `scripting`,
`webNavigation`, and `<all_urls>`. The broad host grant permits active-viewport
capture through `captureVisibleTab`; Effector still limits page tools to normal
HTTP(S) pages. This is
intentionally one clear capability choice rather than per-site or per-tab
grants. It does not include incognito or restricted browser pages.

### Advanced evaluation

Advanced evaluation requires the globally enabled Page tools capability plus
optional `userScripts` permission and Chrome's user-controlled user-script
setting. It is a separate toggle because arbitrary source is substantially more
powerful than the typed page tools.

The V1 extension never requests `debugger`. A later debugger-capable manifest or
extension variant may add non-active-tab and full-page capture after a separate
architecture and permission review; Chrome does not allow `debugger` as an
optional permission. Until then, canonical accessibility data, trusted input,
console/network inspection, and other debugger workflows remain delegated to
Chrome DevTools MCP.

## Explicit version-one cuts

The following are intentionally deferred to keep the tool schema and
implementation straightforward:

- Undo and general rollback
- Browser change feeds and subscriptions
- Debugger-backed non-active/full-page screenshots, element crops, and visual
  labels
- Per-client, per-tab, per-window, and per-origin authorization
- Per-operation confirmation receipts
- Role/text/CSS fallback locators
- Raw coordinates and trusted input
- Password, file-input, and hidden-control value inspection
- Main-world JavaScript evaluation
- Arbitrary Chrome, DOM, or CDP forwarding

These features can be added only when a concrete workflow justifies their schema,
implementation, security, and token cost.

## Token-efficiency budget

Before release, generated tool definitions and representative results are
measured in tokens. The implementation should preserve these properties:

- Tool descriptions are one sentence.
- Argument descriptions are one sentence and do not repeat shared rules.
- Compact browser output does not repeat placement or default booleans.
- Compact page output does not include geometry, default element state, raw DOM,
  or form values.
- Visual image bytes appear once, only when requested, and are never repeated in
  structured JSON or text fallback.
- `inspectAfter` avoids a second inspect call when an action needs immediate
  semantic or visual verification.
- Preview and apply do not echo operations.
- Action success does not echo its inputs.
- Text fallback does not duplicate structured JSON.
- Optional branches are not advertised before implementation.

## Current implemented tools

The `browser.snapshot` contract above is implemented. During migration, the two
legacy tools below also remain available.

### `browser.list`

Takes no arguments and returns `browserInstanceId`, extension identity and
connection time, plus window, group, tab, and discarded-tab counts.

### `tabs.list`

Returns a live, bounded tab page with page-local window and group context.

| Parameter | Behavior |
| --- | --- |
| `windowId` | Filter by one runtime-scoped Chrome window ID. |
| `groupId` | Filter by one group ID; `-1` means ungrouped. |
| `activeOnly` | `true` filters to active tabs; `false` applies no filter. |
| `discardedOnly` | `true` filters to discarded tabs without waking them; `false` applies no filter. |
| `limit` | Defaults to 100, converts 0 to 1, and caps values at 250. |
| `cursor` | Zero-based live offset returned as `nextCursor`. |

Existing pages are not frozen and can skip or repeat tabs when Chrome changes.
Successful calls return structured content plus equivalent pretty JSON; current
errors are text-only. Neither current tool activates, reloads, attaches to, or
wakes discarded tabs.

## Authoritative platform references

- [Chrome Tabs API](https://developer.chrome.com/docs/extensions/reference/api/tabs)
- [Chrome Windows API](https://developer.chrome.com/docs/extensions/reference/api/windows)
- [Chrome Tab Groups API](https://developer.chrome.com/docs/extensions/reference/api/tabGroups)
- [Chrome Scripting API](https://developer.chrome.com/docs/extensions/reference/api/scripting)
- [Chrome User Scripts API](https://developer.chrome.com/docs/extensions/reference/api/userScripts)
- [Chrome extension permissions](https://developer.chrome.com/docs/extensions/reference/api/permissions)
- [MCP tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools)
