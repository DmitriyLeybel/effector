use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

const TOKEN: &str = "23456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef01";

#[test]
fn doctor_uses_privacy_safe_snapshot_counts_and_prints_only_health() {
    let address = reserve_address();
    let mut broker = Command::new(env!("CARGO_BIN_EXE_effector"))
        .arg("native-host")
        .env("EFFECTOR_MCP_ADDRESS", address.to_string())
        .env("EFFECTOR_MCP_TOKEN", TOKEN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for_port(address);

    write_native_frame(
        broker.stdin.as_mut().unwrap(),
        &json!({
            "type":"ready",
            "protocolVersion":3,
            "protocolAbiRevision":1,
            "implementations":[
                {"method":"browser.list","abiRevision":1},
                {"method":"browser.snapshot","abiRevision":1},
                {"method":"tabs.list","abiRevision":1}
            ],
            "browserInstanceId":"doctor-browser",
            "extensionId":"extension-test",
            "extensionVersion":"1.0.0",
            "userAgent":"Chrome test",
            "capabilityRevision":1,
            "capabilities":{
                "browserSnapshot":capability(true),
                "browserChange":capability(false),
                "pageTools":capability(false),
                "advancedEvaluation":capability(false),
                "frozenTabs":false,
                "sharedTabGroups":false
            }
        }),
    );
    let _ = read_native_frame(broker.stdout.as_mut().unwrap());

    let doctor = Command::new(env!("CARGO_BIN_EXE_effector"))
        .arg("doctor")
        .env("EFFECTOR_MCP_ADDRESS", address.to_string())
        .env("EFFECTOR_MCP_TOKEN", TOKEN)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let request = read_native_frame(broker.stdout.as_mut().unwrap());
    assert_eq!(request["method"], "browser.snapshot");
    assert_eq!(request["params"], json!({}));
    write_native_frame(
        broker.stdin.as_mut().unwrap(),
        &json!({
            "type":"response",
            "browserInstanceId":"doctor-browser",
            "requestId":request["requestId"],
            "ok":true,
            "dispatch":{"state":"completed"},
            "result":{
                "browserInstanceId":"doctor-browser",
                "modelRevision":1,
                "capturedAt":"2026-08-03T12:00:00Z",
                "supportsFrozenTabs":false,
                "supportsSharedTabGroups":false,
                "windows":[{"key":"window-key","id":1,"focused":true}],
                "groups":[],
                "tabs":[{
                    "key":"tab-key","id":2,"windowKey":"window-key","index":0,
                    "title":"private title","url":"https://private.test/","active":true,
                    "highlighted":true,"pinned":false,"discarded":false
                }]
            }
        }),
    );

    let output = doctor.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "doctor failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Effector MCP broker and Chrome extension are reachable.\n"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("private"));

    drop(broker.stdin.take());
    let status = broker.wait().unwrap();
    assert!(status.success(), "broker exited with {status}");
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

fn reserve_address() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
}

fn wait_for_port(address: SocketAddr) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while TcpStream::connect(address).is_err() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(TcpStream::connect(address).is_ok());
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
