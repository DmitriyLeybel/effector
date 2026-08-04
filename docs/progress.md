# Progress

Last updated: 2026-08-03

## Completed

- [x] Selected the project name **Effector**.
- [x] Limited the current scope to Chrome and documented Chromium APIs.
- [x] Renamed the project to **Effector**.
- [x] Removed browser-vendor probes and private browsing dumps.
- [x] Generalized the extension into a read-only Chrome inventory prototype.
- [x] Captured Playwright MCP browser-management gaps.
- [x] Compared official Chrome agent tooling with the complete Chrome
      Extensions API namespace surface and defined Effector's complementary
      coverage boundary.
- [x] Reviewed the initial Native Messaging/MCP architecture.
- [x] Researched ACP as the preferred side-panel-to-agent protocol and
      documented its display, lifecycle, security, and MCP integration model.
- [x] Accepted an MCP-first, harness-agnostic delivery plan with ACP deferred.
- [x] Selected Rust and created one native executable with broker, MCP,
      installer, and diagnostics modes.
- [x] Added the MV3 service worker and Native Messaging bridge connection.
- [x] Implemented the initial authenticated loopback IPC and stdio transport
      spike, then superseded it with broker-hosted Streamable HTTP.
- [x] Added `browser.list` and filtered, paginated `tabs.list` MCP tools.
- [x] Added integration tests for authenticated HTTP discovery, direct
      MCP-to-Native-Messaging routing, and broker shutdown after simulated
      Native Messaging EOF.
- [x] Added protocol-version rejection, Host and Origin rejection, bearer
      challenge, atomic token creation, and Unix token-file safety coverage.
- [x] Bounded total queued and in-flight browser requests under one deadline and
      supervised reader, writer, and HTTP task shutdown.
- [x] Consolidated MCP into the Chrome-owned process at a fixed authenticated
      loopback endpoint.
- [x] Rebuilt the HTTP broker for Windows with LLVM-MinGW, deployed it to a
      stable per-user path, and refreshed the Native Messaging registration.
- [x] Cross-compiled and launched the Windows x86-64 executable from WSL.
- [x] Registered the native host and manually validated the live broker path
      against Windows Chrome.
- [x] Enabled WSL2 mirrored networking and manually validated `browser.list`,
      filtered and paginated `tabs.list`, and discarded-tab filtering from
      an MCP client in WSL against Windows Chrome.
- [x] Created modular architecture, decision, research, and planning docs.
- [x] Added public repository license, privacy, security, contribution, CI, and
      dependency-update files.
- [x] Disabled incognito access, enforced both sides of the native handshake,
      bounded response bytes, and normalized public window and group fields.
- [x] Specified a simplified, token-conscious five-tool MCP surface and
      comprehensive phased implementation plan without describing future
      capabilities as implemented.
- [x] Accepted ADRs 0005 and 0006 for the target browser identity, snapshot,
      mutation-plan, global page-permission, and isolated page-tool foundations.
- [x] Upgraded the extension/broker boundary to strict protocol version 2 with
      browser identity, capability revisions, typed errors, bounded requests,
      and visible mixed-version failure.
- [x] Added an event-reconciled extension browser model with random object
      incarnations and implemented `browser.snapshot` counts, compact/full
      projections, opaque references, filters, and immutable cursor pages.
- [x] Moved `effector doctor` to privacy-safe snapshot counts and added Rust
      process tests plus dependency-free extension protocol/model tests.
- [x] Drafted the Part 2 execution plan from Part 1 implementation learnings,
      including foundation closure, protocol evolution, browser changes, Page
      tools, validation, and legacy removal milestones.
- [x] Accepted ADR 0007 and upgraded the matched extension/broker boundary to
      strict protocol v3 with exact ABI and implementation negotiation, rich
      capability facts, typed dispatch state, and a bounded PNG artifact shape.
- [x] Extracted the production background controller behind dependency injection
      and added ready, reconnect, stale-port, duplicate-request, and status tests.
- [x] Drafted the Part 3 workflow-efficiency overlay, preserving the five-tool
      surface while gating destructive clarity, activation recovery, bounded
      page-action sequences, automatic waits, and token trajectories on evidence.
- [x] Added a standalone pinned-tokenizer trajectory harness and frozen synthetic
      P3.0 corpus. G3.0/G3.1 pass; bounded `page.act` sequences fail G3.2 and are
      rejected for V1, leaving singular action plus `inspectAfter` and explicit
      inspection loops.

## Current state

- The extension can read windows, Tab Groups, and tabs through its popup or the
  native broker without activating discarded tabs.
- The native executable builds and its direct MCP HTTP routing tests pass on
  Linux, and the release executable cross-links for Windows through LLVM-MinGW.
- A development installer registers the current binary for Chrome on Linux,
  macOS, and Windows code paths; Linux and Windows have been exercised locally.
- The installed Windows HTTP broker, bearer authentication, Native Messaging
  extension path, and `browser.list` result are validated against live Chrome.
- Tool discovery and live calls from an MCP client in WSL to Windows Chrome are
  validated with WSL2 mirrored networking.
- MCP clients now connect to `http://127.0.0.1:37654/mcp` with the persistent
  bearer credential printed by `effector install`.
- No ACP client, agent supervisor, or side-panel conversation UI exists.
- `browser.snapshot` is implemented alongside migration-only `browser.list` and
  `tabs.list`. Browser changes and all page tools remain unimplemented.
- Part 3 freezes destructive preview metadata, compact activation recovery, and
  250-millisecond best-effort automatic DOM quiet for their future owning
  branches. None is currently advertised.
- Protocol v3 and snapshot behavior pass automated Rust tests. The
  dependency-free extension suite includes model, protocol-v3, and background
  controller tests; the new JavaScript tests still need to run in an environment
  with Node. Live Chrome validation is still required across supported platforms
  and restart paths.
- ADR 0004 records the accepted HTTP transport. Cross-platform restart and
  multiple-profile behavior still need validation.

## Next validation

- Test Chrome, native-host, and MCP-client restart and reconnection behavior.
- Validate the native registration path on macOS.
- Validate fixed-port and bearer-header support in additional MCP clients.
- Add automated integration coverage for `tabs.list` filtering and pagination.
- Run the expanded dependency-free extension suite with Node.
- Live-validate `browser.snapshot` ordering, immutable pagination, unsupported
  fields, and no-disturbance guarantees with real Chrome.

## Working rule

Update this file when implementation state changes. Record durable architecture
choices in an ADR rather than burying them in progress notes.
