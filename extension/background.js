const NATIVE_HOST = "com.effector.browser";
const INTERNAL_PROTOCOL_VERSION = 1;
const MAX_PAGE_SIZE = 250;
const MAX_RESPONSE_BYTES = 60 * 1024 * 1024;
const runtimeEpoch = crypto.randomUUID();
const installationIdPromise = loadInstallationId();

let nativePort = null;
let brokerReady = false;
let reconnectAttempt = 0;
let reconnectTimer = null;
let lastError = null;
let connectedAt = null;
let mcpEndpoint = null;

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

function connectBroker() {
  if (nativePort) return;
  clearTimeout(reconnectTimer);

  try {
    const port = chrome.runtime.connectNative(NATIVE_HOST);
    nativePort = port;
    brokerReady = false;
    lastError = null;
    connectedAt = new Date().toISOString();

    port.onMessage.addListener((message) => handleNativeMessage(port, message));
    port.onDisconnect.addListener(() => {
      if (nativePort !== port) return;
      lastError = chrome.runtime.lastError?.message ?? lastError ?? "Native broker disconnected";
      nativePort = null;
      brokerReady = false;
      connectedAt = null;
      mcpEndpoint = null;
      scheduleReconnect();
    });

    void sendReady(port).catch((error) => {
      if (nativePort === port) {
        lastError = error?.message ?? String(error);
        port.disconnect();
      }
    });
  } catch (error) {
    lastError = error?.message ?? String(error);
    nativePort = null;
    scheduleReconnect();
  }
}

function scheduleReconnect() {
  clearTimeout(reconnectTimer);
  const delay = Math.min(30_000, 500 * (2 ** reconnectAttempt));
  reconnectAttempt = Math.min(reconnectAttempt + 1, 6);
  reconnectTimer = setTimeout(connectBroker, delay);
}

async function sendReady(port) {
  const message = {
    type: "ready",
    protocolVersion: INTERNAL_PROTOCOL_VERSION,
    browserInstanceId: await browserInstanceId(),
    extensionId: chrome.runtime.id,
    extensionVersion: chrome.runtime.getManifest().version,
    userAgent: navigator.userAgent
  };
  if (nativePort === port) port.postMessage(message);
}

async function handleNativeMessage(port, message) {
  if (nativePort !== port) return;
  if (message?.type === "ready_ack") {
    if (
      message.protocolVersion !== INTERNAL_PROTOCOL_VERSION ||
      typeof message.mcpEndpoint !== "string" ||
      !message.mcpEndpoint
    ) {
      lastError = "Native broker returned an incompatible handshake";
      port.disconnect();
      return;
    }
    reconnectAttempt = 0;
    brokerReady = true;
    mcpEndpoint = message.mcpEndpoint;
    return;
  }
  if (
    message?.type !== "request" ||
    typeof message.requestId !== "string" ||
    typeof message.method !== "string"
  ) return;

  if (!brokerReady) {
    port.postMessage({
      type: "response",
      requestId: message.requestId,
      ok: false,
      error: {
        code: "protocol_not_ready",
        message: "Native broker handshake is not complete"
      }
    });
    return;
  }

  try {
    const result = await dispatch(message.method, message.params ?? {});
    if (nativePort === port) {
      const response = {
        type: "response",
        requestId: message.requestId,
        ok: true,
        result
      };
      if (new TextEncoder().encode(JSON.stringify(response)).byteLength > MAX_RESPONSE_BYTES) {
        throw Object.assign(new Error("Chrome response exceeded the safe message size"), {
          code: "response_too_large"
        });
      }
      port.postMessage(response);
    }
  } catch (error) {
    if (nativePort === port) {
      port.postMessage({
        type: "response",
        requestId: message.requestId,
        ok: false,
        error: {
          code: error?.code ?? "extension_error",
          message: error?.message ?? String(error)
        }
      });
    }
  }
}

async function dispatch(method, params) {
  switch (method) {
    case "browser.list":
      return readBrowserSummary();
    case "tabs.list":
      return readTabsPage(params);
    default:
      throw Object.assign(new Error(`Unknown browser method: ${method}`), {
        code: "method_not_found"
      });
  }
}

async function readBrowserSummary() {
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

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message?.type === "bridge.status") {
    sendResponse({
      connected: Boolean(nativePort && brokerReady),
      connectedAt,
      lastError,
      nativeHost: NATIVE_HOST,
      mcpEndpoint
    });
    return false;
  }
  if (message?.type === "bridge.reconnect") {
    reconnectAttempt = 0;
    connectBroker();
    sendResponse({ accepted: true });
    return false;
  }
  return false;
});

chrome.runtime.onStartup.addListener(connectBroker);
chrome.runtime.onInstalled.addListener(connectBroker);
connectBroker();
