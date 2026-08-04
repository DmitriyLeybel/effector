use std::{
    fmt,
    sync::{
        Arc,
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
use serde::Serialize;
use serde_json::Value;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, stdin, stdout},
    net::TcpListener,
    sync::{Semaphore, mpsc, oneshot},
    time::{Instant, timeout, timeout_at},
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
    request_lifecycle::{RequestLifecycle, RequestWaiterGuard},
    runtime::BrokerRuntime,
    settings,
};

const MAX_PENDING_BROWSER_REQUESTS: usize = 128;
const BROWSER_REQUEST_DEADLINE: Duration = Duration::from_secs(30);

enum NativeWriterMessage {
    Payload(Vec<u8>),
    Request {
        request_id: String,
        method: BrowserMethod,
        params: Value,
        broker_deadline: Instant,
        written: oneshot::Sender<WriterOutcome>,
    },
}

impl NativeWriterMessage {
    fn untracked(payload: Vec<u8>) -> Self {
        Self::Payload(payload)
    }
}

#[derive(Clone, Copy)]
enum WriterOutcome {
    Dispatched,
    Cancelled,
    Expired,
}

enum WaitFailure {
    Deadline,
    Request(BrowserRequestError),
}

#[derive(Clone)]
pub(crate) struct BrowserRequestHandle {
    native_tx: mpsc::Sender<NativeWriterMessage>,
    lifecycle: RequestLifecycle,
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

    let (native_tx, native_rx) = mpsc::channel::<NativeWriterMessage>(128);
    let lifecycle = RequestLifecycle::default();
    let ready = Arc::new(AtomicBool::new(false));
    let runtime = BrokerRuntime::new();
    let in_flight = Arc::new(Semaphore::new(MAX_PENDING_BROWSER_REQUESTS));
    let browser = BrowserRequestHandle {
        native_tx: native_tx.clone(),
        lifecycle: lifecycle.clone(),
        in_flight: in_flight.clone(),
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

    let mut writer = tokio::spawn(native_writer(native_rx, lifecycle.clone(), ready.clone()));
    let mut reader = tokio::spawn(native_reader(
        lifecycle.clone(),
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
    in_flight.close();
    runtime.clear();
    lifecycle.clear();
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
    lifecycle: RequestLifecycle,
    native_tx: mpsc::Sender<NativeWriterMessage>,
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
                if let Some(matched) = lifecycle.take_response(&request_id) {
                    message.validate_for_method(matched.method)?;
                    if let Some(waiter) = matched.waiter {
                        let _ = waiter.send(message);
                    }
                } else {
                    // Every currently reachable method is a read with the same
                    // response policy. Future operation records must outlive
                    // their responses rather than using this fallback.
                    message.validate_for_method(BrowserMethod::BrowserList)?;
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
                    .send(NativeWriterMessage::untracked(serialize_outgoing(
                        &acknowledgement,
                        MAX_HOST_TO_CHROME_BYTES,
                    )?))
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

async fn native_writer(
    mut messages: mpsc::Receiver<NativeWriterMessage>,
    lifecycle: RequestLifecycle,
    ready: Arc<AtomicBool>,
) -> Result<()> {
    let mut output = stdout();
    while let Some(message) = messages.recv().await {
        let (payload, request_id, written) = match message {
            NativeWriterMessage::Payload(payload) => (payload, None, None),
            NativeWriterMessage::Request {
                request_id,
                method,
                params,
                broker_deadline,
                written,
            } => {
                if !ready.load(Ordering::Acquire) {
                    let _ = written.send(WriterOutcome::Cancelled);
                    continue;
                }
                let extension_budget = broker_deadline
                    .saturating_duration_since(Instant::now())
                    .saturating_sub(Duration::from_secs(1));
                if extension_budget.is_zero() {
                    let _ = written.send(WriterOutcome::Expired);
                    continue;
                }
                let policy = method.policy();
                let deadline_ms = u32::try_from(extension_budget.as_millis())
                    .unwrap_or(u32::MAX)
                    .min(policy.deadline_ms);
                if deadline_ms == 0 {
                    let _ = written.send(WriterOutcome::Expired);
                    continue;
                }
                if !lifecycle.begin_dispatch(&request_id) {
                    let _ = written.send(WriterOutcome::Cancelled);
                    continue;
                }
                let request = BrowserRequest {
                    message_type: "request",
                    request_id: &request_id,
                    method: method.as_str(),
                    params,
                    request_class: policy.request_class,
                    deadline_ms,
                };
                (
                    serialize_outgoing(&request, MAX_PRODUCT_REQUEST_BYTES)?,
                    Some(request_id),
                    Some(written),
                )
            }
        };
        output
            .write_all(&(payload.len() as u32).to_ne_bytes())
            .await?;
        output.write_all(&payload).await?;
        output.flush().await?;
        if let Some(request_id) = request_id.as_deref() {
            lifecycle.mark_dispatched(request_id);
        }
        if let Some(written) = written {
            let _ = written.send(WriterOutcome::Dispatched);
        }
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

        let deadline = Instant::now() + BROWSER_REQUEST_DEADLINE;
        let permit = timeout_at(deadline, self.in_flight.clone().acquire_owned())
            .await
            .map_err(|_| {
                BrowserRequestError::Infrastructure(
                    "browser request capacity was not available before the deadline".to_owned(),
                )
            })?
            .map_err(|_| {
                BrowserRequestError::Infrastructure(
                    "browser request capacity is unavailable".to_owned(),
                )
            })?;
        if !self.ready.load(Ordering::Acquire) {
            return Err(BrowserRequestError::Infrastructure(
                "Chrome extension disconnected".to_owned(),
            ));
        }
        let extension_budget = deadline
            .saturating_duration_since(Instant::now())
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
        let preflight_request = BrowserRequest {
            message_type: "request",
            request_id: &request_id,
            method: method.as_str(),
            params: params.clone(),
            request_class: policy.request_class,
            deadline_ms,
        };
        serialize_outgoing(&preflight_request, MAX_PRODUCT_REQUEST_BYTES)
            .map_err(|error| BrowserRequestError::Infrastructure(error.to_string()))?;
        let (response_tx, response_rx) = oneshot::channel();
        self.lifecycle
            .register(request_id.clone(), method, response_tx)
            .map_err(|error| BrowserRequestError::Infrastructure(error.to_owned()))?;
        let mut waiter_guard = RequestWaiterGuard::new(self.lifecycle.clone(), request_id.clone());
        let (written_tx, written_rx) = oneshot::channel();

        let waiting = async {
            self.native_tx
                .send(NativeWriterMessage::Request {
                    request_id: request_id.clone(),
                    method,
                    params,
                    broker_deadline: deadline,
                    written: written_tx,
                })
                .await
                .map_err(|_| {
                    WaitFailure::Request(BrowserRequestError::Infrastructure(
                        "Chrome extension disconnected".to_owned(),
                    ))
                })?;
            let mut written_rx = written_rx;
            let mut response_rx = response_rx;
            tokio::select! {
                biased;
                response = &mut response_rx => response
                    .map_err(|_| WaitFailure::Request(BrowserRequestError::Infrastructure(
                        "Chrome extension disconnected before replying".to_owned(),
                    )))
                    .and_then(|response| browser_response_result(response).map_err(WaitFailure::Request)),
                written = &mut written_rx => match written {
                    Ok(WriterOutcome::Dispatched) => response_rx
                        .await
                        .map_err(|_| WaitFailure::Request(BrowserRequestError::Infrastructure(
                            "Chrome extension disconnected before replying".to_owned(),
                        )))
                        .and_then(|response| browser_response_result(response).map_err(WaitFailure::Request)),
                    Ok(WriterOutcome::Cancelled) => Err(WaitFailure::Request(
                        BrowserRequestError::Infrastructure(
                            "browser request was cancelled before dispatch".to_owned(),
                        ),
                    )),
                    Ok(WriterOutcome::Expired) => Err(WaitFailure::Deadline),
                    Err(_) => Err(WaitFailure::Request(BrowserRequestError::Infrastructure(
                        "Chrome extension disconnected while dispatching".to_owned(),
                    ))),
                }
            }
        };

        let result = match timeout_at(deadline, waiting).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(WaitFailure::Request(error))) => Err(error),
            Ok(Err(WaitFailure::Deadline)) | Err(_) => {
                waiter_guard.timed_out();
                Err(BrowserRequestError::Infrastructure(
                    "Chrome extension did not reply before the deadline".to_owned(),
                ))
            }
        };
        drop(permit);
        result
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
