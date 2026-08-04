use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use anyhow::{Context, Result, bail};
use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) const INTERNAL_PROTOCOL_VERSION: u32 = 3;
pub(crate) const PROTOCOL_ABI_REVISION: u32 = 1;
pub(crate) const READ_DEADLINE_MS: u32 = 29_000;
pub(crate) const MAX_HOST_TO_CHROME_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_PRODUCT_REQUEST_BYTES: usize = 768 * 1024;
pub(crate) const MAX_CHROME_TO_HOST_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_ARTIFACT_DECODED_BYTES: usize = 8 * 1024 * 1024;
const MAX_IMPLEMENTATION_ENTRIES: usize = 64;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CapabilityStatus {
    pub implemented: bool,
    pub desired: bool,
    pub granted: bool,
    pub supported: bool,
    pub probe_passed: bool,
    pub effective: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Capabilities {
    pub browser_snapshot: CapabilityStatus,
    pub browser_change: CapabilityStatus,
    pub page_tools: CapabilityStatus,
    pub advanced_evaluation: CapabilityStatus,
    pub frozen_tabs: bool,
    pub shared_tab_groups: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ImplementationEntry {
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub abi_revision: u32,
}

pub(crate) fn local_implementations() -> Vec<ImplementationEntry> {
    BrowserMethod::ALL
        .iter()
        .map(|method| ImplementationEntry {
            method: method.as_str().to_owned(),
            branch: None,
            abi_revision: 1,
        })
        .collect()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReadyMessage {
    #[serde(rename = "type")]
    pub _message_type: ReadyType,
    pub protocol_version: u32,
    pub protocol_abi_revision: u32,
    pub implementations: Vec<ImplementationEntry>,
    pub browser_instance_id: String,
    pub extension_id: String,
    pub extension_version: String,
    pub user_agent: String,
    pub capability_revision: u64,
    pub capabilities: Capabilities,
}

#[derive(Debug, Deserialize)]
pub(crate) enum ReadyType {
    #[serde(rename = "ready")]
    Ready,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CapabilitiesChangedMessage {
    #[serde(rename = "type")]
    pub _message_type: CapabilitiesChangedType,
    pub browser_instance_id: String,
    pub capability_revision: u64,
    pub capabilities: Capabilities,
}

#[derive(Debug, Deserialize)]
pub(crate) enum CapabilitiesChangedType {
    #[serde(rename = "capabilities_changed")]
    CapabilitiesChanged,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DomainError {
    pub code: String,
    pub message: String,
}

impl DomainError {
    pub(crate) fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: sanitize_code(code.into()),
            message: sanitize_message(message.into()),
        }
    }

    pub(crate) fn invalid_argument(message: impl Into<String>) -> Self {
        Self::new("INVALID_ARGUMENT", message)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DispatchState {
    NotDispatched,
    Completed,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DispatchMetadata {
    pub state: DispatchState,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum Artifact {
    Image { mime_type: String, data: String },
}

impl Artifact {
    fn validate(&self) -> Result<()> {
        match self {
            Self::Image { mime_type, data } => {
                if mime_type != "image/png" {
                    bail!("extension image artifact must use image/png");
                }
                validate_base64(data)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ResponseMessage {
    #[serde(rename = "type")]
    pub _message_type: ResponseType,
    pub browser_instance_id: String,
    pub request_id: String,
    pub ok: bool,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<DomainError>,
    pub dispatch: DispatchMetadata,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Deserialize)]
pub(crate) enum ResponseType {
    #[serde(rename = "response")]
    Response,
}

impl ResponseMessage {
    pub(crate) fn validate(mut self) -> Result<Self> {
        validate_bounded("response browserInstanceId", &self.browser_instance_id, 256)?;
        validate_bounded("response requestId", &self.request_id, 64)?;
        if self.ok != self.result.is_some() || self.ok == self.error.is_some() {
            bail!("extension response must contain exactly result or error consistent with ok");
        }
        if let Some(error) = self.error.as_mut() {
            error.code = sanitize_code(std::mem::take(&mut error.code));
            error.message = sanitize_message(std::mem::take(&mut error.message));
        }
        if self.artifacts.len() > 1 {
            bail!("extension response included more than one artifact");
        }
        for artifact in &self.artifacts {
            artifact.validate()?;
        }
        if self.ok && self.dispatch.state != DispatchState::Completed {
            bail!("successful extension response must have completed dispatch state");
        }
        Ok(self)
    }

    pub(crate) fn validate_for_method(&self, method: BrowserMethod) -> Result<()> {
        let policy = method.policy();
        if !policy.permits_artifacts && !self.artifacts.is_empty() {
            bail!(
                "extension response included an artifact for method {}",
                method.as_str()
            );
        }
        if policy.request_class == RequestClass::Read
            && self.dispatch.state == DispatchState::Unknown
        {
            bail!("extension returned uncertain dispatch state for a read request");
        }
        Ok(())
    }
}

pub(crate) enum IncomingMessage {
    Ready(ReadyMessage),
    CapabilitiesChanged(CapabilitiesChangedMessage),
    Response(ResponseMessage),
}

pub(crate) fn parse_incoming(payload: &[u8]) -> Result<IncomingMessage> {
    let value: Value = serde_json::from_slice(payload).context("parse extension message")?;
    match value.get("type").and_then(Value::as_str) {
        Some("ready") => {
            let protocol_version = value.get("protocolVersion").and_then(Value::as_u64);
            if protocol_version != Some(u64::from(INTERNAL_PROTOCOL_VERSION)) {
                bail!(
                    "Chrome extension protocol version mismatch: expected {INTERNAL_PROTOCOL_VERSION}, received {}",
                    protocol_version
                        .map(|version| version.to_string())
                        .unwrap_or_else(|| "missing".to_owned())
                );
            }
            let protocol_abi_revision = value.get("protocolAbiRevision").and_then(Value::as_u64);
            if protocol_abi_revision != Some(u64::from(PROTOCOL_ABI_REVISION)) {
                bail!(
                    "Chrome extension protocol ABI mismatch: expected {PROTOCOL_ABI_REVISION}, received {}",
                    protocol_abi_revision
                        .map(|revision| revision.to_string())
                        .unwrap_or_else(|| "missing".to_owned())
                );
            }
            let message: ReadyMessage =
                serde_json::from_value(value).context("decode ready message")?;
            validate_ready(&message)?;
            Ok(IncomingMessage::Ready(message))
        }
        Some("capabilities_changed") => {
            let message: CapabilitiesChangedMessage =
                serde_json::from_value(value).context("decode capabilities_changed message")?;
            validate_bounded(
                "capabilities_changed browserInstanceId",
                &message.browser_instance_id,
                256,
            )?;
            if message.capability_revision == 0 {
                bail!("capabilities_changed capabilityRevision must be at least 1");
            }
            validate_capabilities(&message.capabilities)?;
            Ok(IncomingMessage::CapabilitiesChanged(message))
        }
        Some("response") => {
            let message: ResponseMessage =
                serde_json::from_value(value).context("decode response message")?;
            Ok(IncomingMessage::Response(message.validate()?))
        }
        Some(other) => bail!("unsupported extension message type {other}"),
        None => bail!("extension message omitted type"),
    }
}

fn validate_ready(message: &ReadyMessage) -> Result<()> {
    if message.protocol_version != INTERNAL_PROTOCOL_VERSION {
        bail!(
            "Chrome extension protocol version mismatch: expected {INTERNAL_PROTOCOL_VERSION}, received {}",
            message.protocol_version
        );
    }
    if message.protocol_abi_revision != PROTOCOL_ABI_REVISION {
        bail!(
            "Chrome extension protocol ABI mismatch: expected {PROTOCOL_ABI_REVISION}, received {}",
            message.protocol_abi_revision
        );
    }
    validate_bounded("ready browserInstanceId", &message.browser_instance_id, 256)?;
    validate_bounded("ready extensionId", &message.extension_id, 64)?;
    validate_bounded("ready extensionVersion", &message.extension_version, 64)?;
    validate_bounded("ready userAgent", &message.user_agent, 512)?;
    if message.capability_revision == 0 {
        bail!("ready capabilityRevision must be at least 1");
    }
    validate_implementations(&message.implementations)?;
    validate_capabilities(&message.capabilities)?;
    validate_capability_implementations(&message.capabilities, &message.implementations)?;
    Ok(())
}

pub(crate) fn negotiate_implementations(
    remote: &[ImplementationEntry],
) -> Result<Vec<ImplementationEntry>> {
    validate_implementations(remote)?;
    let local = local_implementations();
    let remote_by_key: HashMap<(&str, Option<&str>), &ImplementationEntry> = remote
        .iter()
        .map(|entry| ((entry.method.as_str(), entry.branch.as_deref()), entry))
        .collect();
    let mut negotiated = Vec::with_capacity(local.len());
    for entry in local {
        let key = (entry.method.as_str(), entry.branch.as_deref());
        let Some(remote_entry) = remote_by_key.get(&key) else {
            bail!(
                "Chrome extension does not implement required method {}",
                entry.method
            );
        };
        if remote_entry.abi_revision != entry.abi_revision {
            bail!(
                "Chrome extension method ABI mismatch for {}: expected {}, received {}",
                entry.method,
                entry.abi_revision,
                remote_entry.abi_revision
            );
        }
        negotiated.push(entry);
    }
    Ok(negotiated)
}

fn validate_implementations(entries: &[ImplementationEntry]) -> Result<()> {
    if entries.is_empty() || entries.len() > MAX_IMPLEMENTATION_ENTRIES {
        bail!(
            "implementation manifest must contain 1 through {MAX_IMPLEMENTATION_ENTRIES} entries"
        );
    }
    let mut keys = HashSet::with_capacity(entries.len());
    for entry in entries {
        validate_bounded("implementation method", &entry.method, 128)?;
        if !entry.method.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.' || byte == b'_'
        }) {
            bail!("implementation method contains invalid characters");
        }
        if let Some(branch) = &entry.branch {
            validate_bounded("implementation branch", branch, 64)?;
        }
        if entry.abi_revision == 0 {
            bail!("implementation abiRevision must be at least 1");
        }
        if !keys.insert((entry.method.as_str(), entry.branch.as_deref())) {
            bail!("implementation manifest contains a duplicate entry");
        }
    }
    Ok(())
}

fn validate_capabilities(capabilities: &Capabilities) -> Result<()> {
    for (name, status) in [
        ("browserSnapshot", &capabilities.browser_snapshot),
        ("browserChange", &capabilities.browser_change),
        ("pageTools", &capabilities.page_tools),
        ("advancedEvaluation", &capabilities.advanced_evaluation),
    ] {
        validate_bounded("capability reason", &status.reason, 64)?;
        if !matches!(
            status.reason.as_str(),
            "available"
                | "disabled"
                | "permissionMissing"
                | "unsupported"
                | "probeFailed"
                | "notImplemented"
                | "dependencyUnavailable"
        ) {
            bail!("capability {name} has an invalid safe reason");
        }
        let effective = status.implemented
            && status.desired
            && status.granted
            && status.supported
            && status.probe_passed;
        if status.effective != effective {
            bail!("capability {name} has inconsistent effective state");
        }
        if status.effective != (status.reason == "available") {
            bail!("capability {name} has an inconsistent reason");
        }
    }
    Ok(())
}

pub(crate) fn validate_capability_implementations(
    capabilities: &Capabilities,
    implementations: &[ImplementationEntry],
) -> Result<()> {
    let has_method = |method: &str| implementations.iter().any(|entry| entry.method == method);
    if capabilities.browser_snapshot.effective && !has_method("browser.snapshot") {
        bail!("browserSnapshot is effective without a browser.snapshot implementation");
    }
    if capabilities.browser_change.effective && !has_method("browser.change") {
        bail!("browserChange is effective without a browser.change implementation");
    }
    if capabilities.page_tools.effective && (!has_method("page.inspect") || !has_method("page.act"))
    {
        bail!("pageTools is effective without page.inspect and page.act implementations");
    }
    if capabilities.advanced_evaluation.effective
        && (!capabilities.page_tools.effective || !has_method("page.evaluate"))
    {
        bail!("advancedEvaluation is effective without Page tools and page.evaluate");
    }
    Ok(())
}

fn validate_nonempty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("Chrome extension {field} must be nonempty");
    }
    Ok(())
}

fn validate_bounded(field: &str, value: &str, max_bytes: usize) -> Result<()> {
    validate_nonempty(field, value)?;
    if value.len() > max_bytes {
        bail!("Chrome extension {field} exceeded {max_bytes} bytes");
    }
    Ok(())
}

fn validate_base64(data: &str) -> Result<()> {
    if data.is_empty() || !data.len().is_multiple_of(4) {
        bail!("extension image artifact data is not valid base64");
    }
    let padding = data.bytes().rev().take_while(|byte| *byte == b'=').count();
    if padding > 2
        || data[..data.len() - padding]
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/'))
        || data[..data.len() - padding].contains('=')
    {
        bail!("extension image artifact data is not valid base64");
    }
    let decoded_bytes = data.len() / 4 * 3 - padding;
    if decoded_bytes > MAX_ARTIFACT_DECODED_BYTES {
        bail!("extension image artifact exceeded the decoded byte limit");
    }
    Ok(())
}

fn sanitize_code(code: String) -> String {
    if !code.is_empty()
        && code.len() <= 64
        && code
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        code
    } else {
        "INTERNAL_ERROR".to_owned()
    }
}

fn sanitize_message(message: String) -> String {
    let mut sanitized = String::with_capacity(message.len().min(512));
    for character in message.trim().chars() {
        if sanitized.len() + character.len_utf8() > 512 {
            break;
        }
        if !character.is_control() || character == ' ' {
            sanitized.push(character);
        }
    }
    if sanitized.is_empty() {
        "The browser request failed.".to_owned()
    } else {
        sanitized
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadyAck<'a> {
    #[serde(rename = "type")]
    pub message_type: &'static str,
    pub protocol_version: u32,
    pub protocol_abi_revision: u32,
    pub implementations: &'a [ImplementationEntry],
    pub broker_pid: u32,
    pub mcp_endpoint: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BrowserRequest<'a> {
    #[serde(rename = "type")]
    pub message_type: &'static str,
    pub request_id: &'a str,
    pub method: &'static str,
    pub params: Value,
    pub request_class: RequestClass,
    pub deadline_ms: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // Reserved v3 wire classes are enabled as their methods ship.
pub(crate) enum RequestClass {
    Read,
    BrowserOperation,
    PageAction,
    Evaluation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BrowserMethod {
    BrowserList,
    BrowserSnapshot,
    TabsList,
}

pub(crate) struct MethodPolicy {
    pub request_class: RequestClass,
    pub deadline_ms: u32,
    pub permits_artifacts: bool,
}

impl BrowserMethod {
    pub(crate) const ALL: [Self; 3] = [Self::BrowserList, Self::BrowserSnapshot, Self::TabsList];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::BrowserList => "browser.list",
            Self::BrowserSnapshot => "browser.snapshot",
            Self::TabsList => "tabs.list",
        }
    }

    pub(crate) const fn policy(self) -> MethodPolicy {
        MethodPolicy {
            request_class: RequestClass::Read,
            deadline_ms: READ_DEADLINE_MS,
            permits_artifacts: false,
        }
    }
}

impl fmt::Display for BrowserMethod {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        BrowserMethod, INTERNAL_PROTOCOL_VERSION, IncomingMessage, PROTOCOL_ABI_REVISION,
        negotiate_implementations, parse_incoming,
    };

    #[test]
    fn ready_requires_the_complete_v3_capability_snapshot() {
        let complete = json!({
            "type": "ready",
            "protocolVersion": INTERNAL_PROTOCOL_VERSION,
            "protocolAbiRevision": PROTOCOL_ABI_REVISION,
            "implementations": [
                {"method":"browser.list","abiRevision":1},
                {"method":"browser.snapshot","abiRevision":1},
                {"method":"tabs.list","abiRevision":1}
            ],
            "browserInstanceId": "browser",
            "extensionId": "extension",
            "extensionVersion": "1.0",
            "userAgent": "Chrome",
            "capabilityRevision": 1,
            "capabilities": {
                "browserSnapshot": capability(true),
                "browserChange": capability(false),
                "pageTools": capability(false),
                "advancedEvaluation": capability(false),
                "frozenTabs": true,
                "sharedTabGroups": false
            }
        });
        assert!(matches!(
            parse_incoming(&serde_json::to_vec(&complete).unwrap()).unwrap(),
            IncomingMessage::Ready(_)
        ));

        let mut contradictory = complete.clone();
        contradictory["capabilities"]["browserChange"] = capability(true);
        assert!(parse_incoming(&serde_json::to_vec(&contradictory).unwrap()).is_err());

        let mut incomplete = complete;
        incomplete["capabilities"]
            .as_object_mut()
            .unwrap()
            .remove("pageTools");
        assert!(parse_incoming(&serde_json::to_vec(&incomplete).unwrap()).is_err());
    }

    #[test]
    fn response_requires_exactly_one_payload() {
        let invalid = json!({
            "type": "response",
            "browserInstanceId": "browser",
            "requestId": "request",
            "ok": true,
            "dispatch": {"state":"completed"},
            "result": {},
            "error": {"code": "NOPE", "message": "nope"}
        });
        assert!(parse_incoming(&serde_json::to_vec(&invalid).unwrap()).is_err());

        let invalid_dispatch = json!({
            "type": "response",
            "browserInstanceId": "browser",
            "requestId": "request",
            "ok": true,
            "dispatch": {"state":"notDispatched"},
            "result": {}
        });
        assert!(parse_incoming(&serde_json::to_vec(&invalid_dispatch).unwrap()).is_err());
    }

    #[test]
    fn implementation_negotiation_ignores_new_remote_methods_but_rejects_changed_abis() {
        let mut remote = super::local_implementations();
        remote.push(super::ImplementationEntry {
            method: "page.inspect".to_owned(),
            branch: Some("semantic".to_owned()),
            abi_revision: 1,
        });
        let negotiated = negotiate_implementations(&remote).unwrap();
        assert_eq!(negotiated, super::local_implementations());

        remote[0].abi_revision = 2;
        assert!(negotiate_implementations(&remote).is_err());
    }

    #[test]
    fn typed_png_artifacts_are_parsed_but_rejected_for_current_read_methods() {
        let response = json!({
            "type": "response",
            "browserInstanceId": "browser",
            "requestId": "request",
            "ok": true,
            "dispatch": {"state":"completed"},
            "result": {},
            "artifacts": [{
                "type":"image",
                "mimeType":"image/png",
                "data":"iVBORw=="
            }]
        });
        let IncomingMessage::Response(response) =
            parse_incoming(&serde_json::to_vec(&response).unwrap()).unwrap()
        else {
            panic!("expected response");
        };
        assert!(
            response
                .validate_for_method(BrowserMethod::BrowserSnapshot)
                .is_err()
        );
    }

    fn capability(effective: bool) -> Value {
        json!({
            "implemented": effective,
            "desired": effective,
            "granted": effective,
            "supported": effective,
            "probePassed": effective,
            "effective": effective,
            "reason": if effective { "available" } else { "notImplemented" }
        })
    }

    use serde_json::Value;
}
