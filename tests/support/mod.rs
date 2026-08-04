#![allow(dead_code)]

use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use rmcp::{
    RoleClient, ServiceExt,
    model::ClientInfo,
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use uuid::Uuid;

pub const DEFAULT_BROWSER_ID: &str = "browser-test";

pub struct TestBroker {
    child: TestChild,
    address: SocketAddr,
    token: String,
    state_dir: PathBuf,
}

pub struct TestChild {
    child: Option<Child>,
    state_dir: PathBuf,
}

impl TestBroker {
    pub fn spawn() -> Self {
        let address = reserve_address();
        let token = test_token();
        let state_dir = test_state_dir();
        Self::spawn_with_installation(address, token, state_dir)
    }

    pub fn spawn_with_installation(address: SocketAddr, token: String, state_dir: PathBuf) -> Self {
        let child = spawn_native_host(address, Some(&token), &state_dir);
        wait_for_port(address);
        Self {
            child,
            address,
            token,
            state_dir,
        }
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    pub fn child_mut(&mut self) -> &mut Child {
        &mut self.child
    }

    pub fn write(&mut self, message: &Value) {
        write_native_frame(self.child.stdin.as_mut().unwrap(), message);
    }

    pub fn read(&mut self) -> Value {
        read_native_frame(self.child.stdout.as_mut().unwrap())
    }

    pub fn take_native_io(&mut self) -> (ChildStdin, ChildStdout) {
        (
            self.child.stdin.take().unwrap(),
            self.child.stdout.take().unwrap(),
        )
    }

    pub fn restore_native_io(&mut self, stdin: ChildStdin, stdout: ChildStdout) {
        self.child.stdin = Some(stdin);
        self.child.stdout = Some(stdout);
    }

    pub fn shutdown(&mut self) {
        drop(self.child.stdin.take());
        let status = wait_for_exit(&mut self.child);
        assert!(status.success(), "broker exited with {status}");
    }
}

impl Drop for TestBroker {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
    }
}

impl TestChild {
    pub fn wait_with_output(&mut self) -> std::io::Result<Output> {
        self.child.take().unwrap().wait_with_output()
    }
}

impl Deref for TestChild {
    type Target = Child;

    fn deref(&self) -> &Self::Target {
        self.child.as_ref().unwrap()
    }
}

impl DerefMut for TestChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.child.as_mut().unwrap()
    }
}

impl Drop for TestChild {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut()
            && child.try_wait().ok().flatten().is_none()
        {
            drop(child.stdin.take());
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_dir_all(&self.state_dir);
    }
}

pub fn spawn_native_host(address: SocketAddr, token: Option<&str>, state_dir: &Path) -> TestChild {
    let mut command = Command::new(env!("CARGO_BIN_EXE_effector"));
    command
        .arg("native-host")
        .env("EFFECTOR_MCP_ADDRESS", address.to_string())
        .env("EFFECTOR_STATE_DIR", state_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(token) = token {
        command.env("EFFECTOR_MCP_TOKEN", token);
    } else {
        command.env_remove("EFFECTOR_MCP_TOKEN");
    }
    TestChild {
        child: Some(command.spawn().unwrap()),
        state_dir: state_dir.to_owned(),
    }
}

pub async fn connect_mcp(
    address: SocketAddr,
    token: &str,
) -> RunningService<RoleClient, ClientInfo> {
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(format!("http://{address}/mcp"))
            .auth_header(token),
    );
    ClientInfo::default().serve(transport).await.unwrap()
}

pub fn ready(
    browser_id: &str,
    capability_revision: u64,
    browser_snapshot: bool,
    frozen_tabs: bool,
    shared_tab_groups: bool,
) -> Value {
    json!({
        "type":"ready",
        "protocolVersion":3,
        "protocolAbiRevision":1,
        "implementations":implementations(),
        "browserInstanceId":browser_id,
        "extensionId":"extension-test",
        "extensionVersion":"1.0.0",
        "userAgent":"Chrome test",
        "capabilityRevision":capability_revision,
        "capabilities":capabilities(browser_snapshot, frozen_tabs, shared_tab_groups)
    })
}

pub fn capabilities(browser_snapshot: bool, frozen_tabs: bool, shared_tab_groups: bool) -> Value {
    json!({
        "browserSnapshot":capability(browser_snapshot),
        "browserChange":capability(false),
        "pageTools":capability(false),
        "advancedEvaluation":capability(false),
        "frozenTabs":frozen_tabs,
        "sharedTabGroups":shared_tab_groups
    })
}

pub fn capability(effective: bool) -> Value {
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

pub fn implementations() -> Value {
    json!([
        {"method":"browser.list","abiRevision":1},
        {"method":"browser.snapshot","abiRevision":1},
        {"method":"tabs.list","abiRevision":1}
    ])
}

pub fn successful_response(browser_id: &str, request: &Value, result: Value) -> Value {
    json!({
        "type":"response",
        "browserInstanceId":browser_id,
        "requestId":request["requestId"],
        "ok":true,
        "dispatch":{"state":"completed"},
        "result":result
    })
}

pub fn assert_json_golden(relative_path: &str, actual: &Value) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative_path);
    let rendered = format!("{}\n", serde_json::to_string_pretty(actual).unwrap());
    if std::env::var("EFFECTOR_UPDATE_GOLDENS").as_deref() == Ok("1") {
        fs::write(&path, &rendered).unwrap();
    }
    let expected = fs::read_to_string(&path).unwrap();
    assert_eq!(rendered, expected, "golden changed: {}", path.display());
}

pub fn normalize_opaque_refs(value: &mut Value) {
    fn replace(
        string: &mut String,
        replacements: &mut HashMap<String, String>,
        counts: &mut HashMap<&'static str, usize>,
    ) {
        let Some(prefix) = ["bs", "cur", "grp", "tab", "win"]
            .into_iter()
            .find(|prefix| string.starts_with(&format!("{prefix}_")))
        else {
            return;
        };
        let replacement = replacements.entry(string.clone()).or_insert_with(|| {
            let count = counts.entry(prefix).or_default();
            *count += 1;
            format!("{prefix}_REF_{count}")
        });
        *string = replacement.clone();
    }

    fn normalize(
        value: &mut Value,
        replacements: &mut HashMap<String, String>,
        counts: &mut HashMap<&'static str, usize>,
    ) {
        match value {
            Value::Array(values) => {
                for value in values {
                    normalize(value, replacements, counts);
                }
            }
            Value::Object(values) => {
                for (field, value) in values {
                    if matches!(
                        field.as_str(),
                        "browserSnapshotRef" | "nextCursor" | "openerRef" | "ref"
                    ) && let Value::String(string) = value
                    {
                        replace(string, replacements, counts);
                    } else {
                        normalize(value, replacements, counts);
                    }
                }
            }
            _ => {}
        }
    }

    normalize(value, &mut HashMap::new(), &mut HashMap::new());
}

pub fn test_state_dir() -> PathBuf {
    std::env::temp_dir().join(format!("effector-test-{}", Uuid::new_v4()))
}

pub fn test_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub fn reserve_address() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
}

pub fn wait_for_port(address: SocketAddr) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while TcpStream::connect(address).is_err() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        TcpStream::connect(address).is_ok(),
        "MCP endpoint did not bind"
    );
}

pub fn wait_for_exit(child: &mut Child) -> ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "broker did not exit after Chrome EOF"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

pub fn read_native_frame(input: &mut impl Read) -> Value {
    let mut length = [0_u8; 4];
    input.read_exact(&mut length).unwrap();
    let mut body = vec![0_u8; u32::from_ne_bytes(length) as usize];
    input.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}

pub fn write_native_frame(output: &mut impl Write, message: &Value) {
    let encoded = serde_json::to_vec(message).unwrap();
    output
        .write_all(&(encoded.len() as u32).to_ne_bytes())
        .unwrap();
    output.write_all(&encoded).unwrap();
    output.flush().unwrap();
}
