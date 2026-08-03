# ADR 0002: Use a browser-owned native broker

Status: superseded by ADR 0004
Date: 2026-07-30

## Context

A Chrome extension cannot expose standard MCP stdio. Native Messaging gives
the extension a supported local-process channel and lets Chrome own the host
process lifetime. However, Chrome owns that process's stdin/stdout, so the same
streams cannot also serve MCP clients.

## Proposed decision

The transport portions below record the initial spike and are retained for
history. ADR 0004 replaces internal IPC and stdio proxies with broker-hosted
Streamable HTTP.

- The extension opens a long-lived Native Messaging port.
- Chrome launches a native broker host.
- The broker exposes authenticated user-local IPC.
- Each stdio MCP client launches an `effector mcp` proxy connected to the
  broker.
- The same broker may supervise ACP agents for the side panel, but each agent
  uses separate child-process streams; ACP never shares Chrome's Native
  Messaging stream or an MCP proxy's stdio.
- Streamable HTTP may be added after client compatibility is measured.

## Consequences

- No user-managed always-on daemon is required.
- Native-host registration still requires a small platform installer.
- Multi-harness routing becomes possible without sharing stdio.
- ACP can provide a standard side-panel-to-agent channel without changing the
  MCP browser-tool boundary.
- The broker and proxy need an internal protocol and discovery mechanism.
- Multiple Chrome profiles and native-host reconnects require explicit
  instance management.

## Validation required before acceptance

- Prove that the Native Messaging port remains stable across MV3 worker and
  Chrome restart scenarios.
- Prove broker discovery and proxy reconnection on Windows, macOS, and Linux.
- Test duplicate host processes and multiple Chrome profiles.
- Confirm the minimum supported Chrome version.
