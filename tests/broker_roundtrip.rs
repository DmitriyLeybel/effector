mod support;

use std::{fs, io::Write};

use serde_json::{Value, json};

use support::{
    read_native_frame, reserve_address, spawn_native_host, test_state_dir, test_token,
    write_native_frame,
};

#[test]
fn native_broker_advertises_http_endpoint_and_exits_on_eof() {
    let address = reserve_address();
    let state_dir = test_state_dir();
    let mut child = spawn_native_host(address, None, &state_dir);

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
}

#[test]
fn native_broker_rejects_an_incompatible_protocol_version() {
    let address = reserve_address();
    let state_dir = test_state_dir();
    let token = test_token();
    let mut child = spawn_native_host(address, Some(&token), &state_dir);

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
    let state_dir = test_state_dir();
    let token = test_token();
    let mut child = spawn_native_host(address, Some(&token), &state_dir);

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
    let state_dir = test_state_dir();
    let token = test_token();
    let mut child = spawn_native_host(address, Some(&token), &state_dir);

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

fn capabilities() -> Value {
    support::capabilities(true, true, false)
}

fn implementations() -> Value {
    support::implementations()
}
