//! Authenticated, bounded body collection and public-task admission before I/O.
use crate::gateway::{ApiError, BoundEntry, Runtime, family};
use crate::{adapter::Request, protocol::openai::Profile};
use axum::{
    body::Body,
    http::{HeaderMap, StatusCode},
};
use futures_util::StreamExt;
use std::sync::Arc;
pub(in crate::gateway) fn conversation(
    headers: &HeaderMap,
) -> Result<crate::semantic::context::ConversationContext, ApiError> {
    use crate::semantic::context::ConversationContext;
    // The explicit header wins, then the agents' declared sessions: opencode
    // `X-Session-Id`, pi `session_id`, Codex `session-id`, then pi's
    // Chat/Messages `x-session-affinity`. Provider session
    // headers (`x-opencode-session`) are never read.
    let name = [
        "x-morphiecore-conversation-id",
        "x-session-id",
        "session_id",
        "session-id",
        "x-session-affinity",
    ]
    .into_iter()
    .find(|name| headers.contains_key(*name));
    if let Some(name) = name {
        let mut values = headers.get_all(name).iter();
        let value = values.next().expect("present");
        if values.next().is_some() || value.len() > 256 {
            return Err(ApiError::invalid());
        }
        return ConversationContext::conversation(value.to_str().map_err(|_| ApiError::invalid())?)
            .map_err(|_| ApiError::invalid());
    }
    // Authentication has already succeeded. This ID only groups this request;
    // it neither infers a conversation nor fills a cross-request cache key.
    let mut seed = [0_u8; 16];
    getrandom::fill(&mut seed)
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "entropy_unavailable"))?;
    ConversationContext::independent_request(&format!("{:032x}", u128::from_be_bytes(seed)))
        .map_err(|_| ApiError::invalid())
}

pub(in crate::gateway) fn headers(headers: &HeaderMap, limit: usize) -> Result<(), ApiError> {
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
pub(in crate::gateway) async fn collect(body: Body, limit: usize) -> Result<Vec<u8>, ApiError> {
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
pub(in crate::gateway) fn prepare(
    state: &Runtime,
    profile: Profile,
    bytes: &[u8],
) -> Result<(Arc<BoundEntry>, Request), ApiError> {
    prepare_in(state, family(profile), bytes, |entry| {
        entry.client.decode_client_request(bytes)
    })
}
/// Resolve gateway history before ordinary task admission or any upstream I/O.
pub(in crate::gateway) fn prepare_responses(
    state: &Runtime,
    bytes: &[u8],
) -> Result<
    (
        Arc<BoundEntry>,
        Request,
        crate::gateway::generation::history::Pending,
    ),
    ApiError,
> {
    use crate::{gateway::generation::history::Pending, protocol::openai::responses::managed};
    let value = crate::protocol::json::decode(bytes).map_err(|_| ApiError::invalid())?;
    let options = managed::Options::read(&value).map_err(|_| ApiError::invalid())?;
    let previous = options
        .previous
        .as_ref()
        .map(|id| state.history.get(id, tokio::time::Instant::now()))
        .transpose()?;
    let mut input = None;
    let (entry, request) = prepare_in(state, family(Profile::Responses), bytes, |entry| {
        let (request, _) = managed::decode(
            entry.client.as_openai()?.0,
            bytes,
            previous.as_ref().map(|old| (&old.input, &old.response)),
        )?;
        if options.store {
            input = Some(request.task.clone());
        }
        Ok(request)
    })?;
    let pending = Pending::new(options, input, state.history.clone())?;
    Ok((entry, request, pending))
}
/// Messages clients use the model's Chat candidates through their own codec.
pub(in crate::gateway) fn prepare_messages(
    state: &Runtime,
    bytes: &[u8],
) -> Result<(Arc<BoundEntry>, Request), ApiError> {
    prepare_in(state, crate::gateway::MESSAGES_FAMILY, bytes, |entry| {
        entry.client.decode_client_request(bytes)
    })
}
fn prepare_in(
    state: &Runtime,
    family: crate::adapter::ClientProtocol,
    bytes: &[u8],
    decode: impl FnOnce(&BoundEntry) -> Result<Request, crate::protocol::CodecError>,
) -> Result<(Arc<BoundEntry>, Request), ApiError> {
    // Only envelope identity is inspected here; semantic interpretation starts
    // after resolving the trusted Public Model/task. Duplicate keys remain errors.
    let value = crate::protocol::json::decode(bytes).map_err(|_| ApiError::invalid())?;
    let model = value
        .get("model")
        .and_then(serde_json::Value::as_str)
        .ok_or(ApiError::invalid())?;
    let entry = state
        .entries
        .get(&(family, model.into()))
        .ok_or(ApiError::new(StatusCode::NOT_FOUND, "model_not_found"))?
        .clone();
    if value.get("session_id").is_some() {
        return Err(ApiError::invalid());
    }
    let mut request = decode(&entry).map_err(|_| ApiError::invalid())?;
    crate::execution::replay::open_from_client(&mut request).map_err(|_| ApiError::invalid())?;
    crate::execution::admit(&entry.public, &request).map_err(|_| ApiError::invalid())?;
    Ok((entry, request))
}
