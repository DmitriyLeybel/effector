mod support;

use std::{
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpStream},
};

use rmcp::model::{CallToolRequestParams, ServerNotification, SubscriptionFilter};
use serde_json::Value;
use serde_json::json;

use support::{
    DEFAULT_BROWSER_ID, TestBroker, ToolListObserver, assert_json_golden, capabilities,
    connect_mcp_observer, connect_mcp_subscription, ready,
};

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

    let first_observer = ToolListObserver::default();
    let second_observer = ToolListObserver::default();
    let first_client = connect_mcp_observer(address, &token, first_observer.clone()).await;
    let second_client = connect_mcp_observer(address, &token, second_observer.clone()).await;
    let server_info = first_client.peer_info().unwrap();
    assert_eq!(
        server_info
            .capabilities
            .tools
            .as_ref()
            .and_then(|tools| tools.list_changed),
        Some(true)
    );

    assert!(
        first_client
            .list_tools(None)
            .await
            .unwrap()
            .tools
            .is_empty()
    );
    assert!(
        second_client
            .list_tools(None)
            .await
            .unwrap()
            .tools
            .is_empty()
    );
    assert!(
        first_client
            .call_tool(CallToolRequestParams::new("browser.list"))
            .await
            .is_err(),
        "a direct pre-ready call must not bypass discovery admission"
    );

    broker.write(&ready(DEFAULT_BROWSER_ID, 1, true, true, true));
    let acknowledgement = broker.read();
    assert_eq!(acknowledgement["type"], "ready_ack");
    first_observer.wait_for_count(1).await;
    second_observer.wait_for_count(1).await;

    let tools = first_client.list_tools(None).await.unwrap();
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
    let second_tools = second_client.list_tools(None).await.unwrap();
    assert_eq!(
        tool_names(&tools.tools),
        tool_names(&second_tools.tools),
        "all sessions must read the same process-wide discovery snapshot"
    );

    broker.write(&json!({
        "type":"capabilities_changed",
        "browserInstanceId":DEFAULT_BROWSER_ID,
        "capabilityRevision":2,
        "capabilities":capabilities(false, true, true)
    }));
    first_observer.wait_for_count(2).await;
    second_observer.wait_for_count(2).await;
    assert!(
        first_client
            .list_tools(None)
            .await
            .unwrap()
            .tools
            .is_empty()
    );
    assert!(
        first_client
            .call_tool(CallToolRequestParams::new("browser.snapshot"))
            .await
            .is_err(),
        "a direct call must fail after its capability becomes ineffective"
    );

    let reconnect_observer = ToolListObserver::default();
    let reconnect_client = connect_mcp_observer(address, &token, reconnect_observer.clone()).await;
    reconnect_observer.wait_for_count(1).await;
    assert!(
        reconnect_client
            .list_tools(None)
            .await
            .unwrap()
            .tools
            .is_empty()
    );

    first_client.cancel().await.unwrap();
    second_client.cancel().await.unwrap();
    reconnect_client.cancel().await.unwrap();

    broker.shutdown();
    assert!(TcpStream::connect(address).is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn modern_subscription_receives_tool_list_changes() {
    let mut broker = TestBroker::spawn();
    let client = connect_mcp_subscription(broker.address(), broker.token()).await;
    let mut first_subscription = client
        .listen(SubscriptionFilter::builder().tools_list_changed().build())
        .await
        .unwrap();
    let mut second_subscription = client
        .listen(SubscriptionFilter::builder().tools_list_changed().build())
        .await
        .unwrap();

    broker.write(&ready(DEFAULT_BROWSER_ID, 1, true, true, true));
    assert_eq!(broker.read()["type"], "ready_ack");
    for subscription in [&mut first_subscription, &mut second_subscription] {
        let notification =
            tokio::time::timeout(std::time::Duration::from_secs(5), subscription.next())
                .await
                .expect("timed out waiting for modern tools/list_changed")
                .unwrap()
                .expect("modern subscription ended before notification");
        assert!(matches!(
            notification,
            ServerNotification::ToolListChangedNotification(_)
        ));
    }

    let mut late_subscription = client
        .listen(SubscriptionFilter::builder().tools_list_changed().build())
        .await
        .unwrap();
    let replay = tokio::time::timeout(std::time::Duration::from_secs(5), late_subscription.next())
        .await
        .expect("timed out waiting for ready-state subscription replay")
        .unwrap()
        .expect("late subscription ended before ready-state replay");
    assert!(matches!(
        replay,
        ServerNotification::ToolListChangedNotification(_)
    ));

    first_subscription.cancel().await.unwrap();
    second_subscription.cancel().await.unwrap();
    late_subscription.cancel().await.unwrap();
    client.cancel().await.unwrap();
    broker.shutdown();
}

fn tool_names(tools: &[rmcp::model::Tool]) -> Vec<&str> {
    let mut names: Vec<_> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    names.sort_unstable();
    names
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
