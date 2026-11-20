//! Authenticated file transcription, bounded native JSON intake and standard projection.
use super::{
    exchange::Receiver,
    lifecycle::{Admission, TaskOutput},
};
use crate::{
    ApiError, Credentials, Runtime, StartupError, TranscriptionEntry, body,
    diagnostics::{Stage, Trace},
    protocol::openai::transcription as codec,
    semantic::task::speech_recognition::RecognitionResult,
    topology::{CompiledTopology, transcription::TranscriptionRoute},
};
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use std::{collections::BTreeMap, sync::Arc};
pub(crate) type BoundTranscription = super::BoundTask<TranscriptionRoute>;
pub(crate) fn bind(
    topology: &CompiledTopology,
    entries: Vec<TranscriptionEntry>,
    credentials: &Credentials,
) -> Result<BTreeMap<String, Arc<BoundTranscription>>, StartupError> {
    super::bind(
        topology,
        entries,
        credentials,
        |entry: &TranscriptionEntry| match &entry.binding {
            Some(binding) => topology.transcription_binding(&entry.model, binding),
            None => topology.transcription_route(&entry.model),
        },
        |entry| &entry.model,
    )
}
pub(crate) async fn handle(State(runtime): State<Arc<Runtime>>, request: Request) -> Response {
    ingress(runtime, request)
        .await
        .unwrap_or_else(IntoResponse::into_response)
}
fn upload_head(headers: &HeaderMap, limit: usize) -> Result<(String, Option<u64>), ApiError> {
    let mut types = headers.get_all("content-type").iter();
    let raw = types
        .next()
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            crate::error::ErrorKind::UnsupportedMediaType,
        ))?;
    let media: mime::Mime = raw.parse().map_err(|_| ApiError::invalid())?;
    if raw.len() > 256
        || types.next().is_some()
        || headers.contains_key("content-encoding")
        || media.essence_str() != "multipart/form-data"
    {
        return Err(ApiError::invalid());
    }
    let mut lengths = headers.get_all("content-length").iter();
    let length = lengths
        .next()
        .map(|v| {
            let raw = v.to_str().map_err(|_| ApiError::invalid())?;
            if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ApiError::invalid());
            }
            raw.parse::<u64>().map_err(|_| ApiError::invalid())
        })
        .transpose()?;
    if lengths.next().is_some() {
        return Err(ApiError::invalid());
    }
    if length.is_some_and(|n| n > limit as u64) {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            crate::error::ErrorKind::RequestTooLarge,
        ));
    }
    Ok((raw.into(), length))
}
async fn ingress(runtime: Arc<Runtime>, request: Request) -> Result<Response, ApiError> {
    let (admitted, (media, length), bytes) = Admission::upload(
        &runtime,
        request,
        runtime.limits.transcription_request_bytes,
        upload_head,
    )
    .await?;
    if length.is_some_and(|n| n != bytes.len() as u64) {
        return Err(ApiError::invalid());
    }
    let request = codec::decode_request(&media, bytes.into())
        .await
        .map_err(|_| ApiError::invalid())?;
    let entry = runtime
        .transcriptions
        .get(&request.model)
        .ok_or(ApiError::new(
            StatusCode::NOT_FOUND,
            crate::error::ErrorKind::ModelNotFound,
        ))?
        .clone();
    let prepared =
        crate::execution::transcription::prepare(&entry.route, &entry.provider, &request)
            .map_err(|_| ApiError::invalid())?;
    admitted.respond(runtime, entry, prepared, Output).await
}
async fn decode(
    receiver: Receiver,
    profile: crate::adapter::transcription::UpstreamAdapter,
    trace: &mut Trace,
) -> Result<RecognitionResult, ApiError> {
    receiver.require_media("application/json")?;
    let bytes = receiver.collect(trace).await?;
    profile
        .decode_response(&bytes)
        .map_err(|_| ApiError::upstream())
}
#[cfg(test)]
pub(crate) async fn receive_for_test(
    upstream: reqwest::Response,
    profile: crate::adapter::transcription::UpstreamAdapter,
    limit: usize,
    trace: &mut Trace,
) -> Result<RecognitionResult, ApiError> {
    decode(Receiver::new(upstream, limit, trace)?, profile, trace).await
}
struct Output;
impl TaskOutput for Output {
    type Route = TranscriptionRoute;
    const RESPONSE: body::ResponseType = body::ResponseType::Json;

    fn receive_limit(&self, _: &BoundTranscription, _: &crate::Limits, wire_limit: usize) -> usize {
        wire_limit
    }

    async fn publish(
        self,
        receiver: Receiver,
        entry: &BoundTranscription,
        limits: &crate::Limits,
        lane: &body::Lane<'_>,
        trace: &mut Trace,
    ) -> Result<(), ApiError> {
        let result = decode(receiver, entry.route.endpoint.profile, trace).await?;
        trace.stage(Stage::Projection);
        let projected = codec::project_result(&result).map_err(|_| ApiError::upstream())?;
        crate::semantic::value::json_size(&projected.body, limits.response_bytes)
            .map_err(|_| ApiError::upstream())?;
        let bytes = serde_json::to_vec(&projected.body).map_err(|_| ApiError::upstream())?;
        body::send_final_frame(lane, bytes.into(), trace).await?;
        Ok(())
    }
}
