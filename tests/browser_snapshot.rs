use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ClientInfo},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Map, Value, json};

const TOKEN: &str = "123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0";

#[tokio::test(flavor = "multi_thread")]
async fn snapshot_counts_compact_cursor_and_typed_errors_round_trip() {
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

    let mut native_stdin = broker.stdin.take().unwrap();
    let mut native_stdout = broker.stdout.take().unwrap();
    write_native_frame(&mut native_stdin, &ready(true, 1));
    assert_eq!(read_native_frame(&mut native_stdout)["protocolVersion"], 3);

    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(format!("http://{address}/mcp"))
            .auth_header(TOKEN),
    );
    let client = ClientInfo::default().serve(transport).await.unwrap();

    let invalid = client
        .call_tool(tool_call(json!({"detail":"counts","limit":1})))
        .await
        .unwrap();
    assert_eq!(invalid.is_error, Some(true));
    assert_eq!(
        invalid.structured_content.unwrap()["code"],
        "INVALID_ARGUMENT"
    );

    let peer = client.peer().clone();
    let counts_call =
        tokio::spawn(async move { peer.call_tool(tool_call(json!({"detail":"counts"}))).await });
    let exchange = tokio::task::spawn_blocking(move || {
        let request = read_native_frame(&mut native_stdout);
        assert_eq!(request["method"], "browser.snapshot");
        assert_eq!(request["params"], json!({}));
        write_snapshot_response(&mut native_stdin, &request, baseline());
        (native_stdin, native_stdout)
    });
    let counts = counts_call.await.unwrap().unwrap();
    assert_ne!(counts.is_error, Some(true));
    assert_eq!(
        counts.structured_content.unwrap(),
        json!({
            "windowCount": 1,
            "groupCount": 1,
            "tabCount": 2,
            "discardedTabCount": 1
        })
    );
    (native_stdin, native_stdout) = exchange.await.unwrap();

    let peer = client.peer().clone();
    let compact_call = tokio::spawn(async move {
        peer.call_tool(tool_call(json!({"detail":"compact","limit":1})))
            .await
    });
    let exchange = tokio::task::spawn_blocking(move || {
        let request = read_native_frame(&mut native_stdout);
        assert_eq!(request["method"], "browser.snapshot");
        write_snapshot_response(&mut native_stdin, &request, baseline());
        (native_stdin, native_stdout)
    });
    let first = compact_call.await.unwrap().unwrap();
    assert_ne!(first.is_error, Some(true));
    let first = first.structured_content.unwrap();
    assert_eq!(first["totalMatched"], 2);
    assert_eq!(first["windows"][0]["partial"], true);
    assert!(first["windows"][0].get("id").is_none());
    assert!(
        first["windows"][0]["items"][0]["tabs"][0]
            .get("highlighted")
            .is_none()
    );
    let snapshot_ref = first["browserSnapshotRef"].as_str().unwrap().to_owned();
    let cursor = first["nextCursor"].as_str().unwrap().to_owned();
    let stable_tab_ref = first["windows"][0]["items"][0]["tabs"][0]["ref"]
        .as_str()
        .unwrap()
        .to_owned();
    (native_stdin, native_stdout) = exchange.await.unwrap();

    // Continuations are served entirely from immutable broker state.
    let second = client
        .call_tool(tool_call(json!({"cursor":cursor})))
        .await
        .unwrap();
    let second = second.structured_content.unwrap();
    assert_eq!(second["browserSnapshotRef"], snapshot_ref);
    assert_eq!(second["nextCursor"], Value::Null);
    assert_eq!(second["windows"][0]["items"][0]["title"], "Direct tab");

    let replay = client
        .call_tool(tool_call(json!({"cursor":cursor})))
        .await
        .unwrap();
    assert_eq!(replay.structured_content.unwrap(), second);

    let peer = client.peer().clone();
    let full_call =
        tokio::spawn(async move { peer.call_tool(tool_call(json!({"detail":"full"}))).await });
    let exchange = tokio::task::spawn_blocking(move || {
        let request = read_native_frame(&mut native_stdout);
        write_snapshot_response(&mut native_stdin, &request, baseline());
        (native_stdin, native_stdout)
    });
    let full = full_call
        .await
        .unwrap()
        .unwrap()
        .structured_content
        .unwrap();
    assert_eq!(full["windows"][0]["top"], 10);
    assert_eq!(full["windows"][0]["items"][0]["group"]["collapsed"], false);
    assert_eq!(full["windows"][0]["items"][0]["group"]["shared"], false);
    let full_tab = &full["windows"][0]["items"][0]["tabs"][0];
    assert_eq!(full_tab["highlighted"], true);
    assert_eq!(full_tab["muted"], true);
    assert_eq!(full_tab["loading"], true);
    assert_eq!(full_tab["ref"], stable_tab_ref);
    assert!(full_tab["openerRef"].as_str().unwrap().starts_with("tab_"));
    (native_stdin, native_stdout) = exchange.await.unwrap();

    let peer = client.peer().clone();
    let error_call =
        tokio::spawn(async move { peer.call_tool(tool_call(json!({"query":"missing"}))).await });
    let exchange = tokio::task::spawn_blocking(move || {
        let request = read_native_frame(&mut native_stdout);
        write_native_frame(
            &mut native_stdin,
            &json!({
                "type":"response",
                "browserInstanceId":"browser-test",
                "requestId":request["requestId"],
                "ok":false,
                "dispatch":{"state":"completed"},
                "error":{"code":"NOT_FOUND","message":"The requested browser object closed."}
            }),
        );
        (native_stdin, native_stdout)
    });
    let error = error_call.await.unwrap().unwrap();
    assert_eq!(error.is_error, Some(true));
    assert_eq!(error.structured_content.unwrap()["code"], "NOT_FOUND");
    (native_stdin, native_stdout) = exchange.await.unwrap();

    let peer = client.peer().clone();
    let newer_call =
        tokio::spawn(async move { peer.call_tool(tool_call(json!({"detail":"counts"}))).await });
    let exchange = tokio::task::spawn_blocking(move || {
        let request = read_native_frame(&mut native_stdout);
        let mut newer = baseline();
        newer["modelRevision"] = json!(8);
        write_snapshot_response(&mut native_stdin, &request, newer);
        (native_stdin, native_stdout)
    });
    assert_ne!(newer_call.await.unwrap().unwrap().is_error, Some(true));
    (native_stdin, native_stdout) = exchange.await.unwrap();

    let peer = client.peer().clone();
    let inconsistent_call =
        tokio::spawn(async move { peer.call_tool(tool_call(json!({"detail":"counts"}))).await });
    let exchange = tokio::task::spawn_blocking(move || {
        let request = read_native_frame(&mut native_stdout);
        let mut inconsistent = baseline();
        inconsistent["modelRevision"] = json!(8);
        inconsistent["tabs"][0]["title"] = json!("Changed without a revision");
        write_snapshot_response(&mut native_stdin, &request, inconsistent);
        (native_stdin, native_stdout)
    });
    let inconsistent = inconsistent_call.await.unwrap().unwrap();
    assert_eq!(inconsistent.is_error, Some(true));
    assert_eq!(
        inconsistent.structured_content.unwrap()["code"],
        "INTERNAL_ERROR"
    );
    (native_stdin, native_stdout) = exchange.await.unwrap();

    let peer = client.peer().clone();
    let stale_call =
        tokio::spawn(async move { peer.call_tool(tool_call(json!({"detail":"counts"}))).await });
    let exchange = tokio::task::spawn_blocking(move || {
        let request = read_native_frame(&mut native_stdout);
        write_snapshot_response(&mut native_stdin, &request, baseline());
        (native_stdin, native_stdout)
    });
    let stale = stale_call.await.unwrap().unwrap();
    assert_eq!(stale.is_error, Some(true));
    assert_eq!(stale.structured_content.unwrap()["code"], "INTERNAL_ERROR");
    (native_stdin, native_stdout) = exchange.await.unwrap();

    write_native_frame(
        &mut native_stdin,
        &json!({
            "type":"capabilities_changed",
            "browserInstanceId":"browser-test",
            "capabilityRevision":2,
            "capabilities":capabilities(false)
        }),
    );
    tokio::time::sleep(Duration::from_millis(30)).await;
    write_native_frame(
        &mut native_stdin,
        &json!({
            "type":"capabilities_changed",
            "browserInstanceId":"browser-test",
            "capabilityRevision":1,
            "capabilities":capabilities(true)
        }),
    );
    tokio::time::sleep(Duration::from_millis(30)).await;
    let unavailable = client
        .call_tool(tool_call(json!({"detail":"counts"})))
        .await
        .unwrap();
    assert_eq!(unavailable.is_error, Some(true));
    assert_eq!(
        unavailable.structured_content.unwrap()["code"],
        "CAPABILITY_UNAVAILABLE"
    );

    client.cancel().await.unwrap();
    broker.stdin = Some(native_stdin);
    broker.stdout = Some(native_stdout);
    drop(broker.stdin.take());
    let status = broker.wait().unwrap();
    assert!(status.success(), "broker exited with {status}");
}

fn ready(browser_snapshot: bool, revision: u64) -> Value {
    json!({
        "type":"ready",
        "protocolVersion":3,
        "protocolAbiRevision":1,
        "implementations":[
            {"method":"browser.list","abiRevision":1},
            {"method":"browser.snapshot","abiRevision":1},
            {"method":"tabs.list","abiRevision":1}
        ],
        "browserInstanceId":"browser-test",
        "extensionId":"extension-test",
        "extensionVersion":"1.0.0",
        "userAgent":"Chrome test",
        "capabilityRevision":revision,
        "capabilities":capabilities(browser_snapshot)
    })
}

fn capabilities(browser_snapshot: bool) -> Value {
    json!({
        "browserSnapshot":capability(browser_snapshot),
        "browserChange":capability(false),
        "pageTools":capability(false),
        "advancedEvaluation":capability(false),
        "frozenTabs":true,
        "sharedTabGroups":true
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

fn baseline() -> Value {
    json!({
        "browserInstanceId":"browser-test",
        "modelRevision":7,
        "capturedAt":"2026-08-03T12:00:00Z",
        "supportsFrozenTabs":true,
        "supportsSharedTabGroups":true,
        "windows":[{
            "key":"window-key","id":42,"focused":true,"top":10,"left":20,
            "width":1200,"height":800,"type":"normal","state":"normal",
            "alwaysOnTop":false
        }],
        "groups":[{
            "key":"group-key","id":73,"windowKey":"window-key",
            "title":"Work","color":"blue","collapsed":false,"shared":false
        }],
        "tabs":[
            {
                "key":"tab-grouped","id":100,"windowKey":"window-key",
                "groupKey":"group-key","index":0,"title":"Grouped tab",
                "url":"https://example.test/grouped","active":true,
                "pendingUrl":"https://example.test/pending","highlighted":true,
                "pinned":false,"audible":false,"muted":true,"status":"loading",
                "discarded":false,"frozen":false,"autoDiscardable":true,
                "lastAccessed":1234.5,"openerKey":"tab-direct",
                "favIconUrl":"https://example.test/favicon.ico"
            },
            {
                "key":"tab-direct","id":101,"windowKey":"window-key",
                "index":1,"title":"Direct tab","url":"https://example.test/direct",
                "active":false,"highlighted":false,"pinned":false,
                "discarded":true,"frozen":false
            }
        ]
    })
}

fn write_snapshot_response(output: &mut impl Write, request: &Value, result: Value) {
    write_native_frame(
        output,
        &json!({
            "type":"response",
            "browserInstanceId":"browser-test",
            "requestId":request["requestId"],
            "ok":true,
            "dispatch":{"state":"completed"},
            "result":result
        }),
    );
}

fn tool_call(arguments: Value) -> CallToolRequestParams {
    let arguments: Map<String, Value> = arguments.as_object().unwrap().clone();
    CallToolRequestParams::new("browser.snapshot").with_arguments(arguments)
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
    assert!(
        TcpStream::connect(address).is_ok(),
        "MCP endpoint did not bind"
    );
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
