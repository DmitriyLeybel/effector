# Privacy

Effector is local-first software. The Chrome extension and native broker run on
the user's machine, and the broker accepts authenticated connections only on
the loopback interface.

## Data accessed

Effector can read metadata exposed by the documented Chrome extension APIs,
including:

- Window, Tab Group, and tab identifiers and organization.
- Tab titles, current and pending URLs, and favicon URLs.
- Active, highlighted, pinned, audible, muted, loading, frozen, and discarded
  state.
- Window position and state.
- Extension version, connection times, and a persistent random installation
  identifier used to distinguish browser runtimes.

Incognito access is disabled by the extension manifest. Effector does not read
page contents, cookies, form data, passwords, downloads, or browsing history.

## How data is used

The extension uses this metadata to display a local inventory and to answer the
read-only `browser.snapshot`, `browser.list`, and `tabs.list` tools requested by
an MCP client that the user configured with Effector's bearer token.

Effector does not send inventory or credentials to an Effector-operated server,
sell data, use data for advertising, or use data to train models. A configured
MCP client may process tool results under that client's own privacy terms.

## Storage and retention

The extension stores a random installation identifier and three global desired
capability settings in Chrome extension storage. Browser changes, Page tools,
and Advanced evaluation settings default to disabled; the current read-only build
does not allow Browser changes to be enabled and does not display Page controls.
The native application stores a random MCP bearer token in the user's Effector
configuration directory. `browser.snapshot` retains bounded browser baselines,
opaque references, and cursor state in broker memory for up to two minutes;
reads do not extend that lifetime. This inventory is never persisted and is
discarded on eviction, broker shutdown, or Chrome disconnection.

The popup can copy or download an inventory report only after an explicit user
action. Those reports contain sensitive browser metadata and remain wherever
the user places them.

## User control

Users can stop access by disabling or removing the extension, closing Chrome,
or removing the Native Messaging registration. Removing extension storage
deletes the installation identifier and capability settings. Removing the
Effector state directory deletes the MCP token. When future capabilities are
available, any authority enabled in extension UI applies to every MCP client
that possesses the installation bearer token; version one has no per-client
authorization layer.

## Reporting concerns

Use the repository's security reporting process for vulnerabilities. Do not
include bearer tokens, tab titles, URLs, or inventory reports in public issues.
