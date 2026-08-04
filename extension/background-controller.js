import {
  IMPLEMENTATIONS,
  INTERNAL_PROTOCOL_VERSION,
  PROTOCOL_ABI_REVISION,
  errorResponse,
  readableRequestId,
  publicError,
  successResponse,
  validateReadyAck,
  validateRequest
} from "./protocol.js";

const NATIVE_HOST = "com.effector.browser";

export function createBackgroundController(chromeApi, dependencies) {
  const {
    browserInstanceId,
    browserModel,
    dispatch,
    userAgent = globalThis.navigator?.userAgent ?? "",
    now = () => new Date().toISOString(),
    setTimeout: scheduleTimeout = globalThis.setTimeout,
    clearTimeout: cancelTimeout = globalThis.clearTimeout
  } = dependencies;

  let nativePort = null;
  let brokerReady = false;
  let negotiatedImplementations = null;
  let inFlightRequestIds = new Set();
  let reconnectAttempt = 0;
  let reconnectTimer = null;
  let lastError = null;
  let connectedAt = null;
  let mcpEndpoint = null;
  let capabilityRevision = 1;
  let capabilities = null;
  let pendingSupport = null;
  let readyCapabilityRevision = 0;
  let capabilitiesReady = null;
  let capabilityPublication = Promise.resolve();

  browserModel.onSupportChanged((support) => {
    if (!capabilities) {
      pendingSupport = support;
      return;
    }
    const nextCapabilities = buildCapabilities(support);
    if (JSON.stringify(capabilities) === JSON.stringify(nextCapabilities)) return;
    capabilities = nextCapabilities;
    capabilityRevision += 1;
    capabilityPublication = capabilityPublication
      .catch(() => {})
      .then(publishCapabilitiesChanged);
  });

  function buildCapabilities(support) {
    return {
      browserSnapshot: capabilityStatus(true, "available"),
      browserChange: capabilityStatus(false, "notImplemented"),
      pageTools: capabilityStatus(false, "notImplemented"),
      advancedEvaluation: capabilityStatus(false, "notImplemented"),
      frozenTabs: support.frozenTabs,
      sharedTabGroups: support.sharedTabGroups
    };
  }

  function capabilityStatus(effective, reason) {
    return {
      implemented: effective,
      desired: effective,
      granted: effective,
      supported: effective,
      probePassed: effective,
      effective,
      reason
    };
  }

  async function initializeCapabilities() {
    const support = await browserModel.getSupport();
    capabilities = buildCapabilities(pendingSupport ?? support);
    pendingSupport = null;
  }

  function ensureCapabilitiesReady() {
    if (!capabilitiesReady) {
      capabilitiesReady = initializeCapabilities().catch((error) => {
        capabilitiesReady = null;
        throw error;
      });
    }
    return capabilitiesReady;
  }

  async function capabilityState() {
    await ensureCapabilitiesReady();
    return {
      revision: capabilityRevision,
      capabilities: { ...capabilities }
    };
  }

  function connectBroker() {
    if (nativePort) return;
    cancelTimeout(reconnectTimer);

    try {
      const port = chromeApi.runtime.connectNative(NATIVE_HOST);
      nativePort = port;
      brokerReady = false;
      negotiatedImplementations = null;
      inFlightRequestIds = new Set();
      readyCapabilityRevision = 0;
      lastError = null;
      connectedAt = now();

      port.onMessage.addListener((message) => handleNativeMessage(port, message));
      port.onDisconnect.addListener(() => {
        clearConnection(
          port,
          chromeApi.runtime.lastError?.message ?? lastError ?? "Native broker disconnected"
        );
      });

      void sendReady(port).catch((error) => {
        if (nativePort === port) {
          disconnectPort(port, error?.message ?? String(error));
        }
      });
    } catch (error) {
      lastError = error?.message ?? String(error);
      nativePort = null;
      scheduleReconnect();
    }
  }

  function scheduleReconnect() {
    cancelTimeout(reconnectTimer);
    const delay = Math.min(30_000, 500 * (2 ** reconnectAttempt));
    reconnectAttempt = Math.min(reconnectAttempt + 1, 6);
    reconnectTimer = scheduleTimeout(connectBroker, delay);
  }

  function clearConnection(port, error) {
    if (nativePort !== port) return false;
    lastError = error;
    nativePort = null;
    brokerReady = false;
    negotiatedImplementations = null;
    inFlightRequestIds = new Set();
    connectedAt = null;
    mcpEndpoint = null;
    scheduleReconnect();
    return true;
  }

  function disconnectPort(port, error) {
    if (!clearConnection(port, error)) return;
    port.disconnect();
  }

  async function sendReady(port) {
    const [instanceId, state] = await Promise.all([
      browserInstanceId(),
      capabilityState()
    ]);
    const message = {
      type: "ready",
      protocolVersion: INTERNAL_PROTOCOL_VERSION,
      protocolAbiRevision: PROTOCOL_ABI_REVISION,
      implementations: IMPLEMENTATIONS,
      browserInstanceId: instanceId,
      extensionId: chromeApi.runtime.id,
      extensionVersion: chromeApi.runtime.getManifest().version,
      userAgent,
      capabilityRevision: state.revision,
      capabilities: state.capabilities
    };
    if (nativePort === port) {
      readyCapabilityRevision = state.revision;
      port.postMessage(message);
    }
  }

  async function handleNativeMessage(port, message) {
    if (nativePort !== port) return;
    if (message?.type === "ready_ack") {
      const validation = validateReadyAck(message);
      if (!validation.ok || brokerReady) {
        const error = validation.ok
          ? "Native broker returned a duplicate handshake"
          : validation.error.message;
        disconnectPort(port, error);
        return;
      }
      reconnectAttempt = 0;
      brokerReady = true;
      negotiatedImplementations = validation.value.implementations;
      mcpEndpoint = message.mcpEndpoint;
      if (readyCapabilityRevision !== capabilityRevision) {
        void publishCapabilitiesChanged();
      }
      return;
    }

    const validation = validateRequest(
      message,
      negotiatedImplementations ?? IMPLEMENTATIONS
    );
    if (!validation.ok) {
      const requestId = readableRequestId(message);
      if (requestId) {
        await sendError(port, requestId, validation.error, "notDispatched");
      } else {
        disconnectPort(port, validation.error.message);
      }
      return;
    }

    if (!brokerReady) {
      await sendError(port, message.requestId, {
        code: "PROTOCOL_NOT_READY",
        message: "Native broker handshake is not complete",
        effectorSafe: true
      }, "notDispatched");
      return;
    }

    if (inFlightRequestIds.has(message.requestId)) {
      disconnectPort(port, "Native broker reused an in-flight request ID");
      return;
    }

    const requestIds = inFlightRequestIds;
    requestIds.add(message.requestId);
    try {
      const result = await withDeadline(
        dispatchRequest(message.method, message.params),
        message.deadlineMs
      );
      if (nativePort === port) {
        const response = successResponse(
          await browserInstanceId(),
          message.requestId,
          result
        );
        if (response) port.postMessage(response);
      }
    } catch (error) {
      await sendError(port, message.requestId, error);
    } finally {
      requestIds.delete(message.requestId);
    }
  }

  async function dispatchRequest(method, params) {
    const result = await dispatch(method, params, { connectedAt });
    if (method === "browser.snapshot") await capabilityPublication;
    return result;
  }

  async function sendError(port, requestId, error, dispatchState = "completed") {
    if (nativePort !== port) return;
    const response = errorResponse(
      await browserInstanceId(),
      requestId,
      publicError(error),
      undefined,
      dispatchState
    );
    if (response) port.postMessage(response);
  }

  function withDeadline(operation, deadlineMs) {
    let timer;
    const deadline = new Promise((_, reject) => {
      timer = scheduleTimeout(() => reject(Object.assign(
        new Error("Extension request deadline elapsed"),
        { code: "TIMEOUT", effectorSafe: true }
      )), deadlineMs);
    });
    return Promise.race([operation, deadline]).finally(() => cancelTimeout(timer));
  }

  async function publishCapabilitiesChanged() {
    const port = nativePort;
    if (!port || !brokerReady) return;
    const [instanceId, state] = await Promise.all([
      browserInstanceId(),
      capabilityState()
    ]);
    if (nativePort !== port || !brokerReady) return;
    port.postMessage({
      type: "capabilities_changed",
      browserInstanceId: instanceId,
      capabilityRevision: state.revision,
      capabilities: state.capabilities
    });
  }

  chromeApi.runtime.onMessage.addListener((message, _sender, sendResponse) => {
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

  chromeApi.runtime.onStartup.addListener(connectBroker);
  chromeApi.runtime.onInstalled.addListener(connectBroker);
  connectBroker();

  return { connectBroker };
}
