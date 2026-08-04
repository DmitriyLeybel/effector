# Roadmap

## Phase 0: architecture and inventory prototype

- [x] Chrome-only scope.
- [x] Read-only inventory popup.
- [x] Architecture review and process model.
- [x] Select Rust for the native executable and initialize the Cargo toolchain.

## Phase 1: transport spike

- [x] Add MV3 service worker.
- [x] Implement per-user Native Messaging registration and broker launch.
- [x] Implement protocol handshake and structured errors.
- [x] Spike authenticated loopback IPC and a harness-facing stdio proxy.
- [x] Replace the proxy with authenticated broker-hosted Streamable HTTP.
- [x] Expose `browser.list` and paginated `tabs.list` read-only tools.
- [x] Test Native Messaging framing, HTTP authentication, MCP discovery, direct
      request routing, and Chrome-owned shutdown with simulated messages.
- [x] Validate the complete flow with Windows Chrome and an MCP client in WSL2.
- [ ] Test Chrome/host/client restart and HTTP reconnection behavior.
- [ ] Validate fixed-port and bearer-header support in target harnesses.

## Phase 2: complete read model

- [x] Complete hierarchical window/group/tab reads beyond page-local context.
- [x] Tab filtering and pagination.
- [ ] Field selection.
- [x] Browser instance IDs.
- [x] Snapshot revision IDs.
- [x] Chrome event aggregation and resynchronization.
- [ ] Multiple Chrome-profile routing.
- [x] Implement the proposed `browser.snapshot` contract with opaque references
      and immutable pagination.

## Phase 3: controlled mutations

- [ ] Create, activate, move, pin, duplicate, discard, and close tabs.
- [ ] Create, focus, update, and close windows.
- [ ] Group, ungroup, move, rename, recolor, and collapse Tab Groups.
- [ ] Typed stale-state conflicts and dry-run support for bulk operations.
- [ ] Implement `browser.change` preview, exact target preconditions, and concise
      process-local apply results.
- [ ] Add the global Browser changes toggle and complete typed operations.
- [ ] Defer undo until a concrete workflow justifies its retained-state and
      schema cost.

## Phase 4: user experience and packaging

- [ ] Side panel for connection status and workspace assignment.
- [ ] Cross-platform native-host installer and uninstaller.
- [x] Initial `effector install` and `effector doctor` developer commands.
- [ ] Stable production extension ID and update strategy.
- [ ] Permission onboarding.
- [x] Deny incognito access until separate identity and routing are designed.
- [x] Publish repository privacy and security policies.
- [ ] Add extension icons and Chrome Web Store listing assets.

## Phase 5: optional integrations

- [ ] Effector workspaces and bounded pending messages.
- [ ] ACP v1 client spike: initialize, authentication, prompt streaming,
      tool cards, permissions, cancellation, and reconnect.
- [ ] Pass the authenticated Effector HTTP endpoint to one ACP agent and
      complete one browser-tool call.
- [ ] Registry-backed ACP agent profiles and session lifecycle controls.
- [ ] Optional PTY/ConPTY terminal mode for important non-ACP CLIs.
- [ ] Evaluate ACP v2 and MCP-over-ACP after their draft features stabilize.
- [ ] Optional page-content capability with separate permissions.
- [ ] Add bounded active-viewport visual inspection without implicit activation;
      keep full-page/background debugger capture delegated.
- [ ] Later evaluate a debugger-capable extension manifest for non-active-tab and
      full-page capture; do not add `debugger` to the V1 core manifest.
- [ ] Under accepted ADR 0006, add one global Page tools grant and stage semantic
      `page.inspect` and typed `page.act`.
- [ ] Add globally enabled advanced `page.evaluate` only after typed page tools.

The detailed target contract and dependency plan are in
[`mcp-tools.md`](mcp-tools.md) and
[`mcp-tool-surface-plan.md`](mcp-tool-surface-plan.md). The code-grounded
continuation is
[`mcp-tool-surface-plan-part-2.md`](mcp-tool-surface-plan-part-2.md). Workflow
efficiency and final page-action schema gates are in
[`mcp-tool-surface-plan-part-3.md`](mcp-tool-surface-plan-part-3.md).
