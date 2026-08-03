# Agent guide

This file gives coding agents the repository-specific context needed to change
Effector safely. Read it before editing code or documentation.

## Product and current scope

Effector is a local-first control surface for a user's existing Google Chrome
session. A Manifest V3 extension reads browser metadata. A Chrome-launched Rust
Native Messaging broker exposes that data to MCP clients over authenticated
Streamable HTTP.

The implemented scope is intentionally narrow:

- Google Chrome and documented Chrome extension APIs only.
- Two read-only MCP tools: `browser.list` and `tabs.list`.
- Metadata reads that do not activate or wake discarded tabs.
- One active Chrome profile because the broker uses one fixed loopback port.
- No mutations, page-content access, CDP, ACP runtime, side-panel conversation
  UI, harness launcher, or production installer yet.
- Windows Chrome with an MCP client in WSL2 has been manually validated. macOS,
  broader client support, and restart behavior still need validation.

Do not describe roadmap features as implemented behavior.

## Repository map

- `Cargo.toml`: single Rust 2024 binary package.
- `native/src/main.rs`: CLI dispatch and Chrome native-host invocation handling.
- `native/src/broker.rs`: Native Messaging framing, request correlation, MCP
  HTTP server, authentication, and shutdown.
- `native/src/mcp.rs`: MCP tool schemas and extension request forwarding.
- `native/src/settings.rs`: loopback endpoint, state directory, and token
  creation and validation.
- `native/src/install.rs`: platform-specific Native Messaging registration.
- `native/src/doctor.rs`: authenticated health check through `browser.list`.
- `extension/manifest.json`: MV3 entry points and permissions.
- `extension/background.js`: native connection, request dispatch, and Chrome
  inventory implementation.
- `extension/popup.js`: direct local inventory UI and broker status.
- `tests/`: process-level Rust integration tests.
- `docs/`: architecture, decisions, tool contract, troubleshooting, progress,
  roadmap, and research.
- `target/`: generated build output; never treat it as source.

The extension has no build step, package manager, or runtime dependency. Chrome
loads `extension/` directly as an unpacked MV3 extension.

## Sources of truth

Use the source and tests as the final authority for executable behavior. Use
these documents for intent and operator guidance:

1. `README.md` for current setup and live validation.
2. `docs/mcp-tools.md` for the implemented MCP contract.
3. `docs/decisions/0004-broker-hosted-streamable-http.md` for the accepted
   transport architecture.
4. `docs/architecture/overview.md` and
   `docs/architecture/process-and-transport.md` for component boundaries and
   process ownership.
5. `docs/architecture/installation-and-packaging.md` and
   `docs/troubleshooting.md` for installation and recovery.
6. `docs/progress.md` for what exists now and `docs/roadmap.md` for proposed
   work.

The stdio-proxy designs in ADRs 0002 and 0003 are superseded by ADR 0004. ACP
research describes future integration, not the current runtime.

## Runtime architecture

1. The extension service worker calls
   `chrome.runtime.connectNative("com.effector.browser")`.
2. Chrome launches the registered Effector executable and owns its stdin and
   stdout.
3. The broker binds `http://127.0.0.1:37654/mcp` and completes the extension
   handshake.
4. An MCP client connects with the per-installation bearer token.
5. MCP tool calls receive a request ID and cross Native Messaging to the
   extension.
6. The extension calls documented Chrome APIs and returns a correlated result.
7. Native Messaging EOF is the broker's shutdown signal. The HTTP server and
   pending browser requests stop with that Chrome connection.

The popup's inventory action is a separate direct-extension path; it does not
call MCP.

## Non-negotiable invariants

- Chrome owns the broker lifetime. Do not add an always-on daemon or launch
  Chrome implicitly without an accepted architecture change.
- Broker stdout is reserved exclusively for length-prefixed Native Messaging
  JSON. Never log or print diagnostics there. Normal CLI subcommands may print
  to stdout; broker diagnostics belong on stderr.
- MCP stays on authenticated loopback Streamable HTTP. Preserve bearer, Host,
  and Origin validation. Never bind to a LAN, WSL gateway, or public address.
- Preserve native host name `com.effector.browser`, internal protocol version
  `1`, request IDs, and the `ready`, `ready_ack`, `request`, and `response`
  message shapes unless Rust, extension, tests, and docs change together.
- Respect Native Messaging limits: 1 MiB broker-to-Chrome and 64 MiB
  Chrome-to-broker. Keep inventory bounded and paginated; extension responses
  retain safety margin below the Chrome-to-broker limit.
- Preserve the bounded writer queue, browser request deadline, pending-request
  cleanup, and bounded graceful shutdown unless a reviewed design replaces
  them.
- Read tools must not activate, reload, attach to, or wake discarded tabs.
- `tabs.list` filters before pagination, defaults to 100 results, caps pages at
  250, and returns only windows and groups referenced by the current page.
- Pagination reads live browser state. Do not imply pages form a frozen snapshot
  until snapshot revisions exist.
- Chrome tab, window, and group numeric IDs are runtime-scoped. Retain
  `browserInstanceId`; never treat numeric IDs as durable across restarts.
- Current tools are read-only. Mutation work requires explicit product scope,
  target identity, stale-state handling, safety policy, tests, and docs.
- Keep Chrome permissions minimal. Do not add `debugger`, `scripting`, host
  permissions, page-content access, arbitrary Chrome API forwarding, or shell
  execution as shortcuts.
- Incognito access remains disabled. Separate incognito support requires an
  explicit identity, routing, fixed-port, privacy, and permission design.
- Tab Groups and future workspaces are organization and routing constructs, not
  authorization boundaries.

## Rust conventions

- Use Rust 2024 and standard `rustfmt` output.
- Use `anyhow::Result`, `Context`, and `bail!` for executable and internal error
  context. Convert failures to MCP tool errors at the MCP boundary.
- Avoid routine `unwrap` and `expect` in production code. They are acceptable in
  tests when a failure should panic the test.
- Prefer narrow visibility such as `pub(crate)`.
- Keep platform-specific installation code behind `#[cfg(target_os = ...)]`.
- MCP parameter structs derive `Deserialize`, `Serialize`, and `JsonSchema`, and
  use camelCase serialization. Tool names remain dotted names such as
  `browser.list`.
- Preserve bounded async channels, deadlines, cancellation, and pending-call
  cleanup when adding concurrent work.

## Extension conventions

- Keep the extension dependency-free and directly loadable as unpacked MV3
  source.
- Follow existing JavaScript style: two-space indentation, semicolons, `const`
  by default, camelCase identifiers, uppercase constants, and async/await.
- Return extension errors as structured `{code, message}` objects.
- Use DOM construction and `textContent` for untrusted values. Do not render raw
  browser or agent data as HTML.
- Keep tab normalization fields in `background.js` and `popup.js` synchronized
  when the two inventory paths expose the same data.
- Preserve the persistent installation UUID, runtime browser identity, and
  bounded native reconnect behavior.

## Validation commands

Run commands from the repository root.

Native-code validation gate:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

Focused integration tests:

```bash
cargo test --test broker_roundtrip
cargo test --test mcp_tools
cargo test --test full_mcp_roundtrip
```

CI uses its runner-provided Node installation for JavaScript syntax checks.
There is no repository-defined JavaScript formatter, linter, package manager,
or automated extension behavior test. Do not introduce Node as an extension
runtime requirement.

## Test expectations

- `tests/broker_roundtrip.rs` covers handshake, protocol mismatch, partial frame
  rejection, token creation, endpoint advertisement, and shutdown after
  simulated Native Messaging EOF.
- `tests/mcp_tools.rs` covers unauthorized HTTP rejection, MCP discovery, tool
  names, Host and Origin rejection, and listener shutdown.
- `tests/full_mcp_roundtrip.rs` covers MCP-to-Native-Messaging routing and the
  structured result path.
- New broker tests must use an ephemeral loopback port and isolated
  `EFFECTOR_STATE_DIR` or `EFFECTOR_MCP_TOKEN` values. Never touch the
  developer's real token, Chrome registration, registry, or config directory.
- Protocol and extension changes need process-level tests and, when Chrome API
  behavior matters, manual live-Chrome validation.

Known automated gaps include `tabs.list` filtering and pagination, `doctor`,
extension JavaScript behavior, reconnection, multiple-profile conflict
behavior, and platform registration.

## Stateful and live validation

`effector install`, Chrome registration, extension loading, and global MCP
configuration modify user state. Do not run or change them unless the task
requires it. Never edit MCP client configuration outside this repository unless
the user explicitly requests that change.

For a requested live validation:

1. Build the correct native executable for Chrome's operating system.
2. Load `extension/` unpacked and copy its exact 32-character extension ID.
3. Run that operating system's executable with
   `install --extension-id <extension-id>` from its stable final path.
4. Reload the extension or restart Chrome and require **Native broker
   connected** in the popup.
5. Run the matching operating system's `effector doctor` while Chrome remains
   open.
6. Start or restart the MCP client after the broker is connected.
7. Validate `browser.list`, then filters and pagination in `tabs.list`. Confirm
   discarded tabs remain discarded.

For Windows Chrome, install and run `doctor` with Windows `effector.exe` in
PowerShell. A Linux executable in WSL uses different registration and state.
An MCP client in WSL2 requires mirrored networking; default NAT does not share
Windows loopback. Do not solve WSL connectivity by weakening loopback binding.

The documented Windows cross-build requires a separately installed LLVM-MinGW
or MinGW-w64 toolchain. `cargo check --target x86_64-pc-windows-gnu` does not
invoke the linker and is not proof of a usable Windows executable. See
`README.md` and `docs/architecture/installation-and-packaging.md` before
cross-building.

## Security and privacy

- The MCP bearer token is secret. Never commit, quote, log, screenshot, or place
  a real token in documentation, tests, issues, or support output.
- Do not inspect or modify user-global MCP configuration unless explicitly
  required. Use placeholders in repository documentation.
- Inventory can contain titles, current and pending URLs, favicons, and browsing
  context. Redact it from logs and reports. Incognito access remains disabled.
- `.gitignore` excludes local reports, inventory dumps, runtime state, `.env`,
  and build output. Do not commit equivalent private data under another name.
- Preserve 64-character hexadecimal token validation and Unix `0600` creation.
- The native host manifest must allowlist the exact extension origin.
- Do not place credentials in extension storage, browser messages, popup state,
  source code, or transcripts.

## Documentation obligations

- Update `docs/progress.md` whenever implementation or validation state changes.
- Record durable architecture changes in a new ADR under `docs/decisions/`. Do
  not rewrite superseded ADR history.
- Update `docs/mcp-tools.md` for every tool, parameter, schema, pagination, or
  result change.
- Update `README.md` and `docs/troubleshooting.md` when setup, credentials,
  startup order, networking, or recovery changes.
- Update the relevant `docs/architecture/` files when process ownership,
  transport, security boundaries, installation, or failure behavior changes.
- Keep `docs/roadmap.md` prospective. Do not mark live or platform validation
  complete based only on mocks, `cargo check`, or cross-compilation.

## Current limitations

- Fixed port `37654` supports one active Chrome profile.
- MCP is unavailable while Chrome or the native connection is stopped.
- Clients must reconnect and initialize after broker restart.
- `tabs.list` returns page-local window and group context, not a frozen snapshot
  or complete hierarchy. Offset pagination can skip or repeat changing tabs.
- Field selection, snapshot revisions, event aggregation, and mutations do not
  exist.
- ACP, side panel, harness supervision, page content, CDP, production packaging,
  and a stable production extension ID do not exist.
- macOS registration and cross-platform restart behavior remain unvalidated.
