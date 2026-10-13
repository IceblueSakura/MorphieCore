//! Authenticated local snapshot operations; none can contact a Provider.
use crate::gateway::{ApiError, Runtime};
use axum::{
    body::Body,
    extract::{Path, RawQuery, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use serde_json::json;
use std::sync::Arc;
use tokio::{sync::OwnedSemaphorePermit, time::Instant};

fn admit(state: &Runtime) -> Result<OwnedSemaphorePermit, ApiError> {
    if state.shutdown.is_cancelled() {
        return Err(ApiError::shutdown());
    }
    state
        .permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::new(StatusCode::TOO_MANY_REQUESTS, "gateway_busy"))
}
fn response(
    bytes: Bytes,
    permit: OwnedSemaphorePermit,
    limit: usize,
) -> Result<Response, ApiError> {
    if bytes.len() > limit {
        return Err(ApiError::upstream());
    }
    let body = Body::from_stream(futures_util::stream::once(async move {
        let _permit = permit;
        Ok::<_, std::io::Error>(bytes)
    }));
    Response::builder()
        .header("content-type", "application/json")
        .header("cache-control", "no-store")
        .header("x-content-type-options", "nosniff")
        .body(body)
        .map_err(|_| ApiError::upstream())
}
pub(in crate::gateway) async fn retrieve(
    State(state): State<Arc<Runtime>>,
    Path(id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    (|| {
        let permit = admit(&state)?;
        crate::protocol::openai::responses::managed::validate_retrieve_query(query.as_deref())
            .map_err(|_| ApiError::invalid())?;
        let snapshot = state.history.get(&id, Instant::now())?;
        response(snapshot.body.clone(), permit, state.limits.response_bytes)
    })()
    .unwrap_or_else(IntoResponse::into_response)
}
pub(in crate::gateway) async fn delete(
    State(state): State<Arc<Runtime>>,
    Path(id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    (|| {
        let permit = admit(&state)?;
        if query.is_some_and(|query| !query.is_empty()) {
            return Err(ApiError::invalid());
        }
        let snapshot = state.history.get(&id, Instant::now())?;
        let bytes = serde_json::to_vec(
            // The Python SDK discards this body; the HTTP API still reports deletion.
            // https://developers.openai.com/api/reference/resources/responses/methods/delete
            &json!({"id":snapshot.id,"object":"response.deleted","deleted":true}),
        )
        .map_err(|_| ApiError::upstream())?;
        if bytes.len() > state.limits.response_bytes {
            return Err(ApiError::upstream());
        }
        state.history.remove(&id, Instant::now())?;
        response(Bytes::from(bytes), permit, state.limits.response_bytes)
    })()
    .unwrap_or_else(IntoResponse::into_response)
}
pub(in crate::gateway) async fn input_items(
    State(state): State<Arc<Runtime>>,
    Path(id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    (|| {
        let permit = admit(&state)?;
        let snapshot = state.history.get(&id, Instant::now())?;
        let value = crate::protocol::openai::responses::managed::input_items(
            &snapshot.input,
            &id,
            query.as_deref(),
        )
        .map_err(|_| ApiError::invalid())?;
        let bytes = serde_json::to_vec(&value).map_err(|_| ApiError::upstream())?;
        response(Bytes::from(bytes), permit, state.limits.response_bytes)
    })()
    .unwrap_or_else(IntoResponse::into_response)
}
