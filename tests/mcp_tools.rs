mod support;

use std::{
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpStream},
};

use serde_json::Value;

use support::{TestBroker, assert_json_golden, connect_mcp};

#[tokio::test(flavor = "multi_thread")]
async fn authenticated_http_mcp_server_advertises_browser_tools_and_stops_with_chrome() {
    let mut broker = TestBroker::spawn();
    let address = broker.address();
    let token = broker.token().to_owned();

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
        Some(&format!("Bearer {token}")),
    );
    assert!(!invalid_host.starts_with("HTTP/1.1 200"), "{invalid_host}");

    let invalid_origin = raw_initialize(
        address,
        &address.to_string(),
        Some("https://example.invalid"),
        Some(&format!("Bearer {token}")),
    );
    assert!(
        !invalid_origin.starts_with("HTTP/1.1 200"),
        "{invalid_origin}"
    );

    let client = connect_mcp(address, &token).await;
    let tools = client.list_tools(None).await.unwrap();
    let mut golden_tools: Vec<Value> = tools
        .tools
        .iter()
        .map(|tool| serde_json::to_value(tool).unwrap())
        .collect();
    golden_tools.sort_by(|left, right| {
        left["name"]
            .as_str()
            .unwrap()
            .cmp(right["name"].as_str().unwrap())
    });
    assert_json_golden("tests/goldens/tools-list.json", &Value::Array(golden_tools));
    let mut names: Vec<&str> = tools.tools.iter().map(|tool| tool.name.as_ref()).collect();
    names.sort_unstable();
    assert_eq!(names, ["browser.list", "browser.snapshot", "tabs.list"]);
    for proposed in [
        "browser.change",
        "page.inspect",
        "page.act",
        "page.evaluate",
    ] {
        assert!(!names.contains(&proposed));
    }
    let snapshot = tools
        .tools
        .iter()
        .find(|tool| tool.name.as_ref() == "browser.snapshot")
        .unwrap();
    assert!(snapshot.output_schema.is_some());
    let input_schema = serde_json::to_string(&snapshot.input_schema).unwrap();
    assert!(input_schema.contains("additionalProperties\":false"));
    assert!(!input_schema.contains("\"null\""));
    assert!(snapshot.input_schema.get("required").is_none());
    let output = snapshot.output_schema.as_ref().unwrap();
    let output_schema = serde_json::to_string(output).unwrap();
    assert!(output_schema.contains("browserSnapshotRef"));
    assert!(output_schema.contains("additionalProperties\":false"));
    let is_required = |definition: &str, field: &str| {
        output["$defs"][definition]["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|required| required == field)
    };
    for (definition, omitted_when_false) in [
        ("SnapshotGroup", &["partial"][..]),
        ("SnapshotTab", &["active", "pinned", "discarded"][..]),
        ("SnapshotWindow", &["focused", "partial"][..]),
    ] {
        for field in omitted_when_false {
            assert!(!is_required(definition, field));
        }
    }
    client.cancel().await.unwrap();

    broker.shutdown();
    assert!(TcpStream::connect(address).is_err());
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
