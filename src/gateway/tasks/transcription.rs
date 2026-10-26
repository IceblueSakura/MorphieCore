//! Authenticated file transcription, bounded native JSON intake and standard projection.
use crate::{
    gateway::{
        ApiError, Credentials, Runtime, StartupError, TranscriptionEntry, body,
        diagnostics::{Stage, Trace},
        ingress::admission,
    },
    protocol::openai::transcription as codec,
    semantic::task::speech_recognition::RecognitionResult,
    topology::{CompiledTopology, transcription::TranscriptionRoute},
};
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::time::Instant;
pub(in crate::gateway) type BoundTranscription = super::BoundTask<TranscriptionRoute>;
pub(in crate::gateway) fn bind(
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
        |entry| entry.model,
    )
}
pub(in crate::gateway) async fn handle(
    State(runtime): State<Arc<Runtime>>,
    request: Request,
) -> Response {
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
            crate::gateway::error::ErrorKind::UnsupportedMediaType,
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
            crate::gateway::error::ErrorKind::RequestTooLarge,
        ));
    }
    Ok((raw.into(), length))
}
async fn ingress(runtime: Arc<Runtime>, request: Request) -> Result<Response, ApiError> {
    let trace = Trace::for_request(runtime.diagnostics.as_ref(), &request);
    if runtime.shutdown.is_cancelled() {
        return Err(ApiError::shutdown());
    }
    let permit = runtime.permits.clone().try_acquire_owned().map_err(|_| {
        ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            crate::gateway::error::ErrorKind::GatewayBusy,
        )
    })?;
    let limit = runtime.limits.transcription_request_bytes;
    let (media, length) = upload_head(request.headers(), limit)?;
    let bytes = tokio::select! {
        biased;
        _=runtime.shutdown.cancelled()=>return Err(ApiError::shutdown()),
        result=tokio::time::timeout(runtime.limits.body_timeout,admission::collect(request.into_body(),limit))=>
            result.map_err(|_|ApiError::new(StatusCode::REQUEST_TIMEOUT,crate::gateway::error::ErrorKind::RequestTimeout))??,
    };
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
            crate::gateway::error::ErrorKind::ModelNotFound,
        ))?
        .clone();
    let prepared =
        crate::execution::transcription::prepare(&entry.route, &entry.provider, &request)
            .map_err(|_| ApiError::invalid())?;
    let deadline = Instant::now()
        + runtime.limits.exchange_timeout.min(Duration::from_millis(
            entry.route.endpoint.execution.timeout_ms,
        ));
    body::respond_producer(
        Exchange {
            runtime: runtime.clone(),
            entry,
            prepared,
        },
        body::ResponseType::Json,
        runtime.limits.clone(),
        deadline,
        runtime.shutdown.clone(),
        permit,
        trace,
    )
    .await
}
async fn produce(
    runtime: Arc<Runtime>,
    entry: Arc<BoundTranscription>,
    prepared: crate::execution::task::PreparedTaskRequest,
    deadline: Instant,
    lane: &body::Lane<'_>,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    trace.begin_candidate(1);
    let result = produce_one(&runtime, &entry, prepared, deadline, lane, trace).await;
    trace.end_candidate(result.as_ref().err().map(|e| e.diagnostic_code()), false);
    result
}
async fn produce_one(
    runtime: &Runtime,
    entry: &BoundTranscription,
    prepared: crate::execution::task::PreparedTaskRequest,
    deadline: Instant,
    lane: &body::Lane<'_>,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    let upstream = super::exchange::send(runtime, entry, prepared, deadline, trace).await?;
    let result = receive(
        upstream,
        entry.route.endpoint.profile,
        runtime
            .limits
            .response_bytes
            .min(entry.route.endpoint.execution.response_body_limit),
        trace,
    )
    .await?;
    trace.stage(Stage::Projection);
    let projected = codec::project_result(&result).map_err(|_| ApiError::upstream())?;
    crate::semantic::value::json_size(&projected.body, runtime.limits.response_bytes)
        .map_err(|_| ApiError::upstream())?;
    let bytes = serde_json::to_vec(&projected.body).map_err(|_| ApiError::upstream())?;
    let mut lifecycle = crate::execution::Lifecycle::new();
    body::send_frame(lane, bytes.into(), trace).await?;
    lifecycle.commit().map_err(|_| ApiError::upstream())?;
    lifecycle.terminal().map_err(|_| ApiError::upstream())?;
    Ok(())
}
pub(in crate::gateway) async fn receive(
    upstream: reqwest::Response,
    profile: crate::adapter::transcription::UpstreamAdapter,
    limit: usize,
    trace: &mut Trace,
) -> Result<RecognitionResult, ApiError> {
    let receiver = super::exchange::Receiver::new(upstream, limit, trace)?;
    receiver.require_media("application/json")?;
    let bytes = receiver.collect(trace).await?;
    profile
        .decode_response(&bytes)
        .map_err(|_| ApiError::upstream())
}
/// One bounded upstream exchange behind the response body.
struct Exchange {
    runtime: Arc<Runtime>,
    entry: Arc<BoundTranscription>,
    prepared: crate::execution::task::PreparedTaskRequest,
}
impl body::Produce for Exchange {
    fn produce(
        self,
        _: &crate::gateway::Limits,
        deadline: Instant,
        lane: &body::Lane<'_>,
        trace: &mut Trace,
    ) -> impl std::future::Future<Output = Result<(), ApiError>> + Send {
        produce(
            self.runtime,
            self.entry,
            self.prepared,
            deadline,
            lane,
            trace,
        )
    }
}
