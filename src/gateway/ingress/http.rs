//! HTTP ingress owns authentication ordering, request deadlines and listener lifecycle.
use crate::adapter::ClientProtocol;
use crate::gateway::{
    ApiError, Gateway, Runtime, diagnostics::Trace, generation::exchange, ingress::admission,
};
use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::{future::Future, sync::Arc};
use tokio::net::TcpListener;
impl Gateway {
    /// Embedding callers own listener security; `serve` enforces loopback itself.
    pub fn router(&self) -> Router {
        Router::new()
            .route(
                "/v1/morphiecore/capabilities",
                get(crate::gateway::ingress::capabilities::retrieve),
            )
            .route("/v1/models", get(crate::gateway::ingress::models::list))
            .route(
                "/v1/models/{model}",
                get(crate::gateway::ingress::models::retrieve)
                    .delete(crate::gateway::ingress::models::delete),
            )
            .route("/v1/chat/completions", post(chat))
            .route("/v1/responses", post(responses))
            .route(
                "/v1/responses/{response_id}",
                get(crate::gateway::ingress::responses::retrieve)
                    .delete(crate::gateway::ingress::responses::delete),
            )
            .route(
                "/v1/responses/{response_id}/input_items",
                get(crate::gateway::ingress::responses::input_items),
            )
            .route("/v1/messages", post(messages))
            .route(
                "/v1/embeddings",
                post(crate::gateway::tasks::embeddings::handle),
            )
            .route(
                "/v1/images/generations",
                post(crate::gateway::tasks::images::handle),
            )
            .route(
                "/v1/audio/speech",
                post(crate::gateway::tasks::speech::handle),
            )
            .route(
                "/v1/audio/transcriptions",
                post(crate::gateway::tasks::transcription::handle),
            )
            .fallback(|uri: axum::http::Uri| async move {
                ingress_error(
                    uri.path(),
                    ApiError::new(StatusCode::NOT_FOUND, "not_found"),
                )
            })
            .method_not_allowed_fallback(|uri: axum::http::Uri| async move {
                ingress_error(
                    uri.path(),
                    ApiError::new(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed"),
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
        self.state.history.clear();
    }
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
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                tokio::select! {
                    _ = shutdown => {},
                    _ = self.state.shutdown.cancelled() => {},
                }
                self.shutdown();
            })
            .await
    }
}
fn ingress_error(path: &str, error: ApiError) -> Response {
    if path == "/v1/messages" || path.starts_with("/v1/messages/") {
        error.into_messages_response()
    } else {
        error.into_response()
    }
}
pub(in crate::gateway) async fn authenticate(
    State(state): State<Arc<Runtime>>,
    mut request: Request,
    next: Next,
) -> Response {
    if !state.auth.accepts(request.headers()) {
        return ingress_error(
            request.uri().path(),
            ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized"),
        );
    }
    let Some(sink) = &state.capture else {
        return next.run(request).await;
    };
    use crate::gateway::capture::Boundary;
    let capture = sink.exchange();
    capture.head(
        Boundary::ClientRequest,
        None,
        request.headers(),
        request.uri().query(),
    );
    let content_type = request
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    request.extensions_mut().insert(capture.clone());
    if let Some(body_capture) = capture.body(Boundary::ClientRequest, None, &content_type) {
        let (parts, body) = request.into_parts();
        request = Request::from_parts(parts, crate::gateway::capture::observe(body, body_capture));
    }
    let response = next.run(request).await;
    capture.head(Boundary::ClientResponse, None, response.headers(), None);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let body_capture = capture.body(Boundary::ClientResponse, None, content_type);
    let (parts, body) = response.into_parts();
    let body = match body_capture {
        Some(capture) => crate::gateway::capture::observe(body, capture),
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
pub(in crate::gateway) async fn handle(
    state: Arc<Runtime>,
    protocol: ClientProtocol,
    request: Request,
) -> Result<Response, ApiError> {
    let mut trace = Trace::new(state.diagnostics.as_ref());
    trace.capture = request
        .extensions()
        .get::<crate::gateway::capture::Exchange>()
        .cloned();
    let permit = admission::acquire_permit(&state)?;
    admission::headers(request.headers(), state.limits.request_bytes)?;
    if protocol == ClientProtocol::Messages {
        admission::messages_version(request.headers())?;
    }
    let conversation = admission::conversation(request.headers())?;
    let bytes = tokio::select! {
        biased;
        _=state.shutdown.cancelled()=>return Err(ApiError::shutdown()),
        result=tokio::time::timeout(state.limits.body_timeout,admission::collect(request.into_body(),state.limits.request_bytes))=>result.map_err(|_|ApiError::new(StatusCode::REQUEST_TIMEOUT,"request_timeout"))??,
    };
    let (entry, mut semantic, history) = match protocol {
        ClientProtocol::Responses => {
            let (entry, request, history) = admission::prepare_responses(&state, &bytes)?;
            (entry, request, Some(Box::new(history)))
        }
        protocol => {
            let (entry, request) = admission::prepare(&state, protocol, &bytes)?;
            (entry, request, None)
        }
    };
    semantic.conversation = Some(conversation);
    trace.request_intake(&semantic.task.fidelity);
    trace.ignored_fields(std::mem::take(&mut semantic.ignored_fields));
    let timeout = entry
        .policy
        .total_timeout_ms
        .map(std::time::Duration::from_millis)
        .map_or(state.limits.exchange_timeout, |timeout| {
            timeout.min(state.limits.exchange_timeout)
        });
    let deadline = tokio::time::Instant::now() + timeout;
    let failed = (protocol == ClientProtocol::Responses && semantic.delivery.streaming())
        .then(|| entry.clone());
    match exchange::run(state, entry, semantic, history, deadline, permit, trace).await {
        Err(error) if error.streams_as_failed() && failed.is_some() => {
            failed_stream(&failed.expect("checked"), &error)
        }
        result => result,
    }
    .map(crate::gateway::ingress::capabilities::notice)
}
/// A Responses stream that failed before its first frame still answers as a
/// stream, ending in `response.failed`.
fn failed_stream(
    entry: &crate::gateway::BoundEntry,
    error: &ApiError,
) -> Result<Response, ApiError> {
    let mut delivery = crate::execution::ResponseDelivery::new(
        entry.client.clone(),
        entry.public.id.as_str(),
        Default::default(),
        Default::default(),
        crate::protocol::openai::obfuscation::Obfuscation::Disabled,
    );
    let frames = delivery.fail_stream(&error.body()).map_err(|_| *error)?;
    Response::builder()
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-store")
        .header("x-content-type-options", "nosniff")
        .body(axum::body::Body::from(frames.concat()))
        .map_err(|_| *error)
}
