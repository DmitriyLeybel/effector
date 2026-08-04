# ADR 0007: Version an operation-capable native protocol

Status: accepted
Date: 2026-08-03

## Context

Protocol v2 intentionally supports only correlated reads. It has no safe way to
negotiate methods added by different builds, describe whether a side effect was
dispatched, carry an image, or explain why an optional capability is disabled.
Adding those fields incrementally to v2 would make strict old peers fail in
different places and could let discovery overstate what the connected extension
can decode.

## Decision

- Effector advances the internal Native Messaging protocol to version 3 with
  envelope ABI revision 1. There is no v2 fallback or downgrade.
- Version 3 preserves the `ready`, `ready_ack`, `request`, `response`, and
  `capabilities_changed` message names, browser identity, request IDs, and
  complete monotonic capability snapshots.
- `ready` carries a bounded implementation manifest. Each entry identifies a
  method, an optional independently versioned branch, and a positive ABI
  revision. `ready_ack` returns the exact broker/extension intersection. Extra
  entries are ignored safely; a shared entry with a different ABI fails the
  handshake. Until dynamic discovery ships, all three existing read methods are
  required.
- Requests use one of `read`, `browserOperation`, `pageAction`, or `evaluation`.
  Both peers enforce an allowlisted method/class/deadline policy before dispatch.
  A class existing in the envelope does not make any method reachable.
- Every response includes `dispatch.state` as `notDispatched`, `completed`, or
  `unknown`. Pre-dispatch rejection uses `notDispatched`. `unknown` means a
  dispatched side effect may still complete and must not be retried
  automatically. Current reads reject `unknown`.
- Artifacts are separate from structured results. The first owned artifact is
  `{type:"image", mimeType:"image/png", data:<base64>}`. A response carries at
  most one artifact. Current read methods parse the envelope but reject any
  artifact because none permits one.
- Each global capability reports `implemented`, `desired`, `granted`,
  `supported`, `probePassed`, `effective`, and one bounded safe `reason`.
  `effective` must equal the conjunction of the five preceding facts. Chrome
  metadata support such as frozen tabs remains a separate complete boolean.
- The broker owns side-effecting operation tasks after admission. An MCP waiter
  may leave without cancelling an already dispatched task. One operation result
  is retained and shared by concurrent waiters; late results cannot trigger a
  second dispatch or satisfy another request.
- Dynamic discovery is derived from the negotiated implementation and effective
  capability snapshots. Before that registry lands, discovery remains the
  existing three read tools and direct calls still require a completed
  handshake.

## Initial limits

| Resource | Limit |
| --- | ---: |
| Host-to-Chrome Native Messaging frame | 1 MiB |
| Product request payload | 768 KiB |
| Chrome-to-host Native Messaging frame | 64 MiB |
| Extension response safety margin | 60 MiB |
| Decoded PNG artifact | 8 MiB |
| Artifacts per response | 1 |
| Implementation manifest entries | 64 |
| Native writer queue | 128 |
| Concurrent read requests | 128 |
| Read broker deadline / extension budget | 30 s / 29 s |
| Browser-operation broker deadline / extension budget | 60 s / 58 s |
| Page-action broker deadline / extension budget | 45 s / 43 s |
| Evaluation broker deadline / extension budget | 35 s / 33 s |
| Admitted browser-operation tasks | 32 |
| Concurrent page actions / per document | 16 / 1 |
| Concurrent evaluations / per document | 8 / 1 |

Only the read limits are active while the three read methods are the complete
implementation manifest. A future branch must freeze its result and retained
state limits before entering the manifest.

## Superseded details

This ADR supersedes only ADR 0005's prospective statement that protocol v2
carries artifacts and operation capability semantics, and ADR 0006's
prospective statement that protocol v2 represents dynamic Page-tool discovery.
Their identity, mutation, authorization, page-isolation, and security decisions
remain accepted.

## Consequences

- Same-v3 milestone builds can use their safe implementation intersection
  without pretending unknown schemas are compatible.
- Protocol errors distinguish rejection from uncertain side-effect completion.
- Rich capability state can drive UI and discovery without exposing private
  Chrome failure detail.
- The first v3 release changes no Chrome permission and exposes no new public
  tool or side effect.
- Broker-owned operation lifecycle and dynamic discovery still require their
  dedicated implementation milestones; wire support alone does not provide
  them.

## Rejected alternatives

- Adding optional fields to v2 would violate its strict validation contract.
- Using package versions as compatibility would reject safe mixed builds and
  would not identify the incompatible method.
- Treating a missing manifest entry as compatible would allow dispatch of an
  unknown schema.
- Putting base64 in structured content would duplicate large sensitive data and
  bypass MCP image blocks.
- Cancelling side effects when one HTTP waiter disconnects would make operation
  outcomes unknowable and invite duplicate retries.
