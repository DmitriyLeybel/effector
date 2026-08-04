import browserModel from "./browser-model.js";
import { createBackgroundController } from "./background-controller.js";

const MAX_PAGE_SIZE = 250;
const runtimeEpoch = crypto.randomUUID();
const installationIdPromise = loadInstallationId();

async function loadInstallationId() {
  const stored = await chrome.storage.local.get(["installationId", "browserInstanceId"]);
  let installationId = stored.installationId ?? stored.browserInstanceId;

  if (!installationId) {
    installationId = crypto.randomUUID();
  }
  if (stored.installationId !== installationId) {
    await chrome.storage.local.set({ installationId });
  }
  return installationId;
}

async function browserInstanceId() {
  return `${await installationIdPromise}:${runtimeEpoch}`;
}

async function dispatch(method, params, context) {
  switch (method) {
    case "browser.list":
      return readBrowserSummary(context.connectedAt);
    case "browser.snapshot":
      return browserModel.snapshot(await browserInstanceId());
    case "tabs.list":
      return readTabsPage(params);
    default:
      throw Object.assign(new Error(`Unknown browser method: ${method}`), {
        code: "METHOD_NOT_FOUND",
        effectorSafe: true
      });
  }
}

async function readBrowserSummary(connectedAt) {
  const [windows, groups, tabs, instanceId] = await Promise.all([
    chrome.windows.getAll({ populate: false }),
    chrome.tabGroups.query({}),
    chrome.tabs.query({}),
    browserInstanceId()
  ]);
  return {
    browserInstanceId: instanceId,
    extensionId: chrome.runtime.id,
    extensionVersion: chrome.runtime.getManifest().version,
    connectedAt,
    summary: {
      windowCount: windows.length,
      groupCount: groups.length,
      tabCount: tabs.length,
      discardedTabCount: tabs.filter((tab) => tab.discarded).length
    }
  };
}

async function readTabsPage(params) {
  const query = {};
  if (Number.isInteger(params.windowId)) query.windowId = params.windowId;
  if (params.activeOnly === true) query.active = true;
  if (params.discardedOnly === true) query.discarded = true;

  let tabs = await chrome.tabs.query(query);
  if (Number.isInteger(params.groupId)) {
    tabs = tabs.filter((tab) => tab.groupId === params.groupId);
  }

  const cursor = Math.max(0, Number.isInteger(params.cursor) ? params.cursor : 0);
  const limit = Math.min(
    MAX_PAGE_SIZE,
    Math.max(1, Number.isInteger(params.limit) ? params.limit : 100)
  );
  const page = tabs.slice(cursor, cursor + limit);
  const windowIds = [...new Set(page.map((tab) => tab.windowId))];
  const groupIds = [...new Set(page.map((tab) => tab.groupId).filter((id) => id !== -1))];
  const [allWindows, allGroups, instanceId] = await Promise.all([
    chrome.windows.getAll({ populate: false }),
    chrome.tabGroups.query({}),
    browserInstanceId()
  ]);

  return {
    browserInstanceId: instanceId,
    capturedAt: new Date().toISOString(),
    totalMatched: tabs.length,
    cursor,
    limit,
    nextCursor: cursor + page.length < tabs.length ? cursor + page.length : null,
    windows: allWindows
      .filter((window) => windowIds.includes(window.id))
      .map(normalizeWindow),
    groups: allGroups
      .filter((group) => groupIds.includes(group.id))
      .map(normalizeGroup),
    tabs: page.map(normalizeTab)
  };
}

function normalizeWindow(window) {
  return {
    id: window.id,
    focused: window.focused,
    top: window.top,
    left: window.left,
    width: window.width,
    height: window.height,
    incognito: window.incognito,
    type: window.type,
    state: window.state,
    alwaysOnTop: window.alwaysOnTop
  };
}

function normalizeGroup(group) {
  return {
    id: group.id,
    windowId: group.windowId,
    title: group.title,
    color: group.color,
    collapsed: group.collapsed,
    shared: group.shared
  };
}

function normalizeTab(tab) {
  return {
    id: tab.id,
    windowId: tab.windowId,
    index: tab.index,
    groupId: tab.groupId,
    title: tab.title,
    url: tab.url,
    pendingUrl: tab.pendingUrl,
    active: tab.active,
    highlighted: tab.highlighted,
    pinned: tab.pinned,
    audible: tab.audible,
    mutedInfo: tab.mutedInfo,
    status: tab.status,
    discarded: tab.discarded,
    frozen: tab.frozen,
    autoDiscardable: tab.autoDiscardable,
    incognito: tab.incognito,
    lastAccessed: tab.lastAccessed,
    openerTabId: tab.openerTabId,
    favIconUrl: tab.favIconUrl
  };
}

createBackgroundController(chrome, {
  browserInstanceId,
  browserModel,
  dispatch,
  userAgent: navigator.userAgent
});
