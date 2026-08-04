# Effector documentation

This directory is the project's working memory. Documents are intentionally
small and cross-linked so architecture, decisions, research, and progress can
evolve independently.

## User guides

- [`mcp-tools.md`](mcp-tools.md) — authoritative current tool contract and the
  candidate five-tool version-one surface.
- [`troubleshooting.md`](troubleshooting.md) — startup order, diagnostics, WSL2
  networking, and common recovery steps.

## Architecture

- [`architecture/overview.md`](architecture/overview.md) — system boundaries,
  components, and recommended topology.
- [`architecture/review.md`](architecture/review.md) — review of the initial
  proposal, including failure points and corrections.
- [`architecture/process-and-transport.md`](architecture/process-and-transport.md)
  — process ownership, Native Messaging, and broker-hosted MCP HTTP.
- [`architecture/session-model.md`](architecture/session-model.md) — browser
  instances, Effector workspaces, MCP connections, and harness sessions.
- [`architecture/failure-modes.md`](architecture/failure-modes.md) — expected
  failures and recovery behavior.
- [`architecture/installation-and-packaging.md`](architecture/installation-and-packaging.md)
  — one-binary launch model, development setup, cross-platform packaging, and
  the future ACP seam.

## Decisions

- [`decisions/0001-chrome-only.md`](decisions/0001-chrome-only.md) — Chrome and
  documented Chromium APIs are the product boundary.
- [`decisions/0002-browser-owned-native-broker.md`](decisions/0002-browser-owned-native-broker.md)
  — historical browser-owned broker and stdio-proxy transport spike.
- [`decisions/0003-mcp-first-harness-agnostic.md`](decisions/0003-mcp-first-harness-agnostic.md)
  — accepted MCP-first scope, Rust native executable, no harness selectivity,
  and ACP deferred behind a stable boundary.
- [`decisions/0004-broker-hosted-streamable-http.md`](decisions/0004-broker-hosted-streamable-http.md)
  — accepted one-process broker with authenticated MCP Streamable HTTP.
- [`decisions/0005-browser-incarnations-snapshots-and-mutation-plans.md`](decisions/0005-browser-incarnations-snapshots-and-mutation-plans.md)
  — accepted browser incarnations, broker-owned immutable snapshots, opaque
  references, and exact non-atomic mutation plans.
- [`decisions/0006-global-page-capabilities-and-isolated-tools.md`](decisions/0006-global-page-capabilities-and-isolated-tools.md)
  — accepted global page permissions, DOM-derived tools, active-viewport
  capture, `inspectAfter`, and isolated user-script evaluation.
- [`decisions/0007-version-operation-capable-native-protocol.md`](decisions/0007-version-operation-capable-native-protocol.md)
  — accepted strict protocol v3, implementation negotiation, dispatch state,
  rich capability facts, and typed PNG artifacts.

## Planning and research

- [`progress.md`](progress.md) — current implementation state.
- [`roadmap.md`](roadmap.md) — staged delivery plan.
- [`mcp-tool-surface-plan.md`](mcp-tool-surface-plan.md) — Part 1 implementation
  plan and record for protocol v2, browser identity, and `browser.snapshot`.
- [`mcp-tool-surface-plan-part-2.md`](mcp-tool-surface-plan-part-2.md) —
  code-grounded continuation for operations, Page tools, dynamic discovery, and
  final migration.
- [`mcp-tool-surface-plan-part-3.md`](mcp-tool-surface-plan-part-3.md) —
  decision-gated workflow-efficiency overlay for destructive clarity, explicit
  activation recovery, bounded page actions, automatic waits, and token traces.
- [`research/playwright-mcp-gaps.md`](research/playwright-mcp-gaps.md) — browser
  management capabilities that complement Playwright MCP.
- [`research/chrome-devtools-and-extension-api-coverage.md`](research/chrome-devtools-and-extension-api-coverage.md)
  — official Chrome agent/tooling coverage versus all Chrome Extensions API
  namespaces, including the recommended Effector boundary.
- [`research/acp-harness-communication.md`](research/acp-harness-communication.md)
  — ACP research, side-panel display model, ACP/MCP topology, security, and the
  recommended harness-integration path.

## Document conventions

- Decision records describe durable choices and their consequences.
- Architecture documents describe the current target design, not promises.
- Unresolved questions are listed explicitly rather than hidden in prose.
- Progress records what exists today; roadmap records intended work.
