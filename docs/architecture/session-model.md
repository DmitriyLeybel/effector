# Session model

Status: proposed
Last updated: 2026-07-30

The word “session” is overloaded. Effector keeps five identities separate.

## Identity types

### Browser instance

A running Chrome profile with one installed extension instance.

```text
browserInstanceId = generated extension installation ID + runtime epoch
```

It owns windows, groups, and tabs. Numeric Chrome IDs are namespaced beneath
this browser instance.

### MCP connection/session

The protocol relationship between one MCP client and the broker's Streamable
HTTP endpoint. Its lifetime begins at MCP initialization and ends at explicit
deletion, broker shutdown, or inactivity expiry.

It is not a Chrome session and not necessarily a harness conversation.

### ACP agent process and session

The broker may supervise an ACP-compatible agent process. That ACP connection
can own one or more opaque agent session IDs. ACP is the preferred standard for
creating sessions, submitting prompts, streaming UI events, and requesting
permissions. MCP remains separate and does not define a universal API for
injecting turns into harness conversations.

All optional ACP lifecycle behavior, including list, load, resume, close, and
delete, is capability-gated. An opaque session ID is not evidence that the
agent can restore it after process exit.

### Effector workspace

A user-defined routing construct that may associate:

- One browser instance.
- A set of tab IDs or group IDs.
- Zero or more connected MCP clients.
- An optional harness adapter and opaque conversation ID.
- Pending user-originated messages with expiry and delivery status.

An Effector workspace is not a security boundary. Authorization is enforced by
broker connection policy, not by Chrome Tab Groups.

## Suggested records

```text
BrowserInstance {
  id, extensionVersion, chromeVersion, connectedAt, capabilities, revision
}

ClientConnection {
  id, transport, connectedAt, capabilities, selectedBrowserInstanceId
}

Workspace {
  id, name, browserInstanceId, tabRefs[], groupRefs[], clientIds[],
  harnessBinding?, createdAt, updatedAt
}

HarnessBinding {
  protocol, launchProfile, agentProcessId?, agentSessionId?, capabilities,
  state
}
```

## Resource assignment

Tab and group assignment is explicit broker state. A tab may be visible to
more than one workspace unless an exclusive lease is requested. Exclusive
leases are advisory coordination between trusted clients; they cannot stop the
user or another extension from changing Chrome state.

## Message delivery

Messages from a future side panel are routed to ACP `session/prompt` when an
active ACP binding exists. A bounded pending-message record may be used while
the agent is starting:

```text
PendingMessage {
  id, workspaceId, body, createdAt, expiresAt,
  targetAdapter, status, deliveryAttempts
}
```

Only a binding that advertises inbound prompt support may receive them. Generic
MCP notifications are not presented as user chat turns.

## Harness launch policy

The ACP client in the native broker can launch a compatible local agent. A
launch profile contains a fixed executable plus structured, validated
arguments. Browser-originated messages select a profile by ID and supply data
fields; they never supply an arbitrary command line.

Unsupported CLIs require an ACP adapter or a separately scoped PTY terminal
mode. Effector cannot attach to an unrelated already-running CLI unless that
CLI exposes a control API.
