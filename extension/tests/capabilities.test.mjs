import assert from "node:assert/strict";
import test from "node:test";

import {
  CAPABILITY_SETTING_KEYS,
  createCapabilityController
} from "../capabilities.js";

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

function createHarness(options = {}) {
  const values = { ...(options.stored ?? {}) };
  const setCalls = [];
  const storageChanged = new MockEvent();
  const permissionsAdded = new MockEvent();
  const permissionsRemoved = new MockEvent();
  const supportChanged = new MockEvent();
  let support = options.support ?? { frozenTabs: false, sharedTabGroups: false };
  let permissionGranted = options.permissionGranted ?? true;

  const storage = {
    async get(keys) {
      return Object.fromEntries(keys
        .filter((key) => Object.hasOwn(values, key))
        .map((key) => [key, values[key]]));
    },
    async set(update) {
      setCalls.push({ ...update });
      const changes = {};
      for (const [key, value] of Object.entries(update)) {
        changes[key] = { oldValue: values[key], newValue: value };
        values[key] = value;
      }
      storageChanged.emit(changes, "local");
    }
  };
  const permissions = {
    onAdded: permissionsAdded,
    onRemoved: permissionsRemoved,
    async contains() {
      return permissionGranted;
    }
  };
  const browserSupport = {
    async getSupport() {
      return { ...support };
    },
    onSupportChanged(listener) {
      supportChanged.addListener(listener);
      return () => {};
    }
  };
  const controller = createCapabilityController({
    storage,
    storageChanged,
    permissions,
    browserSupport,
    implementations: options.implementations,
    requiredPermissions: options.requiredPermissions,
    runtimeSupport: options.runtimeSupport,
    probes: options.probes
  });

  return {
    controller,
    permissionsAdded,
    permissionsRemoved,
    setCalls,
    storageChanged,
    supportChanged,
    values,
    setPermissionGranted(value) {
      permissionGranted = value;
      permissionsAdded.emit({ permissions: ["scripting"] });
    },
    setSupport(value) {
      support = value;
      supportChanged.emit(value);
    }
  };
}

test("registers observers synchronously and defaults every missing setting off", async () => {
  const harness = createHarness();

  assert.equal(harness.storageChanged.listeners.length, 1);
  assert.equal(harness.permissionsAdded.listeners.length, 1);
  assert.equal(harness.permissionsRemoved.listeners.length, 1);
  assert.equal(harness.supportChanged.listeners.length, 1);

  const state = await harness.controller.getState();
  assert.deepEqual(harness.values, {
    browserChangesEnabled: false,
    pageToolsEnabled: false,
    advancedEvaluationEnabled: false
  });
  assert.deepEqual(harness.setCalls[0], {
    browserChangesEnabled: false,
    pageToolsEnabled: false,
    advancedEvaluationEnabled: false
  });
  assert.equal(state.revision, 1);
  assert.deepEqual(state.capabilities.browserSnapshot, status());
  assert.deepEqual(state.capabilities.browserChange, status({
    implemented: false,
    desired: false
  }));
});

test("fills only missing or malformed upgrade settings and preserves other storage", async () => {
  const harness = createHarness({
    stored: {
      installationId: "installation-1",
      browserChangesEnabled: true,
      pageToolsEnabled: "invalid",
      advancedEvaluationEnabled: false
    }
  });

  const state = await harness.controller.getState();
  assert.deepEqual(harness.setCalls[0], { pageToolsEnabled: false });
  assert.equal(harness.values.installationId, "installation-1");
  assert.equal(state.capabilities.browserChange.desired, true);
  assert.equal(state.capabilities.browserChange.reason, "notImplemented");
  assert.equal(state.capabilities.pageTools.desired, false);
});

test("persists enable and disable transitions with monotonic immutable snapshots", async () => {
  const harness = createHarness({
    stored: disabledSettings(),
    implementations: implementations({ browserChange: true })
  });
  const initial = await harness.controller.getState();
  const revisions = [];
  harness.controller.onChanged((state) => revisions.push(state.revision));

  const enabled = await harness.controller.setDesired("browserChange", true);
  const unchanged = await harness.controller.setDesired("browserChange", true);
  const disabled = await harness.controller.setDesired("browserChange", false);

  assert.deepEqual(initial.capabilities.browserChange, status({ desired: false }));
  assert.deepEqual(enabled.capabilities.browserChange, status());
  assert.equal(enabled.revision, initial.revision + 1);
  assert.equal(unchanged.revision, enabled.revision);
  assert.equal(disabled.revision, enabled.revision + 1);
  assert.deepEqual(revisions, [2, 3]);
  assert.ok(Object.isFrozen(enabled));
  assert.ok(Object.isFrozen(enabled.capabilities.browserChange));
  assert.throws(() => {
    enabled.capabilities.browserChange.desired = false;
  }, TypeError);
});

test("preserves an explicit desired setting across a worker restart", async () => {
  const first = createHarness({
    stored: disabledSettings(),
    implementations: implementations({ browserChange: true })
  });
  await first.controller.setDesired("browserChange", true);

  const restarted = createHarness({
    stored: first.values,
    implementations: implementations({ browserChange: true })
  });
  const state = await restarted.controller.getState();

  assert.equal(state.revision, 1);
  assert.equal(state.capabilities.browserChange.desired, true);
  assert.equal(state.capabilities.browserChange.effective, true);
  assert.deepEqual(restarted.setCalls, []);
});

test("reconciles permission, support, runtime, probe, and parent dependency facts", async () => {
  let browserChangeSupported = false;
  const harness = createHarness({
    stored: enabledSettings(),
    implementations: implementations({
      browserChange: true,
      pageTools: true,
      advancedEvaluation: true
    }),
    permissionGranted: false,
    requiredPermissions: {
      pageTools: { permissions: ["scripting"] }
    },
    runtimeSupport: {
      browserChange: () => browserChangeSupported
    },
    probes: {
      browserSnapshot: async () => {
        throw new Error("private probe detail");
      }
    }
  });

  const initial = await harness.controller.getState();
  assert.equal(initial.capabilities.browserSnapshot.reason, "probeFailed");
  assert.equal(initial.capabilities.browserChange.reason, "unsupported");
  assert.equal(initial.capabilities.pageTools.reason, "permissionMissing");
  assert.equal(initial.capabilities.advancedEvaluation.reason, "dependencyUnavailable");

  browserChangeSupported = true;
  harness.setPermissionGranted(true);
  const updated = await harness.controller.getState();
  assert.equal(updated.capabilities.browserChange.reason, "available");
  assert.equal(updated.capabilities.pageTools.reason, "available");
  assert.equal(updated.capabilities.advancedEvaluation.reason, "available");
  assert.ok(updated.revision > initial.revision);
});

test("reconciles browser support changes without revising unchanged state", async () => {
  const harness = createHarness({ stored: disabledSettings() });
  const initial = await harness.controller.getState();

  harness.setSupport({ frozenTabs: true, sharedTabGroups: false });
  const changed = await harness.controller.getState();
  harness.setSupport({ frozenTabs: true, sharedTabGroups: false });
  const unchanged = await harness.controller.getState();

  assert.equal(changed.capabilities.frozenTabs, true);
  assert.equal(changed.revision, initial.revision + 1);
  assert.equal(unchanged.revision, changed.revision);
});

test("does not overwrite a storage update that arrives during the initial read", async () => {
  const firstRead = deferred();
  const storageChanged = new MockEvent();
  const values = {};
  let reads = 0;
  const storage = {
    async get(keys) {
      reads += 1;
      if (reads === 1) return firstRead.promise;
      return Object.fromEntries(keys
        .filter((key) => Object.hasOwn(values, key))
        .map((key) => [key, values[key]]));
    },
    async set(update) {
      Object.assign(values, update);
      storageChanged.emit(Object.fromEntries(Object.entries(update).map(([key, newValue]) => (
        [key, { newValue }]
      ))), "local");
    }
  };
  const supportChanged = new MockEvent();
  const controller = createCapabilityController({
    storage,
    storageChanged,
    browserSupport: {
      async getSupport() {
        return { frozenTabs: false, sharedTabGroups: false };
      },
      onSupportChanged(listener) {
        supportChanged.addListener(listener);
      }
    },
    implementations: implementations({ browserChange: true })
  });
  await Promise.resolve();

  values.browserChangesEnabled = true;
  storageChanged.emit({
    browserChangesEnabled: { newValue: true }
  }, "local");
  firstRead.resolve({});

  const state = await controller.getState();
  assert.equal(values.browserChangesEnabled, true);
  assert.equal(state.capabilities.browserChange.effective, true);
});

function deferred() {
  let resolve;
  const promise = new Promise((complete) => {
    resolve = complete;
  });
  return { promise, resolve };
}

function disabledSettings() {
  return Object.fromEntries(Object.values(CAPABILITY_SETTING_KEYS).map((key) => [key, false]));
}

function enabledSettings() {
  return Object.fromEntries(Object.values(CAPABILITY_SETTING_KEYS).map((key) => [key, true]));
}

function implementations(overrides = {}) {
  return {
    browserSnapshot: true,
    browserChange: false,
    pageTools: false,
    advancedEvaluation: false,
    ...overrides
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
