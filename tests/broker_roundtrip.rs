use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    process::{Command, Stdio},
};

use serde_json::{Value, json};
use uuid::Uuid;

#[test]
fn native_broker_advertises_http_endpoint_and_exits_on_eof() {
    let address = reserve_address();
    let state_dir = std::env::temp_dir().join(format!("effector-test-{}", Uuid::new_v4()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_effector"))
        .arg("native-host")
        .env("EFFECTOR_MCP_ADDRESS", address.to_string())
        .env("EFFECTOR_STATE_DIR", &state_dir)
        .env_remove("EFFECTOR_MCP_TOKEN")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    write_native_frame(
        child.stdin.as_mut().unwrap(),
        &json!({
            "type":"ready",
            "protocolVersion":3,
            "protocolAbiRevision":1,
            "implementations":implementations(),
            "browserInstanceId":"browser-test",
            "extensionId":"extension-test",
            "extensionVersion":"1.0.0",
            "userAgent":"Chrome test",
            "capabilityRevision":1,
            "capabilities":capabilities()
        }),
    );
    let response = read_native_frame(child.stdout.as_mut().unwrap());
    assert_eq!(response["type"], "ready_ack");
    assert_eq!(response["protocolVersion"], 3);
    assert_eq!(response["protocolAbiRevision"], 1);
    assert_eq!(response["implementations"], implementations());
    assert_eq!(response["mcpEndpoint"], format!("http://{address}/mcp"));
    let token = fs::read_to_string(state_dir.join("mcp-token")).unwrap();
    assert_eq!(token.trim().len(), 64);

    drop(child.stdin.take());
    let status = child.wait().unwrap();
    assert!(status.success(), "broker exited with {status}");
    let _ = fs::remove_dir_all(state_dir);
}

#[test]
fn native_broker_rejects_an_incompatible_protocol_version() {
    let address = reserve_address();
    let mut child = Command::new(env!("CARGO_BIN_EXE_effector"))
        .arg("native-host")
        .env("EFFECTOR_MCP_ADDRESS", address.to_string())
        .env(
            "EFFECTOR_MCP_TOKEN",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    write_native_frame(
        child.stdin.as_mut().unwrap(),
        &json!({
            "type":"ready",
            "protocolVersion":2,
            "browserInstanceId":"browser-test",
            "extensionId":"extension-test",
            "extensionVersion":"1.0.0",
            "userAgent":"Chrome test",
            "capabilityRevision":1,
            "capabilities":capabilities()
        }),
    );
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("protocol version mismatch"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_broker_rejects_a_partial_frame_length() {
    let address = reserve_address();
    let mut child = Command::new(env!("CARGO_BIN_EXE_effector"))
        .arg("native-host")
        .env("EFFECTOR_MCP_ADDRESS", address.to_string())
        .env(
            "EFFECTOR_MCP_TOKEN",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    child.stdin.as_mut().unwrap().write_all(&[1]).unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("read complete Native Messaging frame length"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_broker_rejects_a_response_for_another_browser_identity() {
    let address = reserve_address();
    let mut child = Command::new(env!("CARGO_BIN_EXE_effector"))
        .arg("native-host")
        .env("EFFECTOR_MCP_ADDRESS", address.to_string())
        .env(
            "EFFECTOR_MCP_TOKEN",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    write_native_frame(
        child.stdin.as_mut().unwrap(),
        &json!({
            "type":"ready",
            "protocolVersion":3,
            "protocolAbiRevision":1,
            "implementations":implementations(),
            "browserInstanceId":"browser-test",
            "extensionId":"extension-test",
            "extensionVersion":"1.0.0",
            "userAgent":"Chrome test",
            "capabilityRevision":1,
            "capabilities":capabilities()
        }),
    );
    let _ = read_native_frame(child.stdout.as_mut().unwrap());
    write_native_frame(
        child.stdin.as_mut().unwrap(),
        &json!({
            "type":"response",
            "browserInstanceId":"other-browser",
            "requestId":"unknown-request",
            "ok":true,
            "dispatch":{"state":"completed"},
            "result":{}
        }),
    );
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("browser identity"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn reserve_address() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
}

fn capabilities() -> Value {
    json!({
        "browserSnapshot": capability(true),
        "browserChange": capability(false),
        "pageTools": capability(false),
        "advancedEvaluation": capability(false),
        "frozenTabs": true,
        "sharedTabGroups": false
    })
}

fn capability(effective: bool) -> Value {
    json!({
        "implemented":effective,
        "desired":effective,
        "granted":effective,
        "supported":effective,
        "probePassed":effective,
        "effective":effective,
        "reason":if effective { "available" } else { "notImplemented" }
    })
}

fn implementations() -> Value {
    json!([
        {"method":"browser.list","abiRevision":1},
        {"method":"browser.snapshot","abiRevision":1},
        {"method":"tabs.list","abiRevision":1}
    ])
}

fn read_native_frame(input: &mut impl Read) -> Value {
    let mut length = [0_u8; 4];
    input.read_exact(&mut length).unwrap();
    let mut body = vec![0_u8; u32::from_ne_bytes(length) as usize];
    input.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn write_native_frame(output: &mut impl Write, message: &Value) {
    let encoded = serde_json::to_vec(message).unwrap();
    output
        .write_all(&(encoded.len() as u32).to_ne_bytes())
        .unwrap();
    output.write_all(&encoded).unwrap();
    output.flush().unwrap();
}
