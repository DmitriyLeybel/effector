# MCP tool surface implementation plan

Status: Part 1 implemented through phase 2; later phases superseded by Part 2
Last updated: 2026-08-03

This document is the implementation plan for the contract in
[`mcp-tools.md`](mcp-tools.md). Phases 0 through 2 now describe implemented
foundations and `browser.snapshot`. Phases 3 through 8 preserve the original
prospective design context but are superseded for execution by Part 2. This
document does not grant Chrome permissions by itself.

The code-grounded continuation for the remaining phases is
[`mcp-tool-surface-plan-part-2.md`](mcp-tool-surface-plan-part-2.md). It carries
forward incomplete Part 1 validation/spikes before browser changes or Page
permissions are introduced.

Part 2 also recommends a new ADR for protocol v3. If accepted, that ADR will
supersede only the prospective v2 artifact, operation-lifecycle, and dynamic
capability details in this plan and ADRs 0005/0006; their identity,
authorization, mutation, and page-safety decisions remain in force.

## Outcome

The target model-facing surface is:

1. `browser.snapshot`
2. `browser.change`
3. `page.inspect`
4. `page.act`
5. `page.evaluate`

During migration, the implemented `browser.list` and `tabs.list` tools may remain
available temporarily. The final V1 surface has only the five target names, with
page tools omitted from discovery while their global capabilities are disabled.

## Locked V1 decisions

These decisions keep the product and schemas straightforward:

- The installation bearer is the single MCP trust boundary. Every authenticated
  client has the same enabled authority.
- The extension has three global controls: Browser changes, Page tools, and
  Advanced evaluation.
- All three controls default to disabled on fresh installation and upgrade until
  the user explicitly enables them.
- There is no per-client, per-tab, per-window, per-origin, or per-operation
  authorization system.
- Browser-change preview is always available. Apply requires the global Browser
  changes control.
- Page tools use one global optional permission grant and are absent from tool
  discovery while disabled.
- Advanced evaluation is a separate global capability and is absent while
  disabled or unsupported.
- Reads never activate, focus, reload, scroll, or wake as an implicit side
  effect.
- V1 visual inspection captures only the selected tab's current viewport. Its
  window need not be focused, but Effector never selects the tab implicitly.
- `page.act.inspectAfter` provides one-call action-and-inspect workflows,
  including scroll-and-capture.
- Arbitrary evaluation uses an isolated user-script world on Chrome 135 or
  later. Main-world execution is deferred.
- Undo, browser event feeds, fallback locators, raw coordinates, trusted input,
  full-page/background capture, and generic Chrome/DOM/CDP forwarding are
  deferred.
- `debugger` is not part of V1. A later debugger-capable manifest or extension
  variant may add non-active-tab and full-page capture after a separate decision;
  Chrome does not allow `debugger` as an optional permission.

## Current baseline

| Area | Current implementation | Required V1 work |
| --- | --- | --- |
| MCP tools | `browser.snapshot` plus migration-only `browser.list` and `tabs.list` | Browser changes, dynamic page-tool discovery, and image blocks |
| Native protocol | Version 2 typed handshake, capabilities, errors, request metadata, and response identity | Typed image artifacts and later operation classes |
| Broker state | Browser identity, capabilities, references, immutable browser snapshots, limits, and read deadlines | Mutation plans, page snapshots, artifacts, and operation-aware deadlines |
| Browser reads | Event-backed normalized model, object incarnations, and consistent snapshot capture | Live-platform race and restart validation |
| Browser changes | None | Global toggle, exact preconditions, ordered non-atomic executor, partial/unknown outcomes |
| Page access | None | Optional permissions, document tracking, injected semantic agent, active viewport capture, actions |
| Evaluation | None | Conditional `userScripts.execute()` path for Chrome 135+ |
| Extension UI | Broker status and direct inventory report | Three capability controls and permission onboarding/revocation |
| Tests | Rust process coverage plus dependency-free extension model/protocol tests | Synthetic live Chrome suite and token/byte budgets |

## Target architecture

```text
MCP clients
    │ authenticated Streamable HTTP
    ▼
Rust broker
    ├── dynamic MCP tool registry
    ├── typed public schemas and compact results
    ├── opaque-reference registry
    ├── immutable snapshot/page stores
    ├── browser-change plan store
    └── capability state and notifications
    │ typed Native Messaging protocol
    ▼
Extension service worker
    ├── live browser model and object incarnations
    ├── global capability state
    ├── browser-change executor
    ├── page/document coordinator
    └── visual capture coordinator
         │ fixed packaged injection
         ▼
    isolated-world page agent
         ├── semantic inspection
         ├── element registry
         ├── DOM actions
         └── bounded post-action observation
```

Chrome continues to own broker lifetime. The broker remains authenticated
loopback HTTP and stdout remains Native Messaging only. Broker references,
snapshots, plans, and artifacts are memory-only; extension capability settings
persist in `chrome.storage.local`.

## State ownership

### Rust broker

One process-wide runtime, shared by every MCP session, owns:

- The connected `browserInstanceId` and extension capability snapshot.
- Public opaque references and their extension-internal targets.
- Immutable browser snapshots, page snapshots, cursors, and expiry accounting.
- Browser-change previews, plan state, ordered outcomes, and deduplication.
- Aggregate record, byte, object, and concurrency limits.
- Dynamic tool discovery and tool-list-change fan-out.
- Conversion of one internal image artifact into one MCP image content block.

This state must not live in one `EffectorMcp` instance because the Streamable
HTTP service creates handlers for multiple MCP sessions.

### Extension service worker

The extension owns:

- The live window/group/tab model and Chrome numeric IDs.
- Random internal incarnation keys so reused Chrome IDs cannot satisfy stale
  broker references.
- Window, group, tab, navigation, activation, viewport, and scroll generations.
- Persistent global capability settings and effective Chrome permission state.
- Exact-document page routing and page-agent lifecycle.
- Final permission, target, and stale-state checks immediately before Chrome or
  page operations.
- Browser-change, screenshot, action, and evaluation execution.

### Injected page agent

One dependency-free fixed script in the extension package owns:

- DOM-derived semantic extraction.
- A bounded map from internal node tokens to exact DOM elements.
- Visibility, enabled, obstruction, and basic action-compatibility checks.
- DOM-backed click, fill, select, check, scroll, and focus operations.
- Mutation observation used by the bounded automatic wait.

The page agent never receives arbitrary Chrome API names, native commands, or
filesystem access. Caller JavaScript uses the separate `userScripts` path.

## Reference and lifetime model

The extension returns internal incarnation keys and document/node tokens. Rust
maps them to public random typed references.

| Public reference | Broker record | Invalidated by |
| --- | --- | --- |
| `windowRef` | Window incarnation and current Chrome ID | Window removal, broker restart |
| `groupRef` | Group incarnation, window incarnation, Chrome ID | Group removal, broker restart |
| `tabRef` | Tab-container incarnation and Chrome ID | Tab removal/replacement, broker restart |
| `documentRef` | Tab incarnation, frame/document ID, page epoch | Navigation/document replacement, broker restart |
| `browserSnapshotRef` | Retained immutable normalized browser state | TTL/eviction, broker restart |
| `pageSnapshotRef` | Retained semantic records for one document | Navigation, TTL/eviction, broker restart |
| `elementRef` | Page snapshot, document/frame, internal node token | Navigation, node replacement/removal, TTL, broker restart |
| `planRef` | Exact operations, preconditions, state, retained result | TTL/eviction, broker restart |

Rules:

- Chrome numeric IDs never cross the public MCP boundary.
- The same live tab receives the same `tabRef` across snapshots and navigation.
- A `tabRef` survives navigation; a `documentRef` does not.
- Snapshot cursor reads use retained data and never query live Chrome.
- Page cursors fail stale after navigation because their element references no
  longer describe the live document.
- Broker restart invalidates all public references, even if the extension worker
  and browser instance survive.
- Closed retained objects return `NOT_FOUND`; expired retained handles return
  `HANDLE_EXPIRED`.

## Internal protocol version 2

[ADR 0005](decisions/0005-browser-incarnations-snapshots-and-mutation-plans.md)
fixes a coordinated Native Messaging upgrade to protocol version 2. It adds:

- A full capability snapshot in `ready`, represented by explicit booleans rather
  than an open-ended capability-name list.
- A monotonic capability revision.
- An unsolicited `capabilities_changed` full-state message.
- Typed response errors that preserve `code` and safe `message`.
- Optional image artifacts separate from structured results.
- Extension object-incarnation keys in internal inventory and result records.
- Request class/deadline metadata where operations need more than the current
  fixed timeout.

The fundamental `ready`, `ready_ack`, `request`, `response`, and `requestId`
correlation model remains. Rust, extension, tests, and protocol documentation
change together. Old and new protocol versions fail visibly rather than
silently mixing behavior.

An internal visual response should resemble:

```json
{
  "type": "response",
  "requestId": "...",
  "ok": true,
  "result": {
    "documentKey": "...",
    "width": 1440,
    "height": 900,
    "scrollX": 0,
    "scrollY": 1820,
    "mimeType": "image/png"
  },
  "artifacts": [
    {
      "type": "image",
      "mimeType": "image/png",
      "data": "base64..."
    }
  ]
}
```

Image data appears once. The broker validates and moves it into an MCP image
content block; it never copies the base64 into structured content or text.

## Capability discovery

Effective capability is the conjunction of stored global setting, currently
granted Chrome permission, Chrome version/API support, and live API probes.

| Capability state | Advertised tools |
| --- | --- |
| Core only | `browser.snapshot`, `browser.change` |
| Page tools enabled | Core plus `page.inspect`, `page.act` |
| Advanced evaluation enabled and supported | All five |

Migration releases may additionally advertise `browser.list` and `tabs.list`.

Rules:

- `browser.change` remains visible because preview is read-only; apply rechecks
  the Browser changes toggle.
- Hidden page tools still reject a stale direct call with
  `CAPABILITY_DISABLED` or `CAPABILITY_UNAVAILABLE`; discovery is not the
  authorization boundary.
- Capability changes update one broker-wide state snapshot.
- The broker advertises MCP tool-list-change support and notifies every eligible
  initialized session.
- Clients without list-change support must reconnect to discover newly enabled
  tools.
- The extension rechecks toggles and permissions at dispatch time.
- Revocation before dispatch produces no side effect. Revocation after a
  side-effecting dispatch may produce a partial or unknown result.

## MCP contract implementation

### Strict inputs

- Use camelCase `Deserialize`, `Serialize`, and `JsonSchema` types.
- Use tagged enums for `browser.change` mode, browser operation type, page view,
  and page action type.
- Reject unknown fields on every object/union branch.
- Use typed newtypes for each opaque reference kind.
- Model omitted versus explicit `null` for `tab.move.groupRef` explicitly.
- Validate aggregate limits at runtime; schema annotations alone are not
  enforcement.
- Custom-decode tool arguments when necessary to return structured
  `INVALID_ARGUMENT` errors rather than framework-generated text.

### Typed outputs

- Define and publish an output schema for every advertised tool branch.
- Validate extension output by deserializing into the expected Rust type.
- Use success-or-domain-error output unions if structured errors are returned
  with MCP `isError=true`.
- Return one short text summary, canonical structured content, and at most one
  image block.
- Never use a helper that serializes the complete structured result into text.
- Never echo operation/action input in a successful result.

### Errors and uncertainty

- Preserve extension domain error codes instead of converting them to `anyhow`
  strings.
- Keep broker infrastructure errors separate from browser/page domain errors.
- Sanitize and cap Chrome-provided error messages.
- Failures known to occur before dispatch use MCP `isError=true`.
- Once side effects may have been dispatched, return a normal result with
  `partial` or `unknown`; never encourage a blind retry.
- `page.act` success followed by `inspectAfter` failure returns action status and
  a compact `inspectionError`, not a false claim that the action failed.

## Initial limits and budgets

These are starting values for implementation and measurement, not promises that
must survive performance testing unchanged.

| Resource | Starting limit |
| --- | ---: |
| Broker-to-extension serialized request | 768 KiB |
| Non-image structured result | 4 MiB |
| Encoded active-viewport image | 8 MiB |
| Image dimensions | 4096 by 4096 pixels |
| Aggregate retained broker state | 32 MiB |
| Retained browser snapshots | 16 |
| Retained page snapshots | 8 |
| Retained plans | 64 |
| Browser snapshot TTL | 2 minutes |
| Page snapshot/element TTL | 1 minute |
| Plan TTL | 5 minutes |
| Browser operations per plan | 50 |
| Total target references per plan | 100 |
| Semantic element records | 2,000 |
| Semantic text | 256 KiB |
| Evaluation source | 64 KiB |
| Evaluation arguments | 256 KiB |
| Evaluation result | 1 MiB |

All limits need boundary tests. Base64 expands image payloads, so screenshot
limits apply to the encoded Native Messaging envelope as well as decoded bytes.
The existing 1 MiB broker-to-Chrome and 64 MiB Chrome-to-broker limits remain
hard ceilings, not product result budgets.

## Phase 0: Decisions and spikes

### ADRs

The two required foundation ADRs are accepted:

1. [ADR 0005](decisions/0005-browser-incarnations-snapshots-and-mutation-plans.md)
   fixes browser incarnations, public references, complete broker-retained
   baselines, immutable snapshots, exact mutation preconditions, and non-atomic
   plan execution.
2. [ADR 0006](decisions/0006-global-page-capabilities-and-isolated-tools.md)
   fixes global page permissions, DOM-derived semantics/actions, active viewport
   capture, `inspectAfter`, and isolated user-script evaluation.

The second ADR records the later debugger-capable variant as future work, not a
V1 permission.

Locked first-slice foundation details are protocol version 2 as a coordinated
upgrade; explicit capability booleans; focused-window-first then ascending
runtime Chrome window ID ordering; complete browser baselines retained by the
broker; FIFO bounded eviction; fixed TTLs that reads do not refresh; random
server-side cursors; no retention for counts; failure for unsupported `frozen`
filters; persistent extension toggles; and memory-only broker references,
snapshots, cursors, and plans.

### Required spikes

1. **Schema/error spike:** Generate representative operation/action unions and
   output-error unions with the locked `schemars`/`rmcp` versions. Verify
   discriminators, `additionalProperties:false`, bounds, and target-client
   handling of structured errors.
2. **Dynamic discovery spike:** Prove capability filtering and tool-list-change
   delivery to two MCP sessions. Confirm behavior for clients that do not
   refresh dynamically.
3. **Plan concurrency spike:** Apply one mock plan concurrently, cancel waiters
   before and after dispatch, inject a late response, and prove one dispatch.
4. **Image round-trip spike:** Send a synthetic PNG through Native Messaging and
   MCP. Verify one image block, no duplicated base64, limits, and rendering in
   target clients.
5. **Extension capability spike:** Exercise stored settings, optional permission
   grant/revocation, startup races, Chrome version gates, and capability-change
   ordering in mocks and live Chrome.
6. **Compact fallback spike:** Confirm target clients consume structured content
   when text is only a summary. If a required client cannot, document a
   compatibility mode rather than duplicating every result by default.

### Exit gate

- ADRs are accepted.
- Every example in `mcp-tools.md` has a matching proposed schema.
- The spikes settle framework and client compatibility questions.
- Final starting limits and timeout classes are recorded.
- No manifest permission is changed before this gate passes.

## Phase 1: Contract and broker foundation

### Native work

- Create one `BrokerRuntime` before the MCP service factory and share it with
  every `EffectorMcp` instance.
- Retain the connected browser identity and extension capabilities.
- Replace untyped response parsing with typed protocol envelopes and errors.
- Reject oversized outbound requests before inserting pending state or queueing
  the writer; never silently drop them.
- Add operation-aware request deadlines with margin above public action/evaluate
  timeouts.
- Add bounded registries for references, snapshots, page snapshots, and plans.
- Use FIFO eviction and fixed creation-time TTLs that reads do not refresh.
- Issue random server-side cursor handles; do not expose offsets or
  client-decodable retention state.
- Replace the static tool handler path where needed for dynamic discovery,
  strict structured argument errors, output schemas, and image blocks.
- Preserve bearer, Host, Origin, loopback, bounded queues, and Chrome-owned
  shutdown behavior.

### Extension work

- Add typed internal message validation and capability snapshots.
- Add `capabilities_changed` delivery.
- Keep existing read methods operational during migration.
- Ensure reconnect/new broker handshake resets all broker-scoped internal page
  and reference epochs.

### Tests

- Protocol revision mismatch and malformed capability snapshots.
- Browser identity retained and checked.
- Structured domain errors survive a full MCP round trip.
- Unknown fields, bad discriminators, wrong reference types, and invalid bounds.
- Exact limit-minus-one, limit, and limit-plus-one cases.
- Oversized outbound message fails immediately rather than timing out.
- Queue/semaphore saturation remains bounded.
- EOF/restart clears pending calls and retained state.
- New broker/old extension and old broker/new extension upgrade ordering fails
  visibly at the protocol boundary; matched versions reconnect cleanly.
- Current `browser.list`/`tabs.list` regression behavior is frozen before shared
  normalization refactoring.

### Exit gate

- No proposed feature bypasses strict decoding or central limits.
- Existing tools retain their current behavior.
- Rust and extension errors no longer expose internal chains.
- Tool definitions and output schemas have deterministic golden fixtures.

## Phase 2: Live browser model and `browser.snapshot`

### Extension browser model

- Move window/group/tab normalization to one shared module.
- Register all Chrome event listeners synchronously at service-worker module
  evaluation before asynchronous baseline work.
- Build a normalized in-memory model with random object incarnation keys and a
  monotonic revision.
- Handle window create/remove/focus/bounds events; tab
  create/update/move/attach/detach/activate/highlight/remove/replace events; and
  group create/update/move/remove events.
- Buffer and reconcile events that race the initial Chrome queries.
- Refetch an object when an event lacks enough fields to update safely.
- Expose one consistent internal snapshot capture method.

### Broker snapshot work

- Intern stable public window/group/tab references from incarnation keys.
- Implement counts, compact, and full projections.
- Apply exact target scope and metadata filters before pagination.
- Enforce runtime cross-field rules: target scopes are mutually exclusive,
  cursor calls contain only cursor, and counts rejects limit/cursor.
- Implement case-insensitive title/URL query.
- Preserve tab-strip ordering through each window's ordered `items` array.
- Order the focused window first, then remaining windows by ascending runtime
  Chrome window ID.
- Retain the complete bounded browser baseline, derive the matching immutable
  snapshot once, and serve cursor pages locally.
- Return counts without retaining a snapshot, cursor, or counts record.
- Reject unsupported filters, such as frozen state on an older Chrome, instead
  of treating missing as false.
- Set `partial` only when pagination omits matching children from a returned
  window or group.
- Fail if one record cannot fit rather than returning a non-advancing cursor.

### Tests

- Empty, grouped, ungrouped, pinned, discarded, frozen, and multi-window state.
- Interleaved grouped/ungrouped ordering and cursor splits through a group.
- Every boolean filter as omitted, true, and false.
- Mutually exclusive scopes, cursor-only continuation, and counts-only negative
  argument cases.
- Query, counts privacy, compact omission, full optional fields.
- Event races during baseline and after snapshot retention.
- Cursor replay, tampering, expiry, eviction, and broker restart.
- 0, 1, 100, 250, and over-250 synthetic tabs.
- Live validation that discarded/frozen tabs and window focus are unchanged.

### Exit gate

- Empty `browser.snapshot` is useful and bounded.
- Cursor pages never mix live states.
- Counts return no title or URL data.
- `effector doctor` can move to `browser.snapshot(detail="counts")` without
  printing inventory.
- Ship snapshot beside legacy tools; do not deprecate legacy tools yet.

## Phase 3: `browser.change`

### Plan model

Preview resolves public references against one retained browser snapshot and
stores:

- Exact typed operations and order.
- Target/destination incarnation keys.
- Only the preconditions relevant to each operation.
- `stopOnError`, summary, warnings, expiry, and browser instance.
- State: `ready`, `applying`, `finished`, or `unknown`.
- One retained result for process-local deduplication.

The plan store atomically marks a plan applying before Native Messaging
dispatch. Concurrent apply calls join the same result. HTTP cancellation does
not reset a dispatched plan to ready. Broker restart invalidates the plan and
requires a fresh snapshot.

### Phase 3A: Toggle, preview, and simple apply

- Add the global Browser changes control to extension storage and popup UI,
  initialized disabled for fresh installations and upgrades.
- Keep preview available while apply is disabled.
- Implement concise preview summary/warnings without echoing operations.
- Add overlap serialization and bounded concurrency for disjoint plans.
- Implement `tab.move`, `tab.update`, `tab.activate`, `group.create`,
  `group.update`, and `window.update`.
- Recheck capability and exact preconditions in the extension immediately before
  each operation.
- Read back/reconcile postconditions and return ordered concise outcomes.
- Capture and retain a fresh normalized browser snapshot after apply whenever
  state can be established. Return its reference for applied and partial
  results; omit it for unknown outcomes that cannot be reconciled.

Relevant preconditions should be narrow:

| Operation | Preconditions |
| --- | --- |
| `tab.update` | Tab incarnation plus properties being changed |
| `tab.activate` | Tab/window incarnations; current activation only when needed |
| `tab.move` | Tab incarnation/order and destination incarnation/index context |
| `group.create` | Tab incarnations and same-window placement |
| `group.update` | Group incarnation plus properties being changed |
| `window.update` | Window incarnation plus properties being changed |

Title or URL changes should not stale an unrelated pin/move plan. Unrelated
browser activity should not invalidate exact targets.

### Phase 3B: Complete operation set

- Add tab open, close, duplicate, discard, and reload.
- Add group move and tab grouping/ungrouping through `tab.move`.
- Add window create and close.
- Define final-tab/final-window behavior explicitly.
- Validate URL schemes and active-tab discard rejection.
- Treat multi-step operations as composite and allow per-operation `partial`.

### Outcome rules

- Known failure before dispatch: MCP domain error, no side effects.
- Verified operation rejection: `failed`.
- Composite first step succeeds and later step fails: `partial` with known
  created references.
- Dispatch or read-back loses certainty: `unknown`.
- `stopOnError=true`: later entries are `skipped`.
- `stopOnError=false`: only independent later operations continue.
- Every partial/unknown result directs recovery through a fresh snapshot.

### Tests

- Preview invokes no mutating Chrome API.
- Apply cannot alter previewed operations or run a consumed plan twice.
- Toggle enable/disable and revocation between operations.
- Relevant stale changes versus unrelated activity.
- Overlapping plans, disjoint plans, and two MCP sessions applying one plan.
- Timeouts/cancellation before queue, after queue, after dispatch, and during
  read-back.
- Every operation, invalid combination, dragged-tab transient failure,
  destructive warning, final-window behavior, partial, and unknown outcome.
- Destructive live tests run only in a marked throwaway profile/window.

### Exit gate

- Browser changes remain disabled by default for existing users.
- There is no arbitrary Chrome API forwarding or custom confirmation system.
- Apply is exact, bounded, non-atomic, and process-locally deduplicated.
- A fresh snapshot is sufficient for all documented recovery.

## Phase 4: Global page capability and semantic inspection

### Permission UI

- Add optional `scripting`, `webNavigation`, and `userScripts` permissions plus
  optional `<all_urls>` host access to the manifest.
- Extend the popup with the Page tools control. The Browser changes control
  already exists from Phase 3; the Advanced evaluation control is added in
  Phase 7.
- Page tools enablement requests `scripting`, `webNavigation`, and `<all_urls>`
  from the popup click gesture.
- Initialize Page tools disabled for fresh installations and upgrades.
- Effector still enforces HTTP(S)-only page targets despite the broad capture
  grant.
- Denial leaves the global setting disabled.
- Disabling removes optional permissions where practical and always blocks calls
  immediately.
- Observe storage and `chrome.permissions` changes and publish a fresh capability
  snapshot.
- Incognito and restricted browser pages remain denied.

### Document tracking

- Track main-frame and child-frame Chrome document IDs through webNavigation.
- Register navigation listeners synchronously at service-worker module load.
- At startup, permission enablement, worker recovery, and target resolution,
  reconcile pre-existing documents with `webNavigation.getAllFrames()` rather
  than assuming an event was observed.
- Map exact tab/document/frame identities to broker-visible internal keys.
- Treat same-URL reload as a new document.
- Reject discarded, frozen, restricted, and stale targets before injection.
- Inject one fixed packaged page agent into exact authorized documents.
- Namespace page-agent state by connection/page epoch so old node tokens cannot
  satisfy new broker references.

### Semantic page agent

- Traverse the main DOM and open shadow roots; document closed-shadow limits.
- Extract bounded visible prose, headings, landmarks, links, forms, and
  interactive elements.
- Deduplicate text represented structurally by headings/control names.
- Derive a documented basic role and accessible-name approximation.
- Include useful ordinary non-password control values and select option
  value/label pairs.
- Always omit password, file-input, and hidden-control values.
- Omit default visible/enabled state; include exceptional state.
- Include geometry and secondary structure only in full detail.
- Assign random internal node tokens and retain exact node mappings with bounded
  lifecycle.
- Maintain viewport and scroll generation counters in the page agent so visual
  capture can detect transient resize/scroll activity even when final values
  return to their starting values.
- Sort frame results deterministically because Chrome does not guarantee
  non-main-frame injection result order.

### Broker page snapshots

- Validate returned document IDs and bounded records.
- Replace internal node tokens with public `elementRef` values.
- Retain semantic pages for immutable cursor replay.
- Revalidate document identity before cursor/element use.
- Serve compact/full and main/all-frame branches without advertising
  unimplemented combinations.
- Implement explicit viewport/document scope projection and exact
  `elementRef`-plus-immediate-context extraction. Only `scope="auto"` policy
  remains configurable by the ADR.

### Tests

- Permission absent, granted, denied, revoked, and changed during a call.
- HTTP(S), restricted, incognito, discarded, frozen, loading, and navigated
  documents.
- Main, same-origin, and cross-origin frames.
- Open shadow DOM and closed-shadow limitation.
- Same-URL reload, history traversal, and frame navigation.
- Documents already loaded before permission enablement and after service-worker
  restart/recovery.
- Text deduplication, hostile strings, ordinary values, select options, and
  password/file/hidden omission.
- Compact/full fields, node/text/byte limits, cursors, stale elements, and
  similar replacement elements that must not retarget.
- Viewport/document scope projection and element-context inspection.
- No activation, focus, scroll, reload, or wake during semantic reads.

### Exit gate

- Page tools are globally disabled by default and cannot self-enable through
  MCP.
- Privacy/security documentation reflects page text and ordinary control values.
- Semantic output is explicitly DOM-derived, not a canonical accessibility tree.
- Ship semantic `page.inspect` before visual capture or actions.

## Phase 5: Active-viewport visual inspection

### Capture algorithm

1. Resolve exact tab and document.
2. Confirm the target is already active in its own window; window focus is not
   required.
3. Record tab/window incarnation, activation generation, navigation/document
   generation, viewport generation, and scroll generation/coordinates.
4. Read exact document and viewport metrics from fixed page-agent code.
5. Call `chrome.tabs.captureVisibleTab(windowId, {format: "png"})`.
6. Drain/reconcile events and reread tab/document/viewport/scroll state.
7. Reject if any generation changed, even when final values equal initial
   values.
8. Validate data URL prefix, PNG payload, encoded bytes, and dimensions.
9. Create a retained visual page-snapshot metadata record and return its
   `pageSnapshotRef` plus exactly one image artifact. Image bytes need not be
   retained.

This catches ordinary and A-to-B-to-A activation/navigation/scroll races as far
as Chrome events permit. If live validation cannot establish the guarantee,
weaken the documented claim rather than shipping an unprovable identity promise.

### `both`

- Collect viewport-scoped semantic content and image sequentially.
- Use one document/page snapshot identity.
- Reject the complete result if activation, navigation, viewport, or scroll
  generation changes between phases.
- Return one structured semantic result and one image block.

### Rate and concurrency

- Serialize or tightly bound captures.
- Respect Chrome's capture rate limit and map it to `RATE_LIMITED`.
- Do not retry capture automatically in a way that could cross page state.
- Never activate, focus, scroll, stitch, or attach a debugger.

### Tests

- Active target in focused and unfocused windows.
- Non-active target fails before capture.
- Activation A-to-B and A-to-B-to-A races.
- Navigation, same-URL reload, viewport resize/zoom, scroll, and away-and-back
  scroll races.
- Permission revocation and target close during capture.
- Rate limiting, malformed image, byte/dimension boundaries, PNG MIME.
- Image appears once and never in structured JSON, text, logs, or reports.
- Deterministic red/blue synthetic pages prove which tab was captured.
- Live behavior on Windows, macOS, and Linux.

### Exit gate

- Active viewport capture is reliable enough for the documented claim.
- Visual output contains compact metadata and one image payload only.
- Full-page/non-active capture remains clearly deferred to the later debugger
  capability.

## Phase 6: Structured page actions and `inspectAfter`

### Page-agent actions

- Resolve only exact `elementRef` node tokens; no locator or coordinate fallback.
- Verify document/frame, node connection, visibility, enabled state,
  obstruction, and action compatibility.
- Implement DOM-backed click, fill replace/append, select, check, scroll, and
  focus.
- Report DOM interaction honestly; do not describe synthetic events as trusted
  physical input.

### Service-worker actions

- Implement navigate, back, forward, and reload after exact document precheck.
- Restrict navigation URLs to the documented schemes.
- Register navigation/mutation observation before dispatch.
- Define one V1 auto-wait rule: navigation readiness when observed, otherwise a
  short DOM-mutation quiet interval, all bounded by `timeoutMs`.
- For `wait="none"`, return immediately after dispatch bookkeeping without DOM
  quiet or navigation-readiness waiting; report a new document only when it is
  already known.
- Return the new `documentRef` after navigation/reload and usually the same one
  otherwise.

### `inspectAfter`

- Reuse the internal semantic/visual/both pipelines; do not define a second page
  representation.
- If visual output is requested, run active-tab capture preflight before the
  action. Failure then causes no mutation.
- After action and wait, inspect the current resulting document at compact
  viewport scope.
- Return the action result plus optional `inspection` and at most one image.
- If action succeeds but inspection fails, return `status:"applied"` with a
  compact `inspectionError` and no blind retry recommendation.

### Tests

- Every action variant and invalid target/action combination.
- Removed/replaced node, hidden, disabled, obscured, readonly, and incompatible
  controls.
- Input/textarea/contenteditable policy, replace/append, and event order.
- Single/multiple select and option values.
- Checkbox/radio desired-state behavior.
- Document and nested-container scroll.
- Navigate/back/forward/reload identity and URL rejection.
- Navigation between target resolution and dispatch.
- Action-triggered navigation, DOM churn, quiet wait, timeout, and unknown
  outcome.
- Distinguish `wait="none"` from `wait="auto"` for DOM and navigation actions.
- Cross-origin frame and open-shadow-root actions.
- Scroll-and-capture, click-and-inspect, navigate-and-inspect, visual preflight
  failure before mutation, and action success plus inspection failure.
- No implicit activation, wake, upload, native dialog, browser UI, or trusted
  input path.

### Exit gate

- One action per call and no hidden retargeting.
- Default success does not echo inputs.
- `inspectAfter` removes ordinary verification calls without making
  `page.inspect` mutating.
- Documentation states that Effector does not infer purchase, publication,
  deletion, authentication, or other site-level meaning.

## Phase 7: Advanced isolated evaluation

### Capability

- Advanced evaluation depends on Page tools.
- Add its popup control in this phase. Initialize it disabled on every fresh
  installation and upgrade; enabling requires an explicit popup gesture.
- Request optional `userScripts` from an extension-UI gesture.
- Require Chrome 135+ for one-shot exact-document execution.
- On Chrome 135-137, explain the Developer mode requirement.
- On Chrome 138+, explain and probe the per-extension Allow User Scripts setting.
- Probe at startup and call time because API availability can become stale after
  revocation until service-worker reload.

### Execution

- Resolve `documentRef` to exact tab and Chrome document ID.
- Reject restricted, discarded, frozen, stale, or unsupported targets.
- Enforce source and argument limits before transport.
- Safely serialize `args` into the generated isolated user-script wrapper; never
  interpolate raw values into source.
- Invoke `chrome.userScripts.execute()` in `USER_SCRIPT` world against exact
  document IDs.
- Accept exactly one expected result, validate serialization, and enforce result
  bytes.
- Return only `{value}` or `{status:"unknown"}`.
- Treat timeout as observation timeout, not cancellation. Never automatically
  retry arbitrary code.

### Tests

- Chrome 134 absent, Chrome 135-137 Developer mode path, Chrome 138+ user toggle.
- Permission/toggle revocation while the worker remains alive and after restart.
- Exact document, navigation before/during execution, restricted/frozen/
  discarded targets.
- Quotes, backslashes, script-like strings, Unicode separators, and large args.
- Values, promises, thrown/rejected errors, non-serializable/oversized results.
- Timeout followed by a delayed side effect to prove non-cancellation.
- No source, args, values, or page errors in logs.

### Exit gate

- The tool is absent unless every global/Chrome capability is effective.
- It cannot self-enable through MCP.
- Security and privacy docs explicitly cover arbitrary page code.

## Phase 8: Compatibility and migration

### Rollout order

1. Foundation release with no tool behavior change.
2. `browser.snapshot` alongside both legacy tools.
3. `browser.change` preview, then opt-in simple apply.
4. Complete browser mutations after live destructive testing.
5. Optional semantic page inspection.
6. Active-viewport visual inspection.
7. Structured page actions and `inspectAfter`.
8. Conditional advanced evaluation.
9. V1 release candidate with legacy tools marked deprecated.
10. Legacy removal after client and live-platform gates pass.

### Compatibility rules

- Never advertise a tool branch the connected extension cannot implement.
- A protocol mismatch fails visibly and requires broker/extension upgrade.
- Capability disablement blocks new calls immediately.
- In-flight reads fail closed after revocation.
- In-flight side-effecting calls return partial/unknown if dispatch may have
  occurred.
- Legacy tool semantics remain frozen during deprecation.
- Clients without tool-list-change support reconnect after toggles change.
- Move `effector doctor` to counts after snapshot ships and stop printing full
  structured inventory.

### Legacy removal gate

- `doctor`, README examples, troubleshooting, tests, and live runbooks use the
  target tools.
- Target MCP clients support the compact structured results and image blocks.
- Upgrade ordering and restart behavior are validated.
- A documented deprecation release has shipped, or maintainers explicitly choose
  a pre-release breaking removal.

## File-by-file implementation map

### Native Rust

| File | Planned work |
| --- | --- |
| `native/src/main.rs` | Declare new internal modules |
| `native/src/broker.rs` | Build shared runtime; typed protocol; capabilities; owned artifacts; immediate request-size failures; timeout classes; shutdown cleanup |
| `native/src/mcp.rs` | Custom dynamic discovery/call dispatch; strict schemas; compact result/error/image construction |
| `native/src/protocol.rs` | New: protocol envelopes, version, frame limits, typed errors/artifacts |
| `native/src/runtime.rs` | New: browser identity, capability watch state, registries, stores, quotas |
| `native/src/references.rs` | New: typed public refs, interning, lookup, expiry/tombstones |
| `native/src/browser_snapshot.rs` | New: normalized records, filters, hierarchy projection, cursor store |
| `native/src/browser_change.rs` | New: operation schemas, preview, preconditions, plan state machine, outcomes |
| `native/src/page.rs` | New: document/element refs, semantic cursors, action/evaluation result conversion, image artifacts |
| `native/src/doctor.rs` | Migrate to counts and print health-only output |
| `Cargo.toml` | Add only direct dependencies proven necessary, likely `base64`; avoid image libraries unless required |

The exact module split may be adjusted during implementation, but broker-wide
state, protocol types, browser tools, and page tools should not accumulate in the
current small `mcp.rs`.

### Extension

| File | Planned work |
| --- | --- |
| `extension/background.js` | Keep connection/orchestration; import modules; dispatch typed methods; capability notifications; artifacts |
| `extension/browser-model.js` | New: normalization, incarnations, baseline/event reconciliation, consistent snapshots |
| `extension/capabilities.js` | New: stored toggles, effective permissions/version probes, change notifications |
| `extension/browser-change.js` | New: typed Chrome mutation executor and partial outcomes |
| `extension/page-tools.js` | New: target resolution, document tracking, injection, capture, actions, evaluation coordination |
| `extension/page-agent.js` | New: fixed isolated semantic inspector, element registry, DOM actions, mutation wait |
| `extension/manifest.json` | Add optional permissions/host permissions after ADR approval; do not add `debugger` |
| `extension/popup.html` | Add accessible global controls/capability explanations and load popup JS as a module when imports are introduced |
| `extension/popup.js` | Request/remove permissions from click gestures; render effective states; stop duplicating normalization |
| `extension/popup.css` | Style capability controls and warning states consistently |

The extension remains dependency-free and directly loadable as unpacked MV3
source. No build step or runtime package manager is introduced.

### Tests and CI

| File | Planned work |
| --- | --- |
| `tests/support/` | Shared broker spawning, framing, MCP clients, delayed/failed mock extension responses |
| `tests/broker_roundtrip.rs` | Protocol revision, capabilities, limits, restart, shutdown |
| `tests/mcp_tools.rs` | Dynamic names, schemas, annotations, list-change behavior |
| `tests/full_mcp_roundtrip.rs` | Structured errors, snapshots, plans, semantic refs, image blocks, evaluation |
| `tests/browser_snapshot.rs` | New focused snapshot/cursor contract tests |
| `tests/browser_change.rs` | New preview/apply/concurrency/uncertainty tests |
| `tests/page_tools.rs` | New inspect/action/evaluate process tests |
| `tests/doctor.rs` | New counts-only diagnostic tests |
| `extension/tests/*.test.mjs` | New dependency-free Node tests for model, capabilities, changes, page logic, protocol |
| `.github/workflows/ci.yml` | Syntax-check all modules and run `node --test extension/tests/*.test.mjs` |

## Validation strategy

### Every native-code phase

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

### Every extension phase

```bash
node --check extension/background.js
node --check extension/popup.js
node --test extension/tests/*.test.mjs
```

CI should syntax-check every new extension module. Node remains a development
test tool, not an extension runtime dependency.

### Live synthetic fixtures

Add local pages for:

- Basic text, headings, links, controls, long/hostile strings.
- Text/textarea/select/checkbox/radio/password/file/hidden/disabled controls.
- Navigation, reload, history, delayed load, and DOM churn.
- Same-origin and cross-origin frames.
- Open and closed shadow roots.
- Obscured elements, contenteditable, nested scroll containers, action events.
- Deterministic red and blue pages for screenshot target attribution.

Use a throwaway Chrome profile and a dependency-free local HTTP server. Live
tests never print or retain page content, inventory, images, evaluation source,
or bearer tokens.

### Platform/version matrix

| Environment | Required validation |
| --- | --- |
| Linux Chrome stable | Full synthetic page and browser-state suite |
| Windows Chrome stable | Native Messaging, mutations, unfocused-window capture, restart |
| macOS Chrome stable | Registration, permissions, capture, restart |
| Chrome stable and beta | Event ordering and optional-permission regressions |
| Chrome 134 or earlier test image | Evaluation absent/unavailable |
| Chrome 135-137 test image | Developer-mode userScripts path |
| Chrome 138+ | Allow User Scripts path |
| WSL2 mirrored networking | Discovery and representative calls to Windows Chrome |
| Two MCP sessions | Shared capabilities, overlapping/disjoint plans, list changes |

## Token-efficiency gate

For each release, record UTF-8 bytes and model tokens for:

- Core-only tool discovery.
- Core plus page tools.
- All five tools.
- The temporary migration surface with legacy tools.
- Empty and 100-tab compact snapshots.
- Compact semantic inspection.
- Visual metadata excluding image bytes.
- Browser-change preview/apply.
- Successful/failed page action and `inspectAfter`.

Review rules:

- Tool and argument descriptions remain one sentence where possible.
- Shared rules are not repeated in every argument.
- Compact results omit placement already expressed by hierarchy, default
  booleans, empty fields, geometry, raw DOM, and repeated text.
- Preview/apply and action success do not echo inputs.
- Image bytes occur once.
- Text fallback does not duplicate structured JSON.
- No unimplemented schema branch is advertised.
- Any material token increase needs a concrete workflow justification.

## Documentation obligations

Before each capability ships, update the applicable documents:

- `docs/mcp-tools.md`: exact implemented branches, schemas, limits, and errors.
- `docs/progress.md`: implementation and live-validation status.
- `docs/roadmap.md`: keep unfinished work prospective.
- `docs/architecture/overview.md`: state ownership and permission boundaries.
- `docs/architecture/process-and-transport.md`: protocol revision, capability
  events, image artifacts, deadlines, and retained state.
- `docs/architecture/failure-modes.md`: stale refs, partial/unknown outcomes,
  permission revocation, capture races, evaluation timeout.
- `README.md` and `docs/troubleshooting.md`: enablement, startup, rediscovery,
  recovery, Chrome version/userScripts requirements.
- `PRIVACY.md`: page text, ordinary control values, screenshots, in-memory
  retention, evaluation source/results.
- `SECURITY.md`: the bearer authorizes all globally enabled capabilities and
  arbitrary evaluation risk.

Required disclosures include:

- Local-first does not control what a configured MCP client does with results.
- Screenshots and page semantics can contain sensitive data.
- Page actions can have high-impact site effects; Effector does not classify
  business meaning.
- Evaluation can modify page/storage, send requests, navigate, expose data, and
  continue after timeout.
- No inventory, page content, screenshots, code, results, or credentials enter
  logs or crash diagnostics.
- Incognito remains disabled.

## Later debugger capability

After V1, a separate ADR may propose a debugger-capable manifest or extension
variant for:

- Non-active-tab screenshots.
- Full-page and element capture without scrolling/stitching.
- Canonical accessibility/DOM snapshot data.
- Trusted input where DOM actions are insufficient.

That work must account for the non-optional `debugger` permission, install-time
warning, DevTools attachment conflicts, detachment, separate extension identity
or packaging, and whether it duplicates Chrome DevTools MCP. It is not a hidden
follow-up inside the V1 Page tools toggle.

## Remaining implementation decisions

Settle these in the ADRs/spikes before the affected phase:

1. Final TTL, byte, token, and concurrency limits.
2. Exact precondition matrix for every browser operation.
3. Final-tab and final-window close behavior.
4. Exact `scope="auto"` semantic selection policy.
5. Role/name approximation and supported open-shadow behavior.
6. Exact DOM event sequences for click/fill/select/check.
7. Automatic-wait navigation readiness and quiet interval.
8. Final result schema for action success plus `inspectAfter` failure.
9. CSS viewport versus image-pixel dimension field names.
10. Final minimum Chrome version for Page tools while evaluation remains 135+.
11. Whether required target clients accept compact text fallback with canonical
    structured content.

## Final V1 release criteria

- All five contracts are implemented with no placeholder schema branches.
- Browser changes, Page tools, and Advanced evaluation are disabled by default
  for existing users.
- Direct calls cannot bypass disabled/revoked capabilities.
- Snapshot consistency, stale references, partial mutations, capture races,
  page actions, and isolated evaluation pass deterministic and live tests.
- Active viewport capture works in focused and unfocused windows without
  implicit selection/focus.
- Partial and unknown outcomes have safe fresh-snapshot/inspection recovery.
- Windows, Linux, macOS, restart, upgrade, and WSL2 representative paths are
  validated.
- Token, byte, retained-state, concurrency, deadline, and image budgets are
  measured and enforced.
- Documentation and privacy/security statements match shipped behavior.
- Legacy tools have been removed or have an explicit, tested deprecation plan.
- No real browser data or bearer credentials appear in test artifacts.
