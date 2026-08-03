# Contributing

Effector is an early-stage, security-sensitive bridge to a user's real browser.
Read `AGENTS.md`, `README.md`, and the relevant architecture decision before
changing behavior.

## Development setup

Prerequisites:

- A current stable Rust toolchain with `rustfmt` and Clippy.
- Google Chrome for optional live extension validation.
- No Node.js installation is required for the extension runtime; CI uses
  runner-provided Node only for JavaScript syntax checks.

Run the native validation gate from the repository root:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

## Change expectations

- Keep broker stdout reserved for Native Messaging frames.
- Preserve authenticated loopback-only MCP and minimal Chrome permissions.
- Add process-level tests for broker and protocol changes.
- Update `docs/mcp-tools.md` whenever a tool contract changes.
- Update `docs/progress.md` when implementation or validation state changes.
- Add a new ADR for durable architecture changes rather than rewriting accepted
  decision history.
- Treat browser metadata and installation tokens as sensitive. Never include
  real inventory or credentials in tests, commits, issues, or screenshots.

Live Chrome installation changes user state. Do not require it for ordinary
unit or integration tests, and document any manual validation performed.
