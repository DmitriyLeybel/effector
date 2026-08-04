export const CAPABILITY_SETTING_KEYS = Object.freeze({
  browserChange: "browserChangesEnabled",
  pageTools: "pageToolsEnabled",
  advancedEvaluation: "advancedEvaluationEnabled"
});

const CONTROLLED_CAPABILITIES = Object.freeze(Object.keys(CAPABILITY_SETTING_KEYS));
const DEFAULT_IMPLEMENTATIONS = Object.freeze({
  browserSnapshot: true,
  browserChange: false,
  pageTools: false,
  advancedEvaluation: false
});

export function createCapabilityController(dependencies) {
  const {
    storage,
    storageChanged,
    permissions,
    browserSupport,
    implementations = DEFAULT_IMPLEMENTATIONS,
    requiredPermissions = {},
    runtimeSupport = {},
    probes = {}
  } = dependencies;

  const listeners = new Set();
  let snapshot = null;
  let reconciliation = Promise.resolve();
  let mutation = Promise.resolve();
  let sourceGeneration = 0;

  storageChanged.addListener((changes, areaName) => {
    if (areaName !== "local") return;
    if (CONTROLLED_CAPABILITIES.some((name) => (
      Object.hasOwn(changes, CAPABILITY_SETTING_KEYS[name])
    ))) {
      sourceChanged();
    }
  });
  permissions?.onAdded?.addListener(sourceChanged);
  permissions?.onRemoved?.addListener(sourceChanged);
  browserSupport.onSupportChanged(sourceChanged);

  const initialized = queueReconciliation();

  function queueReconciliation() {
    reconciliation = reconciliation
      .catch(() => {})
      .then(reconcile);
    return reconciliation;
  }

  function sourceChanged() {
    sourceGeneration += 1;
    queueReconciliation();
  }

  async function reconcile() {
    const generation = sourceGeneration;
    const desired = await readDesiredSettings(generation);
    if (!desired) return snapshot;
    const [support, grants, supported, probeResults] = await Promise.all([
      browserSupport.getSupport(),
      evaluateFacts(requiredPermissions, checkPermissions, true),
      evaluateFacts(runtimeSupport, evaluateRuntimeSupport, true),
      evaluateFacts(probes, evaluateProbe, true)
    ]);
    if (generation !== sourceGeneration) return snapshot;

    const pageTools = capabilityStatus({
      implemented: implementation("pageTools"),
      desired: desired.pageTools,
      granted: grants.pageTools,
      supported: supported.pageTools,
      probePassed: probeResults.pageTools
    });
    const capabilities = {
      browserSnapshot: capabilityStatus({
        implemented: implementation("browserSnapshot"),
        desired: true,
        granted: grants.browserSnapshot,
        supported: supported.browserSnapshot,
        probePassed: probeResults.browserSnapshot
      }),
      browserChange: capabilityStatus({
        implemented: implementation("browserChange"),
        desired: desired.browserChange,
        granted: grants.browserChange,
        supported: supported.browserChange,
        probePassed: probeResults.browserChange
      }),
      pageTools,
      advancedEvaluation: capabilityStatus({
        implemented: implementation("advancedEvaluation"),
        desired: desired.advancedEvaluation,
        granted: grants.advancedEvaluation,
        supported: supported.advancedEvaluation,
        probePassed: probeResults.advancedEvaluation,
        dependencyAvailable: pageTools.effective
      }),
      frozenTabs: support.frozenTabs === true,
      sharedTabGroups: support.sharedTabGroups === true
    };

    if (snapshot && equalCapabilities(snapshot.capabilities, capabilities)) return snapshot;

    snapshot = deepFreeze({
      revision: snapshot ? snapshot.revision + 1 : 1,
      capabilities
    });
    for (const listener of listeners) listener(snapshot);
    return snapshot;
  }

  async function readDesiredSettings(generation) {
    const keys = Object.values(CAPABILITY_SETTING_KEYS);
    const stored = await storage.get(keys);
    const defaults = {};
    const desired = {};

    for (const name of CONTROLLED_CAPABILITIES) {
      const key = CAPABILITY_SETTING_KEYS[name];
      if (typeof stored[key] !== "boolean") defaults[key] = false;
      desired[name] = typeof stored[key] === "boolean" ? stored[key] : false;
    }
    if (generation !== sourceGeneration) return null;
    if (Object.keys(defaults).length > 0) await storage.set(defaults);
    return desired;
  }

  function implementation(name) {
    return implementations[name] === true;
  }

  async function checkPermissions(requirement) {
    if (!requirement) return true;
    if (!permissions?.contains) return false;
    try {
      return await permissions.contains(requirement);
    } catch (_error) {
      return false;
    }
  }

  async function evaluateFact(fact) {
    return typeof fact === "function" ? fact() : fact;
  }

  async function evaluateProbe(probe) {
    try {
      return await evaluateFact(probe);
    } catch (_error) {
      return false;
    }
  }

  async function evaluateRuntimeSupport(fact) {
    try {
      return await evaluateFact(fact);
    } catch (_error) {
      return false;
    }
  }

  async function evaluateFacts(facts, evaluator, defaultValue) {
    const entries = await Promise.all([
      "browserSnapshot",
      "browserChange",
      "pageTools",
      "advancedEvaluation"
    ].map(async (name) => {
      if (!Object.hasOwn(facts, name)) return [name, defaultValue];
      return [name, await evaluator(facts[name]) === true];
    }));
    return Object.fromEntries(entries);
  }

  async function whenIdle() {
    let pending;
    do {
      pending = reconciliation;
      await pending;
    } while (pending !== reconciliation);
  }

  async function getState() {
    try {
      await whenIdle();
    } catch (_error) {
      await queueReconciliation();
      await whenIdle();
    }
    if (!snapshot) {
      await queueReconciliation();
      await whenIdle();
    }
    return snapshot;
  }

  function setDesired(name, desired) {
    if (!CONTROLLED_CAPABILITIES.includes(name) || typeof desired !== "boolean") {
      return Promise.reject(new TypeError("Invalid capability desired state"));
    }
    mutation = mutation
      .catch(() => {})
      .then(async () => {
        await getState();
        await storage.set({ [CAPABILITY_SETTING_KEYS[name]]: desired });
        await queueReconciliation();
        return getState();
      });
    return mutation;
  }

  return {
    initialized,
    getState,
    onChanged(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    setDesired,
    whenIdle
  };
}

function capabilityStatus({
  implemented,
  desired,
  granted,
  supported,
  probePassed,
  dependencyAvailable = true
}) {
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

function equalCapabilities(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function deepFreeze(value) {
  Object.freeze(value);
  for (const child of Object.values(value)) {
    if (child && typeof child === "object" && !Object.isFrozen(child)) deepFreeze(child);
  }
  return value;
}
