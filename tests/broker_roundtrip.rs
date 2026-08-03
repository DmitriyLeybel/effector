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
            "protocolVersion":1,
            "browserInstanceId":"browser-test"
        }),
    );
    let response = read_native_frame(child.stdout.as_mut().unwrap());
    assert_eq!(response["type"], "ready_ack");
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
            "browserInstanceId":"browser-test"
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

fn reserve_address() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
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
