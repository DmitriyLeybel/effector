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
  const setDesiredCalls = [];
  let nextTimerId = 1;
  let capabilityListener = null;
  let currentCapabilityState = overrides.capabilityState ?? state();

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
  const capabilityController = overrides.capabilityController ?? {
    getState: overrides.getCapabilityState ?? (async () => currentCapabilityState),
    onChanged(listener) {
      capabilityListener = listener;
      return () => {
        capabilityListener = null;
      };
    },
    async setDesired(name, desired) {
      setDesiredCalls.push({ name, desired });
      return currentCapabilityState;
    },
    async whenIdle() {
      if (overrides.whenCapabilitiesIdle) await overrides.whenCapabilitiesIdle();
    }
  };
  const dispatch = overrides.dispatch ?? (async (method, params, context) => {
    dispatchCalls.push({ method, params, context });
    return { method };
  });

  createBackgroundController(chromeApi, {
    browserInstanceId: overrides.browserInstanceId ?? (async () => "browser-1"),
    capabilityController,
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
    setDesiredCalls,
    timers,
    emitCapabilities(nextState) {
      currentCapabilityState = nextState;
      assert.ok(capabilityListener, "capability listener was registered");
      capabilityListener(nextState);
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

function sendAsyncRuntimeMessage(harness, message) {
  return new Promise((resolve) => {
    const returns = harness.chromeApi.runtime.onMessage.emit(message, {}, resolve);
    assert.deepEqual(returns, [true]);
  });
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

  harness.emitCapabilities(state(2, { frozenTabs: true }));
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

test("coalesces rapid capability revisions after the handshake", async () => {
  const harness = createHarness();
  const port = harness.ports[0];
  await waitFor(() => port.messages.length === 1);
  port.onMessage.emit(readyAck());

  harness.emitCapabilities(state(2, { frozenTabs: true }));
  harness.emitCapabilities(state(3, {
    frozenTabs: true,
    sharedTabGroups: true
  }));

  await waitFor(() => port.messages.length === 2);
  assert.equal(port.messages[1].type, "capabilities_changed");
  assert.equal(port.messages[1].capabilityRevision, 3);
  assert.equal(port.messages[1].capabilities.frozenTabs, true);
  assert.equal(port.messages[1].capabilities.sharedTabGroups, true);
});

test("publishes a capability change before a dependent browser result", async () => {
  let emitCapabilities;
  const harness = createHarness({
    dispatch: async () => {
      emitCapabilities(state(2, { frozenTabs: true }));
      return { captured: true };
    }
  });
  emitCapabilities = harness.emitCapabilities;
  const port = harness.ports[0];
  await waitFor(() => port.messages.length === 1);
  port.onMessage.emit(readyAck());

  port.onMessage.emit(request());
  await waitFor(() => port.messages.length === 3);

  assert.equal(port.messages[1].type, "capabilities_changed");
  assert.equal(port.messages[1].capabilityRevision, 2);
  assert.equal(port.messages[2].type, "response");
  assert.equal(port.messages[2].requestId, "request-1");
});

test("waits for capability reconciliation before a dependent browser result", async () => {
  const reconciliation = deferred();
  const harness = createHarness({
    whenCapabilitiesIdle: () => reconciliation.promise
  });
  const port = harness.ports[0];
  await waitFor(() => port.messages.length === 1);
  port.onMessage.emit(readyAck());

  port.onMessage.emit(request());
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(port.messages.length, 1, "result bypassed capability reconciliation");

  harness.emitCapabilities(state(2, { frozenTabs: true }));
  reconciliation.resolve();
  await waitFor(() => port.messages.length === 3);
  assert.equal(port.messages[1].type, "capabilities_changed");
  assert.equal(port.messages[2].type, "response");
});

test("serves popup capability state and rejects unavailable enablement", async () => {
  const harness = createHarness();
  const expected = state();

  assert.deepEqual(
    await sendAsyncRuntimeMessage(harness, { type: "capabilities.get" }),
    { ok: true, state: expected }
  );
  assert.deepEqual(
    await sendAsyncRuntimeMessage(harness, {
      type: "capabilities.setBrowserChanges",
      enabled: true
    }),
    {
      ok: false,
      state: expected,
      error: {
        code: "CAPABILITY_UNAVAILABLE",
        message: "Browser changes are unavailable in this build."
      }
    }
  );
  assert.deepEqual(harness.setDesiredCalls, []);
  assert.deepEqual(sendRuntimeMessage(harness, {
    type: "capabilities.setBrowserChanges",
    enabled: "yes"
  }), {
    ok: false,
    error: {
      code: "INVALID_CAPABILITY_SETTING",
      message: "Browser changes must be enabled or disabled explicitly."
    }
  });
});

test("routes an available Browser changes setting through the controller", async () => {
  const disabled = status({ implemented: true, desired: false });
  const enabled = status({ implemented: true, desired: true });
  const before = state(4, { browserChange: disabled });
  const after = state(5, { browserChange: enabled });
  const calls = [];
  const capabilityController = {
    async getState() {
      return before;
    },
    onChanged() {
      return () => {};
    },
    async whenIdle() {},
    async setDesired(name, desired) {
      calls.push({ name, desired });
      return after;
    }
  };
  const harness = createHarness({ capabilityController });

  assert.deepEqual(
    await sendAsyncRuntimeMessage(harness, {
      type: "capabilities.setBrowserChanges",
      enabled: true
    }),
    { ok: true, state: after }
  );
  assert.deepEqual(calls, [{ name: "browserChange", desired: true }]);
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

function status({
  implemented = true,
  desired = true,
  granted = true,
  supported = true,
  probePassed = true,
  dependencyAvailable = true
} = {}) {
  const effective = implemented && desired && granted && supported && probePassed &&
    dependencyAvailable;
  let reason = "available";
  if (!implemented) reason = "notImplemented";
  else if (!desired) reason = "disabled";
  else if (!granted) reason = "permissionMissing";
  else if (!supported) reason = "unsupported";
  else if (!probePassed) reason = "probeFailed";
  else if (!dependencyAvailable) reason = "dependencyUnavailable";
  return {
    implemented,
    desired,
    granted,
    supported,
    probePassed,
    effective,
    reason
  };
}

function state(revision = 1, overrides = {}) {
  return {
    revision,
    capabilities: {
      browserSnapshot: capability(true),
      browserChange: overrides.browserChange ?? capability(false),
      pageTools: capability(false),
      advancedEvaluation: capability(false),
      frozenTabs: overrides.frozenTabs ?? false,
      sharedTabGroups: overrides.sharedTabGroups ?? false
    }
  };
}
