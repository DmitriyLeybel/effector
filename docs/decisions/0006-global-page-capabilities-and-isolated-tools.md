# ADR 0006: Use global page capabilities and isolated page tools

Status: accepted
Date: 2026-08-03

## Context

Page semantics, screenshots, actions, and arbitrary evaluation require broader
Chrome permissions and expose more sensitive data and authority than tab-strip
inventory. Per-site prompts and per-client policy would add a second
authorization system, while the installation bearer already grants every MCP
session access to the capabilities the user enables.

Chrome's documented extension APIs can provide DOM-derived semantics and
actions, active-viewport capture, and isolated user-script evaluation without
the `debugger` permission. They cannot provide the same background/full-page
capture and canonical debugger data without accepting a much stronger,
non-optional permission.

## Decision

- Effector uses global extension controls for Browser changes, Page tools, and
  Advanced evaluation. All default to disabled on fresh installation and
  upgrade and can be enabled only through extension UI. Possession of the MCP
  bearer grants the same effective enabled capabilities to every authenticated
  MCP session; there is no per-client, per-origin, per-tab, or per-window policy
  in this slice.
- The extension persists the user's global toggle choices and reconciles them
  with current Chrome permissions and API support. The broker retains only the
  current effective capability snapshot and memory-only public references,
  snapshots, cursors, plans, and artifacts.
- Protocol version 2 reports explicit Browser changes, Page tools, and Advanced
  evaluation booleans in `ready`, with a monotonic capability revision, and
  sends a complete `capabilities_changed` state when effective capabilities
  change. The broker uses those booleans for tool discovery, but the extension
  rechecks permission and toggle state immediately before dispatch.
- Enabling Page tools is one global optional permission flow for `scripting`,
  `webNavigation`, and `<all_urls>`. Effector still limits targets to normal
  HTTP(S) pages, keeps incognito disabled, and rejects restricted, discarded,
  frozen, or stale documents rather than activating or waking them.
- Semantic inspection and structured element actions come from one fixed,
  packaged, dependency-free page agent in an isolated extension world. Its
  output is explicitly DOM-derived and bounded; it is not raw DOM, Chrome's
  canonical accessibility tree, or an arbitrary Chrome/DOM forwarding API.
- Element actions resolve only exact document and element references. V1 has no
  fallback locators, raw coordinates, trusted physical input, uploads, native
  dialogs, or browser-UI control.
- Visual inspection uses `chrome.tabs.captureVisibleTab` for only the current
  viewport of a tab that is already active in its own window. The window need
  not be focused. Effector never activates, focuses, scrolls, stitches, reloads,
  or wakes a target implicitly to capture it.
- Capture records and rechecks tab, document, activation, viewport, and scroll
  identity around the Chrome call. A changed identity rejects the image rather
  than returning an uncertain attribution. Full-page, element, and non-active
  tab capture are deferred.
- `page.act.inspectAfter` reuses the same semantic, visual, or combined inspect
  pipeline after one structured action and its bounded wait. A requested visual
  follow-up is preflighted before mutation. If the action succeeds but the
  inspection fails, the result preserves action success and reports a compact
  inspection error.
- Advanced evaluation is a separate global capability layered on Page tools. It
  uses optional `userScripts` and `chrome.userScripts.execute()` against an exact
  document in the isolated `USER_SCRIPT` world on Chrome 135 or later. Source,
  arguments, result, and wait time are bounded. Timeout is observation failure,
  not cancellation, and execution is never retried automatically.
- Main-world arbitrary evaluation is deferred. Runtime source is not passed to
  `scripting.executeScript()`.
- The V1 manifest does not request `debugger`. A debugger-capable manifest or
  extension variant requires a later ADR covering its non-optional install-time
  warning, attachment conflicts, packaging/identity, capture semantics, and
  overlap with Chrome DevTools MCP.

## Consequences

- The user makes a small number of understandable global authority decisions,
  and stale discovery cannot bypass extension-side checks.
- Page tools remain absent from MCP discovery while their effective global
  capability is disabled or unavailable. Clients that do not process tool-list
  changes must reconnect after a toggle changes.
- The broad host grant can expose sensitive rendered content across normal web
  origins, so page records and images are bounded, memory-only, and excluded
  from logs.
- DOM-derived semantics and synthetic DOM actions are useful but do not claim
  debugger-level fidelity or trusted user input.
- Active-viewport capture avoids implicit browser disturbance but requires an
  explicit browser change before inspecting a non-active tab visually.
- `inspectAfter` supports action-and-verification workflows without making
  `page.inspect` mutating or creating a second page representation.
- Arbitrary evaluation remains visibly stronger, version-gated, separately
  enabled, and capable of effects that may continue after timeout.

## Rejected alternatives

- Per-client or per-origin grants would create an authorization layer that the
  first slice does not need and could misleadingly imply isolation between
  equally trusted bearer holders.
- `activeTab` would not support the selected global page capability or reliable
  capture of an already-active tab in an unfocused window.
- Implicit activation, scrolling, or stitching would violate read behavior and
  disturb the user's browser session.
- Main-world evaluation through generic script injection would broaden page
  interference and bypass the selected isolated user-script boundary.
- Adding `debugger` to V1 would impose a powerful non-optional permission for
  workflows intentionally deferred from this slice.
