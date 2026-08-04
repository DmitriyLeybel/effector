# ADR 0005: Use incarnations, immutable snapshots, and exact mutation plans

Status: accepted
Date: 2026-08-03

## Context

The legacy inventory tools expose runtime-scoped Chrome numeric IDs and
read live state again for each offset page. That is sufficient for read-only
inventory, but it cannot safely support immutable pagination or a later preview
and apply workflow. Chrome can reuse numeric IDs, browser state can change
between calls, and Chrome does not provide a transaction spanning multiple tab,
group, and window operations.

The first browser-management slice needs one identity and retention model that
works across MCP sessions sharing the Chrome-owned broker without treating live
Chrome IDs or client-provided cursors as authority.

## Decision

- The extension assigns a random internal incarnation key whenever it observes
  a new browser, window, group, tab, or document incarnation. A tab incarnation
  survives navigation; a document incarnation does not. Reuse of a Chrome
  numeric ID creates a different incarnation.
- The broker maps internal incarnation keys to random, typed, server-issued
  public references. Runtime Chrome numeric IDs do not cross the new public MCP
  boundary. All public references are process-local and become invalid when the
  broker restarts.
- The extension supplies complete normalized browser baselines to the broker.
  The broker retains a complete bounded baseline before deriving immutable
  browser snapshots, projections, filters, and cursor pages. Cursor reads use
  only retained state and never query live Chrome.
- Baseline window order is the focused window first, followed by all remaining
  windows in ascending runtime Chrome window ID order. Each window preserves
  Chrome tab-strip order. Chrome IDs are used only for this internal ordering
  and extension dispatch.
- Browser/page snapshot, cursor, plan, and artifact stores are bounded and use
  FIFO eviction. Each expiring record has a fixed TTL measured from creation;
  successful reads do not refresh that TTL or move a record in the eviction
  order.
- Pagination cursors are random server-side handles. A cursor record identifies
  the retained immutable snapshot, projection, and next position. Clients cannot
  construct an offset or alter filters through a cursor.
- `detail="counts"` is computed from a captured baseline and returned without
  retaining a browser snapshot, cursor, or count record. Counts therefore
  cannot be continued or used as a mutation base.
- If the connected Chrome version cannot report a requested property, including
  `frozen`, a filter on that property fails as unsupported. Missing capability
  is not interpreted as `false`.
- Mutation preview resolves typed public references against one retained
  browser snapshot and stores the exact ordered operations, target and
  destination incarnations, relevant preconditions, `stopOnError`, expiry, and
  one eventual result. Apply cannot replace or edit those stored operations.
- Mutation execution is explicitly non-atomic. The extension rechecks the exact
  relevant preconditions immediately before each operation and returns ordered
  `applied`, `partial`, `failed`, `skipped`, or `unknown` outcomes. A timeout or
  disconnect after dispatch does not imply cancellation.
- A plan is marked applying before dispatch. Concurrent applies join the one
  retained result, and a completed plan is not dispatched again while retained.
- The Native Messaging boundary advances from protocol version 1 to version 2
  as one coordinated Rust, extension, test, and documentation upgrade. Version
  2 retains the existing correlated `ready`, `ready_ack`, `request`, and
  `response` model while adding object incarnations, typed errors and artifacts,
  and one explicit boolean per capability rather than an open-ended
  capability-name list. These include effective Browser changes, Page tools,
  Advanced evaluation, and frozen-tab metadata support. Version 1 and version 2
  peers fail visibly rather than negotiating a mixed behavior set.
- Global capability toggles persist in extension storage. Public references,
  complete baselines, snapshots, cursors, plans, deduplication results, and MCP
  session views remain broker memory only.

## Consequences

- Browser pagination can be stable even while the user continues changing live
  Chrome state.
- Complete baselines consume more broker memory than page-local results, so a
  baseline that cannot fit the configured bounds fails instead of degrading to
  a partial or live snapshot.
- FIFO eviction and non-refreshing TTLs make expiry predictable and prevent a
  client from retaining sensitive inventory indefinitely through reads.
- Random public references and cursors prevent stale Chrome IDs and caller-made
  offsets from retargeting operations.
- Counts remain low-retention health data and cannot accidentally become a
  durable inventory snapshot.
- Preview reduces accidental operation drift but cannot make Chrome mutations
  transactional. Partial and unknown outcomes require a fresh snapshot before a
  retry decision.
- A broker restart deliberately loses every public handle and pending plan even
  when the extension's persistent capability settings survive.

## Rejected alternatives

- Exposing Chrome numeric IDs publicly would permit ID reuse to retarget stale
  calls.
- Live offset pagination would continue to skip or repeat records as Chrome
  changes.
- Sliding TTLs or least-recently-used retention would let reads prolong
  sensitive state and make expiry less predictable.
- Client-decodable cursors would expose retention internals and permit callers
  to alter continuation state.
- Treating unsupported boolean properties as false would return incorrect
  filtered results.
- Generic rollback would imply guarantees Chrome cannot provide for close,
  navigation, discard, and other externally observable operations.
