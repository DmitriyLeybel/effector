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

- [ ] Complete hierarchical window/group/tab reads beyond page-local context.
- [x] Tab filtering and pagination.
- [ ] Field selection.
- [x] Browser instance IDs.
- [ ] Snapshot revision IDs.
- [ ] Chrome event aggregation and resynchronization.
- [ ] Multiple Chrome-profile routing.

## Phase 3: controlled mutations

- [ ] Create, activate, move, pin, duplicate, discard, and close tabs.
- [ ] Create, focus, update, and close windows.
- [ ] Group, ungroup, move, rename, recolor, and collapse Tab Groups.
- [ ] Typed stale-state conflicts and dry-run support for bulk operations.

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
- [ ] Reconsider CDP only for a demonstrated gap that Chrome APIs cannot fill.
