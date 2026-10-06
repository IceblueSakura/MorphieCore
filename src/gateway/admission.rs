//! Authenticated, bounded body collection and public-task admission before I/O.
use super::{ApiError, BoundEntry, Runtime, family};
use crate::{adapter::Request, protocol::openai::Profile};
use axum::{
    body::Body,
    http::{HeaderMap, StatusCode},
};
use futures_util::StreamExt;
use std::sync::Arc;
pub(super) fn headers(headers: &HeaderMap, limit: usize) -> Result<(), ApiError> {
    let mut types = headers.get_all("content-type").iter();
    let value = types.next().ok_or(ApiError::new(
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "unsupported_media_type",
    ))?;
    if types.next().is_some() {
        return Err(ApiError::invalid());
    }
    let media: mime::Mime =
        value
            .to_str()
            .ok()
            .and_then(|v| v.parse().ok())
            .ok_or(ApiError::new(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_media_type",
            ))?;
    if media.type_() != mime::APPLICATION
        || media.subtype() != mime::JSON
        || media
            .params()
            .any(|(k, v)| k == mime::CHARSET && v != mime::UTF_8)
        || headers.contains_key("content-encoding")
    {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
        ));
    }
    if let Some(value) = headers.get("content-length") {
        let length = value
            .to_str()
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or(ApiError::invalid())?;
        if length > limit as u64 {
            return Err(ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "request_too_large",
            ));
        }
    }
    Ok(())
}
pub(super) async fn collect(body: Body, limit: usize) -> Result<Vec<u8>, ApiError> {
    let mut stream = body.into_data_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ApiError::invalid())?;
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "request_too_large",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
pub(super) fn prepare(
    state: &Runtime,
    profile: Profile,
    bytes: &[u8],
) -> Result<(Arc<BoundEntry>, Request), ApiError> {
    // Only envelope identity is inspected here; semantic interpretation starts
    // after resolving the trusted Public Model/task. Duplicate keys remain errors.
    let value = crate::protocol::openai::json::decode(bytes).map_err(|_| ApiError::invalid())?;
    let model = value
        .get("model")
        .and_then(serde_json::Value::as_str)
        .ok_or(ApiError::invalid())?;
    let entry = state
        .entries
        .get(&(family(profile), model.into()))
        .ok_or(ApiError::new(StatusCode::NOT_FOUND, "model_not_found"))?
        .clone();
    let mut request = entry
        .client
        .decode_request(bytes)
        .map_err(|_| ApiError::invalid())?;
    let mut settings = request.task.semantic.settings().clone();
    match settings.controls.max_output_tokens {
        Some(n) if n > state.limits.max_output_tokens => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "output_limit_exceeded",
            ));
        }
        None if entry.public.contract.max_output_tokens => {
            settings.controls.max_output_tokens = Some(state.limits.default_output_tokens);
        }
        _ => {}
    }
    request.task.semantic = request
        .task
        .semantic
        .with_settings(settings)
        .map_err(|_| ApiError::invalid())?;
    crate::execution::admit(&entry.public, &request).map_err(|_| ApiError::invalid())?;
    Ok((entry, request))
}
