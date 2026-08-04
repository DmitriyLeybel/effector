export const INTERNAL_PROTOCOL_VERSION = 3;
export const PROTOCOL_ABI_REVISION = 1;
export const MAX_RESPONSE_BYTES = 60 * 1024 * 1024;
export const MAX_ARTIFACT_DECODED_BYTES = 8 * 1024 * 1024;
export const IMPLEMENTATIONS = Object.freeze([
  Object.freeze({ method: "browser.list", abiRevision: 1 }),
  Object.freeze({ method: "browser.snapshot", abiRevision: 1 }),
  Object.freeze({ method: "tabs.list", abiRevision: 1 })
]);

const MAX_ERROR_CODE_LENGTH = 64;
const MAX_ERROR_MESSAGE_LENGTH = 512;
const MAX_IMPLEMENTATION_ENTRIES = 64;
const READY_ACK_FIELDS = new Set([
  "type",
  "protocolVersion",
  "protocolAbiRevision",
  "implementations",
  "brokerPid",
  "mcpEndpoint"
]);
const REQUEST_FIELDS = new Set([
  "type",
  "requestId",
  "method",
  "params",
  "requestClass",
  "deadlineMs"
]);
const METHOD_POLICIES = new Map([
  ["browser.list", { requestClass: "read", deadlineMs: 29_000 }],
  ["browser.snapshot", { requestClass: "read", deadlineMs: 29_000 }],
  ["tabs.list", { requestClass: "read", deadlineMs: 29_000 }]
]);
const DISPATCH_STATES = new Set(["notDispatched", "completed", "unknown"]);

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function hasExactlyFields(value, required, optional = new Set()) {
  return Object.keys(value).every((field) => required.has(field) || optional.has(field)) &&
    [...required].every((field) => Object.hasOwn(value, field));
}

function invalid(message) {
  return {
    ok: false,
    error: {
      code: "INVALID_REQUEST",
      message,
      effectorSafe: true
    }
  };
}

function implementationKey(entry) {
  return `${entry.method}\u0000${entry.branch ?? ""}`;
}

function validateImplementations(entries) {
  if (!Array.isArray(entries) || entries.length < 1 || entries.length > MAX_IMPLEMENTATION_ENTRIES) {
    return invalid("Implementation manifest size is invalid");
  }
  const keys = new Set();
  for (const entry of entries) {
    if (!isObject(entry) || !hasExactlyFields(
      entry,
      new Set(["method", "abiRevision"]),
      new Set(["branch"])
    )) {
      return invalid("Implementation manifest entry fields are invalid");
    }
    if (
      typeof entry.method !== "string" ||
      entry.method.length < 1 ||
      entry.method.length > 128 ||
      !/^[a-z0-9._]+$/.test(entry.method) ||
      !Number.isInteger(entry.abiRevision) ||
      entry.abiRevision < 1 ||
      (Object.hasOwn(entry, "branch") && (
        typeof entry.branch !== "string" ||
        entry.branch.length < 1 ||
        entry.branch.length > 64
      ))
    ) {
      return invalid("Implementation manifest entry is invalid");
    }
    const key = implementationKey(entry);
    if (keys.has(key)) return invalid("Implementation manifest contains a duplicate entry");
    keys.add(key);
  }
  return { ok: true, value: entries };
}

export function validateReadyAck(message) {
  if (!isObject(message) || !hasExactlyFields(message, READY_ACK_FIELDS)) {
    return invalid("Native broker returned an invalid ready acknowledgement");
  }
  const manifest = validateImplementations(message.implementations);
  if (
    message.type !== "ready_ack" ||
    message.protocolVersion !== INTERNAL_PROTOCOL_VERSION ||
    message.protocolAbiRevision !== PROTOCOL_ABI_REVISION ||
    !Number.isInteger(message.brokerPid) ||
    message.brokerPid < 1 ||
    typeof message.mcpEndpoint !== "string" ||
    message.mcpEndpoint.length === 0 ||
    !manifest.ok
  ) {
    return invalid("Native broker returned an incompatible ready acknowledgement");
  }
  const acknowledged = new Map(message.implementations.map((entry) => [implementationKey(entry), entry]));
  if (acknowledged.size !== IMPLEMENTATIONS.length) {
    return invalid("Native broker returned an invalid implementation intersection");
  }
  for (const local of IMPLEMENTATIONS) {
    const remote = acknowledged.get(implementationKey(local));
    if (!remote || remote.abiRevision !== local.abiRevision) {
      return invalid("Native broker does not support the required implementation set");
    }
  }
  return { ok: true, value: message };
}

export function validateRequest(message, implementations = IMPLEMENTATIONS) {
  if (!isObject(message) || !hasExactlyFields(message, REQUEST_FIELDS)) {
    return invalid("Request envelope fields are invalid");
  }
  if (message.type !== "request") return invalid("Request type must be request");
  if (typeof message.requestId !== "string" || message.requestId.length === 0) {
    return invalid("requestId must be a nonempty string");
  }
  if (typeof message.method !== "string" || message.method.length === 0) {
    return invalid("method must be a nonempty string");
  }
  if (!isObject(message.params)) return invalid("params must be an object");
  const implementation = implementations.find((entry) => (
    entry.method === message.method && !Object.hasOwn(entry, "branch")
  ));
  const policy = METHOD_POLICIES.get(message.method);
  if (!implementation || !policy) return invalid("method was not negotiated");
  if (message.requestClass !== policy.requestClass) {
    return invalid(`requestClass must be ${policy.requestClass}`);
  }
  if (
    !Number.isInteger(message.deadlineMs) ||
    message.deadlineMs < 1 ||
    message.deadlineMs > policy.deadlineMs
  ) {
    return invalid(`deadlineMs must be an integer from 1 through ${policy.deadlineMs}`);
  }
  return { ok: true, value: message };
}

export function readableRequestId(message) {
  return isObject(message) &&
    typeof message.requestId === "string" &&
    message.requestId.length > 0
    ? message.requestId
    : null;
}

export function sanitizeError(error) {
  const unsafeCode = typeof error?.code === "string" ? error.code : "INTERNAL_ERROR";
  const code = /^[A-Z0-9_]+$/.test(unsafeCode) && unsafeCode.length <= MAX_ERROR_CODE_LENGTH
    ? unsafeCode
    : "INTERNAL_ERROR";
  const unsafeMessage = typeof error?.message === "string"
    ? error.message
    : String(error ?? "Extension request failed");
  const message = unsafeMessage
    .replace(/[\u0000-\u001f\u007f]/g, " ")
    .slice(0, MAX_ERROR_MESSAGE_LENGTH) || "Extension request failed";
  return { code, message };
}

export function publicError(error) {
  if (error?.effectorSafe === true) return error;
  return {
    code: "INTERNAL_ERROR",
    message: "The Chrome extension request failed."
  };
}

function byteLength(value) {
  return new TextEncoder().encode(JSON.stringify(value)).byteLength;
}

function dispatchMetadata(state) {
  if (!DISPATCH_STATES.has(state)) throw new TypeError("Invalid dispatch state");
  return { state };
}

function validateArtifacts(artifacts) {
  if (!Array.isArray(artifacts) || artifacts.length > 1) {
    throw new TypeError("A response may contain at most one artifact");
  }
  for (const artifact of artifacts) {
    if (
      !isObject(artifact) ||
      !hasExactlyFields(artifact, new Set(["type", "mimeType", "data"])) ||
      artifact.type !== "image" ||
      artifact.mimeType !== "image/png"
    ) {
      throw new TypeError("Response artifact must be a PNG image");
    }
    imageArtifact(artifact.data);
  }
}

export function imageArtifact(data) {
  if (typeof data !== "string" || data.length === 0 || data.length % 4 !== 0) {
    throw new TypeError("PNG artifact data must be base64");
  }
  const match = data.match(/={0,2}$/);
  const padding = match ? match[0].length : 0;
  const body = data.slice(0, data.length - padding);
  if (!/^[A-Za-z0-9+/]+$/.test(body) || body.includes("=")) {
    throw new TypeError("PNG artifact data must be base64");
  }
  const decodedBytes = (data.length / 4) * 3 - padding;
  if (decodedBytes > MAX_ARTIFACT_DECODED_BYTES) {
    throw new RangeError("PNG artifact exceeds the decoded byte limit");
  }
  return { type: "image", mimeType: "image/png", data };
}

export function successResponse(
  browserInstanceId,
  requestId,
  result,
  maxBytes = MAX_RESPONSE_BYTES,
  artifacts = []
) {
  validateArtifacts(artifacts);
  const response = {
    type: "response",
    requestId,
    browserInstanceId,
    ok: true,
    result,
    dispatch: dispatchMetadata("completed")
  };
  if (artifacts.length > 0) response.artifacts = artifacts;
  if (byteLength(response) <= maxBytes) return response;
  return errorResponse(browserInstanceId, requestId, {
    code: "RESPONSE_TOO_LARGE",
    message: "Chrome response exceeded the safe message size"
  }, maxBytes);
}

export function errorResponse(
  browserInstanceId,
  requestId,
  error,
  maxBytes = MAX_RESPONSE_BYTES,
  dispatchState = "completed"
) {
  const response = {
    type: "response",
    requestId,
    browserInstanceId,
    ok: false,
    error: sanitizeError(error),
    dispatch: dispatchMetadata(dispatchState)
  };
  if (byteLength(response) > maxBytes) return null;
  return response;
}
