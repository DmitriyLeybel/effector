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
use serde_json::{Value, json};

const TOKEN: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

#[tokio::test(flavor = "multi_thread")]
async fn http_mcp_tool_call_reaches_the_extension_boundary_and_returns() {
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
    write_native_frame(
        &mut native_stdin,
        &json!({
            "type":"ready",
            "protocolVersion":1,
            "browserInstanceId":"live-browser-test"
        }),
    );
    let ready_ack = read_native_frame(&mut native_stdout);
    assert_eq!(ready_ack["type"], "ready_ack");

    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(format!("http://{address}/mcp"))
            .auth_header(TOKEN),
    );
    let client = ClientInfo::default().serve(transport).await.unwrap();
    let peer = client.peer().clone();
    let pending_call = tokio::spawn(async move {
        peer.call_tool(CallToolRequestParams::new("browser.list"))
            .await
    });

    let exchange = tokio::task::spawn_blocking(move || {
        let request = read_native_frame(&mut native_stdout);
        assert_eq!(request["method"], "browser.list");
        write_native_frame(
            &mut native_stdin,
            &json!({
                "type":"response",
                "requestId":request["requestId"],
                "ok":true,
                "result":{
                    "browserInstanceId":"live-browser-test",
                    "summary":{"windowCount":2,"groupCount":3,"tabCount":12}
                }
            }),
        );
        (native_stdin, native_stdout)
    });

    let result = pending_call.await.unwrap().unwrap();
    let (native_stdin, native_stdout) = exchange.await.unwrap();
    broker.stdin = Some(native_stdin);
    broker.stdout = Some(native_stdout);
    assert_eq!(
        result.structured_content.unwrap()["browserInstanceId"],
        "live-browser-test"
    );

    client.cancel().await.unwrap();
    drop(broker.stdin.take());
    let status = broker.wait().unwrap();
    assert!(status.success(), "broker exited with {status}");
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
