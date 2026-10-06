//! HTTP ingress owns authentication ordering, request deadlines and listener lifecycle.
use super::{ApiError, Gateway, Runtime, admission, diagnostics::Trace, exchange};
use crate::protocol::openai::Profile;
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
            .route("/v1/models", get(super::models::list))
            .route(
                "/v1/models/{model}",
                get(super::models::retrieve).delete(super::models::delete),
            )
            .route("/v1/chat/completions", post(chat))
            .route("/v1/responses", post(responses))
            .route("/v1/images/generations", post(super::images::handle))
            .route("/v1/audio/speech", post(super::speech::handle))
            .route(
                "/v1/audio/transcriptions",
                post(super::transcription::handle),
            )
            .fallback(|| async { ApiError::new(StatusCode::NOT_FOUND, "not_found") })
            .method_not_allowed_fallback(|| async {
                ApiError::new(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed")
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
pub(super) async fn authenticate(
    State(state): State<Arc<Runtime>>,
    request: Request,
    next: Next,
) -> Response {
    if !state.auth.accepts(request.headers()) {
        return ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    next.run(request).await
}
async fn chat(State(state): State<Arc<Runtime>>, request: Request) -> Response {
    handle(state, Profile::Chat, request)
        .await
        .unwrap_or_else(IntoResponse::into_response)
}
async fn responses(State(state): State<Arc<Runtime>>, request: Request) -> Response {
    handle(state, Profile::Responses, request)
        .await
        .unwrap_or_else(IntoResponse::into_response)
}
pub(super) async fn handle(
    state: Arc<Runtime>,
    profile: Profile,
    request: Request,
) -> Result<Response, ApiError> {
    let trace = Trace::new(state.diagnostics.as_ref(), request.headers());
    if state.shutdown.is_cancelled() {
        return Err(ApiError::shutdown());
    }
    let permit = state
        .permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::new(StatusCode::TOO_MANY_REQUESTS, "gateway_busy"))?;
    admission::headers(request.headers(), state.limits.request_bytes)?;
    let bytes = tokio::select! {
        biased;
        _=state.shutdown.cancelled()=>return Err(ApiError::shutdown()),
        result=tokio::time::timeout(state.limits.body_timeout,admission::collect(request.into_body(),state.limits.request_bytes))=>result.map_err(|_|ApiError::new(StatusCode::REQUEST_TIMEOUT,"request_timeout"))??,
    };
    let (entry, semantic) = admission::prepare(&state, profile, &bytes)?;
    let deadline = tokio::time::Instant::now() + state.limits.exchange_timeout;
    exchange::run(state, entry, semantic, deadline, permit, trace).await
}
