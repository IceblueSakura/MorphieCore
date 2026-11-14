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
        |entry| &entry.model,
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
    let started = Instant::now();
    let mut trace = Trace::for_request(runtime.diagnostics.as_ref(), &request);
    let permit = admission::acquire_permit(&runtime)?;
    admission::headers(request.headers(), runtime.limits.request_bytes)?;
    let bytes = crate::gateway::ingress::http::read_body(
        &runtime,
        request.into_body(),
        runtime.limits.request_bytes,
        started + runtime.limits.exchange_timeout,
        &mut trace,
    )
    .await?;
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
    let deadline = started
        + runtime.limits.exchange_timeout.min(Duration::from_millis(
            entry.route.endpoint.execution.timeout_ms,
        ));
    if Instant::now() >= deadline {
        return Err(ApiError::timeout());
    }
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
    async fn produce(
        self,
        _: &crate::gateway::Limits,
        deadline: Instant,
        lane: &body::Lane<'_>,
        trace: &mut Trace,
    ) -> Result<(), ApiError> {
        let Self {
            runtime,
            entry,
            request,
            prepared,
        } = self;
        trace.begin_candidate(1);
        trace.models(
            entry.route.model.as_str(),
            &entry.route.endpoint.upstream_model,
        );
        let result: Result<(), ApiError> = async {
            let upstream =
                super::exchange::send(&runtime, &entry, prepared, deadline, trace).await?;
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
            body::send_final_audio(lane, audio, trace).await?;
            Ok(())
        }
        .await;
        trace.end_candidate(result.as_ref().err().map(|e| e.diagnostic_code()), false);
        result
    }
}
