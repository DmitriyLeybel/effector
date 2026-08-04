use std::{
    collections::HashMap,
    fmt,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
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
use serde::Serialize;
use serde_json::Value;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, stdin, stdout},
    net::TcpListener,
    sync::{Semaphore, mpsc, oneshot},
    time::timeout,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    mcp::EffectorMcp,
    protocol::{
        BrowserMethod, BrowserRequest, DomainError, INTERNAL_PROTOCOL_VERSION, IncomingMessage,
        MAX_CHROME_TO_HOST_BYTES, MAX_HOST_TO_CHROME_BYTES, MAX_PRODUCT_REQUEST_BYTES,
        PROTOCOL_ABI_REVISION, ReadyAck, ResponseMessage, negotiate_implementations,
        parse_incoming,
    },
    runtime::BrokerRuntime,
    settings,
};

const MAX_PENDING_BROWSER_REQUESTS: usize = 128;
const BROWSER_REQUEST_DEADLINE: Duration = Duration::from_secs(30);
type Pending = Arc<Mutex<HashMap<String, PendingRequest>>>;

struct PendingRequest {
    method: BrowserMethod,
    sender: oneshot::Sender<ResponseMessage>,
}

#[derive(Clone)]
pub(crate) struct BrowserRequestHandle {
    native_tx: mpsc::Sender<Vec<u8>>,
    pending: Pending,
    in_flight: Arc<Semaphore>,
    ready: Arc<AtomicBool>,
}

#[derive(Debug)]
pub(crate) enum BrowserRequestError {
    Domain(DomainError),
    Infrastructure(String),
}

impl fmt::Display for BrowserRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => write!(formatter, "{}: {}", error.code, error.message),
            Self::Infrastructure(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for BrowserRequestError {}

struct PendingRequestGuard {
    pending: Pending,
    request_id: String,
}

impl Drop for PendingRequestGuard {
    fn drop(&mut self) {
        pending_requests(&self.pending).remove(&self.request_id);
    }
}

fn pending_requests(pending: &Pending) -> MutexGuard<'_, HashMap<String, PendingRequest>> {
    pending
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn browser_response_result(response: ResponseMessage) -> Result<Value, BrowserRequestError> {
    let ResponseMessage {
        ok, result, error, ..
    } = response;
    if ok {
        result.ok_or_else(|| {
            BrowserRequestError::Infrastructure(
                "Chrome extension returned success without a result".to_owned(),
            )
        })
    } else {
        Err(BrowserRequestError::Domain(error.unwrap_or_else(|| {
            DomainError::new("INTERNAL_ERROR", "The Chrome extension request failed.")
        })))
    }
}

pub async fn run() -> Result<()> {
    let settings = settings::load_mcp_settings()?;
    let listener = TcpListener::bind(settings.address)
        .await
        .with_context(|| format!("bind Effector MCP endpoint at {}", settings.endpoint()))?;
    let endpoint = settings.endpoint();

    let (native_tx, native_rx) = mpsc::channel::<Vec<u8>>(128);
    let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
    let ready = Arc::new(AtomicBool::new(false));
    let runtime = BrokerRuntime::new();
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
    let service_runtime = runtime.clone();
    let service: StreamableHttpService<EffectorMcp, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(EffectorMcp::new(browser.clone(), service_runtime.clone())),
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
        runtime.clone(),
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
    runtime.clear();
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
    native_tx: mpsc::Sender<Vec<u8>>,
    mcp_endpoint: String,
    ready: Arc<AtomicBool>,
    runtime: BrokerRuntime,
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
        match parse_incoming(&payload)? {
            IncomingMessage::Response(message) => {
                if runtime.browser_instance_id().as_deref()
                    != Some(message.browser_instance_id.as_str())
                {
                    bail!("Chrome extension response browser identity did not match ready");
                }
                let request_id = message.request_id.clone();
                if let Some(pending) = pending_requests(&pending).remove(&request_id) {
                    message.validate_for_method(pending.method)?;
                    let _ = pending.sender.send(message);
                } else if !message.artifacts.is_empty() {
                    bail!("unmatched extension response included an artifact");
                }
            }
            IncomingMessage::Ready(message) => {
                if ready.load(Ordering::Acquire) {
                    bail!("Chrome extension sent a duplicate ready message");
                }
                let implementations = negotiate_implementations(&message.implementations)?;
                runtime.connect(message, implementations.clone());
                let acknowledgement = ReadyAck {
                    message_type: "ready_ack",
                    protocol_version: INTERNAL_PROTOCOL_VERSION,
                    protocol_abi_revision: PROTOCOL_ABI_REVISION,
                    implementations: &implementations,
                    broker_pid: std::process::id(),
                    mcp_endpoint: &mcp_endpoint,
                };
                native_tx
                    .send(serialize_outgoing(
                        &acknowledgement,
                        MAX_HOST_TO_CHROME_BYTES,
                    )?)
                    .await
                    .context("queue ready acknowledgement")?;
                ready.store(true, Ordering::Release);
            }
            IncomingMessage::CapabilitiesChanged(message) => {
                runtime
                    .apply_capabilities(message)
                    .map_err(anyhow::Error::msg)?;
            }
        }
    }
}

async fn native_writer(mut messages: mpsc::Receiver<Vec<u8>>) -> Result<()> {
    let mut output = stdout();
    while let Some(payload) = messages.recv().await {
        output
            .write_all(&(payload.len() as u32).to_ne_bytes())
            .await?;
        output.write_all(&payload).await?;
        output.flush().await?;
    }
    Ok(())
}

fn serialize_outgoing(message: &impl Serialize, limit: usize) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(message).context("serialize Native Messaging output")?;
    if payload.len() > MAX_HOST_TO_CHROME_BYTES {
        bail!("Native Messaging output exceeded the 1 MiB hard limit");
    }
    if payload.len() > limit {
        bail!("browser request exceeded the 768 KiB product limit");
    }
    Ok(payload)
}

impl BrowserRequestHandle {
    pub(crate) async fn request(
        &self,
        method: BrowserMethod,
        params: Value,
    ) -> Result<Value, BrowserRequestError> {
        if !self.ready.load(Ordering::Acquire) {
            return Err(BrowserRequestError::Infrastructure(
                "Chrome extension handshake is not complete".to_owned(),
            ));
        }

        let operation = async {
            let started = Instant::now();
            let _permit = self.in_flight.clone().acquire_owned().await.map_err(|_| {
                BrowserRequestError::Infrastructure(
                    "browser request capacity is unavailable".to_owned(),
                )
            })?;
            let extension_budget = BROWSER_REQUEST_DEADLINE
                .saturating_sub(started.elapsed())
                .saturating_sub(Duration::from_secs(1));
            if extension_budget.is_zero() {
                return Err(BrowserRequestError::Infrastructure(
                    "browser request capacity was not available before the deadline".to_owned(),
                ));
            }
            let policy = method.policy();
            let deadline_ms = u32::try_from(extension_budget.as_millis())
                .unwrap_or(u32::MAX)
                .min(policy.deadline_ms);
            let request_id = Uuid::new_v4().to_string();
            let request = BrowserRequest {
                message_type: "request",
                request_id: &request_id,
                method: method.as_str(),
                params,
                request_class: policy.request_class,
                deadline_ms,
            };
            let payload = serialize_outgoing(&request, MAX_PRODUCT_REQUEST_BYTES)
                .map_err(|error| BrowserRequestError::Infrastructure(error.to_string()))?;
            let (sender, receiver) = oneshot::channel();
            pending_requests(&self.pending)
                .insert(request_id.clone(), PendingRequest { method, sender });
            let _pending_guard = PendingRequestGuard {
                pending: self.pending.clone(),
                request_id,
            };

            if self.native_tx.send(payload).await.is_err() {
                return Err(BrowserRequestError::Infrastructure(
                    "Chrome extension disconnected".to_owned(),
                ));
            }

            let response = receiver.await.map_err(|_| {
                BrowserRequestError::Infrastructure(
                    "Chrome extension disconnected before replying".to_owned(),
                )
            })?;
            browser_response_result(response)
        };

        match timeout(BROWSER_REQUEST_DEADLINE, operation).await {
            Ok(result) => result,
            Err(_) => Err(BrowserRequestError::Infrastructure(
                "Chrome extension did not reply before the deadline".to_owned(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::protocol::{
        BrowserRequest, DispatchMetadata, DispatchState, DomainError, MAX_PRODUCT_REQUEST_BYTES,
        READ_DEADLINE_MS, RequestClass, ResponseMessage, ResponseType,
    };

    use super::{
        BrowserRequestError, authorization_matches, browser_response_result, serialize_outgoing,
    };

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
        let response = ResponseMessage {
            _message_type: ResponseType::Response,
            browser_instance_id: "browser".to_owned(),
            request_id: "request".to_owned(),
            ok: true,
            result: Some(json!({"count": 1})),
            error: None,
            dispatch: DispatchMetadata {
                state: DispatchState::Completed,
            },
            artifacts: Vec::new(),
        };
        assert_eq!(
            browser_response_result(response).unwrap(),
            json!({"count": 1})
        );
    }

    #[test]
    fn typed_browser_error_is_preserved() {
        let response = ResponseMessage {
            _message_type: ResponseType::Response,
            browser_instance_id: "browser".to_owned(),
            request_id: "request".to_owned(),
            ok: false,
            result: None,
            error: Some(DomainError::new("NOT_FOUND", "The tab closed.")),
            dispatch: DispatchMetadata {
                state: DispatchState::Completed,
            },
            artifacts: Vec::new(),
        };
        let error = browser_response_result(response).unwrap_err();
        assert!(matches!(error, BrowserRequestError::Domain(error) if error.code == "NOT_FOUND"));
    }

    #[test]
    fn oversized_request_is_rejected_before_queueing() {
        let request_id = "request".to_owned();
        let oversized = BrowserRequest {
            message_type: "request",
            request_id: &request_id,
            method: "browser.snapshot",
            params: json!({"value": "x".repeat(MAX_PRODUCT_REQUEST_BYTES)}),
            request_class: RequestClass::Read,
            deadline_ms: READ_DEADLINE_MS,
        };
        let error = serialize_outgoing(&oversized, MAX_PRODUCT_REQUEST_BYTES)
            .unwrap_err()
            .to_string();
        assert!(error.contains("product limit"), "{error}");
    }
}
