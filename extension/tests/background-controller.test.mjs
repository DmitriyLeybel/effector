import assert from "node:assert/strict";
import test from "node:test";

import { createBackgroundController } from "../background-controller.js";

class MockEvent {
  constructor() {
    this.listeners = [];
  }

  addListener(listener) {
    this.listeners.push(listener);
  }

  emit(...args) {
    return this.listeners.map((listener) => listener(...args));
  }
}

function deferred() {
  let resolve;
  const promise = new Promise((complete) => {
    resolve = complete;
  });
  return { promise, resolve };
}

function createPort() {
  return {
    onMessage: new MockEvent(),
    onDisconnect: new MockEvent(),
    messages: [],
    disconnectCalls: 0,
    postMessage(message) {
      this.messages.push(message);
    },
    disconnect() {
      this.disconnectCalls += 1;
    }
  };
}

function createHarness(overrides = {}) {
  const ports = [];
  const nativeHosts = [];
  const timers = new Map();
  const dispatchCalls = [];
  let nextTimerId = 1;
  let supportListener = null;

  const runtime = {
    id: "extension-id",
    lastError: null,
    onMessage: new MockEvent(),
    onStartup: new MockEvent(),
    onInstalled: new MockEvent(),
    getManifest: () => ({ version: "0.2.0" }),
    connectNative(host) {
      nativeHosts.push(host);
      const port = createPort();
      ports.push(port);
      return port;
    }
  };
  const chromeApi = { runtime };
  const browserModel = {
    getSupport: overrides.getSupport ?? (async () => ({
      frozenTabs: false,
      sharedTabGroups: false
    })),
    onSupportChanged(listener) {
      supportListener = listener;
      return () => {
        supportListener = null;
      };
    }
  };
  const dispatch = overrides.dispatch ?? (async (method, params, context) => {
    dispatchCalls.push({ method, params, context });
    return { method };
  });

  createBackgroundController(chromeApi, {
    browserInstanceId: overrides.browserInstanceId ?? (async () => "browser-1"),
    browserModel,
    dispatch,
    userAgent: "test-agent",
    now: () => "2026-08-03T12:00:00.000Z",
    setTimeout(callback, delay) {
      const id = nextTimerId;
      nextTimerId += 1;
      timers.set(id, { callback, delay });
      return id;
    },
    clearTimeout(id) {
      timers.delete(id);
    }
  });

  return {
    chromeApi,
    dispatchCalls,
    nativeHosts,
    ports,
    timers,
    emitSupport(support) {
      assert.ok(supportListener, "support listener was registered");
      supportListener(support);
    },
    runTimerWithDelay(delay) {
      const entry = [...timers.entries()].find(([, timer]) => timer.delay === delay);
      assert.ok(entry, `timer with delay ${delay} was scheduled`);
      const [id, timer] = entry;
      timers.delete(id);
      timer.callback();
    }
  };
}

function readyAck(endpoint = "http://127.0.0.1:37654/mcp") {
  return {
    type: "ready_ack",
    protocolVersion: 3,
    protocolAbiRevision: 1,
    implementations: [
      { method: "browser.list", abiRevision: 1 },
      { method: "browser.snapshot", abiRevision: 1 },
      { method: "tabs.list", abiRevision: 1 }
    ],
    brokerPid: 123,
    mcpEndpoint: endpoint
  };
}

function request(overrides = {}) {
  return {
    type: "request",
    requestId: "request-1",
    method: "browser.snapshot",
    params: {},
    requestClass: "read",
    deadlineMs: 29_000,
    ...overrides
  };
}

async function waitFor(predicate, message = "condition was not reached") {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    if (predicate()) return;
    await Promise.resolve();
  }
  assert.fail(message);
}

function sendRuntimeMessage(harness, message) {
  let response;
  const returns = harness.chromeApi.runtime.onMessage.emit(
    message,
    {},
    (value) => {
      response = value;
    }
  );
  assert.deepEqual(returns, [false]);
  return response;
}

test("registers listeners synchronously and orders ready before capability publication", async () => {
  const harness = createHarness();
  const port = harness.ports[0];

  assert.equal(harness.chromeApi.runtime.onMessage.listeners.length, 1);
  assert.equal(harness.chromeApi.runtime.onStartup.listeners.length, 1);
  assert.equal(harness.chromeApi.runtime.onInstalled.listeners.length, 1);
  assert.equal(port.onMessage.listeners.length, 1);
  assert.equal(port.onDisconnect.listeners.length, 1);
  assert.deepEqual(port.messages, []);

  await waitFor(() => port.messages.length === 1, "ready was not posted");
  assert.deepEqual(port.messages[0], {
    type: "ready",
    protocolVersion: 3,
    protocolAbiRevision: 1,
    implementations: [
      { method: "browser.list", abiRevision: 1 },
      { method: "browser.snapshot", abiRevision: 1 },
      { method: "tabs.list", abiRevision: 1 }
    ],
    browserInstanceId: "browser-1",
    extensionId: "extension-id",
    extensionVersion: "0.2.0",
    userAgent: "test-agent",
    capabilityRevision: 1,
    capabilities: {
      browserSnapshot: capability(true),
      browserChange: capability(false),
      pageTools: capability(false),
      advancedEvaluation: capability(false),
      frozenTabs: false,
      sharedTabGroups: false
    }
  });

  harness.emitSupport({ frozenTabs: true, sharedTabGroups: false });
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(port.messages.length, 1, "capabilities published before ready acknowledgement");

  port.onMessage.emit(readyAck());
  await waitFor(() => port.messages.length === 2, "capability update was not posted after ready acknowledgement");
  assert.deepEqual(port.messages[1], {
    type: "capabilities_changed",
    browserInstanceId: "browser-1",
    capabilityRevision: 2,
    capabilities: {
      browserSnapshot: capability(true),
      browserChange: capability(false),
      pageTools: capability(false),
      advancedEvaluation: capability(false),
      frozenTabs: true,
      sharedTabGroups: false
    }
  });
});

test("rejects a pre-handshake request without dispatching it", async () => {
  const harness = createHarness();
  const port = harness.ports[0];
  await waitFor(() => port.messages.length === 1);

  port.onMessage.emit(request());

  await waitFor(() => port.messages.length === 2, "pre-handshake error was not posted");
  assert.deepEqual(harness.dispatchCalls, []);
  assert.deepEqual(port.messages[1], {
    type: "response",
    requestId: "request-1",
    browserInstanceId: "browser-1",
    ok: false,
    error: {
      code: "PROTOCOL_NOT_READY",
      message: "Native broker handshake is not complete"
    },
    dispatch: { state: "notDispatched" }
  });
});

test("rejects a duplicate in-flight request ID without a second dispatch", async () => {
  const resultGate = deferred();
  let dispatchCount = 0;
  const harness = createHarness({ dispatch: () => {
    dispatchCount += 1;
    return resultGate.promise;
  } });
  const port = harness.ports[0];
  await waitFor(() => port.messages.length === 1);
  port.onMessage.emit(readyAck());

  port.onMessage.emit(request());
  port.onMessage.emit(request());
  assert.equal(dispatchCount, 1);
  assert.equal(port.disconnectCalls, 1);
  assert.equal(sendRuntimeMessage(harness, { type: "bridge.status" }).lastError,
    "Native broker reused an in-flight request ID");

  resultGate.resolve({ value: true });
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(port.messages.length, 1, "disconnected port received an in-flight result");
  harness.runTimerWithDelay(500);
  assert.equal(harness.ports.length, 2, "local disconnect did not reconnect");
});

test("disconnects on incompatible and duplicate ready acknowledgements", async (t) => {
  await t.test("incompatible acknowledgement", async () => {
    const harness = createHarness();
    const port = harness.ports[0];
    await waitFor(() => port.messages.length === 1);

    port.onMessage.emit({ ...readyAck(), protocolVersion: 1 });

    assert.equal(port.disconnectCalls, 1);
    assert.equal(
      sendRuntimeMessage(harness, { type: "bridge.status" }).lastError,
      "Native broker returned an incompatible ready acknowledgement"
    );
  });

  await t.test("duplicate acknowledgement", async () => {
    const harness = createHarness();
    const port = harness.ports[0];
    await waitFor(() => port.messages.length === 1);

    port.onMessage.emit(readyAck());
    port.onMessage.emit(readyAck());

    assert.equal(port.disconnectCalls, 1);
    assert.equal(
      sendRuntimeMessage(harness, { type: "bridge.status" }).lastError,
      "Native broker returned a duplicate handshake"
    );
  });
});

test("reconnects and isolates old-port messages and in-flight results", async () => {
  const resultGate = deferred();
  const calls = [];
  const harness = createHarness({
    dispatch: async (...args) => {
      calls.push(args);
      return resultGate.promise;
    }
  });
  const oldPort = harness.ports[0];
  await waitFor(() => oldPort.messages.length === 1);
  oldPort.onMessage.emit(readyAck("http://127.0.0.1:37654/old"));
  oldPort.onMessage.emit(request());
  await waitFor(() => calls.length === 1, "request was not dispatched on the first port");

  harness.chromeApi.runtime.lastError = { message: "first port closed" };
  oldPort.onDisconnect.emit();
  harness.runTimerWithDelay(500);

  assert.equal(harness.ports.length, 2);
  const currentPort = harness.ports[1];
  await waitFor(() => currentPort.messages.length === 1);
  currentPort.onMessage.emit(readyAck("http://127.0.0.1:37654/current"));

  oldPort.onMessage.emit(request({ requestId: "stale-request" }));
  oldPort.onDisconnect.emit();
  assert.equal(calls.length, 1, "an old-port request reached dispatch");

  resultGate.resolve({ value: "stale" });
  await waitFor(() => harness.timers.size === 0, "old request deadline was not cleared");
  assert.equal(oldPort.messages.length, 1, "old port received an in-flight result");
  assert.equal(currentPort.messages.length, 1, "new port received an old-port result");
  assert.deepEqual(sendRuntimeMessage(harness, { type: "bridge.status" }), {
    connected: true,
    connectedAt: "2026-08-03T12:00:00.000Z",
    lastError: null,
    nativeHost: "com.effector.browser",
    mcpEndpoint: "http://127.0.0.1:37654/current"
  });
});

test("reports status and accepts an explicit reconnect", async () => {
  const harness = createHarness();
  const firstPort = harness.ports[0];
  await waitFor(() => firstPort.messages.length === 1);

  assert.deepEqual(sendRuntimeMessage(harness, { type: "bridge.status" }), {
    connected: false,
    connectedAt: "2026-08-03T12:00:00.000Z",
    lastError: null,
    nativeHost: "com.effector.browser",
    mcpEndpoint: null
  });

  firstPort.onMessage.emit(readyAck());
  assert.equal(sendRuntimeMessage(harness, { type: "bridge.status" }).connected, true);

  harness.chromeApi.runtime.lastError = { message: "broker stopped" };
  firstPort.onDisconnect.emit();
  assert.deepEqual(sendRuntimeMessage(harness, { type: "bridge.status" }), {
    connected: false,
    connectedAt: null,
    lastError: "broker stopped",
    nativeHost: "com.effector.browser",
    mcpEndpoint: null
  });

  assert.deepEqual(sendRuntimeMessage(harness, { type: "bridge.reconnect" }), {
    accepted: true
  });
  assert.equal(harness.ports.length, 2);
  assert.equal(harness.timers.size, 0);
  assert.deepEqual(harness.nativeHosts, [
    "com.effector.browser",
    "com.effector.browser"
  ]);
  assert.equal(sendRuntimeMessage(harness, { type: "unknown" }), undefined);
});

function capability(effective) {
  return {
    implemented: effective,
    desired: effective,
    granted: effective,
    supported: effective,
    probePassed: effective,
    effective,
    reason: effective ? "available" : "notImplemented"
  };
}
