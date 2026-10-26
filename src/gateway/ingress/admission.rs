//! Authenticated, bounded body collection and public-task admission before I/O.
use crate::{
    gateway::{ApiError, BoundEntry, Runtime},
    protocol::{
        ClientProtocol, GenerationRequestEnvelope as Request,
        conversion::generation::ConversionBoundary,
    },
};
use axum::{
    body::Body,
    http::{HeaderMap, StatusCode},
};
use futures_util::StreamExt;
use std::sync::Arc;
pub(super) fn acquire_permit(
    state: &Runtime,
) -> Result<tokio::sync::OwnedSemaphorePermit, ApiError> {
    if state.shutdown.is_cancelled() {
        return Err(ApiError::shutdown());
    }
    state.permits.clone().try_acquire_owned().map_err(|_| {
        ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            crate::gateway::error::ErrorKind::GatewayBusy,
        )
    })
}
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
    getrandom::fill(&mut seed).map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            crate::gateway::error::ErrorKind::EntropyUnavailable,
        )
    })?;
    ConversationContext::independent_request(&format!("{:032x}", u128::from_be_bytes(seed)))
        .map_err(|_| ApiError::invalid())
}

pub(in crate::gateway) fn headers(headers: &HeaderMap, limit: usize) -> Result<(), ApiError> {
    let mut types = headers.get_all("content-type").iter();
    let value = types.next().ok_or(ApiError::new(
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        crate::gateway::error::ErrorKind::UnsupportedMediaType,
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
                crate::gateway::error::ErrorKind::UnsupportedMediaType,
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
            crate::gateway::error::ErrorKind::UnsupportedMediaType,
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
                crate::gateway::error::ErrorKind::RequestTooLarge,
            ));
        }
    }
    Ok(())
}
pub(super) fn messages_version(headers: &HeaderMap) -> Result<(), ApiError> {
    let mut versions = headers.get_all("anthropic-version").iter();
    if versions.next().map(|v| v.as_bytes())
        != Some(crate::protocol::anthropic::messages::API_VERSION.as_bytes())
        || versions.next().is_some()
    {
        return Err(ApiError::invalid());
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
                crate::gateway::error::ErrorKind::RequestTooLarge,
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
/// Ordinary codec admission without managed history. Production Responses
/// ingress uses `prepare_responses` to resolve history first.
pub(in crate::gateway) fn prepare(
    state: &Runtime,
    protocol: ClientProtocol,
    bytes: &[u8],
    report: &crate::execution::report::Report,
) -> Result<(Arc<BoundEntry>, Request), ApiError> {
    prepare_in(state, protocol, bytes, report, |entry| {
        entry.client.decode_client_request(bytes)
    })
}
/// Resolve gateway history before ordinary task admission or any upstream I/O.
pub(in crate::gateway) fn prepare_responses(
    state: &Runtime,
    bytes: &[u8],
    report: &crate::execution::report::Report,
) -> Result<
    (
        Arc<BoundEntry>,
        Request,
        crate::gateway::generation::history::Pending,
    ),
    ApiError,
> {
    use crate::{gateway::generation::history::Pending, protocol::openai::responses::managed};
    let value = crate::protocol::json::decode(bytes).map_err(invalid_request(report))?;
    let options = managed::Options::read(&value).map_err(invalid_request(report))?;
    let previous = options
        .previous
        .as_ref()
        .map(|id| state.history.get(id, tokio::time::Instant::now()))
        .transpose()?;
    let mut input = None;
    let (entry, request) = prepare_in(state, ClientProtocol::Responses, bytes, report, |entry| {
        let (request, _) = managed::decode(
            entry.client.as_openai()?.0,
            bytes,
            previous.as_ref().map(|old| (&old.input, &old.response)),
        )?;
        if options.store {
            let mut retained = request.task.clone();
            retained.evidence = Default::default();
            input = Some(retained);
        }
        Ok(request)
    })?;
    let pending = Pending::new(options, input, state.history.clone())?;
    Ok((entry, request, pending))
}
fn prepare_in(
    state: &Runtime,
    family: crate::protocol::ClientProtocol,
    bytes: &[u8],
    report: &crate::execution::report::Report,
    decode: impl FnOnce(&BoundEntry) -> Result<Request, crate::protocol::CodecError>,
) -> Result<(Arc<BoundEntry>, Request), ApiError> {
    // Only envelope identity is inspected here; semantic interpretation starts
    // after resolving the trusted Public Model/task. Duplicate keys remain errors.
    let value = crate::protocol::json::decode(bytes).map_err(invalid_request(report))?;
    let model = value
        .get("model")
        .and_then(serde_json::Value::as_str)
        .ok_or(ApiError::invalid())?;
    let entry = state
        .entries
        .get(&(family, model.into()))
        .ok_or(ApiError::new(
            StatusCode::NOT_FOUND,
            crate::gateway::error::ErrorKind::ModelNotFound,
        ))?
        .clone();
    if value.get("session_id").is_some() {
        return Err(ApiError::invalid());
    }
    let mut request = decode(&entry).map_err(invalid_request(report))?;
    crate::execution::replay::open_from_client(&mut request).map_err(invalid_request(report))?;
    // Decode/replay validation is one checked intake unit. Later public
    // admission cannot erase it or assign it to a candidate attempt.
    report.intake(std::mem::take(&mut request.task.evidence));
    crate::execution::admit(&entry.public, &request).map_err(|error| {
        report.failure(Some(ConversionBoundary::RequestProject), &error);
        ApiError::invalid()
    })?;
    Ok((entry, request))
}
/// Record an undecodable request and answer with the generic invalid-request error.
fn invalid_request(
    report: &crate::execution::report::Report,
) -> impl Fn(crate::protocol::CodecError) -> ApiError + '_ {
    |error| {
        report.failure(
            Some(ConversionBoundary::RequestDecode),
            &crate::execution::AttemptError::Codec(error),
        );
        ApiError::invalid()
    }
}
