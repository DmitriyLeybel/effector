import assert from "node:assert/strict";
import test from "node:test";

import { createBrowserModel } from "../browser-model.js";

class MockEvent {
  constructor() {
    this.listeners = [];
  }

  addListener(listener) {
    this.listeners.push(listener);
  }

  emit(...args) {
    for (const listener of this.listeners) listener(...args);
  }
}

function deferred() {
  let resolve;
  const promise = new Promise((complete) => {
    resolve = complete;
  });
  return { promise, resolve };
}

function copy(value) {
  return JSON.parse(JSON.stringify(value));
}

function createMockChrome(initialState, baselineGate = null) {
  const state = copy(initialState);
  const calls = [];
  let reads = 0;

  async function read(name, value) {
    calls.push(name);
    reads += 1;
    const captured = copy(value);
    if (baselineGate && reads <= 3) await baselineGate.promise;
    return captured;
  }

  const chromeApi = {
    windows: {
      getAll: (query) => {
        assert.deepEqual(query, { populate: false });
        return read("windows.getAll", state.windows);
      },
      onCreated: new MockEvent(),
      onRemoved: new MockEvent(),
      onFocusChanged: new MockEvent(),
      onBoundsChanged: new MockEvent()
    },
    tabs: {
      query: (query) => {
        assert.deepEqual(query, {});
        return read("tabs.query", state.tabs);
      },
      create: () => calls.push("tabs.create"),
      update: () => calls.push("tabs.update"),
      reload: () => calls.push("tabs.reload"),
      discard: () => calls.push("tabs.discard"),
      onCreated: new MockEvent(),
      onUpdated: new MockEvent(),
      onMoved: new MockEvent(),
      onAttached: new MockEvent(),
      onDetached: new MockEvent(),
      onActivated: new MockEvent(),
      onHighlighted: new MockEvent(),
      onRemoved: new MockEvent(),
      onReplaced: new MockEvent()
    },
    tabGroups: {
      query: (query) => {
        assert.deepEqual(query, {});
        return read("tabGroups.query", state.groups);
      },
      update: () => calls.push("tabGroups.update"),
      onCreated: new MockEvent(),
      onUpdated: new MockEvent(),
      onMoved: new MockEvent(),
      onRemoved: new MockEvent()
    }
  };

  return { chromeApi, state, calls };
}

function uuidSequence() {
  let next = 0;
  return () => `key-${++next}`;
}

test("snapshot has the complete normalized shape and source ordering", async () => {
  const mock = createMockChrome({
    windows: [
      {
        id: 2,
        focused: true,
        top: 10,
        left: 20,
        width: 1200,
        height: 800,
        type: "normal",
        state: "normal",
        alwaysOnTop: false
      },
      { id: 1, focused: false }
    ],
    groups: [
      {
        id: 7,
        windowId: 2,
        title: "Work",
        color: "blue",
        collapsed: false,
        shared: false
      }
    ],
    tabs: [
      {
        id: 10,
        windowId: 2,
        index: 1,
        groupId: 7,
        title: "Second",
        url: "https://second.example/",
        active: false,
        highlighted: false,
        pinned: true,
        audible: false,
        mutedInfo: { muted: true },
        status: "complete",
        discarded: false,
        frozen: false,
        autoDiscardable: true,
        lastAccessed: 123,
        favIconUrl: "https://second.example/icon.png"
      },
      {
        id: 11,
        windowId: 2,
        index: 0,
        groupId: -1,
        pendingUrl: "https://first.example/",
        active: true,
        highlighted: true,
        pinned: false,
        discarded: true,
        frozen: true,
        openerTabId: 10
      }
    ]
  });
  const model = createBrowserModel(mock.chromeApi, {
    randomUUID: uuidSequence(),
    now: () => "2026-08-03T12:00:00.000Z"
  });

  const snapshot = await model.snapshot("browser-1");

  assert.deepEqual(snapshot, {
    browserInstanceId: "browser-1",
    modelRevision: 1,
    capturedAt: "2026-08-03T12:00:00.000Z",
    supportsFrozenTabs: true,
    supportsSharedTabGroups: true,
    windows: [
      {
        key: "key-1",
        id: 2,
        focused: true,
        top: 10,
        left: 20,
        width: 1200,
        height: 800,
        type: "normal",
        state: "normal",
        alwaysOnTop: false
      },
      { key: "key-2", id: 1, focused: false }
    ],
    groups: [
      {
        key: "key-3",
        id: 7,
        windowKey: "key-1",
        color: "blue",
        collapsed: false,
        title: "Work",
        shared: false
      }
    ],
    tabs: [
      {
        key: "key-4",
        id: 10,
        windowKey: "key-1",
        index: 1,
        active: false,
        highlighted: false,
        pinned: true,
        discarded: false,
        title: "Second",
        url: "https://second.example/",
        audible: false,
        status: "complete",
        frozen: false,
        autoDiscardable: true,
        lastAccessed: 123,
        favIconUrl: "https://second.example/icon.png",
        groupKey: "key-3",
        muted: true
      },
      {
        key: "key-5",
        id: 11,
        windowKey: "key-1",
        index: 0,
        active: true,
        highlighted: true,
        pinned: false,
        discarded: true,
        pendingUrl: "https://first.example/",
        frozen: true,
        openerKey: "key-4"
      }
    ]
  });
  assert.deepEqual(mock.calls, [
    "windows.getAll",
    "tabGroups.query",
    "tabs.query"
  ]);

  snapshot.tabs[0].title = "mutated test value";
  const clonedAgain = await model.snapshot("browser-1");
  assert.equal(clonedAgain.tabs[0].title, "Second");
  assert.equal(clonedAgain.modelRevision, 1);
});

test("tab keys survive updates and change on replacement", async () => {
  const mock = createMockChrome({
    windows: [{ id: 1, focused: true }],
    groups: [],
    tabs: [{
      id: 10,
      windowId: 1,
      index: 0,
      groupId: -1,
      title: "Before",
      url: "https://before.example/",
      active: true,
      highlighted: true,
      pinned: false,
      discarded: false
    }]
  });
  const model = createBrowserModel(mock.chromeApi, { randomUUID: uuidSequence() });
  const initial = await model.snapshot("browser-1");

  mock.state.tabs[0].title = "After navigation";
  mock.state.tabs[0].url = "https://after.example/";
  mock.chromeApi.tabs.onUpdated.emit(10, { status: "complete" });
  const navigated = await model.snapshot("browser-1");

  assert.equal(navigated.tabs[0].key, initial.tabs[0].key);
  assert.equal(navigated.modelRevision, 2);

  mock.state.tabs[0] = { ...mock.state.tabs[0], id: 20 };
  mock.chromeApi.tabs.onReplaced.emit(20, 10);
  const replaced = await model.snapshot("browser-1");

  assert.notEqual(replaced.tabs[0].key, initial.tabs[0].key);
  assert.equal(replaced.tabs[0].id, 20);
  assert.equal(replaced.modelRevision, 3);
});

test("an event during the initial baseline forces authoritative reconciliation", async () => {
  const gate = deferred();
  const mock = createMockChrome({
    windows: [{ id: 1, focused: true }],
    groups: [],
    tabs: [{
      id: 10,
      windowId: 1,
      index: 0,
      groupId: -1,
      title: "Stale",
      active: true,
      highlighted: true,
      pinned: false,
      discarded: false
    }]
  }, gate);
  const model = createBrowserModel(mock.chromeApi, { randomUUID: uuidSequence() });

  const eventObjects = [
    mock.chromeApi.windows.onCreated,
    mock.chromeApi.windows.onRemoved,
    mock.chromeApi.windows.onFocusChanged,
    mock.chromeApi.windows.onBoundsChanged,
    mock.chromeApi.tabs.onCreated,
    mock.chromeApi.tabs.onUpdated,
    mock.chromeApi.tabs.onMoved,
    mock.chromeApi.tabs.onAttached,
    mock.chromeApi.tabs.onDetached,
    mock.chromeApi.tabs.onActivated,
    mock.chromeApi.tabs.onHighlighted,
    mock.chromeApi.tabs.onRemoved,
    mock.chromeApi.tabs.onReplaced,
    mock.chromeApi.tabGroups.onCreated,
    mock.chromeApi.tabGroups.onUpdated,
    mock.chromeApi.tabGroups.onMoved,
    mock.chromeApi.tabGroups.onRemoved
  ];
  assert.ok(eventObjects.every((event) => event.listeners.length === 1));

  mock.state.tabs[0].title = "Current";
  mock.chromeApi.tabs.onUpdated.emit(10, { title: "Current" });
  gate.resolve();

  const snapshot = await model.snapshot("browser-1");
  assert.equal(snapshot.tabs[0].title, "Current");
  assert.equal(snapshot.modelRevision, 1);
  assert.equal(mock.calls.length, 6);
});

test("support changes are reconciled without invoking mutating APIs", async () => {
  const mock = createMockChrome({
    windows: [{ id: 1, focused: true }],
    groups: [{ id: 7, windowId: 1, color: "grey", collapsed: false }],
    tabs: [{
      id: 10,
      windowId: 1,
      index: 0,
      groupId: 7,
      active: true,
      highlighted: true,
      pinned: false,
      discarded: false
    }]
  });
  const model = createBrowserModel(mock.chromeApi, { randomUUID: uuidSequence() });
  await model.initialized;
  const changes = [];
  model.onSupportChanged((support) => changes.push(support));

  mock.state.tabs[0].frozen = false;
  mock.state.groups[0].shared = true;
  mock.chromeApi.tabs.onUpdated.emit(10, { frozen: false });
  const snapshot = await model.snapshot("browser-1");

  assert.equal(snapshot.supportsFrozenTabs, true);
  assert.equal(snapshot.supportsSharedTabGroups, true);
  assert.deepEqual(changes, [{ frozenTabs: true, sharedTabGroups: true }]);
  assert.ok(mock.calls.every((call) => [
    "windows.getAll",
    "tabGroups.query",
    "tabs.query"
  ].includes(call)));
});
