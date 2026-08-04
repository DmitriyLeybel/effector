const NO_GROUP_ID = -1;

function copyDefined(target, source, fields) {
  for (const field of fields) {
    if (source[field] !== undefined) target[field] = source[field];
  }
  return target;
}

function sameValue(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function queriedSupport(tabs, groups) {
  return {
    frozenTabs: tabs.some((tab) => Object.hasOwn(tab, "frozen")),
    sharedTabGroups: groups.some((group) => Object.hasOwn(group, "shared"))
  };
}

export function createBrowserModel(chromeApi, options = {}) {
  const randomUUID = options.randomUUID ?? (() => globalThis.crypto.randomUUID());
  const now = options.now ?? (() => new Date().toISOString());
  const drainEvents = options.drainEvents ?? (() => new Promise((resolve) => setTimeout(resolve, 0)));
  const keys = {
    windows: new Map(),
    groups: new Map(),
    tabs: new Map()
  };
  const forceNewIds = {
    windows: new Set(),
    groups: new Set(),
    tabs: new Set()
  };
  const supportListeners = new Set();

  let eventGeneration = 0;
  let reconciledGeneration = -1;
  let reconciliation = null;
  let records = null;
  let support = {
    frozenTabs: false,
    sharedTabGroups: false
  };
  let modelRevision = 0;

  function markDirty() {
    eventGeneration += 1;
    void reconcile().catch(() => {});
  }

  function markCreated(kind, id) {
    if (Number.isInteger(id)) {
      keys[kind].delete(id);
      forceNewIds[kind].add(id);
    }
    markDirty();
  }

  function markRemoved(kind, id) {
    if (Number.isInteger(id)) keys[kind].delete(id);
    markDirty();
  }

  chromeApi.windows.onCreated.addListener((window) => markCreated("windows", window?.id));
  chromeApi.windows.onRemoved.addListener((windowId) => markRemoved("windows", windowId));
  chromeApi.windows.onFocusChanged.addListener(markDirty);
  chromeApi.windows.onBoundsChanged.addListener(markDirty);

  chromeApi.tabs.onCreated.addListener((tab) => markCreated("tabs", tab?.id));
  chromeApi.tabs.onUpdated.addListener(markDirty);
  chromeApi.tabs.onMoved.addListener(markDirty);
  chromeApi.tabs.onAttached.addListener(markDirty);
  chromeApi.tabs.onDetached.addListener(markDirty);
  chromeApi.tabs.onActivated.addListener(markDirty);
  chromeApi.tabs.onHighlighted.addListener(markDirty);
  chromeApi.tabs.onRemoved.addListener((tabId) => markRemoved("tabs", tabId));
  chromeApi.tabs.onReplaced.addListener((addedTabId, removedTabId) => {
    keys.tabs.delete(removedTabId);
    keys.tabs.delete(addedTabId);
    forceNewIds.tabs.add(addedTabId);
    markDirty();
  });

  chromeApi.tabGroups.onCreated.addListener((group) => markCreated("groups", group?.id));
  chromeApi.tabGroups.onUpdated.addListener(markDirty);
  chromeApi.tabGroups.onMoved.addListener(markDirty);
  chromeApi.tabGroups.onRemoved.addListener((group) => {
    const groupId = Number.isInteger(group) ? group : group?.id;
    markRemoved("groups", groupId);
  });

  function keyFor(kind, id) {
    if (forceNewIds[kind].delete(id)) keys[kind].delete(id);
    if (!keys[kind].has(id)) keys[kind].set(id, randomUUID());
    return keys[kind].get(id);
  }

  function pruneKeys(kind, ids) {
    for (const id of keys[kind].keys()) {
      if (!ids.has(id)) keys[kind].delete(id);
    }
  }

  async function readChromeState() {
    const [windows, groups, tabs] = await Promise.all([
      chromeApi.windows.getAll({ populate: false }),
      chromeApi.tabGroups.query({}),
      chromeApi.tabs.query({})
    ]);
    return { windows, groups, tabs };
  }

  function normalize(source, currentSupport) {
    const windowIds = new Set(source.windows.map((window) => window.id));
    const groupIds = new Set(source.groups.map((group) => group.id));
    const tabIds = new Set(source.tabs.map((tab) => tab.id));

    pruneKeys("windows", windowIds);
    pruneKeys("groups", groupIds);
    pruneKeys("tabs", tabIds);

    for (const window of source.windows) keyFor("windows", window.id);
    for (const group of source.groups) keyFor("groups", group.id);
    for (const tab of source.tabs) keyFor("tabs", tab.id);

    const windows = source.windows.map((window) => copyDefined({
      key: keys.windows.get(window.id),
      id: window.id,
      focused: window.focused === true
    }, window, ["top", "left", "width", "height", "type", "state", "alwaysOnTop"]));

    const groups = source.groups.map((group) => {
      const normalized = copyDefined({
        key: keys.groups.get(group.id),
        id: group.id,
        windowKey: keys.windows.get(group.windowId),
        color: group.color,
        collapsed: group.collapsed === true
      }, group, ["title"]);
      if (currentSupport.sharedTabGroups) normalized.shared = group.shared === true;
      return normalized;
    });

    const tabs = source.tabs.map((tab) => {
      const normalized = copyDefined({
        key: keys.tabs.get(tab.id),
        id: tab.id,
        windowKey: keys.windows.get(tab.windowId),
        index: tab.index,
        active: tab.active === true,
        highlighted: tab.highlighted === true,
        pinned: tab.pinned === true,
        discarded: tab.discarded === true
      }, tab, [
        "title",
        "url",
        "pendingUrl",
        "audible",
        "status",
        "autoDiscardable",
        "lastAccessed",
        "favIconUrl"
      ]);
      if (currentSupport.frozenTabs) normalized.frozen = tab.frozen === true;
      if (tab.groupId !== undefined && tab.groupId !== NO_GROUP_ID && keys.groups.has(tab.groupId)) {
        normalized.groupKey = keys.groups.get(tab.groupId);
      }
      if (tab.mutedInfo?.muted !== undefined) normalized.muted = tab.mutedInfo.muted;
      if (Number.isInteger(tab.openerTabId) && keys.tabs.has(tab.openerTabId)) {
        normalized.openerKey = keys.tabs.get(tab.openerTabId);
      }
      return normalized;
    });

    return { windows, groups, tabs };
  }

  function apply(source) {
    const queried = queriedSupport(source.tabs, source.groups);
    const nextSupport = {
      frozenTabs: support.frozenTabs || queried.frozenTabs,
      sharedTabGroups: support.sharedTabGroups || queried.sharedTabGroups
    };
    const nextRecords = normalize(source, nextSupport);
    const supportChanged = !sameValue(support, nextSupport);

    if (!sameValue(records, nextRecords)) {
      records = nextRecords;
      modelRevision += 1;
    }
    support = nextSupport;
    if (supportChanged) {
      for (const listener of supportListeners) listener({ ...support });
    }
  }

  function reconcile() {
    if (reconciliation) return reconciliation;
    if (records && reconciledGeneration === eventGeneration) return Promise.resolve();

    reconciliation = (async () => {
      while (reconciledGeneration !== eventGeneration || !records) {
        const generationAtRead = eventGeneration;
        const source = await readChromeState();
        await drainEvents();
        if (generationAtRead !== eventGeneration) continue;
        apply(source);
        reconciledGeneration = generationAtRead;
      }
    })().finally(() => {
      reconciliation = null;
    });
    return reconciliation;
  }

  async function snapshot(browserInstanceId) {
    await reconcile();
    return {
      browserInstanceId,
      modelRevision,
      capturedAt: now(),
      supportsFrozenTabs: support.frozenTabs,
      supportsSharedTabGroups: support.sharedTabGroups,
      windows: clone(records.windows),
      groups: clone(records.groups),
      tabs: clone(records.tabs)
    };
  }

  async function getSupport() {
    await reconcile();
    return { ...support };
  }

  const initialized = reconcile();

  return {
    initialized,
    getSupport,
    onSupportChanged(listener) {
      supportListeners.add(listener);
      return () => supportListeners.delete(listener);
    },
    snapshot,
    whenIdle: reconcile
  };
}

const browserModel = globalThis.chrome ? createBrowserModel(globalThis.chrome) : null;

export default browserModel;
