use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use axum::{
    Router,
    extract::{Request, State},
    http::{
        StatusCode,
        header::{AUTHORIZATION, WWW_AUTHENTICATE},
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, stdin, stdout},
    net::TcpListener,
    sync::{Semaphore, mpsc, oneshot},
    time::timeout,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{mcp::EffectorMcp, settings};

const MAX_HOST_TO_CHROME_BYTES: usize = 1024 * 1024;
const MAX_CHROME_TO_HOST_BYTES: usize = 64 * 1024 * 1024;
const INTERNAL_PROTOCOL_VERSION: u32 = 1;
const MAX_PENDING_BROWSER_REQUESTS: usize = 128;
const BROWSER_REQUEST_DEADLINE: Duration = Duration::from_secs(30);
type Pending = Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>;

#[derive(Clone)]
pub(crate) struct BrowserRequestHandle {
    native_tx: mpsc::Sender<Value>,
    pending: Pending,
    in_flight: Arc<Semaphore>,
    ready: Arc<AtomicBool>,
}

struct PendingRequestGuard {
    pending: Pending,
    request_id: String,
}

impl Drop for PendingRequestGuard {
    fn drop(&mut self) {
        pending_requests(&self.pending).remove(&self.request_id);
    }
}

fn pending_requests(pending: &Pending) -> MutexGuard<'_, HashMap<String, oneshot::Sender<Value>>> {
    pending
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn browser_response_result(response: Value) -> Result<Value> {
    if response.get("ok").and_then(Value::as_bool) == Some(true) {
        response
            .get("result")
            .cloned()
            .context("Chrome extension returned success without a result")
    } else {
        let message = response
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("unknown Chrome extension error");
        bail!("{message}")
    }
}

pub async fn run() -> Result<()> {
    let settings = settings::load_mcp_settings()?;
    let listener = TcpListener::bind(settings.address)
        .await
        .with_context(|| format!("bind Effector MCP endpoint at {}", settings.endpoint()))?;
    let endpoint = settings.endpoint();

    let (native_tx, native_rx) = mpsc::channel::<Value>(128);
    let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
    let ready = Arc::new(AtomicBool::new(false));
    let browser = BrowserRequestHandle {
        native_tx: native_tx.clone(),
        pending: pending.clone(),
        in_flight: Arc::new(Semaphore::new(MAX_PENDING_BROWSER_REQUESTS)),
        ready: ready.clone(),
    };
    let cancellation = CancellationToken::new();

    let http_config = StreamableHttpServerConfig::default()
        .with_json_response(true)
        .with_sse_keep_alive(None)
        .with_cancellation_token(cancellation.child_token())
        .with_allowed_hosts([
            settings.address.to_string(),
            format!("localhost:{}", settings.address.port()),
        ])
        .with_allowed_origins([
            format!("http://{}", settings.address),
            format!("http://localhost:{}", settings.address.port()),
        ]);
    let service: StreamableHttpService<EffectorMcp, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(EffectorMcp::new(browser.clone())),
            Default::default(),
            http_config,
        );
    let expected_token: Arc<str> = settings.token.into();
    let router = Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn_with_state(expected_token, authorize));

    let mut writer = tokio::spawn(native_writer(native_rx));
    let mut reader = tokio::spawn(native_reader(
        pending.clone(),
        native_tx.clone(),
        endpoint,
        ready.clone(),
    ));
    let mut http = tokio::spawn({
        let cancellation = cancellation.clone();
        async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move { cancellation.cancelled_owned().await })
                .await
                .context("serve Effector MCP over HTTP")
        }
    });

    let outcome = tokio::select! {
        result = &mut reader => match result {
            Ok(result) => result,
            Err(error) => Err(error).context("join Native Messaging reader"),
        },
        result = &mut writer => match result {
            Ok(Ok(())) => Err(anyhow!("Native Messaging writer stopped unexpectedly")),
            Ok(Err(error)) => Err(error).context("write Native Messaging output"),
            Err(error) => Err(error).context("join Native Messaging writer"),
        },
        result = &mut http => match result {
            Ok(Ok(())) => Err(anyhow!("Effector MCP HTTP server stopped unexpectedly")),
            Ok(Err(error)) => Err(error),
            Err(error) => Err(error).context("join Effector MCP HTTP server"),
        },
    };

    cancellation.cancel();
    ready.store(false, Ordering::Release);
    pending_requests(&pending).clear();
    drop(native_tx);

    if !reader.is_finished() {
        reader.abort();
        let _ = reader.await;
    }
    let graceful_shutdown = async {
        if !http.is_finished() {
            (&mut http)
                .await
                .context("join Effector MCP HTTP server during shutdown")??;
        }
        if !writer.is_finished() {
            (&mut writer)
                .await
                .context("join Native Messaging writer during shutdown")??;
        }
        Ok(())
    };
    let shutdown_outcome = match timeout(Duration::from_secs(5), graceful_shutdown).await {
        Ok(result) => result,
        Err(_) => Err(anyhow!("Effector broker shutdown exceeded five seconds")),
    };
    if !http.is_finished() {
        http.abort();
        let _ = http.await;
    }
    if !writer.is_finished() {
        writer.abort();
        let _ = writer.await;
    }

    outcome.and(shutdown_outcome)
}

async fn authorize(
    State(expected_token): State<Arc<str>>,
    request: Request,
    next: Next,
) -> Response {
    let authorized = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| authorization_matches(value, &expected_token));
    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            [(WWW_AUTHENTICATE, "Bearer realm=\"effector\"")],
            "Unauthorized",
        )
            .into_response();
    }
    next.run(request).await
}

fn authorization_matches(value: &str, expected_token: &str) -> bool {
    value.split_once(' ').is_some_and(|(scheme, token)| {
        scheme.eq_ignore_ascii_case("bearer") && token == expected_token
    })
}

async fn native_reader(
    pending: Pending,
    native_tx: mpsc::Sender<Value>,
    mcp_endpoint: String,
    ready: Arc<AtomicBool>,
) -> Result<()> {
    let mut input = stdin();
    loop {
        let mut length_bytes = [0_u8; 4];
        match input.read(&mut length_bytes[..1]).await {
            Ok(0) => return Ok(()),
            Ok(_) => {}
            Err(error) => return Err(error).context("read Native Messaging frame length"),
        }
        input
            .read_exact(&mut length_bytes[1..])
            .await
            .context("read complete Native Messaging frame length")?;

        let length = u32::from_ne_bytes(length_bytes) as usize;
        if length > MAX_CHROME_TO_HOST_BYTES {
            bail!("Chrome message exceeded {MAX_CHROME_TO_HOST_BYTES} bytes");
        }
        let mut payload = vec![0_u8; length];
        input
            .read_exact(&mut payload)
            .await
            .context("read Native Messaging frame body")?;
        let message: Value = serde_json::from_slice(&payload).context("parse extension message")?;

        match message.get("type").and_then(Value::as_str) {
            Some("response") => {
                if let Some(request_id) = message.get("requestId").and_then(Value::as_str)
                    && let Some(sender) = pending_requests(&pending).remove(request_id)
                {
                    let _ = sender.send(message);
                }
            }
            Some("ready") => {
                let protocol_version = message.get("protocolVersion").and_then(Value::as_u64);
                if protocol_version != Some(u64::from(INTERNAL_PROTOCOL_VERSION)) {
                    bail!(
                        "Chrome extension protocol version mismatch: expected {INTERNAL_PROTOCOL_VERSION}, received {}",
                        protocol_version
                            .map(|version| version.to_string())
                            .unwrap_or_else(|| "missing".to_owned())
                    );
                }
                if message
                    .get("browserInstanceId")
                    .and_then(Value::as_str)
                    .is_none_or(str::is_empty)
                {
                    bail!("Chrome extension ready message omitted browserInstanceId");
                }
                native_tx
                    .send(json!({
                        "type": "ready_ack",
                        "protocolVersion": INTERNAL_PROTOCOL_VERSION,
                        "brokerPid": std::process::id(),
                        "mcpEndpoint": mcp_endpoint,
                    }))
                    .await
                    .context("queue ready acknowledgement")?;
                ready.store(true, Ordering::Release);
            }
            _ => {}
        }
    }
}

async fn native_writer(mut messages: mpsc::Receiver<Value>) -> Result<()> {
    let mut output = stdout();
    while let Some(message) = messages.recv().await {
        let payload = serde_json::to_vec(&message)?;
        if payload.len() > MAX_HOST_TO_CHROME_BYTES {
            continue;
        }
        output
            .write_all(&(payload.len() as u32).to_ne_bytes())
            .await?;
        output.write_all(&payload).await?;
        output.flush().await?;
    }
    Ok(())
}

impl BrowserRequestHandle {
    pub(crate) async fn request(&self, method: &str, params: Value) -> Result<Value> {
        if !self.ready.load(Ordering::Acquire) {
            bail!("Chrome extension handshake is not complete");
        }

        let operation = async {
            let _permit = self
                .in_flight
                .clone()
                .acquire_owned()
                .await
                .context("acquire browser request capacity")?;
            let request_id = Uuid::new_v4().to_string();
            let request = json!({
                "type": "request",
                "requestId": request_id,
                "method": method,
                "params": params,
            });
            let (sender, receiver) = oneshot::channel();
            pending_requests(&self.pending).insert(request_id.clone(), sender);
            let _pending_guard = PendingRequestGuard {
                pending: self.pending.clone(),
                request_id,
            };

            if self.native_tx.send(request).await.is_err() {
                bail!("Chrome extension disconnected");
            }

            let response = receiver
                .await
                .context("Chrome extension disconnected before replying")?;
            browser_response_result(response)
        };

        match timeout(BROWSER_REQUEST_DEADLINE, operation).await {
            Ok(result) => result,
            Err(_) => bail!("Chrome extension did not reply before the deadline"),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{authorization_matches, browser_response_result};

    #[test]
    fn bearer_scheme_is_case_insensitive_but_token_is_exact() {
        assert!(authorization_matches("Bearer secret", "secret"));
        assert!(authorization_matches("bearer secret", "secret"));
        assert!(!authorization_matches("Basic secret", "secret"));
        assert!(!authorization_matches("Bearer SECRET", "secret"));
        assert!(!authorization_matches("Bearer  secret", "secret"));
    }

    #[test]
    fn successful_browser_response_requires_a_result() {
        assert_eq!(
            browser_response_result(json!({"ok": true, "result": {"count": 1}})).unwrap(),
            json!({"count": 1})
        );
        let error = browser_response_result(json!({"ok": true}))
            .unwrap_err()
            .to_string();
        assert!(error.contains("without a result"), "{error}");
    }
}
