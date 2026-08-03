# Chrome tooling and extension-API coverage

Status: research snapshot
Last reviewed: 2026-07-30
Primary baselines: [Chrome DevTools for agents](https://developer.chrome.com/docs/devtools/agents/get-started), [Chrome DevTools MCP v1.6.0 tool reference](https://github.com/ChromeDevTools/chrome-devtools-mcp/blob/chrome-devtools-mcp-v1.6.0/docs/tool-reference.md), and the [Chrome Extensions API index](https://developer.chrome.com/docs/extensions/reference/api)

## Decision summary

No current official Google tool makes an extension-backed Effector redundant.
The products meet at tabs, navigation, and screenshots, but their centers of
gravity are different:

- **Chrome DevTools for agents owns the inspected page.** Its MCP server is the
  stronger tool for DOM interaction, accessibility snapshots, console and
  network inspection, Lighthouse, performance traces, emulation, screenshots,
  screencasts, and JavaScript heap analysis.
- **A Chrome extension owns the user's browser state.** It can represent real
  windows, exact tab-strip ordering, Tab Groups, pinned/muted/discarded/frozen
  state, recently closed sessions, and browser events without treating every
  tab as a renderer debugging target.
- **Effector should complement, not clone, DevTools.** Its default MCP surface
  should expose windows, groups, tabs, and passive browser-state events. Page
  debugging should be delegated to Chrome DevTools MCP. More sensitive profile
  APIs such as history, cookies, downloads, and settings should be separate,
  optional capability packs if they are ever added.

The Chrome Extensions reference currently links 85 API namespaces. That does
not mean Effector should publish 85 families of agent tools. Many namespaces
exist to implement an extension's own UI and lifecycle, and several are
ChromeOS-, enterprise-, kiosk-, policy-, or hardware-specific.

## Comparison boundary

This document compares documented public contracts, not everything that could
be built with their underlying primitives.

- **DevTools MCP coverage** means a named tool in the v1.6.0 tool reference.
  A custom `evaluate_script`, a private source-code hook, or a separately built
  raw-CDP client is recorded as a workaround, not native MCP coverage.
- **Extension coverage** means a documented Manifest V3 Chrome Extensions API,
  subject to its manifest permission, host permission, user gesture, Chrome
  version, platform, and policy restrictions.
- **Extension API coverage is not page access by default.** `chrome.tabs`
  controls browser tabs, but arbitrary DOM access requires host access plus
  `chrome.scripting`, temporary `activeTab`, a content script, or the powerful
  `chrome.debugger` permission.
- **`chrome.debugger` is not the whole of CDP.** It is an alternate CDP
  transport, but Chrome exposes only a documented set of protocol domains for
  security reasons; see the [Debugger API](https://developer.chrome.com/docs/extensions/reference/api/debugger).
- **The API index is grouped exhaustively by namespace below.** Method-level
  comparison is included for the browser-management namespaces that matter to
  Effector; enumerating every method of extension-internal and device-provider
  APIs would obscure the product boundary.

## Current official Chrome automation landscape

| Surface | What it is good at | Relationship to Effector |
| --- | --- | --- |
| [Chrome DevTools for agents](https://developer.chrome.com/docs/devtools/agents/get-started) | The official agent suite: MCP server, a narrower CLI, and agent skills. The v1.6.0 reference catalogs 51 tools across input, navigation, emulation, performance, network, debugging, memory, extension development, third-party tools, and WebMCP. | Primary comparison and intended companion. It can launch a managed profile or, with Chrome 144+ auto-connect and user approval, attach to a running profile. Its MCP page records contain an MCP page ID, URL, truncated title, selection state, and optional isolated-context name—not Chrome window/group/tab-strip metadata. |
| [Chrome DevTools Protocol (CDP)](https://chromedevtools.github.io/devtools-protocol/) | Low-level browser and renderer instrumentation organized into domains such as DOM, Runtime, Network, Target, Browser, Performance, and Tracing. Tip-of-tree changes frequently and carries no backward-compatibility guarantee. | A construction primitive, not a ready MCP contract. CDP can fill many DevTools gaps, but its targets and browser windows are not a full model of Chrome's user-facing tab strip or profile data. Effector's accepted scope currently excludes CDP. |
| [Puppeteer](https://pptr.dev/) | Chrome-team-maintained JavaScript library for launching or connecting to browsers and automating pages over CDP and WebDriver BiDi. | Useful for controlled automation sessions. It does not expose Chrome's Extensions API as an agent contract, and its `Page` model is not a Tab Group/session-management model. Chrome DevTools MCP itself uses Puppeteer. |
| [ChromeDriver](https://developer.chrome.com/docs/chromedriver/) | Standalone server implementing W3C WebDriver and WebDriver BiDi for local or remote automated testing. Chrome options can install extensions and configure a test session. | Test-session automation, not an installed bridge to browser-owned profile state. There is no official ChromeDriver MCP surface. |
| [Chrome DevTools Recorder](https://developer.chrome.com/docs/devtools/recorder/overview) | Records, replays, imports, and exports user flows, including Puppeteer formats and Lighthouse-enhanced flows. | Human-assisted workflow capture, not a general live browser-state API or MCP server. |
| [Chrome Extensions API](https://developer.chrome.com/docs/extensions/reference/api) | Permissioned access from an installed extension to browser UI/state, profile data, events, content injection, and selected OS/enterprise integrations. | Effector's privileged browser-side substrate. Native Messaging is still required to bridge this in-browser API to a local MCP process. |
| WebMCP and page-exposed developer tools | Lets a website expose its own semantic tools; DevTools MCP can list and invoke these behind experimental categories. | The page chooses the tools. It provides no general window, tab, group, or browser-profile management. |

The official suite now calls the overall product **Chrome DevTools for agents**;
`chrome-devtools-mcp` is its MCP component. Its experimental **Extensions**
category is for extension development: install an unpacked extension, list
installed extensions, reload one, trigger its action, or uninstall it. It does
not expose `chrome.tabs`, `chrome.windows`, or other extension namespaces to an
agent. In the pinned v1.6.0 contract, this category is disabled by default and
supported only for a pipe-launched browser, not auto-connect, `browserUrl`, or
`wsEndpoint` connections.

## Direct Chrome DevTools MCP gap matrix

### Browser-owned state where Effector adds coverage

| Capability | Chrome DevTools MCP v1.6.0 | Extension API | Effector opportunity |
| --- | --- | --- | --- |
| Identify connected browser/profile | One MCP server connects to or launches one browser/profile; no browser-instance inventory tool | One extension instance runs in a particular profile; runtime/incognito context is available | Stable installation/browser-instance ID and explicit profile routing |
| Enumerate windows | `list_pages` is a flat page list | `windows.getAll({populate:true})` returns window records and tabs | Exact browser → windows → groups/tabs hierarchy |
| Read window type, state, focus, bounds, incognito | Not in the public page record | `chrome.windows` exposes these fields | Typed, passive inventory |
| Create, focus, resize, minimize, maximize, fullscreen, or close windows | `resize_page` changes the selected page's window size; no general window CRUD tools | `windows.create()`, `update()`, and `remove()` | Native window tools with stale-ID checking |
| Read exact tab-strip order and window membership | Not exposed | `Tab.index` and `Tab.windowId` | Preserve physical order per window |
| Read Tab Groups and membership | Not exposed | `chrome.tabGroups`; every `Tab` carries `groupId` | List groups with title, color, collapsed state, and member tabs |
| Create/modify/move/destroy groups | Not exposed | `tabs.group()`, `tabs.ungroup()`, `tabGroups.update()`, `tabGroups.move()` | First-class group operations; group deletion is represented by ungrouping/closing its tabs |
| Read pinned, highlighted, audible, muted, opener, and auto-discardable state | Not exposed | `chrome.tabs.Tab` and `tabs.update()` | Passive read plus deliberate mutations |
| Read discarded/frozen state without attaching | No documented non-waking guarantee; page discovery initializes page objects and reads renderer-backed titles | `Tab.discarded` and `Tab.frozen` are browser metadata | Explicit `wake: false` inventory guarantee; never activate, reload, or attach during reads |
| Read last-accessed time | Not exposed | `Tab.lastAccessed` | Sorting and stale-tab workflows |
| Read pending navigation, load status, favicon, and incognito state | Page list exposes current URL/title only | `pendingUrl`, `status`, `favIconUrl`, and `incognito` | Full metadata with sensitive fields controlled by permission/policy |
| Use browser-native tab IDs | MCP page IDs are stable for the MCP process and change after reconnect; they are not Chrome tab IDs | `Tab.id` is stable for the Chrome browser session | Return browser-instance ID plus native tab/window/group ID |
| Move, pin, mute, duplicate, discard, highlight, or group a tab | No public tools | Direct `chrome.tabs` methods | Typed tab-strip mutations |
| Close the last tab/page | `close_page` refuses to close the final open page | `tabs.remove()` can close tabs; closing the final tab can close a window depending on Chrome behavior | Model the destructive consequence and require explicit intent |
| Recently closed tabs/windows | Not exposed | `chrome.sessions.getRecentlyClosed()` | Optional recent-session inventory |
| Restore a closed tab/window | Not exposed | `chrome.sessions.restore()` | Native restoration, which is more faithful than opening the old URL |
| Browser-state event stream | MCP responses refresh page listings, but there is no public window/group/tab event stream | Window, tab, group, and session namespaces publish lifecycle/update events | Revisioned snapshots plus bounded change feed/subscription |
| Multiple live Chrome profiles | Auto-connect chooses one profile; separate connections are needed | An extension installation runs per profile and incognito access is separately granted | Discover and route among connected Effector instances |
| Passive inventory at very large tab counts | Page tooling is oriented around live pages; no metadata-only or field-selection mode is documented | `tabs.query()` reads browser metadata without renderer automation | Pagination, filters, field selection, and no per-tab renderer work |

The source of `list_pages` confirms the public structured record contains only
`id`, `url`, `title`, `selected`, and optional `isolatedContext`; see
[`createStructuredPage`](https://github.com/ChromeDevTools/chrome-devtools-mcp/blob/chrome-devtools-mcp-v1.6.0/src/McpResponse.ts).
This is why a flat page list cannot reconstruct windows or groups.

### Capabilities the official MCP already handles better

| Capability | Chrome DevTools MCP | Normal extension path | Recommendation |
| --- | --- | --- | --- |
| Semantic page interaction | Accessibility snapshot plus click, hover, drag, fill, keyboard, dialog, upload, and wait tools | Content scripts or `scripting` with host access; Effector would need its own element identity and action model | Do not duplicate; compose with DevTools MCP |
| Arbitrary page evaluation | `evaluate_script` | `scripting.executeScript()` or `chrome.debugger`, both requiring broader trust | Defer from Effector core |
| Console inspection | List/get console messages, including extension service-worker filtering when extension tooling is enabled | Content scripts do not see the full DevTools console; debugger/devtools APIs are needed | Delegate |
| Request/response inspection | List/get page network requests and bodies | `webRequest` has a different permissioned event model; full response-body debugging generally needs `debugger` | Delegate |
| Performance and Core Web Vitals | Trace capture and DevTools Performance Insights | `chrome.debugger` tracing could be built, but this recreates DevTools | Delegate |
| Lighthouse | First-class accessibility, SEO, best-practices, and agentic-browsing audit | No equivalent general Extensions API | Delegate |
| JavaScript memory debugging | Heap capture, queries, retainers, dominators, paths, and comparisons | Requires debugger/CDP; `system.memory` is host memory, not a JS heap profiler | Delegate |
| Emulation | Viewport/device, geolocation, network, CPU, user agent, color scheme, and extra headers | Mostly debugger/CDP; extension settings APIs are not equivalent | Delegate |
| Screenshots and screencasts | Viewport/full-page/element screenshots; experimental video screencast | `captureVisibleTab`, `tabCapture`, or desktop capture have different scope and prompts | Delegate page artifacts; add tab capture only for a distinct product need |
| Extension development lifecycle | Experimental install/list/reload/trigger/uninstall tools and extension service-worker console data | `chrome.management` is powerful and cannot normally install arbitrary unpacked directories | Do not add extension-management tools merely to match it |
| Page-defined WebMCP/developer tools | Experimental discovery and execution | An Effector extension could relay them, but adds no browser-state advantage | Delegate |

## Method-level browser-management comparison

### `chrome.tabs`

The [Tabs API](https://developer.chrome.com/docs/extensions/reference/api/tabs)
is much broader than the name suggests. Sensitive `url`, `pendingUrl`, `title`,
and `favIconUrl` fields require the `tabs` permission or matching host access;
many tab mutations themselves require no `tabs` permission.

| Tabs capability | Public DevTools MCP | Recommended Effector posture |
| --- | --- | --- |
| `get`, `query` and full `Tab` metadata | Partial: flat pages with ID/URL/title/selected only | Core read model |
| `create` | Yes: `new_page`, including background creation and automation-only isolated contexts | Core; target a real window and optionally an index/opener |
| `update` URL/active/highlighted/pinned/muted/auto-discardable/opener | Partial: navigation and activation only | Core for active, pinned, muted, and navigation; evaluate lesser-used fields explicitly |
| `move` | No | Core |
| `duplicate` | No | Core |
| `discard` | No | Core, with clear unload semantics |
| `remove` | Yes, except the MCP refuses its last page | Core with confirmation hooks |
| `reload`, `goBack`, `goForward` | Yes | Expose only if browser-management workflows need a single Effector endpoint; otherwise avoid duplicating DevTools |
| `group`, `ungroup` | No | Core |
| `highlight` | No | Optional; multi-selection is real tab-strip state but lower priority |
| `detectLanguage` | No dedicated tool | Optional, low priority |
| `captureVisibleTab` | `take_screenshot` is stronger for normal page screenshots | Do not duplicate by default |
| `connect`, `sendMessage` to content scripts | No generic equivalent | Internal extension plumbing unless a separately permissioned page capability is designed |
| Zoom getters/setters and settings | No | Optional browser-state feature, not initial core |
| Tab create/update/move/attach/detach/remove/replace/zoom events | No public event feed | Core change-feed inputs |

The `Tab` object supplies the crucial fields a page abstraction omits:
`windowId`, `index`, `groupId`, `active`, `highlighted`, `pinned`, `audible`,
`mutedInfo`, `autoDiscardable`, `discarded`, `frozen`, `lastAccessed`,
`openerTabId`, `pendingUrl`, `status`, `incognito`, favicon, dimensions, and
session-scoped identity. Some fields are optional or version-dependent.

### `chrome.windows`, `chrome.tabGroups`, and `chrome.sessions`

| Namespace | Documented operations/events | Public DevTools MCP | Recommended Effector posture |
| --- | --- | --- | --- |
| [`chrome.windows`](https://developer.chrome.com/docs/extensions/reference/api/windows) | Get current/last/all/specific windows; create, update, remove; bounds/created/focus/removed events | No general equivalent | Core |
| [`chrome.tabGroups`](https://developer.chrome.com/docs/extensions/reference/api/tabGroups) | Get/query/update/move groups; created/moved/removed/updated events | None | Core |
| [`chrome.sessions`](https://developer.chrome.com/docs/extensions/reference/api/sessions) | Query recently closed and synced-device sessions, restore a session, observe recent-session changes | None | Optional module; begin with recently closed local state and make sync/privacy behavior explicit |

Neither the normal Tab Groups API nor DevTools MCP provides a complete API for
Chrome's saved-but-not-open Tab Groups. Effector should not promise that state
unless Chrome adds a documented API.

## Complete Extensions API namespace map

Every namespace currently linked from the official API index appears once in
this table. “MCP overlap” refers to the public Chrome DevTools MCP tool contract,
not possible raw CDP commands.

| Capability family | Extension namespaces | Chrome DevTools MCP overlap | Effector disposition |
| --- | --- | --- | --- |
| Live browser organization | `tabs`, `tabGroups`, `windows`, `sessions` | Partial pages/navigation only; no browser hierarchy, groups, rich tab state, recent sessions, or events | **Core differentiator** |
| Personal browser data | `bookmarks`, `history`, `topSites`, `readingList`, `downloads` | None as browser-profile stores; a page may initiate and DevTools may observe a download | Separate opt-in modules only; high privacy and destructive-action sensitivity |
| Site data, policy, and privacy settings | `browsingData`, `contentSettings`, `cookies`, `privacy`, `proxy` | Page/network debugging overlaps conceptually, but no equivalent profile-wide public tools | Exclude from core; consider narrow, optional tools only with a demonstrated use case |
| Page observation and intervention | `debugger`, `scripting`, `userScripts`, `webNavigation`, `webRequest`, `declarativeNetRequest`, `declarativeContent`, `pageCapture`, `tabCapture`, `desktopCapture`, `dom`, `dns` | Strong overlap for page input, DOM/a11y, network, debugging, emulation, screenshots, and screencast | Delegate to DevTools; any Effector content/capture pack must be separately permissioned |
| DevTools-extension integration | `devtools.inspectedWindow`, `devtools.network`, `devtools.panels`, `devtools.performance`, `devtools.recorder` | DevTools MCP directly exposes selected portions of the same debugging system | Not an Effector agent API; only relevant if Effector ships a DevTools panel |
| Extension product surfaces | `action`, `commands`, `contextMenus`, `omnibox`, `sidePanel`, `notifications`, `search` | `trigger_extension_action` only in the experimental extension-development category | Use internally for Effector UX; do not mirror as arbitrary MCP calls |
| Extension execution and plumbing | `alarms`, `events`, `extension`, `extensionTypes`, `i18n`, `offscreen`, `permissions`, `runtime`, `storage`, `storage.StorageArea`, `types` | None | Internal implementation surface, not browser-control tools |
| Identity, messaging, and user-presence state | `gcm`, `instanceID`, `identity`, `idle`, `loginState` | None | Exclude; unrelated and likely to expand privacy/authentication scope |
| Extension/application management | `management`, `mimeHandler` | Experimental install/list/reload/trigger/uninstall overlaps part of `management` | Exclude from browser-management MCP; DevTools already owns extension-development workflows |
| Accessibility and presentation settings | `accessibilityFeatures`, `fontSettings`, `tts`, `ttsEngine` | Lighthouse/a11y-tree tools inspect a page, not the user's Chrome settings or system speech | Exclude by default; page accessibility belongs to DevTools |
| Host/process/system state | `processes`, `power`, `system.cpu`, `system.display`, `system.memory`, `system.storage`, `systemLog` | JS heap and viewport tools are not equivalent | Exclude; several APIs are platform/channel restricted and reveal host details |
| OS, device, and provider integration | `audio`, `certificateProvider`, `documentScan`, `fileBrowserHandler`, `fileSystemProvider`, `input.ime`, `platformKeys`, `printerProvider`, `printing`, `printingMetrics`, `vpnProvider`, `wallpaper`, `webAuthenticationProxy` | None | Out of product scope; many require ChromeOS, kiosk, policy, or provider-specific deployment |
| Enterprise integration | `enterprise.deviceAttributes`, `enterprise.hardwarePlatform`, `enterprise.login`, `enterprise.networkingAttributes`, `enterprise.platformKeys` | None | Out of scope; managed-device only and inappropriate for a general harness bridge |

The API index spells nested namespaces with slashes (for example
`system/cpu`) while JavaScript uses dots (`chrome.system.cpu`). Likewise, the
linked `storage/StorageArea` page documents the `chrome.storage.StorageArea`
type rather than a separately permissioned top-level service. These supporting
reference entries are included so the 85-entry audit is reproducible.

The namespace index is not the whole extension platform. Manifest-defined
capabilities such as background service workers, content scripts,
`host_permissions`, `activeTab`, `externally_connectable`, and web-accessible
resources are also relevant. Native Messaging is exposed through
`chrome.runtime.connectNative()` and `sendNativeMessage()` plus the
`nativeMessaging` permission rather than a `chrome.nativeMessaging` namespace.
Effector uses these as implementation and transport primitives; they are not
additional general-purpose agent tools.

## Recommended Effector MCP boundary

### Default browser-state module

Expose a small, typed surface backed by `tabs`, `tabGroups`, and `windows`:

- Inventory browser instances, windows, groups, and tabs with native IDs and a
  snapshot revision.
- Filter and page large inventories without touching renderer content.
- Create/update/close/focus windows.
- Create/activate/move/pin/mute/duplicate/discard/reload/close tabs.
- Group/ungroup tabs and update/move groups.
- Return changes since a revision or expose a bounded event subscription.
- Make passive reads explicitly non-waking and never attach a debugger as a
  side effect.

An MCP response should preserve the hierarchy rather than forcing clients to
join flat lists:

```text
BrowserInstance
└── Window {id, type, state, focused, bounds, incognito}
    ├── TabGroup {id, title, color, collapsed}
    │   └── Tab {id, index, state, url/title policy}
    └── ungrouped Tab
```

### Optional browser-data modules

Add only after explicit product decisions and runtime permission UX:

- `sessions`: recently closed inventory and restoration.
- `bookmarks` and `readingList`: personal saved-page workflows.
- `downloads`: status and deliberate user-approved operations.
- `history` or `topSites`: only if their value justifies exposing sensitive
  browsing history.

Cookies, broad content settings, browsing-data deletion, proxy control,
extension management, identity, and enterprise/device APIs should not arrive
through a generic “call any Chrome API” escape hatch.

### Delegated page-debugging module

Recommend that harnesses install Chrome DevTools for agents alongside Effector:

```text
Effector MCP                 Chrome DevTools MCP
browser-owned state         renderer-owned state
windows/groups/tabs         DOM/a11y/input
passive discarded metadata  console/network
recent sessions/events      Lighthouse/performance/memory
profile-instance routing    emulation/screenshots/evaluation
```

Cross-tool handoff should use URL plus best-effort tab identity. Chrome
DevTools MCP's public page ID and Chrome's `Tab.id` are different identity
spaces, so an agent may need to match by URL/title and then confirm the chosen
page. A future explicit interop mechanism would be preferable to pretending
the IDs are interchangeable.

## Permissions and trust consequences

| Capability | Typical extension authority | Product consequence |
| --- | --- | --- |
| Full URL/title/favicon inventory | `tabs` permission or matching host access | Chrome presents a browsing-history warning; reports are sensitive even though this is not full history access |
| Tab Group management | `tabGroups` | Chrome presents a group-management warning |
| Window metadata/control | No separate `windows` permission; sensitive populated tab fields still depend on tab/host access | Low incremental permission cost, but closing windows is destructive |
| Recently closed sessions | `sessions`, with `tabs` affecting sensitive fields | Reveals closed and possibly synced session data; separate module recommended |
| Page DOM/script access | `scripting` plus host permissions, or temporary `activeTab` | Broad site-data access; keep outside core |
| CDP-equivalent debugging | `debugger` | Extremely powerful, cannot be requested as an optional permission, and overlaps the official MCP |
| Cookies/history/downloads/settings | Namespace permission plus host access where required | High privacy/destructive impact; runtime opt-in and narrow schemas required |

The [Permissions API](https://developer.chrome.com/docs/extensions/reference/api/permissions)
supports introducing many permissions at runtime, but notably does not allow
`debugger` or `declarativeNetRequest` to be optional. That strengthens the case
for leaving debugging to the separately approved DevTools connection.

## Gaps that remain even with both tools

- Complete saved-but-closed Tab Group inventory is not documented in the
  normal Extensions API.
- Neither tool offers durable tab identity across a Chrome restart.
- Neither can see every profile through one Chrome connection; profiles remain
  separate browser/extension instances.
- Normal extension APIs cannot inspect most browser-internal pages or automate
  arbitrary Chrome toolbar/menu UI.
- Incognito access remains explicit and separated, and some extension APIs are
  unavailable there.
- DevTools attachment and extension metadata reads have different side effects;
  only Effector can make a product-level non-waking inventory guarantee.
- Both surfaces are versioned. Effector must negotiate Chrome version and its
  own protocol version rather than assuming every documented field exists.

## Validation checklist

Re-run this comparison when any of the following changes:

1. The Chrome DevTools MCP tool reference adds windows, tab groups, rich tab
   metadata, recent sessions, or browser-state events.
2. Chrome changes auto-connect/profile routing or adds a supported native tab
   ID to the public MCP contract.
3. The Chrome Extensions API adds saved Tab Group or workspace/session APIs.
4. Effector proposes `scripting`, host permissions, `debugger`, history,
   cookies, downloads, or browser-setting permissions.
5. Chrome adds or removes API namespaces, or a relevant field raises the
   required minimum Chrome version.
