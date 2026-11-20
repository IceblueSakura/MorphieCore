//! Authenticated Speech admission and bounded static binary delivery.
pub(crate) mod aliyun;
use crate::tasks::{
    exchange::Receiver,
    lifecycle::{Admission, TaskOutput},
};
use crate::{
    ApiError, Credentials, Runtime, SpeechEntry, StartupError, body,
    diagnostics::{Stage, Trace},
    ingress::admission,
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
use std::{collections::BTreeMap, sync::Arc};

pub(crate) type BoundSpeech = crate::tasks::BoundTask<SpeechRoute>;
pub(crate) fn bind(
    topology: &CompiledTopology,
    entries: Vec<SpeechEntry>,
    credentials: &Credentials,
) -> Result<BTreeMap<String, Arc<BoundSpeech>>, StartupError> {
    crate::tasks::bind(
        topology,
        entries,
        credentials,
        |entry: &SpeechEntry| topology.speech_route(&entry.model),
        |entry| &entry.model,
    )
}
pub(crate) async fn handle(State(runtime): State<Arc<Runtime>>, request: Request) -> Response {
    ingress(runtime, request)
        .await
        .unwrap_or_else(IntoResponse::into_response)
}
async fn ingress(runtime: Arc<Runtime>, request: Request) -> Result<Response, ApiError> {
    let (admitted, _, bytes) = Admission::upload(
        &runtime,
        request,
        runtime.limits.request_bytes,
        admission::headers,
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
            crate::error::ErrorKind::ModelNotFound,
        ))?
        .clone();
    let request = codec::decode_request(&bytes).map_err(|_| ApiError::invalid())?;
    let prepared = crate::execution::speech::prepare(&entry.route, &entry.provider, &request)
        .map_err(|_| ApiError::invalid())?;
    admitted
        .respond(runtime, entry, prepared, Output(request))
        .await
}
async fn decode(
    receiver: Receiver,
    task: &SpeechRequest,
    profile: crate::adapter::speech::UpstreamAdapter,
    trace: &mut Trace,
) -> Result<AudioArtifact, ApiError> {
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
#[cfg(test)]
pub(crate) async fn receive_for_test(
    upstream: reqwest::Response,
    task: &SpeechRequest,
    profile: crate::adapter::speech::UpstreamAdapter,
    limit: usize,
    trace: &mut Trace,
) -> Result<AudioArtifact, ApiError> {
    decode(
        Receiver::new(upstream, limit.min(MAX_AUDIO_BYTES), trace)?,
        task,
        profile,
        trace,
    )
    .await
}
struct Output(crate::protocol::speech::Request);
impl TaskOutput for Output {
    type Route = SpeechRoute;
    const RESPONSE: body::ResponseType = body::ResponseType::Audio;

    fn receive_limit(
        &self,
        entry: &BoundSpeech,
        limits: &crate::Limits,
        wire_limit: usize,
    ) -> usize {
        if entry.route.endpoint.profile.upstream_sse() {
            wire_limit
        } else {
            limits.speech_bytes.min(wire_limit).min(MAX_AUDIO_BYTES)
        }
    }

    async fn publish(
        self,
        receiver: Receiver,
        entry: &BoundSpeech,
        limits: &crate::Limits,
        lane: &body::Lane<'_>,
        trace: &mut Trace,
    ) -> Result<(), ApiError> {
        let wire_limit = limits
            .response_bytes
            .min(entry.route.endpoint.execution.response_body_limit);
        let audio_limit = limits.speech_bytes.min(wire_limit).min(MAX_AUDIO_BYTES);
        let audio = if entry.route.endpoint.profile.upstream_sse() {
            let result = aliyun::decode(
                receiver,
                audio_limit,
                wire_limit,
                limits.event_bytes,
                limits.max_events,
                trace,
            )
            .await?;
            codec::project_result(&result)
                .map_err(|_| ApiError::upstream())?
                .audio
        } else {
            decode(receiver, &self.0.task, entry.route.endpoint.profile, trace).await?
        };
        trace.stage(Stage::Projection);
        body::send_final_audio(lane, audio, trace).await?;
        Ok(())
    }
}
