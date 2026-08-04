use std::{
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use rmcp::{
    ServiceExt,
    model::ClientInfo,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[tokio::test(flavor = "multi_thread")]
async fn authenticated_http_mcp_server_advertises_browser_tools_and_stops_with_chrome() {
    let address = reserve_address();
    let mut child = spawn_broker(address);
    wait_for_port(address);

    let unauthorized = raw_initialize(address, &address.to_string(), None, None);
    assert!(unauthorized.starts_with("HTTP/1.1 401"), "{unauthorized}");
    assert!(
        unauthorized.contains("www-authenticate: Bearer realm=\"effector\""),
        "{unauthorized}"
    );

    let invalid_host = raw_initialize(
        address,
        "example.invalid",
        None,
        Some(&format!("Bearer {TOKEN}")),
    );
    assert!(!invalid_host.starts_with("HTTP/1.1 200"), "{invalid_host}");

    let invalid_origin = raw_initialize(
        address,
        &address.to_string(),
        Some("https://example.invalid"),
        Some(&format!("Bearer {TOKEN}")),
    );
    assert!(
        !invalid_origin.starts_with("HTTP/1.1 200"),
        "{invalid_origin}"
    );

    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(format!("http://{address}/mcp"))
            .auth_header(TOKEN),
    );
    let client = ClientInfo::default().serve(transport).await.unwrap();
    let tools = client.list_tools(None).await.unwrap();
    let names: Vec<&str> = tools.tools.iter().map(|tool| tool.name.as_ref()).collect();
    assert!(names.contains(&"browser.list"));
    assert!(names.contains(&"browser.snapshot"));
    assert!(names.contains(&"tabs.list"));
    let snapshot = tools
        .tools
        .iter()
        .find(|tool| tool.name.as_ref() == "browser.snapshot")
        .unwrap();
    assert!(snapshot.output_schema.is_some());
    let input_schema = serde_json::to_string(&snapshot.input_schema).unwrap();
    assert!(input_schema.contains("additionalProperties\":false"));
    assert!(!input_schema.contains("\"null\""));
    let output_schema = serde_json::to_string(snapshot.output_schema.as_ref().unwrap()).unwrap();
    assert!(output_schema.contains("browserSnapshotRef"));
    assert!(output_schema.contains("additionalProperties\":false"));
    client.cancel().await.unwrap();

    drop(child.stdin.take());
    wait_for_exit(&mut child);
    assert!(TcpStream::connect(address).is_err());
}

fn reserve_address() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
}

fn spawn_broker(address: SocketAddr) -> Child {
    Command::new(env!("CARGO_BIN_EXE_effector"))
        .arg("native-host")
        .env("EFFECTOR_MCP_ADDRESS", address.to_string())
        .env("EFFECTOR_MCP_TOKEN", TOKEN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
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

fn raw_initialize(
    address: SocketAddr,
    host: &str,
    origin: Option<&str>,
    authorization: Option<&str>,
) -> String {
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#;
    let mut stream = TcpStream::connect(address).unwrap();
    let origin = origin
        .map(|value| format!("Origin: {value}\r\n"))
        .unwrap_or_default();
    let authorization = authorization
        .map(|value| format!("Authorization: {value}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: {host}\r\n{origin}{authorization}Content-Type: application/json\r\nAccept: application/json, text/event-stream\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut response_headers = String::new();
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" || line.is_empty() {
            break;
        }
        response_headers.push_str(&line);
    }
    response_headers
}

fn wait_for_exit(child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "broker exited with {status}");
            return;
        }
        assert!(
            Instant::now() < deadline,
            "broker did not exit after Chrome EOF"
        );
        thread::sleep(Duration::from_millis(20));
    }
}
