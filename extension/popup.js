const runButton = document.querySelector("#run");
const copyButton = document.querySelector("#copy");
const downloadButton = document.querySelector("#download");
const statusElement = document.querySelector("#status");
const summaryElement = document.querySelector("#summary");
const outputElement = document.querySelector("#output");
const bridgeStatusElement = document.querySelector("#bridge-status");
const browserChangesCard = document.querySelector("#browser-changes-card");
const browserChangesToggle = document.querySelector("#browser-changes-toggle");
const browserChangesControlLabel = document.querySelector("#browser-changes-control-label");
const browserChangesStatus = document.querySelector("#browser-changes-status");

let latestReport = null;
let latestCapabilityState = null;
let capabilityRequestGeneration = 0;

const CAPABILITY_REASON_TEXT = Object.freeze({
  disabled: "Disabled by you.",
  permissionMissing: "A required Chrome permission is missing.",
  unsupported: "This Chrome version does not support Browser changes.",
  probeFailed: "Browser changes failed an availability check.",
  notImplemented: "Unavailable in this build. No browser mutation can be dispatched.",
  dependencyUnavailable: "A required capability is unavailable."
});

async function refreshBridgeStatus() {
  try {
    const status = await chrome.runtime.sendMessage({ type: "bridge.status" });
    bridgeStatusElement.classList.toggle("error", !status?.connected);
    bridgeStatusElement.textContent = status?.connected
      ? `Native broker connected${status.mcpEndpoint ? ` at ${status.mcpEndpoint}` : ""}.`
      : `Native broker unavailable${status?.lastError ? `: ${status.lastError}` : "."}`;
  } catch (error) {
    bridgeStatusElement.classList.add("error");
    bridgeStatusElement.textContent = `Bridge status failed: ${error?.message ?? String(error)}`;
  }
}

function renderBrowserChanges(state, errorMessage = null, pending = false) {
  latestCapabilityState = state ?? latestCapabilityState;
  const capability = latestCapabilityState?.capabilities?.browserChange;

  browserChangesCard.classList.toggle("error", Boolean(errorMessage));
  browserChangesCard.classList.toggle("enabled", capability?.effective === true);
  browserChangesCard.classList.toggle("unavailable", capability?.implemented !== true);
  browserChangesToggle.checked = capability?.desired === true;
  browserChangesToggle.disabled = pending || capability?.implemented !== true;

  if (pending) {
    browserChangesControlLabel.textContent = "Updating...";
    browserChangesStatus.textContent = "Saving the global Browser changes setting...";
    return;
  }
  if (errorMessage) {
    browserChangesControlLabel.textContent = "Unavailable";
    browserChangesStatus.textContent = errorMessage;
    return;
  }
  if (!capability) {
    browserChangesControlLabel.textContent = "Unavailable";
    browserChangesStatus.textContent = "Capability state is temporarily unavailable.";
    return;
  }

  browserChangesControlLabel.textContent = capability.effective
    ? "Enabled"
    : capability.desired ? "Requested" : "Disabled";
  browserChangesStatus.textContent = capability.effective
    ? "Enabled globally for every authenticated MCP client."
    : CAPABILITY_REASON_TEXT[capability.reason] ?? "Browser changes are unavailable.";
}

async function refreshCapabilities() {
  const generation = ++capabilityRequestGeneration;
  renderBrowserChanges(latestCapabilityState, null, true);
  try {
    const response = await chrome.runtime.sendMessage({ type: "capabilities.get" });
    if (generation !== capabilityRequestGeneration) return;
    if (!response?.ok) {
      renderBrowserChanges(null, response?.error?.message ??
        "Capability state is temporarily unavailable.");
      return;
    }
    renderBrowserChanges(response.state);
  } catch (_error) {
    if (generation === capabilityRequestGeneration) {
      renderBrowserChanges(null, "Capability state is temporarily unavailable.");
    }
  }
}

async function updateBrowserChanges() {
  const enabled = browserChangesToggle.checked;
  const generation = ++capabilityRequestGeneration;
  renderBrowserChanges(latestCapabilityState, null, true);
  try {
    const response = await chrome.runtime.sendMessage({
      type: "capabilities.setBrowserChanges",
      enabled
    });
    if (generation !== capabilityRequestGeneration) return;
    if (!response?.ok) {
      renderBrowserChanges(response?.state, response?.error?.message ??
        "Browser changes could not be updated.");
      return;
    }
    renderBrowserChanges(response.state);
  } catch (_error) {
    if (generation === capabilityRequestGeneration) {
      renderBrowserChanges(latestCapabilityState, "Browser changes could not be updated.");
    }
  }
}

function errorDetails(error) {
  return {
    name: error?.name ?? "Error",
    message: error?.message ?? String(error)
  };
}

async function capture(label, operation) {
  try {
    return { label, available: true, value: await operation() };
  } catch (error) {
    return { label, available: false, error: errorDetails(error) };
  }
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

function buildSummary(windows, groups, tabs) {
  return {
    windowCount: windows.length,
    tabCount: tabs.length,
    groupCount: groups.length,
    groupedTabCount: tabs.filter((tab) => tab.groupId !== -1).length,
    discardedTabCount: tabs.filter((tab) => tab.discarded).length,
    frozenTabCount: tabs.filter((tab) => tab.frozen).length
  };
}

async function readInventory() {
  runButton.disabled = true;
  copyButton.disabled = true;
  downloadButton.disabled = true;
  statusElement.classList.remove("error");
  statusElement.textContent = "Reading Chrome metadata…";

  try {
    const [tabsResult, windowsResult, groupsResult] = await Promise.all([
      capture("chrome.tabs.query", () => chrome.tabs.query({})),
      capture("chrome.windows.getAll", () =>
        chrome.windows.getAll({ populate: false })
      ),
      capture("chrome.tabGroups.query", () => chrome.tabGroups.query({}))
    ]);

    const tabs = (tabsResult.value ?? []).map(normalizeTab);
    const windows = (windowsResult.value ?? []).map(normalizeWindow);
    const groups = (groupsResult.value ?? []).map(normalizeGroup);
    const complete = [tabsResult, windowsResult, groupsResult]
      .every((result) => result.available);

    latestReport = {
      inventory: {
        name: "Effector Chrome inventory",
        version: chrome.runtime.getManifest().version,
        generatedAt: new Date().toISOString(),
        readOnly: true,
        complete
      },
      environment: {
        extensionId: chrome.runtime.id,
        userAgent: navigator.userAgent
      },
      calls: {
        tabs: tabsResult.available
          ? { label: tabsResult.label, available: true }
          : tabsResult,
        windows: windowsResult.available
          ? { label: windowsResult.label, available: true }
          : windowsResult,
        groups: groupsResult.available
          ? { label: groupsResult.label, available: true }
          : groupsResult
      },
      summary: buildSummary(windows, groups, tabs),
      windows,
      groups,
      tabs
    };

    outputElement.textContent = JSON.stringify(latestReport, null, 2);
    renderSummary(latestReport.summary);
    copyButton.disabled = false;
    downloadButton.disabled = false;
    statusElement.classList.toggle("error", !complete);
    statusElement.textContent = complete
      ? "Inventory complete. Chrome state was not modified."
      : "Partial inventory. One or more Chrome metadata reads failed.";
  } catch (error) {
    latestReport = null;
    summaryElement.hidden = true;
    outputElement.textContent = JSON.stringify({ error: errorDetails(error) }, null, 2);
    statusElement.classList.add("error");
    statusElement.textContent = `Inventory failed: ${error?.message ?? String(error)}`;
  } finally {
    runButton.disabled = false;
  }
}

function metric(label, value) {
  const element = document.createElement("div");
  element.className = "metric";

  const number = document.createElement("strong");
  number.textContent = String(value);
  const caption = document.createElement("span");
  caption.textContent = label;

  element.append(number, caption);
  return element;
}

function renderSummary(summary) {
  summaryElement.replaceChildren(
    metric("Windows", summary.windowCount),
    metric("Tabs", summary.tabCount),
    metric("Groups", summary.groupCount),
    metric("Discarded", summary.discardedTabCount)
  );
  summaryElement.hidden = false;
}

async function copyReport() {
  if (!latestReport) return;

  try {
    await navigator.clipboard.writeText(JSON.stringify(latestReport, null, 2));
    statusElement.textContent = "JSON copied to the clipboard.";
  } catch (error) {
    statusElement.classList.add("error");
    statusElement.textContent = `Copy failed: ${error?.message ?? String(error)}`;
  }
}

function downloadReport() {
  if (!latestReport) return;

  const blob = new Blob([JSON.stringify(latestReport, null, 2)], {
    type: "application/json"
  });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `effector-chrome-inventory-${Date.now()}.json`;
  anchor.click();
  URL.revokeObjectURL(url);
  statusElement.textContent = "JSON report downloaded.";
}

runButton.addEventListener("click", readInventory);
copyButton.addEventListener("click", copyReport);
downloadButton.addEventListener("click", downloadReport);
browserChangesToggle.addEventListener("change", updateBrowserChanges);
void Promise.all([refreshBridgeStatus(), refreshCapabilities()]);
