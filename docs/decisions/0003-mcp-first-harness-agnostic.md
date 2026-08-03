# ADR 0003: Deliver MCP first and remain harness-agnostic

Status: accepted
Date: 2026-07-30

Transport note: ADR 0004 supersedes the local IPC and stdio-proxy clauses. The
MCP-first, harness-agnostic, Rust, and deferred-ACP decisions remain accepted.

## Context

Effector eventually intends to use ACP for user-to-agent communication from a
Chrome side panel. That front-channel work is not required for the first useful
product: an MCP-capable harness can already invoke browser tools if Effector
provides a normal local MCP server.

Adding harness launchers, chat adapters, or ACP session state to the first
release would increase scope without improving MCP interoperability.

The Native Messaging host must also be easy to distribute on Windows, macOS,
and Linux. Requiring a separately installed Node or Python runtime would make
that installation more fragile.

## Decision

- Implement the Chrome extension, Native Messaging broker, local IPC, and MCP
  stdio server first.
- Do not select, identify, launch, or special-case harness products in the MCP
  layer. A harness connects by configuring the `effector mcp` command as an
  ordinary local MCP server.
- Implement the native roles in Rust and distribute them as one executable with
  subcommands. This avoids a language-runtime prerequisite and leaves access to
  official Rust MCP and ACP SDKs.
- Keep browser routing and the internal broker protocol independent of MCP
  request framing.
- Reserve ACP as a later broker module with separate child-process streams. Do
  not add an ACP dependency or placeholder session behavior to the MCP MVP.
- Harness selectivity is out of scope. Browser-instance selection may be added
  when multiple Chrome profiles are supported because that is browser routing,
  not harness compatibility.

## Consequences

- The first integration surface is only two MCP tools: `browser.list` and a
  filtered, paginated `tabs.list`.
- Every standards-compatible MCP harness uses the same executable and tools.
- The extension launches the broker through `runtime.connectNative()`; the
  harness separately launches `effector mcp` over its own stdio.
- One small per-user native-host registration step remains unavoidable because
  Chrome extensions cannot register native applications themselves.
- ACP can later supervise agents in the same broker process without changing
  the extension-to-broker or MCP-proxy-to-broker boundaries.
- Cross-platform package signing, stable installation paths, upgrades, and
  uninstallation remain packaging work even though the runtime is one binary.
