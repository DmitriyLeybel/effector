mod support;

use std::{
    io::Write,
    process::{ChildStdin, ChildStdout},
    time::Duration,
};

use rmcp::{
    RoleClient,
    model::{CallToolRequestParams, CallToolResult, ClientInfo},
    service::RunningService,
};
use serde_json::{Map, Value, json};

use support::{
    DEFAULT_BROWSER_ID, TestBroker, assert_json_golden, connect_mcp, normalize_opaque_refs,
    read_native_frame, successful_response, write_native_frame,
};

#[tokio::test(flavor = "multi_thread")]
async fn snapshot_counts_compact_cursor_and_typed_errors_round_trip() {
    let mut broker = TestBroker::spawn();
    let address = broker.address();
    let token = broker.token().to_owned();
    let (mut native_stdin, mut native_stdout) = broker.take_native_io();
    write_native_frame(&mut native_stdin, &ready(true, 1));
    assert_eq!(read_native_frame(&mut native_stdout)["protocolVersion"], 3);

    let client = connect_mcp(address, &token).await;

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
    broker.restore_native_io(native_stdin, native_stdout);
    broker.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn snapshot_filters_scopes_hierarchy_and_cursor_boundaries_round_trip() {
    let mut broker = TestBroker::spawn();
    let address = broker.address();
    let token = broker.token().to_owned();
    let (mut native_stdin, mut native_stdout) = broker.take_native_io();
    write_native_frame(&mut native_stdin, &ready(true, 1));
    assert_eq!(read_native_frame(&mut native_stdout)["type"], "ready_ack");
    let client = connect_mcp(address, &token).await;

    let (first, stdin, stdout) = snapshot_exchange(
        &client,
        native_stdin,
        native_stdout,
        json!({"detail":"full","limit":1}),
        baseline(),
    )
    .await;
    native_stdin = stdin;
    native_stdout = stdout;
    let first = first.structured_content.unwrap();
    let mut representative = first.clone();
    normalize_opaque_refs(&mut representative);
    assert_json_golden(
        "tests/goldens/browser-snapshot-result.json",
        &representative,
    );
    let window_ref = first["windows"][0]["ref"].as_str().unwrap().to_owned();
    let group_ref = first["windows"][0]["items"][0]["group"]["ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let grouped_tab_ref = first["windows"][0]["items"][0]["tabs"][0]["ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let valid_cursor = first["nextCursor"].as_str().unwrap().to_owned();

    let filter_cases = [
        (json!({}), vec!["Grouped tab", "Direct tab"]),
        (json!({"active":true}), vec!["Grouped tab"]),
        (json!({"active":false}), vec!["Direct tab"]),
        (json!({"pinned":true}), vec!["Grouped tab"]),
        (json!({"pinned":false}), vec!["Direct tab"]),
        (json!({"discarded":true}), vec!["Direct tab"]),
        (json!({"discarded":false}), vec!["Grouped tab"]),
        (json!({"frozen":true}), vec!["Grouped tab"]),
        (json!({"frozen":false}), vec!["Direct tab"]),
        (json!({"query":"GROUPED"}), vec!["Grouped tab"]),
        (json!({"query":"/DIRECT"}), vec!["Direct tab"]),
        (json!({"query":"pending"}), vec!["Grouped tab"]),
        (
            json!({"windowRef":window_ref}),
            vec!["Grouped tab", "Direct tab"],
        ),
        (json!({"groupRef":group_ref}), vec!["Grouped tab"]),
        (json!({"tabRefs":[grouped_tab_ref]}), vec!["Grouped tab"]),
        (json!({"query":"no match"}), vec![]),
    ];
    for (arguments, expected_titles) in filter_cases {
        let (result, stdin, stdout) =
            snapshot_exchange(&client, native_stdin, native_stdout, arguments, baseline()).await;
        native_stdin = stdin;
        native_stdout = stdout;
        assert_ne!(result.is_error, Some(true));
        let result = result.structured_content.unwrap();
        assert_eq!(snapshot_titles(&result), expected_titles);
        assert_eq!(result["totalMatched"], expected_titles.len());
    }

    for arguments in [
        json!({"windowRef":window_ref,"groupRef":group_ref}),
        json!({"tabRefs":[grouped_tab_ref,grouped_tab_ref]}),
        json!({"tabRefs":[]}),
        json!({"cursor":valid_cursor,"active":true}),
        json!({"query":"  "}),
        json!({"limit":0}),
        json!({"limit":251}),
    ] {
        assert_error_code(
            client.call_tool(tool_call(arguments)).await.unwrap(),
            "INVALID_ARGUMENT",
        );
    }

    let mut tampered_cursor = valid_cursor.clone();
    tampered_cursor.push('x');
    assert_error_code(
        client
            .call_tool(tool_call(json!({"cursor":tampered_cursor})))
            .await
            .unwrap(),
        "HANDLE_EXPIRED",
    );

    let (not_found, stdin, stdout) = snapshot_exchange(
        &client,
        native_stdin,
        native_stdout,
        json!({"windowRef":"win_00000000000000000000000000000000"}),
        baseline(),
    )
    .await;
    native_stdin = stdin;
    native_stdout = stdout;
    assert_error_code(not_found, "NOT_FOUND");

    let (page, stdin, stdout) = snapshot_exchange(
        &client,
        native_stdin,
        native_stdout,
        json!({"limit":1}),
        hierarchy_baseline(),
    )
    .await;
    native_stdin = stdin;
    native_stdout = stdout;
    let page = page.structured_content.unwrap();
    assert_eq!(snapshot_titles(&page), ["Grouped tab"]);
    assert_eq!(page["windows"][0]["partial"], true);
    assert_eq!(page["windows"][0]["items"][0]["group"]["partial"], true);

    let page = client
        .call_tool(tool_call(json!({"cursor":page["nextCursor"]})))
        .await
        .unwrap()
        .structured_content
        .unwrap();
    assert_eq!(snapshot_titles(&page), ["Second grouped tab"]);
    assert_eq!(page["windows"][0]["partial"], true);
    assert_eq!(page["windows"][0]["items"][0]["group"]["partial"], true);

    let page = client
        .call_tool(tool_call(json!({"cursor":page["nextCursor"]})))
        .await
        .unwrap()
        .structured_content
        .unwrap();
    assert_eq!(snapshot_titles(&page), ["Direct tab"]);
    assert_eq!(page["windows"][0]["partial"], true);

    let page = client
        .call_tool(tool_call(json!({"cursor":page["nextCursor"]})))
        .await
        .unwrap()
        .structured_content
        .unwrap();
    assert_eq!(snapshot_titles(&page), ["Other window tab"]);
    assert!(page["windows"][0].get("partial").is_none());
    assert_eq!(page["nextCursor"], Value::Null);

    client.cancel().await.unwrap();
    broker.restore_native_io(native_stdin, native_stdout);
    broker.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn snapshot_fifo_retention_evicts_only_the_oldest_cursor() {
    let mut broker = TestBroker::spawn();
    let address = broker.address();
    let token = broker.token().to_owned();
    let (mut native_stdin, mut native_stdout) = broker.take_native_io();
    write_native_frame(&mut native_stdin, &ready(true, 1));
    assert_eq!(read_native_frame(&mut native_stdout)["type"], "ready_ack");
    let client = connect_mcp(address, &token).await;

    let mut cursors = Vec::new();
    for _ in 0..17 {
        let (result, stdin, stdout) = snapshot_exchange(
            &client,
            native_stdin,
            native_stdout,
            json!({"limit":1}),
            baseline(),
        )
        .await;
        native_stdin = stdin;
        native_stdout = stdout;
        cursors.push(
            result.structured_content.unwrap()["nextCursor"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }

    assert_error_code(
        client
            .call_tool(tool_call(json!({"cursor":cursors[0]})))
            .await
            .unwrap(),
        "HANDLE_EXPIRED",
    );
    let retained = client
        .call_tool(tool_call(json!({"cursor":cursors[1]})))
        .await
        .unwrap();
    assert_ne!(retained.is_error, Some(true));
    assert_eq!(
        snapshot_titles(&retained.structured_content.unwrap()),
        ["Direct tab"]
    );

    client.cancel().await.unwrap();
    broker.restore_native_io(native_stdin, native_stdout);
    broker.shutdown();
}

async fn snapshot_exchange(
    client: &RunningService<RoleClient, ClientInfo>,
    mut native_stdin: ChildStdin,
    mut native_stdout: ChildStdout,
    arguments: Value,
    snapshot: Value,
) -> (CallToolResult, ChildStdin, ChildStdout) {
    let peer = client.peer().clone();
    let pending = tokio::spawn(async move { peer.call_tool(tool_call(arguments)).await });
    let exchange = tokio::task::spawn_blocking(move || {
        let request = read_native_frame(&mut native_stdout);
        assert_eq!(request["method"], "browser.snapshot");
        assert_eq!(request["params"], json!({}));
        write_snapshot_response(&mut native_stdin, &request, snapshot);
        (native_stdin, native_stdout)
    });
    let result = pending.await.unwrap().unwrap();
    let (native_stdin, native_stdout) = exchange.await.unwrap();
    (result, native_stdin, native_stdout)
}

fn snapshot_titles(snapshot: &Value) -> Vec<&str> {
    snapshot["windows"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|window| window["items"].as_array().unwrap())
        .flat_map(|item| {
            item.get("tabs")
                .and_then(Value::as_array)
                .map(|tabs| tabs.iter().collect::<Vec<_>>())
                .unwrap_or_else(|| vec![item])
        })
        .map(|tab| tab["title"].as_str().unwrap())
        .collect()
}

fn assert_error_code(result: CallToolResult, expected: &str) {
    assert_eq!(result.is_error, Some(true));
    assert_eq!(result.structured_content.unwrap()["code"], expected);
}

fn ready(browser_snapshot: bool, revision: u64) -> Value {
    support::ready(DEFAULT_BROWSER_ID, revision, browser_snapshot, true, true)
}

fn capabilities(browser_snapshot: bool) -> Value {
    support::capabilities(browser_snapshot, true, true)
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
                "pinned":true,"audible":false,"muted":true,"status":"loading",
                "discarded":false,"frozen":true,"autoDiscardable":true,
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

fn hierarchy_baseline() -> Value {
    let mut baseline = baseline();
    baseline["modelRevision"] = json!(8);
    baseline["windows"].as_array_mut().unwrap().push(json!({
        "key":"window-other","id":10,"focused":false,"top":20,"left":30,
        "width":1000,"height":700,"type":"normal","state":"normal",
        "alwaysOnTop":false
    }));
    let tabs = baseline["tabs"].as_array_mut().unwrap();
    tabs.insert(
        1,
        json!({
            "key":"tab-grouped-2","id":102,"windowKey":"window-key",
            "groupKey":"group-key","index":1,"title":"Second grouped tab",
            "url":"https://example.test/grouped-2","active":false,
            "highlighted":false,"pinned":false,"discarded":false,"frozen":false
        }),
    );
    tabs[2]["index"] = json!(2);
    tabs.push(json!({
        "key":"tab-other","id":103,"windowKey":"window-other","index":0,
        "title":"Other window tab","url":"https://example.test/other",
        "active":true,"highlighted":true,"pinned":false,"discarded":false,
        "frozen":false
    }));
    baseline
}

fn write_snapshot_response(output: &mut impl Write, request: &Value, result: Value) {
    write_native_frame(
        output,
        &successful_response(DEFAULT_BROWSER_ID, request, result),
    );
}

fn tool_call(arguments: Value) -> CallToolRequestParams {
    let arguments: Map<String, Value> = arguments.as_object().unwrap().clone();
    CallToolRequestParams::new("browser.snapshot").with_arguments(arguments)
}
