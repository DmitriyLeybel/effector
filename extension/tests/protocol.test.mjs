import assert from "node:assert/strict";
import test from "node:test";

import {
  IMPLEMENTATIONS,
  INTERNAL_PROTOCOL_VERSION,
  PROTOCOL_ABI_REVISION,
  errorResponse,
  readableRequestId,
  publicError,
  sanitizeError,
  imageArtifact,
  successResponse,
  validateReadyAck,
  validateRequest
} from "../protocol.js";

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

test("protocol v3 accepts only the exact negotiated ready acknowledgement", () => {
  const valid = {
    type: "ready_ack",
    protocolVersion: INTERNAL_PROTOCOL_VERSION,
    protocolAbiRevision: PROTOCOL_ABI_REVISION,
    implementations: IMPLEMENTATIONS,
    brokerPid: 123,
    mcpEndpoint: "http://127.0.0.1:37654/mcp"
  };

  assert.equal(validateReadyAck(valid).ok, true);
  assert.equal(validateReadyAck({ ...valid, protocolVersion: 1 }).ok, false);
  assert.equal(validateReadyAck({ ...valid, protocolAbiRevision: 2 }).ok, false);
  assert.equal(validateReadyAck({
    ...valid,
    implementations: [
      ...IMPLEMENTATIONS,
      { method: "page.inspect", abiRevision: 1 }
    ]
  }).ok, false);
  assert.equal(validateReadyAck({ ...valid, brokerPid: 0 }).ok, false);
  assert.equal(validateReadyAck({ ...valid, unknown: true }).ok, false);
  const missing = { ...valid };
  delete missing.brokerPid;
  assert.equal(validateReadyAck(missing).ok, false);
});

test("request validation enforces exact fields, read class, and deadline bounds", () => {
  assert.equal(validateRequest(request()).ok, true);
  assert.equal(validateRequest(request({ deadlineMs: 1 })).ok, true);
  assert.equal(validateRequest(request({ deadlineMs: 0 })).ok, false);
  assert.equal(validateRequest(request({ deadlineMs: 29_001 })).ok, false);
  assert.equal(validateRequest(request({ deadlineMs: 1.5 })).ok, false);
  assert.equal(validateRequest(request({ requestId: "" })).ok, false);
  assert.equal(validateRequest(request({ method: "" })).ok, false);
  assert.equal(validateRequest(request({ params: [] })).ok, false);
  assert.equal(validateRequest(request({ requestClass: "write" })).ok, false);
  assert.equal(validateRequest({ ...request(), extra: true }).ok, false);
  const missing = request();
  delete missing.params;
  assert.equal(validateRequest(missing).ok, false);
});

test("a request ID remains readable from a malformed pre-handshake request", () => {
  assert.equal(readableRequestId({ requestId: "known", extra: true }), "known");
  assert.equal(readableRequestId({ requestId: "" }), null);
  assert.equal(readableRequestId(null), null);
});

test("responses always carry browser identity and have exclusive result/error payloads", () => {
  assert.deepEqual(successResponse("browser-1", "request-1", { value: 1 }), {
    type: "response",
    requestId: "request-1",
    browserInstanceId: "browser-1",
    ok: true,
    result: { value: 1 },
    dispatch: { state: "completed" }
  });
  assert.deepEqual(errorResponse("browser-1", "request-1", {
    code: "NOT_FOUND",
    message: "Missing"
  }), {
    type: "response",
    requestId: "request-1",
    browserInstanceId: "browser-1",
    ok: false,
    error: { code: "NOT_FOUND", message: "Missing" },
    dispatch: { state: "completed" }
  });
});

test("PNG artifacts are typed and bounded", () => {
  assert.deepEqual(imageArtifact("iVBORw=="), {
    type: "image",
    mimeType: "image/png",
    data: "iVBORw=="
  });
  assert.throws(() => imageArtifact("not base64"), TypeError);
  assert.throws(() => successResponse("browser", "request", {}, undefined, [
    imageArtifact("iVBORw=="),
    imageArtifact("iVBORw==")
  ]), TypeError);
});

test("errors are sanitized and both response branches enforce size", () => {
  const sanitized = sanitizeError({
    code: "NOT VALID",
    message: `unsafe\n${"x".repeat(600)}`
  });
  assert.equal(sanitized.code, "INTERNAL_ERROR");
  assert.equal(sanitized.message.includes("\n"), false);
  assert.equal(sanitized.message.length, 512);

  const oversizedSuccess = successResponse(
    "browser-1",
    "request-1",
    { value: "x".repeat(200) },
    220
  );
  assert.equal(oversizedSuccess.ok, false);
  assert.equal(oversizedSuccess.error.code, "RESPONSE_TOO_LARGE");
  assert.equal(errorResponse(
    "browser-1",
    "request-1",
    { code: "INTERNAL_ERROR", message: "x".repeat(200) },
    20
  ), null);
});

test("unexpected extension failures become privacy-safe public errors", () => {
  assert.deepEqual(publicError(new Error("https://private.example/path")), {
    code: "INTERNAL_ERROR",
    message: "The Chrome extension request failed."
  });
  const safe = {
    code: "TIMEOUT",
    message: "Extension request deadline elapsed",
    effectorSafe: true
  };
  assert.equal(publicError(safe), safe);
});
