# MCP tool surface implementation plan, Part 2

Status: in progress; protocol v3 read foundation implemented
Date: 2026-08-03

This document continues the implementation plan after Part 1. The authoritative
tool contract remains [`mcp-tools.md`](mcp-tools.md). Part 1 is
[`mcp-tool-surface-plan.md`](mcp-tool-surface-plan.md). Accepted ADRs remain the
durable architecture decisions; this plan translates those decisions and the
current code into actionable work without duplicating the complete public
contract.

Part 2 covers the remaining operation-capable foundation, `browser.change`,
semantic and visual `page.inspect`, `page.act`, `page.evaluate`, and migration
and removal of the legacy tools. At plan creation, none of those capabilities
was implemented except `browser.snapshot` and supporting Part 1 foundations.

Implementation has started with ADR 0007, the production background-controller
test seam, and the strict matched-v3 read path. Exact ABI/method negotiation,
rich capability facts, typed dispatch metadata, and bounded PNG artifact parsing
are implemented without changing permissions or public tools. P2.0 live/client
evidence remains open. The first snapshot-boundary slice now includes shared
process-test support, deterministic current-tool schema and representative-result
goldens, optional/non-null input-schema correction, compact-output omission
alignment, filter/scope/hierarchy coverage, cursor FIFO/expiry checks, and exact
result-size boundaries. Snapshot lookup, remaining races and retention cases,
P2.1 broker-owned operation tasks, and generalized retention remain open; they
must close before mutations are advertised.

[`Part 3`](mcp-tool-surface-plan-part-3.md) overlays workflow-efficiency and
schema decision gates on P2.5, P2.7, P2.8, and release measurement. It does not
replace this implementation sequence. Until its bounded-action candidate passes
its gates and receives a focused ADR, P2.8's singular `action` contract remains
authoritative.

[ADR 0006](decisions/0006-global-page-capabilities-and-isolated-tools.md) is the
approval for the narrow Page tools V1 surface. DOM-derived semantic inspection
and actions, already-active viewport capture, and isolated `userScripts`
evaluation are approved within that ADR's boundaries. `debugger`, canonical
DevTools data, trusted input, background or full-page capture, and broader
DevTools work remain deferred and require a later ADR.

## Outcomes

Part 2 is complete when:

- `browser.change` supports preview and the complete documented apply surface
  with exact retained plans and honest non-atomic outcomes.
- `page.inspect` supports bounded DOM-derived semantics and already-active
  viewport images without implicit browser disturbance.
- `page.act` performs one exact structured action and can reuse inspection for
  `inspectAfter`.
- `page.evaluate` is available only through effective isolated `userScripts`
  capability on supported Chrome versions.
- Discovery reflects live effective capability and implemented branches.
- Legacy `browser.list` and `tabs.list` are removed only after the migration
  gate passes.
- Every shipped branch has strict schemas, bounded retention and transport,
  deterministic tests, live-Chrome evidence, and matching operator/privacy
  documentation.

## Current baseline after Part 1

The continuation starts from the source and tests as they exist on 2026-08-03,
not from the prospective module map in Part 1.

| Area | Implemented state | Part 2 consequence |
| --- | --- | --- |
| MCP surface | Static `rmcp` tool router for `browser.snapshot`, `browser.list`, and `tabs.list` | Replace or wrap the static path before dynamic capability filtering |
| Runtime | One `BrokerRuntime` is constructed before the service factory and shared by all MCP sessions | Keep this ownership boundary; add operation tasks, capability watches, registries, and quotas here |
| Protocol | Strict version 2 handshake, request/response identity, read class, one deadline class, typed errors, and capability revision | Use one coordinated v3 before operations; do not incrementally loosen v2 |
| Request lifecycle | One 30-second read lifecycle with waiter-owned pending cleanup | Add operation ownership, dispatch stage, late-result handling, and method budgets |
| Artifacts | Incoming `artifacts` is parsed as values and any nonempty list is rejected | Add typed owned artifacts in v3 before visual work |
| Capabilities | Explicit effective booleans and support booleans; no desired/granted/supported breakdown | Introduce richer internal capability state and derive effective state |
| References | Window/group/tab references live in snapshot-specific runtime structures | Move all public reference kinds and lookup semantics into `references.rs` |
| Retention | Snapshot-local FIFO, fixed TTL, byte accounting, and cursor maps | Generalize aggregate quota, time, eviction, and typed-store APIs |
| Browser model | Synchronous Chrome listener registration followed by event-triggered authoritative rereads | Preserve this model and add navigation listeners with the same startup rule |
| Extension testability | `createBrowserModel(chromeApi, options)` injects Chrome, clock, UUID, and event drain | Establish the same seam for the background controller and each new coordinator |
| Tests | Process tests duplicate broker framing/spawn helpers; extension tests cover model and protocol | Create shared test support before multiplying operation and page suites |
| Permissions | Only current core permissions; no optional Page permissions | Do not change the manifest before P2.0 exits |

## Part 1 implementation learnings

These are constraints learned from implementation, not topics to redesign.

- A process-wide `BrokerRuntime` outside individual MCP sessions is the correct
  ownership model. Session handlers remain lightweight views over shared state.
- `Parameters<Value>` plus custom strict decoding successfully returns canonical
  structured `INVALID_ARGUMENT` errors instead of framework-specific text.
- Extension output is untrusted typed input. Rust must deserialize, validate
  identities, relationships, bounds, and capabilities before retaining or
  publishing it.
- Clone, validate, then commit prevents a malformed baseline from partially
  changing references, revisions, or retained state.
- Serialize and size-check an outbound Native Messaging request before inserting
  pending state or queueing the writer.
- A compact text summary plus canonical structured content avoids duplicating
  large JSON in the model context.
- Chrome listeners must register synchronously during service-worker module
  evaluation. Events trigger authoritative rereads when event payloads cannot
  safely prove complete state.
- Capability publication must complete before returning a result that depends on
  newly observed support, or the broker can reject an otherwise valid result.
- Dependency injection makes dependency-free extension behavior tests practical
  without making Node an extension runtime dependency.
- Service-worker, browser, and broker epochs invalidate different reference
  layers. New page identities must encode those boundaries explicitly.
- The current static tool router cannot produce live capability-filtered
  discovery or fan out tool-list changes.
- The current request lifecycle is read-only: cancellation removes the waiter,
  timeouts do not preserve operation state, and unmatched late responses are
  discarded.
- Protocol v2 deliberately rejects artifacts rather than validating or owning
  them.
- Reference and quota code is snapshot-specific and cannot safely be copied for
  plans and page snapshots.
- Integration tests have no shared support module, making race and two-session
  suites unnecessarily expensive to write and maintain.
- Part 1 still lacks several boundary, restart, and live-Chrome tests listed in
  its exit gate.
- A final-state reread cannot prove an A-to-B-to-A activation, navigation,
  viewport, or scroll race. Generation counters are required where attribution
  depends on no intervening change.

## Locked decisions

Part 2 must not reopen these accepted V1 decisions:

- The installation bearer is the single MCP trust boundary. Every authenticated
  session has the same globally enabled authority.
- Browser changes, Page tools, and Advanced evaluation are three global controls
  in extension UI, each disabled by default on fresh install and upgrade.
- Public references and internal incarnations are exact and opaque. Retained
  records use FIFO eviction and fixed non-refreshing TTLs.
- `browser.change` has preview and apply modes. Apply is non-atomic and reports
  per-operation partial or unknown outcomes when certainty is unavailable.
- Page tools use one fixed isolated DOM-derived agent. There are no fallback
  locators, raw coordinates, trusted input claims, uploads, native-dialog
  control, or arbitrary DOM/Chrome forwarding.
- Visual inspection captures only the active viewport of a tab that is already
  active in its window. It never activates, focuses, scrolls, wakes, or reloads
  implicitly.
- Arbitrary evaluation uses isolated `userScripts` execution on Chrome 135 or
  later. There is no `debugger` or main-world evaluation path.
- Legacy `browser.list` and `tabs.list` remain behaviorally frozen until the
  documented removal gate passes.

## Protocol v3 recommendation

Implement Part 2 as one coordinated Native Messaging protocol v3 upgrade. A new
ADR must be accepted in P2.0 before implementation because ADR 0005 fixed v2 for
the read foundation and v3 changes operation ownership and capability semantics.
The new ADR should supersede only the internal protocol details that v3 replaces;
it must preserve ADRs 0005 and 0006's identity, mutation, authorization, page,
and security decisions.

The ADR must explicitly supersede ADR 0005's prospective claim that v2 carries
operation classes/artifacts and ADR 0006's prospective v2 dynamic-discovery
representation. It must not rewrite either historical ADR.

Incrementally overloading v2 is not recommended. Version 2 strictly accepts only
`requestClass: "read"`, has one read deadline, exposes only effective capability
booleans, rejects nonempty artifacts, and has no representation for whether a
side effect reached dispatch. Those are intentional validation boundaries, not
extension points.

Protocol v3 must provide:

- Typed request classes such as `read`, `browserOperation`, `pageAction`, and
  `evaluation`, with an allowlisted method-to-class mapping on both sides.
- Method-specific deadline budgets, including queue, extension dispatch,
  observation, and broker margin rules.
- Explicit dispatch stage and uncertainty semantics sufficient to distinguish
  rejected-before-dispatch errors from normal partial or unknown operation
  results.
- Typed owned artifacts, initially one validated PNG image artifact, separated
  from structured result data.
- Capability state that distinguishes implementation support, desired setting,
  permission grant, API/version/probe support, effective enabled state, and one
  safe reason.
- An exact implementation manifest for methods and union branches, including
  legacy dispatch, with a schema/ABI revision for each branch. Discovery and
  dispatch use the broker/extension intersection so two v3 builds from different
  milestones never advertise or send a branch the peer cannot decode.
- A monotonic capability revision carrying a complete capability snapshot.
- Visible mixed-build failure for old broker/new extension and new broker/old
  extension, with no silent downgrade or negotiation.
- The existing `ready`, `ready_ack`, `request`, and `response` message names and
  request-ID correlation model.
- The existing browser identity checks, 1 MiB host-to-Chrome hard limit, 64 MiB
  Chrome-to-host hard limit, product safety margins, bounded writer queue, and
  Chrome-owned process lifetime.
- Privacy-safe protocol errors that never expose URLs, titles, page content,
  source, arguments, results, internal chains, or Chrome-generated detail.

An operation response should express its dispatch state in a typed result or
typed response metadata, not only in an error string. The v3 ADR and schema
goldens must settle one representation before P2.1.

## Dynamic discovery policy

Use the following policy unless the P2.0 framework spike proves that `rmcp`
cannot safely implement it:

- Before the extension `ready` message is accepted, `tools/list` returns no
  browser tools. HTTP can be listening, but an unready broker must not advertise
  capabilities it has not negotiated.
- After `ready`, each `tools/list` call reads one process-wide capability and
  implementation snapshot and returns the live filtered list.
- `browser.snapshot` is core whenever the matched extension implements it.
- `browser.change` preview is core once preview is implemented. Apply remains
  guarded by effective Browser changes capability.
- Page tools are hidden unless their required capability is effective and the
  connected build implements the advertised branch.
- `page.evaluate` is hidden unless Page tools and Advanced evaluation are both
  effective and the version/API/settings probes pass.
- Legacy tools remain visible during migration regardless of new optional
  toggles, but their behavior and schemas remain frozen.
- Every direct call repeats implementation, capability, identity, and permission
  checks. Discovery is never authorization.
- Capability changes update one broker-wide snapshot before list-change
  notifications are fanned out to every initialized eligible session.
- Clients that do not consume tool-list changes must reconnect after a toggle or
  permission change.

If the framework cannot represent an empty pre-ready list without misleading
cached state, P2.0 must record a highlighted decision gate in the v3 ADR. The
fallback may delay MCP service readiness until `ready`; it must not advertise
speculative page or mutation tools.

## MCP annotations

`browser.change` must use conservative tool annotations because one tool contains
an apply mode:

- `readOnlyHint: false`
- `destructiveHint: true`
- `idempotentHint: false`
- `openWorldHint: false`

Preview remains guaranteed read-only by its strict `mode: "preview"` branch and
tests, but tool-level annotations cannot vary by call arguments. Descriptions and
client documentation should state that preview performs no mutation while apply
may be destructive.

`page.inspect` remains read-only and non-destructive. `page.act` and
`page.evaluate` are conservatively mutating and non-idempotent. Evaluation is
open-world in effect because page code can perform network and storage effects,
even though the native/Chrome API surface is closed.

## Cross-cutting implementation design

### Generalized references

Create `native/src/references.rs` before browser-change or page work.

- Define typed newtypes for `windowRef`, `groupRef`, `tabRef`, `documentRef`,
  `browserSnapshotRef`, `pageSnapshotRef`, `elementRef`, and `planRef`.
- Keep public values random, process-local, type-prefixed, and non-parseable as
  authority.
- Store internal incarnation/document/node identity separately from public text.
- Distinguish live object lookup, retained handle lookup, closed-object
  tombstones where needed, and expired/evicted handle lookup.
- Return `NOT_FOUND` for a known closed live object and `HANDLE_EXPIRED` for a
  no-longer-retained snapshot, cursor, plan, page, or element handle.
- Make browser snapshots retrievable by `BrowserSnapshotRef`, not only by cursor.
- Preserve a tab reference across navigation and invalidate document/element
  references on every document replacement.
- Clear every public mapping on broker restart or browser identity change.

### Aggregate quotas and time

Move retention policy out of `browser_snapshot.rs` into central APIs owned by
`BrokerRuntime`.

- Inject a monotonic clock for deterministic TTL tests.
- Account for references, baselines, cursor indexes, plans, results, page
  records, and artifact metadata under one aggregate broker-memory budget.
- Enforce per-kind count and byte limits plus one aggregate retained-state cap.
- Use FIFO by creation sequence and fixed creation-time TTLs; lookup never
  refreshes time or queue position.
- Define central reserve, commit, release, expire, and evict operations.
- Clone or stage state, validate and measure it, reserve quota, then commit
  atomically.
- Never hold a runtime mutex across `.await`, Native Messaging send, Chrome
  response wait, image conversion, or MCP notification fan-out.
- Make broker eviction dependency-aware so a page snapshot cannot leave usable
  broker element references and a browser snapshot cannot leave dependent
  cursors. Self-contained plans remain independent after preview commit.
- Treat renderer page-agent node maps as a separate bounded resource. Enforce
  per-document entry, estimated-byte, and fixed-lifetime limits in the agent;
  broker correctness must tolerate earlier agent eviction as `STALE_ELEMENT`.
- Send best-effort release/epoch-reset messages when broker page state is evicted,
  but never rely on their delivery for correctness or memory safety.

### Self-contained plans

A previewed browser-change plan must remain executable without the originating
browser snapshot record after successful preview construction.

- Resolve all public references through the retained snapshot at preview time.
- Copy exact typed operations, target and destination incarnation keys, narrow
  preconditions, order, `stopOnError`, warnings, browser identity, and expiry
  into the plan.
- Do not retain titles, URLs, favicon data, or unrelated snapshot fields in a
  plan unless a specific precondition requires them.
- Do not make plan validity depend on the browser snapshot surviving eviction.
- Mark a plan applying atomically before dispatch and store one eventual result.
- Treat a completed retained plan as process-locally deduplicated; return the
  retained result instead of dispatching again.

### Broker-owned side-effect tasks

HTTP request futures must not own dispatched side effects.

- Before dispatch, cancellation may remove a waiter and cancel queued work.
- At dispatch commitment, move execution into a broker-owned task keyed by plan
  or operation ID.
- HTTP cancellation removes only that waiter; it does not reset a plan, cancel an
  extension operation, or permit a duplicate dispatch.
- Concurrent apply calls for one plan join the same task/result.
- Preserve late extension results long enough to settle the plan even when all
  original HTTP waiters are gone.
- On broker shutdown, mark any dispatched unresolved operation unknown in memory
  before state is dropped; clients still need a fresh snapshot after reconnect.
- Browser and page operation lanes use explicit bounded concurrency rather than
  the read semaphore alone.

### Capability controller

Represent each global capability with separate internal facts:

| Fact | Meaning |
| --- | --- |
| `implemented` | This matched extension and broker build implement the branch |
| `desired` | The persisted user setting requests the capability |
| `granted` | Chrome currently reports all required optional permissions/hosts |
| `supported` | Chrome version and API shape can support the branch |
| `probePassed` | Required live setting/API probe currently succeeds |
| `effective` | All required facts and parent capabilities are true |
| `reason` | One bounded privacy-safe explanation when ineffective |

Browser snapshot support flags such as frozen tabs are feature support facts,
not user-controlled capabilities. Advanced evaluation depends on effective Page
tools as well as its own setting, permission, version, and probe.

The extension is authoritative for desired, granted, supported, probe, and
effective state at dispatch. The broker mirrors the complete state for discovery
and early failure. The popup reads the same controller rather than reconstructing
permission logic independently.

### Background-controller seam

Refactor extension orchestration behind `createBackgroundController(chromeApi,
dependencies)` before adding toggles or page dispatch.

- Inject storage, clock/timers, UUID, browser model, native-port connector,
  capability controller, operation coordinators, and response-byte measurer.
- Register runtime, storage, permission, browser, and navigation listeners
  synchronously when the controller is created.
- Keep asynchronous baseline, storage, permission, and probe work behind
  explicit readiness promises.
- Queue capability publication so a dependent result never overtakes the state
  that authorizes or describes it.
- Invalidate connection-scoped page and operation state when the native port or
  broker epoch changes.
- Test reconnect and startup races without importing a live `chrome` global.

### Error and dispatch precedence

Use one documented precedence so races produce deterministic public behavior:

1. Decode and schema validation.
2. Connected browser and matched protocol/build check.
3. Implemented branch check.
4. Global desired setting check.
5. Permission, version, API, and probe support check.
6. Reference type, retention, browser epoch, and object existence check.
7. Document/page epoch and stale-element check.
8. Operation-specific preconditions and actionability check.
9. Queue admission and pre-dispatch deadline check.
10. Dispatch commitment.
11. Read-back, stabilization, inspection, or postcondition verification.

Failures through step 9 are MCP errors with no side effect. After step 10,
side-effecting calls return normal applied/partial/unknown result shapes whenever
completion cannot be proven. A page action that succeeds but whose
`inspectAfter` fails preserves action success and adds `inspectionError`.

Public errors use the contract codes and bounded fixed messages. Chrome runtime
messages and exception text are diagnostic inputs only and must not cross MCP,
logs, popup output, or test reports when they could contain browsing data.

### Artifact ownership

- The extension creates at most one encoded image artifact for one response.
- Protocol code validates artifact count, type, MIME, encoded bytes, and envelope
  size before posting the response.
- Rust decodes or transfers the artifact into exactly one owned MCP image content
  block.
- Base64 image bytes occur once in the Native Messaging envelope and once in the
  required MCP transport representation, never in structured JSON or text.
- Broker retained state stores only visual metadata and generations. Image bytes
  are not retained after response construction.
- Errors, traces, panic output, screenshots, reports, and golden files never
  contain real image bytes.

## State ownership and retention graph

### Target ownership

| State | Owner | Persistence | Cleared by |
| --- | --- | --- | --- |
| Desired capability settings | Extension capability controller | `chrome.storage.local` | User setting change or extension data removal |
| Permission grants and API probes | Chrome plus extension controller | Reconciled live | Permission/version/setting change or worker restart |
| Effective capability snapshot | Extension authoritative; broker mirrored | Memory only | New revision, disconnect, broker restart |
| Browser numeric IDs and incarnations | Extension browser model | Service-worker memory | Removal/replacement or worker restart |
| Public browser references | Broker reference registry | Memory only | Object removal, browser identity change, broker restart |
| Browser snapshots and cursors | Broker retention manager | Memory only | Fixed TTL, FIFO eviction, identity change, restart |
| Browser-change plans/results | Broker plan store and operation task registry | Memory only | Fixed TTL, FIFO eviction, restart |
| Document/frame generations | Extension page coordinator | Service-worker memory | Navigation, frame removal, worker restart |
| Page-agent node tokens | Exact injected document agent under separate per-document quota | Document memory | Agent TTL/FIFO eviction, node removal, navigation, page-agent reset, or best-effort broker release |
| Public document/page/element refs | Broker reference and page stores | Memory only | Navigation, fixed TTL, FIFO eviction, restart |
| Capture generations | Extension model/page agent | Service-worker/document memory | Relevant event, navigation, restart |
| Image bytes | Extension response then broker response owner | Transient only | Response completion or failure |
| Evaluation source/args/result | Request/response task only | Transient only | Completion, timeout observation, disconnect |

### Retention dependency graph

```text
connected browser epoch
  -> live window/group/tab references
  -> retained browser snapshot
       -> browser cursor records
       -> preview construction only
            -> self-contained plan -> retained apply result

connected browser epoch
  -> service-worker/page epoch
       -> tab incarnation -> document/frame incarnation
            -> retained page snapshot -> page cursor records
                 -> element references -> page-agent node tokens
            -> visual metadata (no retained image bytes)
```

Evicting a browser snapshot removes its cursors but not a self-contained plan
already committed from it. Navigating a document invalidates its page snapshots,
cursors, and elements together. Agent-side node eviction may make an element
stale before its broker page snapshot expires; the broker must fail closed and
never retarget. Losing the service worker or native connection invalidates all
broker-visible page identities even if a Chrome tab numeric ID still exists.

## Unresolved decision register

Every item has an owner milestone. No later milestone may silently choose a
public behavior that belongs to an earlier owner.

| Decision | Owner | Required output |
| --- | --- | --- |
| Exact request, result, image, aggregate retention, count, concurrency, and timeout limits | P2.0 | v3 ADR table plus measured boundary fixtures |
| Relevant-precondition matrix for the six simple operations | P2.3 | Contract appendix and table-driven preview tests |
| Relevant-precondition matrix for destructive/composite operations | P2.5 | Extended appendix and table-driven preview/apply tests |
| Operation composition, duplicate targets, overlap dependencies, index semantics, and no-op policy | P2.3 | Normalization rules and deterministic examples |
| Final-tab and final-window close behavior, including Chrome platform differences | P2.5 | Explicit contract and live throwaway-profile evidence |
| Semantic result schemas for records, forms, landmarks, frames, and full detail | P2.0/P2.6 | Golden JSON Schemas before advertisement |
| Exact `scope="auto"` selection and pagination policy | P2.6 | Bounded algorithm and token measurements |
| Main/all-frame ordering, inaccessible-frame result, per-frame failure, and loading-document behavior | P2.6 | Frame policy and synthetic fixtures |
| DOM-derived role/name approximation and open-shadow treatment | P2.6 | Documented algorithm and conformance fixtures |
| Actionability rules for visibility, obstruction, disabled, readonly, and detached state | P2.8 | Agent checks and action matrix |
| Exact event sequence for click, fill, select, and check | P2.8 | Browser-observed event fixtures |
| Visual CSS viewport dimensions versus encoded image pixel dimensions and field names | P2.7 | Schema decision and zoom/device-scale tests |
| Capture generation sources, A-B-A guarantees, and unavoidable race wording | P2.7 | Algorithm proof boundary and live evidence |
| Capture serialization, Chrome rate-limit policy, and retry guidance | P2.7 | Rate/concurrency limits and public error behavior |
| Implementation proof for the Part 3-frozen navigation readiness, 250 ms DOM quiet interval, and `wait="none"` identity semantics | P2.8 | Timing state machine and fake-clock tests |
| Action success plus inspection success/failure/unknown output unions | P2.8 | Golden schemas and target-client validation |
| Evaluation async wrapper construction and safe argument transfer | P2.9 | Wrapper specification and hostile-string tests |
| Evaluation serialization policy for undefined, non-finite numbers, cycles, BigInt, DOM values, and oversized values | P2.9 | Exact accepted value contract |
| Evaluation exception/rejection mapping and privacy-safe message policy | P2.9 | Error table and fixtures |
| Navigation during evaluation, timeout observation, late completion, and per-document concurrency | P2.9 | Execution state machine and delayed-effect tests |
| Named target MCP clients for structured output, list changes, image blocks, and compact fallback | P2.0 | Compatibility matrix with versions |
| Deprecation duration, release count, telemetry-free evidence, and pre-1.0 breaking policy | P2.10 | Written release policy |
| Minimum Chrome version for Page tools independent of evaluation's Chrome 135 minimum | P2.6 | Manifest/product decision with platform evidence |

## P2.0: Part 1 closure and contract freeze

### Objective

Close known Part 1 gaps, freeze v3 transport/capability rules and cross-cutting
budgets, prove risky framework paths, and accept the protocol v3 ADR before
permission or behavior changes. Public branch schemas freeze in the milestone
that first implements and advertises each branch.

### Native work

- Create `tests/support/mod.rs` with broker spawning, isolated token/address,
  Native Messaging framing, ready/capability builders, MCP client creation,
  deadlines, child cleanup, and delayed/malformed response helpers.
- Add snapshot lookup tests for `browserSnapshotRef`, or explicitly record that
  lookup arrives in P2.1 before preview work.
- Complete snapshot edge tests for all omitted/true/false filters, scopes,
  duplicate refs, empty results, group/window page splits, cursor tampering,
  expiry, FIFO eviction, one-record-too-large, and exact byte boundaries.
- Generate deterministic checked-in schema and representative-result goldens for
  current tools and every first advertised Part 2 branch.
- Spike dynamic discovery and tool-list-change fan-out to two initialized
  sessions, including pre-ready behavior and a client that does not refresh.
- Spike broker-owned plan concurrency: two applies, waiter cancellation before
  and after dispatch, one late response, and exactly one dispatch.
- Spike a synthetic PNG through Native Messaging and MCP with exact byte
  accounting and no duplicated base64.
- Spike capability state ordering across ready, storage, permission, probe,
  result, and two-session discovery changes.
- Spike compact text fallback and canonical structured content in each named
  target client.
- Draft and accept the v3 ADR with envelope schemas, request classes, deadlines,
  dispatch semantics, artifacts, capability state, and mismatch behavior.

### Extension work

- Extract `createBackgroundController(...)` as a behavior-preserving production
  seam and add dependency-injected ready/reconnect tests against that code, not a
  test-only duplicate.
- Complete browser-model tests for remove/reuse, create/remove races, focus
  ordering, grouped contiguity, support publication ordering, failed rereads,
  worker restart, and clone/validate behavior.
- Build protocol/capability/image spikes in test-only modules or branches; do not
  request optional permissions or expose new methods.
- Confirm current final-state reconciliation cannot prove A-B-A and define the
  generation events required by later milestones.

### Tests

- Run the full Rust gate and dependency-free extension tests.
- Live-validate Part 1 snapshot ordering, immutable pagination, stale refs,
  unsupported fields, no activation/focus/wake, Chrome restart, worker restart,
  broker restart, and MCP reconnect.
- Test mixed protocol builds in both directions with clear stderr/popup failure
  and no request processing.
- Record target-client evidence for empty/live discovery, structured errors,
  compact fallback, list changes, and image rendering.

### Documentation

- Add the protocol v3 ADR; do not rewrite ADR 0005 history.
- Update `docs/mcp-tools.md` only for contract decisions that are actually
  frozen, while retaining proposed status for unimplemented branches.
- Update architecture and failure-mode docs for agreed runtime ownership and
  uncertainty semantics.
- Update `docs/progress.md` with Part 1 live-validation evidence only after it is
  performed.

### Exit gate

- The v3 ADR is accepted and transport, capability, implementation-manifest, and
  first-advertised-branch schemas have deterministic goldens.
- Exact initial limits, request classes, method budgets, and concurrency caps are
  recorded.
- Dynamic discovery, plan concurrency, image transport, capability ordering, and
  compact fallback spikes pass for named target clients.
- Part 1 boundary tests and required live snapshot/restart evidence are complete.
- No Chrome manifest permission has changed.

## P2.1: Operation-capable protocol and runtime

### Objective

Land matched protocol v3 and general runtime infrastructure with no new public
tool behavior or Chrome permissions.

### Native work

- Upgrade `protocol.rs`, broker framing, and process tests to strict v3.
- Validate one exact build ABI plus a complete method/branch support manifest in
  `ready`; retain the peer intersection in `BrokerRuntime`.
- Add typed request classes and an allowlisted method/class/deadline table.
- Replace the waiter-only pending map with lifecycle records that track queued,
  dispatched, responded, timed out, and abandoned-waiter states.
- Preserve late operation responses for broker-owned tasks; continue dropping
  safe unmatched read responses after validation.
- Parse typed artifact envelopes into owned bounded values but reject artifacts
  for methods that do not permit them.
- Add `references.rs` with all typed public reference kinds and exact lookup
  errors.
- Add central quota, injected clock, fixed TTL, FIFO eviction, and dependency
  cleanup APIs to `runtime.rs` or a focused `retention.rs` module.
- Move browser snapshot reference/cursor/quota behavior onto the generalized APIs
  without changing its public result.
- Add browser-snapshot lookup by `BrowserSnapshotRef` for preview.
- Add broker-owned operation task infrastructure and overlap-lane primitives.
- Add dynamic-registry data structures and per-session notification fan-out,
  while retaining the same three visible tools until P2.2.
- Audit all runtime lock scopes and assert no `.await` occurs while locked.

### Extension work

- Upgrade strict request and response validation to v3.
- Publish the exact implemented method/branch manifest, including legacy
  dispatch, and reject requests outside the negotiated intersection.
- Validate method-to-class and method-to-deadline bounds before dispatch.
- Return typed dispatch stage/outcome metadata for operation-capable mock methods.
- Implement typed artifact construction and size checks without exposing capture.
- Extend the P2.0 background-controller seam with v3 dispatch and make the
  connection epoch explicit.
- Preserve current browser reads and capability publication ordering.

### Tests

- Matched v3 handshake and both mixed-v2/v3 failure directions.
- Same-v3 builds with different branch manifests advertise only their safe
  intersection; incompatible branch schema/ABI revisions fail visibly.
- Unknown request class, wrong method/class pair, bad deadline, malformed stage,
  malformed artifact, wrong MIME, duplicate artifact, and size boundaries.
- Cancellation before queue, in queue, before dispatch, after dispatch, after
  response, and with a late response.
- One operation dispatch with two MCP waiters and one cancelled waiter.
- Fixed TTL, FIFO, aggregate quota, typed-ref mismatch, dependency eviction, and
  fake-clock tests.
- Snapshot regression goldens and all existing tests through generalized stores.
- Queue and operation-lane saturation remain bounded.

### Documentation

- Update process/transport architecture, failure modes, protocol references,
  progress, and troubleshooting for strict v3 mismatch.
- Keep `docs/mcp-tools.md` availability unchanged.

### Exit gate

- Existing three tools retain behavior on matched v3 builds.
- No side-effecting method is publicly reachable.
- Late responses cannot cause duplicate dispatch or corrupt another request.
- Artifacts are typed and owned but no tool emits one.
- Runtime locks are not held across async work.

## P2.2: Capability controller and dynamic discovery

### Objective

Implement the complete capability state model, live discovery, and the Browser
changes control foundation before exposing browser-change behavior. P2.2 need
not ship independently; if it does, the control remains visibly unavailable
until apply support lands.

### Native work

- Replace effective-only booleans with validated per-capability state from v3.
- Derive one immutable discovery snapshot from connected build support and
  effective capabilities.
- Treat capability effectiveness and method support separately. An effective
  Page capability must not require every future Page method to be implemented;
  discovery intersects each advertised method/branch independently so semantic
  inspect can ship before actions.
- Return no browser tools before ready and a live filtered list after ready.
- Advertise server tool-list-change support and fan out one coalesced notification
  per capability revision to all initialized sessions.
- Recheck capability and implementation state at every direct call.
- Ensure stale or decreasing revisions do not roll back discovery.
- Add safe reason-to-error mapping without exposing extension internals.

### Extension work

- Add `capabilities.js` with injected storage, permissions, runtime version/API
  probes, parent dependency evaluation, and complete snapshots.
- Persist all three desired settings as disabled when absent, including upgrades.
- Add the Browser changes control to popup UI first; do not add Page controls yet.
- Derive apply as ineffective while the matched build lacks apply support. Do not
  let a visible control imply that mutation behavior already exists.
- Keep preview support separate from effective apply permission.
- Observe storage and permission changes synchronously and publish after
  authoritative reconciliation.
- Recheck Browser changes immediately before every future mutation dispatch.
- Expose bounded safe reasons in popup and protocol state.

### Tests

- Fresh install, upgrade with missing keys, unavailable-build state, enable,
  disable, storage race, worker restart, broker reconnect, and revision
  monotonicity.
- Two MCP sessions receive list changes and observe the same tool set.
- One client without list-change handling sees the new list after reconnect.
- Pre-ready list is empty; post-ready core list is exact.
- Stale direct calls fail closed after disable or permission/support change.
- Popup uses controller state and cannot enable capability through MCP.

### Documentation

- Document the three-state controls while clearly marking Page controls as not
  yet present.
- Explain dynamic rediscovery and reconnect behavior in README and
  troubleshooting when it ships.
- Update privacy/security language for bearer-wide Browser changes authority.

### Exit gate

- Browser changes defaults disabled for fresh and upgraded installations.
- Discovery changes are visible to two sessions and direct calls fail closed.
- No Page permission or `userScripts` permission is requested.

## P2.3: `browser.change` preview only

### Objective

Complete a guaranteed read-only preview branch backed by retained immutable
browser snapshots and self-contained exact plans. This is an internal milestone
and is released with P2.4 by default so `planRef` is not dead output across an
upgrade/restart. A separate preview-only release requires a documented workflow
that justifies non-applicable expiring plans.

### Native work

- Add `browser_change.rs` with strict `Parameters<Value>` decoding, tagged mode
  union, operation schemas, output schemas, summaries, and warnings.
- Advertise preview for only the six P2.4 operation types: `tab.move`,
  `tab.update`, `tab.activate`, `group.create`, `group.update`, and
  `window.update`. It is acceptable to stage internal Rust types for the final
  operation set, but no later branch may appear in discovery or accepted input.
- Resolve `browserSnapshotRef` through the generalized store.
- Reject counts results, expired snapshots, wrong-browser snapshots, duplicate
  ambiguous targets, impossible compositions, invalid URLs/indexes, and
  unsupported properties.
- Compute narrow operation-specific preconditions from the snapshot.
- Create a self-contained plan record with exact operations, internal
  incarnations, preconditions, order, `stopOnError`, warning set, expiry, and
  browser identity.
- Commit the plan only after schema validation, aggregate measurement, quota
  reservation, and complete plan validation succeed.
- Return only `planRef`, concise summary, warnings when present, and expiry.
- Mark the tool conservatively mutating/destructive even though preview itself
  is guaranteed read-only.

### Extension work

- No mutation executor is called by preview.
- Expose only support metadata needed to validate operation availability.
- Prepare typed operation validators shared by later apply code, but keep them
  unreachable from browser dispatch.

### Tests

- Every advertised operation branch and every field/cross-field boundary.
- Preview issues no mutating Chrome API call, even with Browser changes enabled.
- Exact reference type, snapshot lookup, expiry, eviction, identity, and
  incarnation behavior.
- Relevant precondition extraction excludes unrelated title/URL/state.
- Duplicate targets, target overlap, destination overlap, operation ordering,
  index shifts, no-ops, and unsupported composition according to frozen policy.
- Plan remains valid after source snapshot eviction and expires on its own TTL.
- Quota failure commits neither plan nor partial references.
- Summary/warnings are stable, concise, and do not echo operations.

### Documentation

- Keep preview proposed in `docs/mcp-tools.md` until it ships with P2.4, unless a
  separately justified preview-only release is approved.
- Document exact accepted operation branches and frozen composition policy.
- Update progress and examples without implying apply exists.

### Exit gate

- Preview is useful while Browser changes is disabled and cannot mutate Chrome.
- Every accepted operation is represented completely in a self-contained plan.
- No unimplemented branch is advertised or accepted.

## P2.4: Simple `browser.change` apply

### Objective

Add bounded opt-in apply for `tab.move`, `tab.update`, `tab.activate`,
`group.create`, `group.update`, and `window.update`.

### Native work

- Add strict apply decoding accepting only a retained `planRef`.
- Publicly advertise the completed P2.3 preview and P2.4 apply branches together.
- Atomically transition ready plans to applying and join concurrent callers.
- Schedule disjoint plans concurrently within a fixed cap and serialize plans
  whose target/destination lanes overlap.
- Treat duplicate apply after completion as a retained-result read.
- Continue the broker-owned task after HTTP cancellation.
- Map pre-dispatch failures to MCP errors and post-dispatch uncertainty to normal
  results.
- Validate typed extension outcomes as untrusted input before plan commit.
- Obtain a fresh authoritative baseline after apply when certainty permits,
  retain it through normal snapshot APIs, and return its reference.
- Require a fresh snapshot after partial or unknown outcomes.

### Extension work

- Add `browser-change.js` with injected Chrome APIs and browser model.
- Dedupe one plan execution by broker operation ID within the connection epoch.
- Recheck effective capability, browser identity, incarnations, and narrow
  preconditions immediately before each operation.
- Implement only the six simple operation types.
- Reconcile authoritative state after each operation or composite step.
- Return ordered applied/failed/skipped/partial/unknown outcomes and only new
  references or recovery facts.
- Stop on first error when requested; otherwise continue only independent lanes.
- Publish capability changes before a result that depends on observed support or
  revocation.

### Tests

- Each operation's success, no-op, invalid state, stale relevant state, and
  unrelated concurrent browser change.
- Duplicate targets and normalized caller order.
- Overlapping plans serialize; disjoint plans respect the concurrency cap.
- Two sessions apply one plan and produce one Chrome dispatch.
- Disable/revoke before queue, before operation, between operations, after
  dispatch, and during read-back.
- Cancellation and timeout at every dispatch stage, including a late success.
- Fresh snapshot on applied/known partial; no misleading snapshot on unknown.
- Live tests in a marked throwaway profile for movement, activation, group, and
  window updates.

### Documentation

- Mark only the six apply operation branches implemented.
- Document non-atomic behavior, deduplication scope, recovery, and Browser changes
  enablement.
- Update privacy/security and troubleshooting for opt-in mutations.

### Exit gate

- Browser changes remains disabled by default and cannot be enabled through MCP.
- One plan dispatch occurs across retries, cancellation, and concurrent sessions.
- Known, partial, and unknown outcomes match actual dispatch certainty.

## P2.5: Destructive and composite `browser.change` apply

### Objective

Complete tab open/close/duplicate/discard/reload, grouping/ungrouping,
`group.move`, and window create/close with explicit final-window policy.

Part 3 freezes `destructive:true` for previews containing `tab.close`,
`tab.discard`, or `window.close`. The field is omitted otherwise; summaries and
deduplicated warning order are deterministic. Apply remains plan-only and rejects
`confirmDestructive`.

### Native work

- Add the remaining operation schema branches only as each is implemented.
- Add preview and apply support for each remaining branch in the same milestone;
  never issue a plan that the matched build cannot apply.
- Add destructive warnings for close, discard, reload, navigation by open, and
  window closure as applicable.
- Return the frozen compact destructive flag without echoing operations or
  browser metadata.
- Model composite operations and known created references without implying
  rollback.
- Encode dependencies so `stopOnError=false` skips dependent operations and may
  continue only independent later operations.
- Preserve exact operation order and frozen index semantics.
- Return fresh snapshot recovery for every established final state.

### Extension work

- Implement `tab.open`, `tab.close`, `tab.duplicate`, `tab.discard`, and
  `tab.reload` with exact URL/active-state checks.
- Implement group/ungroup through `tab.move`, `group.move`, `window.create`, and
  `window.close` using documented Chrome APIs only.
- Detect multi-step partial outcomes and report known created incarnations.
- Apply the frozen final-tab/final-window policy consistently across platforms.
- Never attempt generic rollback or silently recreate closed state.
- Reconcile all affected objects after each composite operation.

### Tests

- Every remaining operation, invalid combination, unsupported URL scheme,
  active-tab discard, and stale target/destination.
- Grouping/ungrouping caller order, cross-window behavior, group contiguity, and
  index edge cases.
- Window creation with and without moved tabs; close final tab/window policy.
- Composite failure after first Chrome call, known created refs, partial, and
  unknown read-back.
- Dragged-tab/transient Chrome failures and user changes during apply.
- No rollback claims and no automatic retries after uncertain dispatch.
- Destructive live suite runs only against an identifiable throwaway Chrome
  profile/window with cleanup that never touches ordinary browsing state.

### Documentation

- Mark the complete browser operation set implemented.
- Publish final-window behavior and platform caveats.
- Update live runbooks with destructive-profile safeguards.

### Exit gate

- All documented browser operations are implemented without placeholder branches.
- Final-tab/final-window behavior is deterministic or explicitly unavailable on
  a validated platform.
- Destructive and composite paths pass throwaway-profile live tests.

## P2.6: Page capability and semantic `page.inspect`

### Objective

Introduce optional Page permissions, exact document identity, the fixed packaged
agent, and semantic-only `page.inspect` branches.

### Native work

- Add `page.rs` or focused page modules for strict inspect schemas, document/page/
  element references, semantic outputs, cursors, and typed errors.
- Add page snapshot and cursor stores to aggregate retention with fixed TTL and
  navigation invalidation.
- Validate extension semantic output as typed untrusted data, including frame,
  document, node, geometry, text, option, and aggregate bounds.
- Replace internal node tokens with public exact `elementRef` values.
- Support only frozen semantic combinations for scope, detail, frames, element
  context, and cursor continuation.
- Hide `page.inspect` until Page tools is effective and semantic implementation
  support is present.
- Recheck effective capability and exact document identity on direct calls and
  cursor/element use.

### Extension work

- Add optional `scripting`, `webNavigation`, and `<all_urls>` to the manifest.
- Do not request or declare optional `userScripts` yet.
- Add Page tools UI and request permissions only from an extension-UI gesture.
- Extend the capability controller with desired/granted/supported/probe/effective
  Page state and removal/revocation handling.
- Add `page-tools.js` document/frame coordinator with synchronously registered
  navigation listeners.
- Reconcile already-loaded frames through `webNavigation.getAllFrames()` at
  startup, permission enablement, worker recovery, and target resolution.
- Issue new document/frame generations for same-URL reload and history/document
  replacement.
- Add a fixed packaged `page-agent.js` with injected dependencies for tests.
- Traverse ordinary DOM and open shadow roots; apply the frozen closed-shadow
  limitation.
- Produce bounded visible prose, headings, links, landmarks, forms, controls,
  values, options, exceptional states, and optional geometry.
- Omit password, file-input, hidden-control, and prohibited sensitive values.
- Assign exact random node tokens in bounded per-document maps.
- Reject restricted, incognito, discarded, frozen, stale, and unsupported
  targets without waking or activating.

### Tests

- Permission absent, denied, granted, revoked, removed, changed during call, and
  restored after worker restart.
- Minimum Chrome version and API-support probes.
- HTTP(S), restricted, incognito, discarded, frozen, loading, same-URL reload,
  history traversal, and navigation races.
- Main, same-origin, cross-origin, inaccessible, removed, and loading frames with
  deterministic ordering and frozen failure policy.
- Ordinary DOM, open/closed shadow roots, headings, links, forms, landmarks,
  hostile strings, text deduplication, and role/name fixtures.
- Password/file/hidden omission and ordinary value/option inclusion.
- Compact/full, viewport/document/auto, element context, pagination, cursor
  replay, stale element, replacement element, TTL, eviction, and byte bounds.
- No activation, focus, scroll, reload, wake, mutation, or source logging.
- Process tests verify discovery appears/disappears for two sessions and direct
  calls fail closed.

### Documentation

- Update manifest/permission setup, README, troubleshooting, architecture,
  privacy, security, progress, and `docs/mcp-tools.md` for implemented semantic
  branches only.
- State that output is DOM-derived and not Chrome's canonical accessibility tree.
- Record ADR 0006 as the approval rather than requesting another page-surface
  decision.

### Exit gate

- Page tools defaults disabled and can be enabled only in extension UI.
- Semantic inspection is bounded, exact-document, and non-disturbing.
- No visual, action, evaluation, or unimplemented semantic branch is advertised.

## P2.7: Visual `page.inspect` and image transport

### Objective

Add reliable already-active viewport capture after the P2.0 image spike has
passed and semantic document identity is established.

Part 3 freezes compact activation recovery: a `documentRef` caller receives the
otherwise unknown current `tabRef`; a `tabRef` caller receives no duplicate.
Recovery remains an explicit snapshot-preview-apply-retry workflow.

### Native work

- Add visual and both input/output branches only after schema and client gates
  pass.
- Add the typed `ACTIVATION_REQUIRED.recovery.tabRef` variant only for a
  `documentRef` caller and prove it creates no hidden snapshot or plan.
- Accept exactly one typed PNG artifact for visual/both methods.
- Validate metadata/artifact consistency, encoded/decoded bytes, dimensions,
  MIME, and method ownership.
- Construct one MCP image block and discard bytes after response completion.
- Retain visual page snapshot metadata and generations, not image bytes.
- Enforce capture concurrency and rate budgets separately from semantic reads.

### Extension work

- Add activation, navigation, viewport, zoom/scale, and scroll generation
  counters from synchronously registered listeners and page-agent observation.
- Resolve exact tab/document and confirm the tab is already active in its window.
- Record all relevant generations and CSS viewport/scroll metrics before capture.
- Call `chrome.tabs.captureVisibleTab(windowId, {format: "png"})` without
  activation or focus.
- Drain events, reread authoritative state and page metrics, and reject on any
  generation change, including A-B-A.
- Validate the data URL, PNG payload, bytes, pixel dimensions, and current
  capability before posting one artifact.
- For `both`, run viewport semantics and capture sequentially under one identity
  check; reject the complete result if attribution changes.
- Do not retry capture automatically across page state.

### Tests

- Active target in focused and unfocused windows and inactive target rejected
  before capture.
- Activation A-B, A-B-A, navigation, same-URL reload, frame replacement,
  viewport resize, zoom/scale, scroll, and scroll-away-and-back races.
- Capability revocation, tab close, worker restart, and connection epoch change
  during capture.
- Chrome rate limit, capture serialization, malformed data URL/PNG, duplicate
  artifact, MIME, dimensions, and exact byte boundaries.
- `both` semantic/image identity and one page snapshot reference.
- One image block only; no base64 in structured content, summary, logs, reports,
  retained state, or goldens.
- Deterministic red/blue live pages prove captured-tab attribution in focused and
  unfocused windows.

### Documentation

- Mark visual/both implemented only after live attribution gates pass.
- Document CSS versus pixel dimensions, sequential non-atomic collection, active
  precondition, rate limiting, and explicit `browser.change` recovery.
- Keep full-page, element crop, visual labels, non-active capture, and debugger
  paths deferred.

### Exit gate

- Generation-based checks support the documented attribution claim on the live
  platform matrix.
- Inactive targets fail without browser disturbance.
- Image bytes occur exactly once in each required transport and are not retained.

## P2.8: `page.act` and `inspectAfter`

### Objective

Perform exactly one structured action against one exact document and optionally
reuse the inspect pipelines for bounded post-action verification.

The frozen Part 3 corpus rejected bounded action sequences at G3.2. P2.8
therefore retains singular `action`; no action-cardinality ADR or sequence
prototype is required for V1.

### Native work

- Add strict action unions only for implemented click, fill, select, check,
  scroll, focus, navigate, back, forward, and reload branches.
- Validate one action per call, exact document/element type, timeout bounds, wait
  mode, and `inspectAfter` combinations.
- Add bounded per-document action lanes and method deadline budgeting.
- Treat dispatch commitment and uncertainty according to v3 lifecycle state.
- Reuse page inspect output types and artifact conversion; do not create another
  page representation.
- Preserve action success when `inspectAfter` fails and return the frozen compact
  `inspectionError` branch.
- Return at most one image and never echo action inputs.

### Extension work

- Resolve exact node tokens with no locator or coordinate fallback.
- Recheck document/frame, node connection, visibility, obstruction, enabled/
  readonly state, and action compatibility immediately before dispatch.
- Implement the frozen synthetic DOM event sequences for element actions.
- Implement service-worker navigation/history/reload actions after exact document
  precheck and URL validation.
- Register navigation and mutation observation before action dispatch.
- Implement frozen `wait="auto"`: authoritative replacement-document readiness
  after navigation, otherwise 250 milliseconds of target-document subtree DOM
  quiet, all within the aggregate deadline. Keep distinct `wait="none"`
  bookkeeping and make no application-stability claim.
- Serialize one action per document while allowing bounded independent documents.
- Preflight active viewport before any action whose `inspectAfter` requests
  visual output; preflight failure performs no action.
- Reuse semantic/visual/both pipelines against the resulting current document.
- Report unknown when dispatch may have occurred but completion cannot be proven.

### Tests

- Every action and every invalid target/action combination.
- Detached/replaced/similar node, hidden, disabled, obscured, readonly,
  incompatible, cross-frame, and open-shadow targets.
- Input, textarea, contenteditable, replace/append, select, checkbox, radio, and
  exact event ordering.
- Element and document scrolling without any hidden scroll in inspect.
- Navigate/back/forward/reload URL and document identity behavior.
- Navigation between resolution and dispatch and action-triggered navigation.
- Auto quiet, delayed navigation, continuous DOM churn, wait none, timeout,
  cancellation, late completion, and uncertain outcome.
- Settling-only timeout returns `waitError` after known action success and omits
  requested inspection; action uncertainty remains `status="unknown"`.
- Per-document serialization and independent-document concurrency limits.
- Click-and-inspect, scroll-and-capture, navigate-and-inspect, visual preflight
  before mutation, action success plus inspection failure, and one image.
- No implicit activation/wake, fallback locator, coordinates, upload, native
  dialog, browser UI, key/hover/drag, or trusted-input claim.

### Documentation

- Mark only tested action branches implemented.
- Document DOM interaction semantics, actionability, wait behavior, uncertainty,
  and inspect-after result unions.
- State that bearer-authorized clients may cause high-impact site effects and
  Effector does not infer business meaning.

### Exit gate

- Every call performs at most one exact action and never silently retargets.
- Default success is compact and does not echo inputs.
- `inspectAfter` reuses inspection without making `page.inspect` mutating.
- Unknown outcomes direct inspection before retry.

## P2.9: `page.evaluate`

### Objective

Add separately enabled, version-gated arbitrary code execution in an exact
isolated user-script document.

### Native work

- Add strict evaluation params and `{value}`/`{status:"unknown"}` outputs.
- Enforce source, argument, timeout, result, request, and transport byte limits
  before dispatch and after typed response decode.
- Hide the tool unless Page tools and Advanced evaluation are both effective.
- Use a separate bounded evaluation lane, including per-document policy.
- Treat timeout after dispatch as observation failure and retain no source,
  arguments, result, or exception detail.
- Never retry evaluation automatically.

### Extension work

- Add optional `userScripts` permission only in this milestone.
- Add Advanced evaluation popup UI with a separate explicit gesture and default
  disabled setting.
- Probe Chrome version, API availability, Developer mode behavior on 135-137,
  and Allow User Scripts behavior on 138+ at startup and dispatch.
- Resolve exact document ID and recheck page/evaluation capabilities immediately
  before execution.
- Construct the frozen async wrapper without interpolating argument values into
  source.
- Execute through `chrome.userScripts.execute()` in `USER_SCRIPT` world only.
- Validate exactly one expected execution result and apply the frozen JSON
  serialization policy.
- Map throws/rejections to privacy-safe typed errors without returning page
  exception strings.
- Return unknown for post-dispatch timeout/navigation uncertainty and allow late
  effects to finish without claiming cancellation.

### Tests

- Chrome 134 absent, Chrome 135-137 Developer mode, Chrome 138+ Allow User Scripts,
  missing API, denied permission, revoked setting, and stale worker probe.
- Exact main/child document targeting, restricted/discarded/frozen targets, and
  navigation before/during execution.
- Quotes, backslashes, template syntax, script-like strings, Unicode separators,
  large source, large arguments, and no source interpolation.
- Plain values, promises, undefined, non-finite values, cycles, BigInt, DOM
  values, thrown/rejected exceptions, and oversized results per frozen policy.
- Timeout followed by delayed DOM/storage/network-observable synthetic effect to
  prove non-cancellation and no automatic retry.
- Same-document and independent-document concurrency policy.
- No source, args, result, exception, page URL/content, or bearer in logs/reports.

### Documentation

- Document the separate stronger toggle, optional permission, Chrome version and
  user setting requirements, isolated world, timeout uncertainty, and arbitrary
  page/storage/network effects.
- Update privacy, security, README, troubleshooting, progress, and tool contract.
- Explicitly state that main-world and `scripting.executeScript()` runtime-source
  paths do not exist.

### Exit gate

- The tool is absent unless every parent and evaluation capability fact is
  effective.
- Evaluation cannot self-enable and runs only in isolated `USER_SCRIPT` world.
- Timeout and navigation never imply cancellation or safe blind retry.

## P2.10: Deprecation and legacy removal

### Objective

Remove `browser.list` and `tabs.list` only after named clients, release policy,
runbooks, and live evidence show the replacement surface is usable.

### Native work

- Move every diagnostic and internal example to `browser.snapshot` counts or the
  target tool surface.
- Mark legacy tool descriptions deprecated for the agreed release window without
  changing behavior.
- Keep legacy schemas and live-pagination semantics frozen during deprecation.
- Remove legacy MCP handlers, schemas, process fixtures, and extension request
  dispatch together in one coordinated release.
- Remove only code made unreachable by that coordinated change; retain shared
  browser normalization used by snapshot.
- Confirm final discovery contains only implemented effective members of the
  five-tool target surface.

### Extension work

- Remove `browser.list` and `tabs.list` native dispatch in the same release as
  native MCP handlers.
- Decide separately whether the direct local inventory popup remains useful; do
  not conflate popup inventory with MCP legacy-tool removal.
- Preserve broker status, controls, and capability onboarding.

### Tests

- Named target clients complete representative snapshot, change, semantic,
  visual, action, and evaluation calls where supported.
- Deprecation release discovery and final removal release discovery match policy.
- Removed direct calls return normal MCP method/tool-not-found behavior and never
  reach the extension.
- Mixed final/legacy-removal builds use the negotiated implementation manifest
  to suppress unsupported legacy dispatch. Any schema/ABI mismatch still fails
  visibly; matched final builds reconnect with the final surface.
- Doctor, README commands, troubleshooting, and live runbooks use no legacy tool.
- Full platform/restart matrix passes after dead-code removal.

### Documentation

- Name target clients and evidence versions.
- Publish deprecation duration/release count or explicitly record the pre-1.0
  breaking-removal decision.
- Remove legacy reference sections only when code is removed; preserve release
  notes/history.
- Update every architecture, setup, troubleshooting, privacy, security, roadmap,
  and progress statement to final shipped behavior.

### Exit gate

- The documented deprecation policy has been satisfied or maintainers recorded
  an explicit pre-release breaking-removal decision.
- Target clients support canonical structured results, list changes or reconnect,
  and image blocks where required.
- Legacy native and extension dispatch are removed together.
- No current runbook or example depends on a removed tool.

## Exact file-by-file map

This map reflects files that exist now and new files recommended by the current
seams. A later implementation may use a more focused split, but ownership must
not collapse back into `mcp.rs` or `background.js`.

### Native files

| File | State | Part 2 work |
| --- | --- | --- |
| `Cargo.toml` | Current | Add only dependencies proven by spikes; prefer existing crates and avoid an image decoder unless metadata validation requires it |
| `native/src/main.rs` | Current | Declare `references`, `browser_change`, page, capability, and optional retention modules |
| `native/src/broker.rs` | Current | v3 lifecycle records, request classes, method budgets, operation tasks, late responses, typed artifacts, dynamic notification plumbing, bounded shutdown |
| `native/src/protocol.rs` | Current | Strict v3 envelopes, richer capabilities, dispatch semantics, typed artifact schemas, hard/product limits, mixed-build failure |
| `native/src/runtime.rs` | Current | Process-wide capability/discovery watch, task registry, lanes, aggregate stores, lock-safe snapshot APIs |
| `native/src/mcp.rs` | Current | Replace static-only discovery/call routing, preserve custom strict decode, canonical summaries, structured outputs, annotations, one image block |
| `native/src/browser_snapshot.rs` | Current | Move refs/clock/quota/eviction to shared APIs; expose lookup by typed snapshot ref; preserve public behavior |
| `native/src/doctor.rs` | Current | Keep counts-only privacy-safe diagnostic and ensure migration never regresses it |
| `native/src/settings.rs` | Current | No authority change; only add product-limit settings if the v3 ADR explicitly requires configurable bounds |
| `native/src/install.rs` | Current | Reflect manifest/version setup changes only when optional Page permissions ship; do not weaken native-host allowlisting |
| `native/src/references.rs` | New | All typed public refs, incarnation mapping, lookup errors, tombstones where justified, epoch invalidation |
| `native/src/retention.rs` | New, recommended | Clock abstraction, aggregate accounting, fixed TTL, FIFO eviction, dependency cleanup, reservation/commit API |
| `native/src/capabilities.rs` | New, recommended | Validated capability facts, effective derivation checks, discovery snapshot, safe reason mapping |
| `native/src/browser_change.rs` | New | Preview schemas, preconditions, plans, summaries/warnings, apply state machine, outcomes |
| `native/src/page.rs` | New initially | Shared document/page/element refs, semantic schemas, page stores, inspect/action/evaluate result conversion |
| `native/src/page_inspect.rs` | New if `page.rs` grows | Semantic/visual parameter validation, typed extension results, page projection/cursors |
| `native/src/page_action.rs` | New if `page.rs` grows | Action schemas, lifecycle/result unions, inspect-after composition |
| `native/src/page_evaluate.rs` | New if `page.rs` grows | Evaluation schemas, byte limits, result/error conversion |

### Extension files

| File | State | Part 2 work |
| --- | --- | --- |
| `extension/manifest.json` | Current | Add optional Page permissions/hosts at P2.6 and optional `userScripts` at P2.9; never add `debugger`; set minimum Chrome only after decision gate |
| `extension/background.js` | Current | Become composition root around injected controller; retain synchronous listener setup and native connection ownership |
| `extension/protocol.js` | Current | Strict v3 validation, request classes/deadlines, dispatch state, capability snapshots, typed one-image artifacts and size checks |
| `extension/browser-model.js` | Current | Preserve authoritative rereads/incarnations; expose exact lookups/generations needed by operations; never mutate through snapshot methods |
| `extension/popup.html` | Current | Add accessible Browser changes, Page tools, and Advanced evaluation controls only in their milestones |
| `extension/popup.js` | Current | Use capability controller messages; request/remove permissions from user gestures; keep direct inventory behavior separate |
| `extension/popup.css` | Current | Add clear disabled/desired/granted/effective/warning states without hiding security consequences |
| `extension/package.json` | Current | Keep ESM test metadata only; no runtime package dependency or build step |
| `extension/background-controller.js` | New | Injectable native connection, ready/capability ordering, typed dispatch, epochs, deadlines, response construction |
| `extension/capabilities.js` | New | Persisted desired settings, permissions, version/API probes, effective state, safe reasons, change publication |
| `extension/browser-change.js` | New | Exact prechecks, operation execution, composite outcomes, authoritative postcondition reads, dedupe by operation ID |
| `extension/page-tools.js` | New | Document/frame coordination, target resolution, injection, capture, service-worker actions, evaluation routing |
| `extension/page-agent.js` | New | Fixed isolated semantic extraction, node-token registry, generation counters, actionability, DOM actions, mutation wait |

### Test and CI files

| File | State | Part 2 work |
| --- | --- | --- |
| `tests/broker_roundtrip.rs` | Current | v3 mismatch, framing, capabilities, artifacts, dispatch stages, shutdown, late responses |
| `tests/mcp_tools.rs` | Current | Empty pre-ready discovery, dynamic lists, annotations, schemas/goldens, two-session notifications |
| `tests/full_mcp_roundtrip.rs` | Current | Representative typed read/operation/page/image/evaluation routing; keep focused rather than exhaustive |
| `tests/browser_snapshot.rs` | Current | Complete Part 1 edge/retention/boundary/restart coverage using shared support |
| `tests/doctor.rs` | Current | Preserve counts-only output and no inventory leakage |
| `tests/support/mod.rs` | New | Shared broker process, framing, MCP clients, fixtures, fake extension behaviors, cleanup |
| `tests/protocol_v3.rs` | New, optional | Focused lifecycle/artifact/capability process matrix if broker roundtrip becomes too large |
| `tests/browser_change.rs` | New | Preview/apply schemas, plans, concurrency, cancellation, preconditions, partial/unknown outcomes |
| `tests/page_tools.rs` | New | Discovery, semantic, visual, action, evaluation process contracts and image blocks |
| `extension/tests/browser-model.test.mjs` | Current | Complete model races, incarnation reuse, generations, operation lookup support |
| `extension/tests/protocol.test.mjs` | Current | v3 exact fields, classes, stages, capabilities, artifacts, byte boundaries |
| `extension/tests/background-controller.test.mjs` | New | Ready/reconnect, epochs, dispatch, deadlines, capability-before-result, error privacy |
| `extension/tests/capabilities.test.mjs` | New | Settings, grants, probes, parent dependencies, revisions, popup-facing states |
| `extension/tests/browser-change.test.mjs` | New | Chrome mutation calls, exact preconditions, composites, revocation, read-back |
| `extension/tests/page-agent.test.mjs` | New | Semantics, privacy omissions, role/name, node identity, actionability, events, waits |
| `extension/tests/page-tools.test.mjs` | New | Documents/frames, capture generations, action routing, evaluation wrapper/probes |
| `.github/workflows/ci.yml` | Current | Continue checking every JS/MJS file and running dependency-free Node tests; add golden/budget jobs only if deterministic and bounded |

## Testing architecture

### Rust process tests

Use process-level tests for every trust or lifecycle boundary. Unit tests remain
appropriate for pure schema, projection, quota, and state-machine logic, but they
do not replace a framed Native Messaging plus HTTP MCP round trip.

Shared support must provide:

- Ephemeral loopback address reservation and isolated state/token environment.
- Child process ownership with bounded wait and stderr capture on failure.
- Native frame read/write with hard timeout and malformed-frame variants.
- Ready and complete capability builders by protocol version.
- Authenticated MCP clients with configurable client capabilities.
- Two-session setup and tool-list-change observation.
- Scripted extension responses before/after dispatch, delayed, malformed,
  oversized, wrong identity, wrong request, late, and disconnected.
- Synthetic baselines, plans, semantics, and tiny PNG artifacts containing no
  real browsing data.
- Fake clock access for in-process retention tests and short real-time bounds
  only where process behavior requires them.

Do not use the developer's real token, fixed port, Chrome registration, state
directory, or MCP client configuration.

### Dependency-free extension tests

Keep production extension modules dependency-free and directly loadable by
Chrome. Node is a CI/development test runner only.

- Export factory functions instead of importing a live `chrome` global in test
  modules.
- Use small mock events that record listener registration time and emit exact
  Chrome-shaped payloads.
- Inject Chrome APIs, storage, permissions, native ports, timers, clock, UUID,
  byte measurement, image parser metadata, and event drains.
- Use fake timers for deadlines, quiet waits, reconnect backoff, and race tests.
- Assert mutating API call sequences and arguments exactly.
- Assert no mutating APIs are called by preview, snapshot, or inspect.
- Test module-level composition separately from pure coordinators.
- Syntax-check every extension JS/MJS file in CI and run
  `node --test extension/tests/*.test.mjs`.
- Do not add npm runtime dependencies, bundling, transpilation, or a Chrome-side
  Node requirement.

### Schema and golden tests

- Check in deterministic tool input/output schemas for every advertised branch.
- Assert `additionalProperties: false`, discriminators, bounds, annotations, and
  omission of unimplemented branches.
- Keep representative result goldens synthetic and small.
- Golden summaries must not duplicate structured JSON.
- Image goldens contain metadata and a generated tiny fixture, never a live
  screenshot.
- Capability goldens include ineffective reasons but no internal exception text.

### Race tests

Every side-effecting method needs explicit cases for:

- Capability or permission loss before validation.
- Loss after validation but before queue.
- Loss in queue.
- Loss immediately before dispatch.
- Loss after dispatch and before response.
- Timeout before and after dispatch.
- HTTP waiter cancellation before and after dispatch.
- Native disconnect and worker/broker epoch change.
- Late response after the original waiter is gone.
- A-B-A generation changes where final state matches initial state.

## Live synthetic fixture matrix

Use a local dependency-free HTTP fixture server and a dedicated throwaway Chrome
profile. Fixtures use synthetic text and deterministic colors only.

| Fixture | Validates |
| --- | --- |
| Basic semantics | Prose deduplication, headings, links, landmarks, compact/full |
| Form controls | Input, textarea, contenteditable, select, checkbox, radio, disabled, readonly |
| Sensitive controls | Password, file, hidden values always omitted |
| Hostile strings | Long text, control characters, Unicode, markup-like text, token/byte bounds |
| DOM replacement | Exact element tokens never retarget similar replacement nodes |
| Open/closed shadow | Supported traversal and documented closed-root limitation |
| Same-origin frames | Multi-frame ordering, semantics, action routing |
| Cross-origin frames | Permission coverage, inaccessible-frame policy, exact document IDs |
| Loading/delayed page | Loading policy, navigation readiness, timeout |
| History/reload | Same-URL reload, back/forward, new document identity |
| DOM churn | Auto-wait quiet interval and timeout/unknown behavior |
| Action events | Exact click/fill/select/check event order and resulting state |
| Obscured controls | Actionability and `NOT_ACTIONABLE` behavior |
| Nested scrolling | Element/document scroll and generation changes |
| Red/blue pages | Active-tab screenshot attribution and A-B-A switching |
| Responsive/zoom page | CSS viewport versus image pixel dimensions and resize races |
| Evaluation page | Serialization, promise, exception, delayed effect, navigation timeout |
| Browser organization set | Windows/groups/tabs, exact indexes, grouping, movement, final-window policy |

Live harness output records pass/fail and sanitized versions only. It must not
print or persist page semantics, inventory, image bytes, evaluation source or
result, or bearer values.

## Platform and version matrix

| Environment | Required evidence |
| --- | --- |
| Linux Chrome stable | Full synthetic browser/page suite and broker/worker/client restart |
| Windows Chrome stable | Native Messaging, mutations, unfocused-window capture, permissions, restart |
| macOS Chrome stable | Registration, permissions, mutations, capture, restart |
| Chrome stable | Release-candidate full suite |
| Chrome beta | Event, permission, capture, and API regression smoke suite |
| Minimum supported Page Chrome | Semantic/action capability and manifest behavior |
| One version below Page minimum | Page tools absent or safe unavailable behavior |
| Chrome 134 or controlled equivalent | Evaluation absent/unavailable |
| Chrome 135-137 | Developer-mode `userScripts` path |
| Chrome 138+ | Allow User Scripts probe and revocation |
| WSL2 mirrored networking with Windows Chrome | Discovery, representative calls, image block, reconnect |
| WSL2 default NAT | Documented unsupported loopback sharing; no weakened bind workaround |
| Two MCP sessions | Shared refs/capabilities, list fan-out, same-plan dedupe, disjoint lanes |
| Each named target client | Structured errors/results, compact fallback, discovery refresh, image rendering |

Cross-compilation or `cargo check` is not live platform validation. Record exact
Chrome, extension, broker, OS, and client versions with each result.

## Security and privacy release gates

Every milestone that expands authority must pass these gates:

- Bearer, Host, Origin, exact extension allowlist, loopback bind, and Chrome-owned
  lifetime remain unchanged.
- All three global controls are absent/false by default on fresh install and
  upgrade and cannot be enabled through MCP.
- Extension dispatch rechecks effective capability; broker discovery alone never
  authorizes a call.
- Optional permissions are requested only by clear extension-UI gestures and are
  no broader than ADR 0006 permits.
- Incognito remains `not_allowed`; no separate identity or route is introduced.
- Page access is limited to normal HTTP(S) documents and documented Chrome APIs.
- No `debugger`, main-world evaluation, arbitrary Chrome API forwarding, shell,
  filesystem, CDP, or native command path exists.
- Exact refs prevent stale or similar targets from being substituted.
- Read tools do not activate, focus, scroll, reload, wake, or mutate.
- Visual preflight occurs before a page action when visual `inspectAfter` could
  otherwise fail after mutation.
- Public errors, stderr, popup status, test reports, and crash diagnostics omit
  inventory, URLs, titles, page data, screenshots, source, args, results, and
  bearer material.
- Password, file-input, hidden-control, and prohibited sensitive values are
  omitted before transport.
- Retained sensitive state is memory-only, bounded, FIFO, and fixed-TTL.
- Image bytes and evaluation values are not retained.
- Partial/unknown outcomes never imply rollback, cancellation, or safe blind
  retry.
- Privacy and security documents disclose bearer-wide enabled authority, page
  data sensitivity, high-impact site actions, and evaluation effects.

Any gate failure blocks release of that capability rather than being documented
as a known security exception.

## Token and byte measurement gates

Measure UTF-8 bytes and model tokens using one pinned tokenizer/version for:

- Pre-ready empty discovery.
- Core discovery with migration tools.
- Core discovery after legacy removal.
- Core plus semantic inspect.
- Core plus semantic/visual/action tools.
- All five tools with evaluation.
- Empty, one-tab, 100-tab, and 250-tab compact snapshots.
- Full snapshot at representative bounds.
- Preview and applied/partial/unknown browser-change results.
- Compact semantic viewport/document pages and cursor continuation.
- Full semantic result at representative element/text limits.
- Visual metadata excluding image bytes and complete encoded image transport.
- Action success, unknown, inspect-after success, and inspection failure.
- Evaluation value, error, and unknown results.

Measure byte boundaries at limit minus one, limit, and limit plus one for:

- Native request envelope and params.
- Native non-image response envelope.
- Encoded image artifact and total Chrome-to-host envelope.
- Every retained record kind and aggregate retained state.
- Text, semantic records, node tokens, cursors, operations, target refs, source,
  arguments, and evaluation result.
- MCP structured content and image block construction.

Release rules:

- Image bytes appear only in the artifact/image path.
- Text fallback remains a short summary, never pretty JSON.
- Preview/apply and action success do not echo inputs.
- Compact outputs omit defaults, repeated placement, geometry, raw DOM, and
  repeated text.
- No unimplemented branch contributes schema/token cost.
- Any material budget increase requires a measured workflow justification and
  updated enforced limits.
- No result may fit a product limit while exceeding a Native Messaging hard
  limit after envelope/base64 overhead.

## Rollout rules

1. Close Part 1 and accept protocol v3 before changing permissions.
2. Ship matched v3 and generalized runtime with no public behavior expansion.
3. Ship capability controller and dynamic discovery with Browser changes off.
4. Complete read-only `browser.change` preview as an internal milestone.
5. Ship preview and simple apply together behind the disabled-by-default Browser
   changes control.
6. Ship destructive/composite apply only after throwaway-profile evidence.
7. Ship optional Page permissions and semantic inspect first.
8. Ship visual inspect only after image and attribution gates pass.
9. Ship page actions only after exact actionability/wait semantics freeze.
10. Ship evaluation last behind a separate disabled-by-default control.
11. Deprecate legacy tools for the frozen policy window.
12. Remove legacy MCP and extension dispatch together.

Additional rollout constraints:

- Broker and extension protocol versions move together and fail visibly when
  mixed.
- No branch is advertised before both sides implement it.
- Capability disable/revocation blocks new dispatch immediately.
- In-flight reads fail closed after revocation.
- In-flight side effects return partial/unknown when dispatch may have occurred.
- Clients without list-change support reconnect after controls change.
- Rollback of a release means restoring a matched broker/extension pair; it does
  not mean protocol downgrade negotiation.
- Roadmap status remains prospective until code and required validation exist.

## Final completion criteria

- All five target tool contracts are implemented with no placeholder schema
  branch.
- `browser.snapshot` retains its Part 1 consistency and no-disturbance behavior.
- Browser changes, Page tools, and Advanced evaluation default disabled for fresh
  and upgraded users.
- Dynamic discovery is live after ready, empty before ready, and consistent
  across two sessions.
- Direct calls cannot bypass implementation, capability, permission, version,
  probe, epoch, reference, or stale-document checks.
- Browser plans are exact, self-contained, bounded, process-locally deduplicated,
  and honest about non-atomic partial/unknown outcomes.
- Page semantics are bounded DOM-derived data with exact document and element
  identity and required sensitive-value omissions.
- Active viewport capture works for already-active tabs in focused and unfocused
  windows without implicit disturbance and rejects generation races.
- Page actions perform one exact structured action and preserve action status
  across inspection failure.
- Evaluation runs only in isolated `USER_SCRIPT` world on supported/effective
  Chrome and treats timeout as unknown observation.
- Aggregate quotas, TTLs, FIFO eviction, method deadlines, concurrency lanes,
  and transport limits are measured and boundary-tested.
- Linux, Windows, macOS, restart, upgrade, Chrome-version, WSL2, two-session, and
  named-client evidence is recorded.
- Security/privacy gates pass and no real browser data, image, source, result, or
  credential appears in artifacts or logs.
- Documentation matches shipped behavior and distinguishes implemented,
  unavailable, and deferred work.
- Legacy tools are removed under the agreed policy or have an explicit tested
  deprecation release still in progress; final V1 completion requires removal.

## Explicit non-goals

- Reopening the single-bearer trust model or adding per-client, per-origin,
  per-tab, per-window, or per-operation authorization.
- Generic undo, rollback, transactions, or claims that Chrome mutations are
  atomic.
- Browser change subscriptions, event feeds, or durable plans across broker
  restart.
- Multiple Chrome-profile routing or weakening the fixed loopback architecture.
- Incognito access.
- `debugger`, CDP, canonical accessibility/DOM snapshots, console/network tools,
  or overlap with Chrome DevTools MCP.
- Background, non-active, full-page, stitched, element-crop, or visual-label
  screenshots.
- Implicit activation, focus, scrolling, reload, wake, or user-session
  disturbance by read tools.
- Fallback role/text/CSS locators, fuzzy retargeting, raw coordinates, trusted
  physical input, hover, key presses, drag/drop, upload, native dialog, or
  browser-UI automation.
- Main-world evaluation or runtime source through `scripting.executeScript()`.
- Arbitrary Chrome, DOM, native command, shell, filesystem, or network proxy
  forwarding.
- Production installer, ACP runtime, side-panel conversation UI, harness
  supervision, or packaging work unrelated to the five-tool surface.
- Runtime npm dependencies, extension bundling, or a Node requirement in Chrome.
- Retention of image bytes, evaluation payloads, page content, or browser data
  for analytics, telemetry, logs, or crash reports.
