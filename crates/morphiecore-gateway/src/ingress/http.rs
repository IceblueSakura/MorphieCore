//! HTTP ingress owns authentication ordering, request deadlines and listener lifecycle.
use crate::{
    ApiError, Gateway, Runtime, diagnostics::Trace, generation::exchange, ingress::admission,
    protocol::ClientProtocol,
};
use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::{
    future::{Future, IntoFuture},
    sync::Arc,
};
use tokio::net::TcpListener;
use tracing::{Instrument, instrument::WithSubscriber};
impl Gateway {
    /// Embedding callers own listener security; `serve` enforces loopback itself.
    pub fn router(&self) -> Router {
        Router::new()
            .route(
                "/v1/morphiecore/capabilities",
                get(crate::ingress::capabilities::retrieve),
            )
            .route("/v1/models", get(crate::ingress::models::list))
            .route(
                "/v1/morphiecore/models",
                get(crate::ingress::discovery::list),
            )
            .route(
                "/v1/morphiecore/models/{model}",
                get(crate::ingress::discovery::retrieve),
            )
            .route(
                "/v1/models/{model}",
                get(crate::ingress::models::retrieve).delete(crate::ingress::models::delete),
            )
            .route("/v1/chat/completions", post(chat))
            .route("/v1/responses", post(responses))
            .route(
                "/v1/responses/{response_id}",
                get(reject_stateful).delete(reject_stateful),
            )
            .route(
                "/v1/responses/{response_id}/input_items",
                get(reject_stateful),
            )
            .route("/v1/messages", post(messages))
            .route("/v1/embeddings", post(crate::tasks::embeddings::handle))
            .route("/v1/images/generations", post(crate::tasks::images::handle))
            .route("/v1/audio/speech", post(crate::tasks::speech::handle))
            .route(
                "/v1/audio/transcriptions",
                post(crate::tasks::transcription::handle),
            )
            .fallback(|uri: axum::http::Uri| async move {
                ingress_error(
                    uri.path(),
                    ApiError::new(StatusCode::NOT_FOUND, crate::error::ErrorKind::NotFound),
                )
            })
            .method_not_allowed_fallback(|uri: axum::http::Uri| async move {
                ingress_error(
                    uri.path(),
                    ApiError::new(
                        StatusCode::METHOD_NOT_ALLOWED,
                        crate::error::ErrorKind::MethodNotAllowed,
                    ),
                )
            })
            .layer(middleware::from_fn_with_state(
                self.state.clone(),
                authenticate,
            ))
            .with_state(self.state.clone())
    }
    pub fn shutdown(&self) {
        self.state.shutdown.cancel();
    }
    /// Own one listener and its credential maintenance for this runtime.
    pub async fn serve(
        self,
        listener: TcpListener,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> std::io::Result<()> {
        if !listener.local_addr()?.ip().is_loopback() {
            return Err(std::io::Error::other("listener must be loopback"));
        }
        // Listener-future cancellation must also release detached body workers.
        let _owner = self.state.shutdown.clone().drop_guard();
        let router = self.router();
        let maintenance = self.clone();
        let storage_tasks = tokio_util::task::TaskTracker::new();
        let server = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                tokio::select! {
                    _ = shutdown => {},
                    _ = self.state.shutdown.cancelled() => {},
                }
                self.shutdown();
            })
            .into_future();
        tokio::pin!(server);
        let result = tokio::select! {
            biased;
            result = &mut server => result,
            _ = maintenance.renew_credentials(&storage_tasks) => server.await,
        };
        maintenance.shutdown();
        maintenance.state.request_tasks.close();
        maintenance.state.request_tasks.wait().await;
        // Cancellation stops Provider I/O, but queued filesystem commits must
        // preserve rotated material and release account locks before we return.
        storage_tasks.close();
        storage_tasks.wait().await;
        result
    }
}
fn ingress_error(path: &str, error: ApiError) -> Response {
    if path == "/v1/messages" || path.starts_with("/v1/messages/") {
        error.into_messages_response()
    } else {
        error.into_response()
    }
}
/// Keep experimental resource handlers unreachable after ingress authentication.
async fn reject_stateful() -> Response {
    ApiError::stateful_disabled().into_response()
}
pub(crate) async fn authenticate(
    State(state): State<Arc<Runtime>>,
    mut request: Request,
    next: Next,
) -> Response {
    let request_id = state.request_ids.next();
    request.extensions_mut().insert(state.request_tasks.clone());
    if let Some(id) = &request_id {
        request.extensions_mut().insert(id.clone());
    }
    let Some(sink) = &state.diagnostics else {
        return authenticated(state, request, next).await;
    };
    let observation = crate::diagnostics::request::RequestTrace::new(
        sink,
        crate::diagnostics::operation(request.uri().path()),
        request_id.as_ref().map(|id| id.0.as_ref()),
    );
    let response = authenticated(state.clone(), request, next)
        .instrument(observation.span())
        .with_subscriber(sink.dispatch())
        .await;
    observation.response(response)
}
async fn authenticated(state: Arc<Runtime>, mut request: Request, next: Next) -> Response {
    let Some(agent) = state.auth.authenticate(request.headers()) else {
        tracing::warn!(name: "gateway.auth.rejected", target: "morphiecore.request",
            { error.type = "gateway.unauthorized" }, "authentication rejected");
        return ingress_error(
            request.uri().path(),
            ApiError::new(
                StatusCode::UNAUTHORIZED,
                crate::error::ErrorKind::Unauthorized,
            ),
        );
    };
    let request_id = request
        .extensions()
        .get::<crate::ingress::correlation::RequestId>()
        .cloned();
    request.extensions_mut().insert(agent.clone());
    if let Some(id) = &request_id {
        request.extensions_mut().insert(id.clone());
    }
    let Some(diagnostics) = &state.diagnostics else {
        return next.run(request).await;
    };
    let emitted = crate::diagnostics::Emitted::default();
    request.extensions_mut().insert(emitted.clone());
    let operation = crate::diagnostics::operation(request.uri().path());
    let capture = state.capture.as_ref().map(|sink| sink.exchange());
    use crate::capture::Boundary;
    if let Some(capture) = &capture {
        let content_type = request
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let observation = capture.body(Boundary::ClientRequest, None, content_type);
        request.extensions_mut().insert(capture.clone());
        let (parts, body) = request.into_parts();
        request = Request::from_parts(parts, crate::capture::observe(body, observation));
    }
    let response = next.run(request).await;
    let error = response.extensions().get::<ApiError>();
    if response.status().as_u16() >= 400
        && !emitted.observed()
        && !error.is_some_and(|error| error.kind == crate::error::ErrorKind::Shutdown)
    {
        Trace::rejected(
            diagnostics,
            request_id.as_ref(),
            &agent,
            capture.clone(),
            operation,
            error.map_or("gateway.http_rejected", ApiError::diagnostic_code),
            response.status().as_u16(),
        );
    }
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let observation = capture
        .as_ref()
        .map(|capture| capture.body(Boundary::ClientResponse, None, content_type));
    let (parts, body) = response.into_parts();
    let body = match observation {
        Some(capture) => crate::capture::observe(body, capture),
        None => body,
    };
    Response::from_parts(parts, body)
}
async fn chat(State(state): State<Arc<Runtime>>, request: Request) -> Response {
    handle(state, ClientProtocol::Chat, request)
        .await
        .unwrap_or_else(IntoResponse::into_response)
}
async fn responses(State(state): State<Arc<Runtime>>, request: Request) -> Response {
    handle(state, ClientProtocol::Responses, request)
        .await
        .unwrap_or_else(IntoResponse::into_response)
}
/// Anthropic Messages clients (Claude Code); errors use Anthropic's envelope.
async fn messages(State(state): State<Arc<Runtime>>, request: Request) -> Response {
    handle(state, ClientProtocol::Messages, request)
        .await
        .unwrap_or_else(ApiError::into_messages_response)
}
/// A request admitted before any upstream I/O.
struct Admitted {
    permit: tokio::sync::OwnedSemaphorePermit,
    conversation: crate::semantic::context::ConversationContext,
    entry: Arc<crate::BoundEntry>,
    request: crate::protocol::GenerationRequestEnvelope,
}
/// Bounded admission: concurrency, headers, body, then codec and task checks.
async fn admit(
    state: &Runtime,
    protocol: ClientProtocol,
    request: Request,
    deadline: tokio::time::Instant,
    trace: &mut Trace,
) -> Result<Admitted, ApiError> {
    let permit = admission::acquire_permit(state)?;
    admission::headers(request.headers(), state.limits.request_bytes)?;
    if protocol == ClientProtocol::Messages {
        admission::messages_version(request.headers())?;
    }
    let conversation = admission::conversation(request.headers())?;
    let bytes = read_body(
        state,
        request.into_body(),
        state.limits.request_bytes,
        deadline,
        trace,
    )
    .await?;
    let (entry, request) = match protocol {
        ClientProtocol::Responses => admission::prepare_responses(state, &bytes, &trace.report)?,
        protocol => admission::prepare(state, protocol, &bytes, &trace.report)?,
    };
    Ok(Admitted {
        permit,
        conversation,
        entry,
        request,
    })
}
/// Collect within the earlier of the upload ceiling and total request deadline.
pub(crate) async fn read_body(
    state: &Runtime,
    body: axum::body::Body,
    limit: usize,
    deadline: tokio::time::Instant,
    trace: &mut Trace,
) -> Result<Vec<u8>, ApiError> {
    use crate::diagnostics::Outcome;
    use crate::execution::{AttemptError, attempt::DeadlineReason};
    let collect = admission::collect(body, limit);
    let body_deadline = tokio::time::Instant::now() + state.limits.body_timeout;
    let (until, reason, error) = if deadline <= body_deadline {
        (deadline, DeadlineReason::Exchange, ApiError::timeout())
    } else {
        (
            body_deadline,
            DeadlineReason::RequestBody,
            ApiError::new(
                StatusCode::REQUEST_TIMEOUT,
                crate::error::ErrorKind::RequestTimeout,
            ),
        )
    };
    let result = tokio::select! {
        biased;
        _ = state.shutdown.cancelled() => {
            trace.outcome(Outcome::Shutdown);
            Err(ApiError::shutdown())
        }
        _ = tokio::time::sleep_until(until) => {
            trace.report.failure(None, &AttemptError::Deadline(reason));
            trace.outcome(Outcome::Timeout);
            Err(error)
        }
        result = collect => result,
    };
    if let Err(error) = &result {
        trace
            .report
            .request_failure(error.diagnostic_code(), error.status.as_u16());
    }
    result
}
pub(crate) async fn handle(
    state: Arc<Runtime>,
    protocol: ClientProtocol,
    request: Request,
) -> Result<Response, ApiError> {
    let started = tokio::time::Instant::now();
    let service_deadline = started + state.limits.exchange_timeout;
    let mut trace = Trace::for_request(state.diagnostics.as_ref(), &request);

    let Admitted {
        permit,
        conversation,
        entry,
        request: mut semantic,
    } = match admit(&state, protocol, request, service_deadline, &mut trace).await {
        Ok(admitted) => admitted,
        Err(error) => {
            if error.kind == crate::error::ErrorKind::Shutdown {
                trace.outcome(crate::diagnostics::Outcome::Shutdown);
            }
            trace
                .report
                .request_failure(error.diagnostic_code(), error.status.as_u16());
            return Err(error);
        }
    };
    semantic.conversation = Some(conversation);
    semantic.ignored_fields.clear();

    let timeout = entry
        .policy
        .total_timeout_ms
        .map(std::time::Duration::from_millis)
        .map_or(state.limits.exchange_timeout, |timeout| {
            timeout.min(state.limits.exchange_timeout)
        });
    let deadline = started + timeout;
    // Synchronous decoding and an immediately ready body can finish after the
    // clock expired without yielding to the deadline branch.
    if tokio::time::Instant::now() >= deadline {
        trace.outcome(crate::diagnostics::Outcome::Timeout);
        trace
            .report
            .request_failure(ApiError::timeout().diagnostic_code(), 504);
        return Err(ApiError::timeout());
    }
    let failed = (protocol == ClientProtocol::Responses && semantic.delivery.streaming())
        .then(|| entry.clone());
    match exchange::run(state, entry, semantic, deadline, permit, trace).await {
        Err(error) if error.streams_as_failed() && failed.is_some() => {
            failed_stream(&failed.expect("checked"), &error)
        }
        result => result,
    }
    .map(crate::ingress::capabilities::notice)
}
/// A Responses stream that failed before its first frame still answers as a
/// stream with a standard error event, without manufacturing a failed Response.
fn failed_stream(entry: &crate::BoundEntry, error: &ApiError) -> Result<Response, ApiError> {
    let mut delivery = crate::execution::ResponseDelivery::new(
        entry.client.clone(),
        entry.public.id.as_str(),
        Default::default(),
        Default::default(),
        crate::protocol::openai::obfuscation::Obfuscation::Disabled,
        true,
    );
    let frames = delivery.fail_stream(&error.body()).map_err(|_| *error)?;
    Response::builder()
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-store")
        .header("x-content-type-options", "nosniff")
        .body(axum::body::Body::from(frames.concat()))
        .map_err(|_| *error)
}
