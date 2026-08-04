# MCP tool surface implementation plan, Part 3

Status: proposed workflow-efficiency overlay
Date: 2026-08-03

## Purpose

Part 3 turns agent-surface feedback into decision gates for the proposed V1
contract. It does not replace the implementation sequence in
[`mcp-tool-surface-plan-part-2.md`](mcp-tool-surface-plan-part-2.md), duplicate
its runtime work, or add another tool.

The authoritative public contract remains [`mcp-tools.md`](mcp-tools.md).
Accepted ADRs remain durable decisions. Part 3 may refine a proposed schema
before advertisement, but any change to an accepted ADR requires a focused new
ADR that states exactly what it supersedes.

Part 3 has four goals:

1. Preserve the five-tool concept set and compact defaults.
2. Make destructive and activation-recovery paths clearer without adding a new
   authorization system or hidden browser disturbance.
3. Decide from measured workflows, not intuition, whether a bounded `page.act`
   action sequence justifies replacing the proposed single-action schema, and
   freeze the existing automatic wait without creating a predicate language.
4. Measure complete agent trajectories before final schema and legacy-removal
   gates, not only isolated tool definitions and responses.

## Decisions preserved

Part 3 does not reopen these rules:

- The final model-facing vocabulary remains exactly `browser.snapshot`,
  `browser.change`, `page.inspect`, `page.act`, and `page.evaluate`.
- Compact output remains the default. Results omit inferable defaults, argument
  echoes, repeated placement, geometry, image base64, and normalized operations.
- Public references remain opaque, process-local, typed, and bound to exact
  browser, document, and element incarnations.
- `tabRef` survives navigation. `documentRef` and `elementRef` do not.
- `browser.change` remains preview/apply, non-atomic, exact-plan, and deduplicated.
  Apply receives only `planRef` and cannot edit the previewed operations.
- The installation bearer is one trust boundary. Browser changes, Page tools,
  and Advanced evaluation remain three global extension controls that default
  disabled. V1 adds no per-client, origin, tab, window, or operation policy.
- Visual inspection never activates, focuses, scrolls, reloads, or wakes a tab
  implicitly.
- Page actions resolve exact references only. No locator fallback, coordinates,
  trusted input, hidden retry, or automatic retargeting is introduced.
- `inspectAfter` reuses the `page.inspect` pipeline and returns at most one image.
- Timeout or disconnect after dispatch does not imply cancellation and never
  permits a blind retry.
- The V1 extension does not request `debugger`.

## Feedback disposition

| Feedback | Part 3 disposition |
| --- | --- |
| Keep five tools | Accept as fixed. Do not add `page.wait`, granular action tools, or a confirmation tool. |
| Preserve compact defaults and exact references | Accept as release invariants and trajectory measurements. |
| Add a small `page.act` sequence | Measure a strict action-only candidate against singular calls before changing the contract. Reject it if safety or efficiency gates fail. |
| Make singular action plus `inspectAfter` cheaper | Required whether or not sequences pass. Reuse the resulting page snapshot family where identity permits. |
| Add `confirmDestructive` | Reject. A caller boolean is not user consent and would weaken the plan-only apply shape. |
| Make destructive changes clearer | Accept compact machine-readable preview metadata and deterministic warnings. |
| Make visual activation recovery first-class | Accept compact recovery metadata only when it adds an otherwise unknown tab reference. Activation still occurs only through explicit `browser.change`. |
| Add cheap reliable waits | Freeze and measure the existing bounded `wait="auto"` heuristic. Defer explicit conditions unless post-V1 evidence shows inspect loops remain a material limiter. |
| Measure real workflow tokens | Accept as a schema-freeze and release gate using synthetic fixtures and a pinned tokenizer. |
| Add locators, coordinates, hover, keys, drag/drop, uploads, dialogs, full-page capture, or main-world evaluation | Keep deferred. |

## Contract refinements that do not need a new ADR

### Destructive preview metadata

`browser.change` preview should make destructive intent machine-readable without
adding another apply argument:

```json
{
  "planRef": "plan_72C",
  "summary": "Close 2 tabs and 1 window",
  "destructive": true,
  "warnings": ["Closing tabs and windows cannot be undone reliably"],
  "expiresAt": "2026-08-03T12:05:00Z"
}
```

Rules:

- Omit `destructive` when false.
- Set it for `tab.close`, `tab.discard`, and `window.close`.
- Keep operation-specific warnings for disruptive but not necessarily destructive
  behavior such as reload, navigation, focus, and moving the final tab.
- Generate one deterministic summary and a deduplicated, deterministic warning
  order from the normalized retained plan.
- Do not echo normalized operations or sensitive tab/window metadata.
- Apply remains `{mode:"apply", planRef}`. `confirmDestructive` is not accepted.
- MCP clients may continue to use their ordinary tool confirmation UI.

This refines proposed preview output but does not change the ADR 0005 trust or
execution model. Freeze it before P2.5 advertises a destructive branch.

### Structured activation recovery

`ACTIVATION_REQUIRED` should carry only the exact recovery information needed to
construct an explicit activation plan:

```json
{
  "code": "ACTIVATION_REQUIRED",
  "message": "Visual inspection requires this tab to be active in its window.",
  "recovery": {"tabRef": "tab_42"}
}
```

Rules:

- Return `recovery.tabRef` only when the caller supplied a `documentRef` and
  therefore does not already have the tab reference.
- When the caller supplied `tabRef`, return only the typed error and message.
- Documentation maps `recovery.tabRef` to a `tab.activate` operation whose
  existing `focusWindow` default remains false.
- Do not create a hidden browser snapshot, preview plan, or apply call.
- The caller still obtains a current `browserSnapshotRef`, previews the suggested
  operation, applies the returned `planRef`, and retries visual inspection.
- Recheck activation and document identity on retry. The recipe is guidance, not
  authority and not an authorization bypass.
- Use the same recovery shape for visual `page.inspect` and visual
  `page.act.inspectAfter` preflight failures.
- Do not include titles, URLs, Chrome IDs, page content, inferable defaults, or
  duplicated caller arguments.

This is compatible with ADR 0006 because activation remains explicit and occurs
only through `browser.change`. Freeze it before P2.7 visual advertisement.

### Singular-action fast path

The single-action path remains first-class even if sequences pass:

- One action plus `inspectAfter` must require one page action dispatch and one
  reused inspection pipeline, not a synthetic second MCP call.
- If the resulting document remains the same, post-action inspection may reuse
  the same document/page-snapshot family while issuing new element references as
  needed. It must not present pre-action semantics as post-action state.
- Navigation returns a new `documentRef` and a new page snapshot identity.
- Success without inspection stays `{documentRef}` and does not gain sequence
  bookkeeping.
- Action success plus inspection failure preserves action success and one compact
  `inspectionError`.

## Decision-gated `page.act` candidate

The current proposed contract and ADR 0006 specify exactly one structured action.
Part 3 does not change that immediately. It defines one candidate to measure
before P2.8 schema freeze.

### Candidate shape

If accepted, replace singular `action` with `actions`; do not support both shapes
indefinitely because `page.act` is not implemented yet.

```json
{
  "documentRef": "doc_B91C",
  "actions": [
    {"type": "fill", "elementRef": "el_name", "text": "Ada"},
    {"type": "check", "elementRef": "el_terms", "checked": true},
    {"type": "click", "elementRef": "el_save"}
  ],
  "wait": "auto",
  "timeoutMs": 10000,
  "inspectAfter": "semantic"
}
```

### Hard bounds

- One through three actions.
- Every action uses the one exact starting `documentRef`. Element-targeted
  actions use exact existing `elementRef` values; navigation and document-scroll
  actions use only their existing strict fields. A sequence cannot discover and
  target a new element.
- One aggregate `timeoutMs` covers queueing, all actions, final automatic
  settling, and `inspectAfter`. Reads and successful intermediate actions never
  extend it.
- Actions execute serially. Stop at the first failure, uncertainty, capability
  loss, stale reference, unexpected document replacement, or timeout.
- There is no `stopOnError`, continue-on-error, branching, loop, parallelism,
  variable, retry, or per-action timeout.
- `click`, `navigate`, `back`, `forward`, and `reload` must be the final action
  because they may replace the document. Any unexpected navigation after another
  action also stops the sequence.
- Actions are never reordered or normalized into a different sequence.
- `inspectAfter` runs once after complete known success. It does not run after an
  unknown dispatch result. Known action success plus inspection failure retains
  the existing compact `inspectionError` rule.
- Visual `inspectAfter` activation, capability, and rate admission are preflighted
  before the first side effect. Preflight failure performs no action. A later
  post-action capture failure preserves action success and returns
  `inspectionError`.

### Automatic wait freeze

Part 3 does not add an explicit wait step, wait-only action, `until` predicate,
or sixth tool. It freezes the existing final `wait="auto"` behavior before P2.8:

- Register target-document mutation and navigation observation before each
  action so unexpected document replacement is not missed.
- Observe the tab's starting main document and the acted frame. Any navigation
  that invalidates either identity stops a non-final action sequence.
- After the final action, if navigation was observed, resolve authoritative frame
  and main-document identity, then wait for the relevant replacement document
  agent and `document.readyState` to reach `interactive` or `complete` within the
  aggregate deadline.
- Otherwise require 250 milliseconds with no target-document subtree
  `childList`, `attributes`, or `characterData` mutation record.
- Observe the acted document/frame, not unrelated frames or the entire browser.
- Continuous relevant mutation reaches the aggregate deadline; activity never
  refreshes or extends it.
- Treat DOM quiet as a best-effort settling heuristic. Never call it network
  idle, application stability, or proof that delayed work is finished.
- Recheck exact document and action result state after waiting. Layout-only or
  compositor-only changes may not create mutation records.
- `wait="none"` skips final navigation/quiet observation after dispatch
  bookkeeping, while still performing identity checks between sequence actions.

Trajectory fixtures must measure SPA inspect polling that remains after this
heuristic. A future exact-condition proposal is considered only after the V1
action path ships and measured polling remains material; it is not part of this
candidate.

### Compact outcomes

Complete success keeps the existing compact shape and does not report action
counts:

```json
{"documentRef":"doc_B91C"}
```

When known earlier side effects succeeded before a later failure, return only
recovery-relevant sequence position:

```json
{
  "status":"partial",
  "completedActions":1,
  "failedActionIndex":1,
  "error":{"code":"NOT_ACTIONABLE","message":"The next element is not actionable."},
  "documentRef":"doc_B91C"
}
```

When dispatch certainty is lost:

```json
{"status":"unknown","completedActions":1,"uncertainActionIndex":1}
```

Rules:

- `completedActions` is a count. Action indexes are zero-based and refer to
  caller order.
- Do not echo successful actions, targets, text, values, or normalized inputs.
- Failure before any side effect uses the normal compact tool error path.
- Failure after a known side effect uses a normal partial result so clients do
  not invite blind retry.
- An extension-returned result may include action progress because the extension
  observed it. A broker-generated post-dispatch timeout or disconnect returns
  only `{status:"unknown"}` and an exact document reference only if independently
  known; it never invents action progress.
- Protocol `dispatch.state="unknown"` represents loss of extension dispatch
  certainty. A returned result with `status="unknown"` uses completed protocol
  dispatch and represents page/Chrome outcome uncertainty. No progress-message
  protocol is added.
- Unknown omits a document reference unless the exact current document is known.
- The caller inspects before any retry after partial or unknown.
- If all actions are known successful and only `inspectAfter` reaches the
  deadline, preserve action success and return compact `inspectionError` rather
  than partial or unknown action status.
- If every action is known successfully completed and only final automatic
  settling reaches the deadline, return normal success with compact `waitError`;
  omit requested `inspectAfter` and omit `documentRef` unless the exact current
  main document is established. Any uncertain action outcome remains `unknown`
  instead. `waitError` means "do not retry the action; inspect current state,"
  not that the application is stable.

```json
{"waitError":{"code":"TIMEOUT","message":"The page did not finish automatic settling in time."}}
```

## Decision gates

### G3.0: Scope lock

Required before Part 3 work affects implementation:

- Confirm exactly five target tools.
- Confirm no new Chrome permission, trust boundary, or policy layer.
- Confirm Part 2 remains the implementation authority.
- Confirm `confirmDestructive` and automatic activation are rejected.

### G3.1: Workflow baseline

Before prototyping, freeze the complete synthetic corpus and classify every
workflow mechanically. A workflow is sequence-eligible only when it has two or
three serial actions, every element reference comes from one starting semantic
inspection, no intermediate action requires newly discovered state, and any
navigation-capable action is final. Give every fixed fixture equal weight. Then
record singular-action baselines using the proposed final schemas:

1. Inspect, fill, check, submit, and inspect a stable form.
2. Inspect, scroll an existing off-viewport element into view, click it, and
   inspect the resulting same-document state.
3. Act on a non-navigating SPA, use final automatic settling, then inspect or
   poll inspection when delayed work outlives the heuristic.
4. Scroll and capture the already-active viewport.
5. Recover from inactive visual inspection through snapshot, activation preview,
   apply, and retry.
6. Reorganize multiple windows and groups, including a destructive preview.

Record MCP calls, Native Messaging dispatches, UTF-8 request/response bytes,
model-visible tokens, and final context growth. Elapsed time is informational
unless captured by one fixed end-to-end runner.

### G3.2: Sequence value

Accept the bounded candidate only if all are true:

- Every sequence-eligible success workflow saves at least one MCP round trip and
  does not increase total model-visible tokens.
- Include at least two distinct common sequence-eligible success workflows and
  their fixed failure/recovery variants.
- The complete eligible corpus reduces canonical final-context tokens by at
  least 15 percent and at least 250 tokens against singular calls that establish
  identical final knowledge.
- One ordinary single-action call grows by no more than the larger of 5 percent
  or 50 tokens.
- All-five discovery grows by no more than the larger of 5 percent or 100
  tokens. There is no discretionary exception.
- Rerender, stale-element, timeout, partial, unknown, and recovery trajectories
  are included in the fixed corpus and do not cost more than singular recovery
  to the same established final state.

These are schema/release gates, not permanent per-commit tests. If the candidate
does not pass, retain singular `action`, freeze `wait="auto"`, and rely on
`inspectAfter` plus explicit inspect loops.

### G3.3: Sequence safety

Before acceptance, freeze and test:

- Three-action cap and one aggregate deadline.
- Exact terminal-action and unexpected-navigation behavior.
- Stop-on-first-failure semantics and partial/unknown result precedence.
- Capability revocation, cancellation, disconnect, and late-result handling at
  each action boundary.
- One page/document lane and no parallel action dispatch.
- Final inspection behavior and visual preflight ordering.
- Exact automatic navigation/DOM-quiet semantics and timeout precedence.
- Settling timeout returns `waitError` without inviting action retry.
- Named-client rendering of the complete actions array in model-visible context
  and, when a client presents ordinary tool confirmation, user-visible context.
  If target clients truncate or obscure the batch, reject sequences for V1.

Any ambiguous retry, retargeting, or document-transition rule rejects the
candidate for V1.

### G3.4: Focused ADR

If and only if G3.1 through G3.3 pass, accept a new ADR that supersedes only ADR
0006's one-action cardinality. Preserve its
global capability, exact-reference, isolated-world, visual, evaluation, and
security decisions. The ADR must address authority compression into one client
confirmation. Update every normative singular-action statement in
`mcp-tools.md`, Part 2 outcomes/P2.8/final gates, architecture, privacy, and
security together. ADR 0007 remains unchanged; reject the candidate if its
implementation would require a new progress message.

### G3.5: Activation recovery

Before visual branches are advertised:

- Prove recovery returns the exact current `tabRef` only for a `documentRef`
  caller and no Chrome numeric ID.
- Prove no activation, snapshot, plan, focus, or retry occurs implicitly.
- Complete the documented snapshot-preview-apply-retry flow in two named MCP
  clients and a live throwaway profile.
- Measure the structured recipe against message-only recovery.

### G3.6: Destructive clarity

Before destructive branches are advertised:

- Freeze the destructive operation set, summary wording, warning ordering, and
  omission rules.
- Prove apply rejects extra confirmation fields and still accepts only `planRef`.
- Scope the field to model-visible preview clarity. Portable MCP annotations
  remain conservatively destructive for both modes, and apply confirmation sees
  an opaque `planRef`; do not require clients to correlate prior preview output.
- Record named-client behavior when a client does provide argument-aware or
  conversational confirmation, without treating it as portable authorization.

### G3.7: Release trajectory

Re-run measurements when:

- Semantic `page.inspect` is advertised.
- Visual inspection is advertised.
- The final `page.act` shape is advertised.
- `page.evaluate` joins discovery.
- Legacy tools are deprecated and removed.

Each report records absolute values and change from the prior gate. Material
growth requires a concrete workflow benefit, not only additional fields.

## Milestone overlay

Part 3 work is interleaved with Part 2 rather than executed after it.

### P3.0: Baseline and scope freeze

Timing: before P2.5/P2.7/P2.8 schema freezes.

- Create synthetic workflow fixtures containing no real browsing data.
- Pin the tokenizer name, version, and invocation used for reports.
- Record current proposed singular-action, activation-error, and destructive
  preview baselines.
- Complete G3.0 and G3.1.

### P3.1: Destructive and activation recovery refinements

Timing: destructive metadata with P2.5; activation recovery with P2.7.

- Freeze schemas and deterministic examples.
- Add typed output/error variants only when their owning branch is implemented.
- Validate two-client rendering and recovery behavior.
- Complete G3.5 and G3.6 without changing permissions or authorization.

### P3.2: `page.act` experiment and decision

Timing: after semantic element identity exists in P2.6 and before P2.8 public
advertisement.

- Prototype action semantics against synthetic page-agent fixtures, then run a
  minimal end-to-end HTTP MCP, broker, framed Native Messaging, and extension-
  controller process spike covering HTTP waiter cancellation, disconnect, late
  response, extension-returned progress, broker-generated unknown, and the
  aggregate deadline.
- Capture named-client rendering and confirmation behavior for the complete
  actions array before deciding the ADR.
- Compare it to repeated singular calls; do not advertise both schemas.
- Run G3.2 and G3.3.
- Reject and delete the prototype if the gates fail.
- If the gates pass, accept the focused ADR and replace the proposed singular
  contract before implementation ships.

### P3.3: Action implementation integration

Timing: within P2.8, not as a parallel runtime project.

- Reuse Part 2 operation ownership, dispatch state, document lanes, page agent,
  inspect pipelines, artifact handling, quotas, and capability checks.
- Implement only the selected singular or bounded shape.
- Keep singular success and common one-action execution on the shortest path.
- Complete schema, process, extension, live, privacy, and security gates.

### P3.4: Release trajectory

Timing: at each advertised page branch and P2.10 legacy-removal gate.

- Publish synthetic measurement reports with no browser data or credentials.
- Track fixed discovery cost and complete workflow context growth.
- Use the reports to remove fields or branches that do not earn their cost.
- Do not add a tokenizer as an extension runtime dependency.

## Focused test matrix

Existing Part 2 protocol, retention, race, platform, permission, and privacy tests
remain authoritative. Part 3 adds only refinement-specific coverage.

### Destructive preview

- Destructive flag omitted/true and never false.
- Close, discard, and window-close combinations.
- Disruptive non-destructive warnings.
- Deterministic summary/warning order and deduplication.
- No title, URL, Chrome ID, or normalized-operation echo.
- Apply rejects `confirmDestructive` and every field except mode/plan reference.

### Activation recovery

- A `documentRef` caller receives the exact `tabRef`; a `tabRef` caller receives
  no duplicated recovery field.
- Inactive target causes zero capture, activation, focus, snapshot, preview, or
  apply calls.
- Stale tab/document and activation races fail closed.
- End-to-end explicit snapshot-preview-apply-retry succeeds.
- Visual `inspectAfter` preflight uses the same shape and performs no action.

### Candidate actions and automatic wait

- Zero, one, three, and four actions.
- Unknown fields and forbidden per-action controls.
- Terminal actions followed by another action are rejected before dispatch.
- Exact serial ordering with no hidden retry, normalization, or parallelism.
- Stale/replaced/similar elements never fall back to another node.
- Navigation wait, 250-millisecond target-document DOM quiet, `wait="none"`, and
  timeout.
- Main-document and acted-frame navigation, including child-frame action that
  replaces the main document.
- Continuous DOM churn reaches the aggregate timeout without extending it.
- Capability loss, navigation, action failure, cancellation, disconnect, and
  late response at every action boundary.
- Known partial, extension-returned unknown, broker-generated unknown, and
  wait/inspection-only timeout precedence with no input echo.
- One aggregate deadline and one document lane under a fake clock.
- Visual preflight before the first side effect and at most one image.
- Complete success, action-success/inspection-failure, and unknown-without-
  inspection branches.

### Trajectories

- Stable form with repeated singular calls versus candidate actions.
- Non-navigating SPA churn with auto wait and inspect polling.
- Scroll/capture with and without `inspectAfter`.
- Inactive visual recovery with message-only and structured recovery.
- Multi-window organization with simple and destructive browser changes.
- Success, partial, unknown, stale reference, and capability-revoked outcomes.

## Measurement method

Use checked-in synthetic requests, results, discovery documents, and canonical
client renderings. Freeze the corpus before implementing the candidate. Measure
identical final knowledge, including recovery to an established state rather
than nominal call completion. For every trajectory record:

| Dimension | Measurement |
| --- | --- |
| Discovery | Core, page-enabled, all-five, and migration tool definitions |
| Calls | MCP calls and extension dispatches |
| Wire | UTF-8 request, structured result, summary, and image-envelope bytes |
| Model context | Canonical final-context tokens for exact tools/list rendering, arguments, summaries, structured results, and recovery calls using one pinned tokenizer/version |
| Artifacts | Encoded and decoded bytes, counted once |
| Outcomes | Success, partial, unknown, and recovery path |
| Delta | Absolute value, prior gate, and singular-action baseline |

Byte and schema boundaries run in normal automated tests. Token reports run at
contract and release gates so the extension remains dependency-free and CI does
not gain a tokenizer solely for every source change.

Record both protocol-canonical JSON and each named client's model-visible
serialization. Count discovery once and report one-, five-, and twenty-call
session amortization. Report image bytes and any client-specific image-token
charge separately. Exclude unrelated system prompts and user conversation. Every
report includes absolute token deltas, percentages, round trips, and the
calls-per-session break-even point.

Use canonical final-context growth for acceptance gates. Report aggregate
per-turn model usage separately by replaying the same fixed prompt envelope and
counting the full model-visible context at each turn; do not mix that value into
the final-context gate.

Reports must contain no real titles, URLs, content, source, arguments, bearer
tokens, extension IDs, or inventory dumps.

## Documentation obligations

- Update `docs/mcp-tools.md` only when a refinement is frozen for implementation.
- Add the focused action-cardinality ADR only if the sequence gates pass; update
  every normative singular-action reference, not only P2.8.
- Add small supersession notes to P2.5, P2.7, and P2.8 rather than duplicating
  their implementation work.
- Update architecture and failure-mode docs for accepted sequence partials,
  automatic-wait semantics, or activation recovery.
- Update `PRIVACY.md` and `SECURITY.md` before multi-step authority or page
  content ships.
- Update README and troubleshooting with explicit activation recovery only when
  visual inspection is implemented.
- Update `docs/progress.md` for actual decisions, implementation, measurements,
  and live evidence; planning alone is not implementation.
- Keep `docs/roadmap.md` prospective until live gates pass.

## Explicit non-goals

Part 3 does not add:

- A sixth tool, including `page.wait`.
- `confirmDestructive`, confirmation receipts, or a second authorization system.
- Automatic activation, focus, snapshot creation, plan creation, apply, or retry.
- Fallback selectors, roles, text, CSS, coordinates, visual targeting, or element
  rediscovery.
- Explicit wait steps or wait-only calls; arbitrary predicates, URL/text/network
  matching, fixed sleeps, or scripts.
- Continue-on-error sequences, loops, branches, variables, parallel steps, or
  reusable macros.
- Hover, key presses, drag/drop, upload, native dialogs, browser UI, or trusted
  physical input.
- Full-page stitching, element crops, visual labels, non-active capture, or
  `debugger`.
- Per-client, per-origin, per-tab, or per-window policy.
- Undo, change feeds, main-world evaluation, or broader Chrome/CDP forwarding.

## Completion gate

Part 3 is complete when:

- The five-tool concept set and trust model remain unchanged.
- Destructive preview and activation recovery refinements are frozen, measured,
  implemented with their owning Part 2 branches, and live validated.
- The bounded `page.act` candidate is either rejected with recorded evidence or
  accepted through a focused ADR and implemented as the sole advertised schema.
- Singular action remains compact and efficient in either outcome.
- Representative trajectories meet their declared round-trip and token gates.
- No deferred capability, permission, policy, or tool leaked into V1.
