# ADR 0004: Serve MCP directly from the Chrome-owned broker

Status: accepted
Date: 2026-08-02

## Context

The first transport spike used one Chrome-owned Native Messaging broker and one
stdio MCP proxy per harness. That preserved broad stdio compatibility but added
an internal protocol, a discovery file, another process per client, and an
extra serialization and authentication hop.

Effector's target clients may use any standard MCP transport. Streamable HTTP
allows the Chrome-owned process to retain exclusive ownership of Native
Messaging stdin/stdout while serving multiple MCP clients on a separate
loopback listener.

## Decision

- The Chrome-owned native host serves MCP Streamable HTTP directly.
- The default endpoint is `http://127.0.0.1:37654/mcp`.
- Every request requires a persistent per-installation bearer token.
- The server binds only to loopback and validates Host and Origin headers.
- Tool handlers forward browser requests in-process to the Native Messaging
  writer and pending-response map.
- Native Messaging EOF shuts down MCP sessions and the HTTP listener before the
  process exits.
- The stdio proxy, internal IPC listener, and broker discovery record are
  removed.
- One active Chrome profile is supported until fixed-port ownership and profile
  routing are designed.

## Consequences

- Chrome starts the only native runtime process automatically.
- Multiple HTTP MCP clients share one browser connection.
- Clients cannot initialize or discover tools while Chrome is not running.
- Clients must support Streamable HTTP and bearer headers.
- Restarted brokers require clients to reconnect and initialize new sessions.
- The installer must provision the persistent credential and print a suitable
  client configuration.

## Supersedes

This ADR supersedes the stdio-proxy and internal-IPC portions of ADR 0002 and
ADR 0003. Their browser-owned lifetime, Native Messaging, MCP-first scope,
harness neutrality, and Rust packaging decisions remain valid.
