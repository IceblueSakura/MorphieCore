//! Authenticated Speech admission and bounded static binary delivery.
pub(in crate::gateway) mod aliyun;
use crate::{
    gateway::{
        ApiError, Credentials, Runtime, SpeechEntry, StartupError, body,
        diagnostics::{Stage, Trace},
        ingress::admission,
    },
    protocol::openai::speech as codec,
    semantic::{
        task::speech_synthesis::SpeechRequest,
        value::{AudioArtifact, MAX_AUDIO_BYTES},
    },
    topology::{CompiledTopology, speech::SpeechRoute},
};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::time::Instant;

pub(in crate::gateway) type BoundSpeech = crate::gateway::tasks::BoundTask<SpeechRoute>;
pub(in crate::gateway) fn bind(
    topology: &CompiledTopology,
    entries: Vec<SpeechEntry>,
    credentials: &Credentials,
) -> Result<BTreeMap<String, Arc<BoundSpeech>>, StartupError> {
    crate::gateway::tasks::bind(
        topology,
        entries,
        credentials,
        |entry: &SpeechEntry| topology.speech_route(&entry.model),
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
    admission::headers(request.headers(), runtime.limits.request_bytes)?;
    let bytes = tokio::select! {
        biased;
        _ = runtime.shutdown.cancelled() => return Err(ApiError::shutdown()),
        result = tokio::time::timeout(runtime.limits.body_timeout, admission::collect(request.into_body(), runtime.limits.request_bytes)) =>
            result.map_err(|_| ApiError::new(StatusCode::REQUEST_TIMEOUT, crate::gateway::error::ErrorKind::RequestTimeout))??,
    };
    let envelope = crate::protocol::json::decode(&bytes).map_err(|_| ApiError::invalid())?;
    let model = envelope
        .get("model")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(ApiError::invalid)?;
    let entry = runtime
        .speech
        .get(model)
        .ok_or(ApiError::new(
            StatusCode::NOT_FOUND,
            crate::gateway::error::ErrorKind::ModelNotFound,
        ))?
        .clone();
    let request = codec::decode_request(&bytes).map_err(|_| ApiError::invalid())?;
    let prepared = crate::execution::speech::prepare(&entry.route, &entry.provider, &request)
        .map_err(|_| ApiError::invalid())?;
    let deadline = Instant::now()
        + runtime.limits.exchange_timeout.min(Duration::from_millis(
            entry.route.endpoint.execution.timeout_ms,
        ));
    body::respond_producer(
        Exchange {
            runtime: runtime.clone(),
            entry,
            request,
            prepared,
        },
        body::ResponseType::Audio,
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
    entry: Arc<BoundSpeech>,
    request: crate::protocol::speech::Request,
    prepared: crate::execution::task::PreparedTaskRequest,
    deadline: Instant,
    lane: &body::Lane<'_>,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    trace.begin_candidate(1);
    let result = produce_one(&runtime, &entry, &request, prepared, deadline, lane, trace).await;
    trace.end_candidate(result.as_ref().err().map(|e| e.diagnostic_code()), false);
    result
}
async fn produce_one(
    runtime: &Runtime,
    entry: &BoundSpeech,
    request: &crate::protocol::speech::Request,
    prepared: crate::execution::task::PreparedTaskRequest,
    deadline: Instant,
    lane: &body::Lane<'_>,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    let upstream = super::exchange::send(runtime, entry, prepared, deadline, trace).await?;
    let limit = runtime
        .limits
        .speech_bytes
        .min(runtime.limits.response_bytes)
        .min(entry.route.endpoint.execution.response_body_limit)
        .min(MAX_AUDIO_BYTES);
    let audio = if entry.route.endpoint.profile.upstream_sse() {
        let result = crate::gateway::tasks::speech::aliyun::receive(
            upstream,
            limit,
            runtime
                .limits
                .response_bytes
                .min(entry.route.endpoint.execution.response_body_limit),
            runtime.limits.event_bytes,
            runtime.limits.max_events,
            trace,
        )
        .await?;
        codec::project_result(&result)
            .map_err(|_| ApiError::upstream())?
            .audio
    } else {
        receive(
            upstream,
            &request.task,
            entry.route.endpoint.profile,
            limit,
            trace,
        )
        .await?
    };
    trace.stage(Stage::Projection);
    let mut lifecycle = crate::execution::Lifecycle::new();
    body::send_audio(lane, audio, trace).await?;
    lifecycle.commit().map_err(|_| ApiError::upstream())?;
    lifecycle.terminal().map_err(|_| ApiError::upstream())?;
    Ok(())
}
pub(in crate::gateway) async fn receive(
    upstream: reqwest::Response,
    task: &SpeechRequest,
    profile: crate::adapter::speech::UpstreamAdapter,
    limit: usize,
    trace: &mut Trace,
) -> Result<AudioArtifact, ApiError> {
    let receiver = super::exchange::Receiver::new(upstream, limit.min(MAX_AUDIO_BYTES), trace)?;
    let encoding = profile
        .reported_encoding(receiver.media_type(), task)
        .map_err(|_| ApiError::upstream())?;
    let bytes = receiver.collect(trace).await?;
    if profile == crate::adapter::speech::UpstreamAdapter::Xiaomi {
        let result = crate::protocol::xiaomi_audio::decode_speech(&bytes, task)
            .map_err(|_| ApiError::upstream())?;
        Ok(codec::project_result(&result)
            .map_err(|_| ApiError::upstream())?
            .audio)
    } else {
        AudioArtifact::new(bytes.into(), encoding).map_err(|_| ApiError::upstream())
    }
}
/// One bounded upstream exchange behind the response body.
struct Exchange {
    runtime: Arc<Runtime>,
    entry: Arc<BoundSpeech>,
    prepared: crate::execution::task::PreparedTaskRequest,
    request: crate::protocol::speech::Request,
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
            self.request,
            self.prepared,
            deadline,
            lane,
            trace,
        )
    }
}
