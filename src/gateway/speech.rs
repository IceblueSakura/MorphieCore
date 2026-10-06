//! Authenticated Speech admission and bounded static binary delivery.
use super::{
    ApiError, Credentials, Runtime, SpeechEntry, StartupError, admission, body, credentials,
    diagnostics::{Stage, Trace},
};
use crate::{
    protocol::openai::speech as codec,
    provider::{CredentialKind, ProviderDefinition},
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

pub(super) struct BoundSpeech {
    route: SpeechRoute,
    provider: ProviderDefinition,
    secret: credentials::Source,
}
pub(super) fn bind(
    topology: &CompiledTopology,
    entries: Vec<SpeechEntry>,
    credentials: &Credentials,
) -> Result<BTreeMap<String, Arc<BoundSpeech>>, StartupError> {
    if entries.len() > 64 {
        return Err(StartupError::Binding);
    }
    let mut result = BTreeMap::new();
    for entry in entries {
        let route = topology
            .speech_route(&entry.model)
            .ok_or(StartupError::Binding)?
            .clone();
        let provider = topology
            .provider(route.endpoint.provider.as_str())
            .ok_or(StartupError::Binding)?
            .clone();
        let binding = credentials
            .get(&route.endpoint.credential)
            .ok_or(StartupError::Credentials)?;
        if binding.fallback || binding.sources.len() != 1 {
            return Err(StartupError::Credentials);
        }
        let secret = binding.sources[0].clone();
        secret.check(CredentialKind::ApiKey, provider.id.as_str())?;
        if result
            .insert(
                entry.model,
                Arc::new(BoundSpeech {
                    route,
                    provider,
                    secret,
                }),
            )
            .is_some()
        {
            return Err(StartupError::Binding);
        }
    }
    Ok(result)
}
pub(super) async fn handle(State(runtime): State<Arc<Runtime>>, request: Request) -> Response {
    ingress(runtime, request)
        .await
        .unwrap_or_else(IntoResponse::into_response)
}
async fn ingress(runtime: Arc<Runtime>, request: Request) -> Result<Response, ApiError> {
    let trace = Trace::new(runtime.diagnostics.as_ref(), request.headers());
    if runtime.shutdown.is_cancelled() {
        return Err(ApiError::shutdown());
    }
    let permit = runtime
        .permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::new(StatusCode::TOO_MANY_REQUESTS, "gateway_busy"))?;
    admission::headers(request.headers(), runtime.limits.request_bytes)?;
    let bytes = tokio::select! {
        biased;
        _ = runtime.shutdown.cancelled() => return Err(ApiError::shutdown()),
        result = tokio::time::timeout(runtime.limits.body_timeout, admission::collect(request.into_body(), runtime.limits.request_bytes)) =>
            result.map_err(|_| ApiError::new(StatusCode::REQUEST_TIMEOUT, "request_timeout"))??,
    };
    let envelope =
        crate::protocol::openai::json::decode(&bytes).map_err(|_| ApiError::invalid())?;
    let model = envelope
        .get("model")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(ApiError::invalid)?;
    let entry = runtime
        .speech
        .get(model)
        .ok_or(ApiError::new(StatusCode::NOT_FOUND, "model_not_found"))?
        .clone();
    let request = codec::decode_request(&bytes).map_err(|_| ApiError::invalid())?;
    crate::lowering::speech::check_request(&request.task, &entry.route.endpoint.capabilities)
        .map_err(|_| ApiError::invalid())?;
    let projected = entry
        .route
        .endpoint
        .profile
        .encode_request(&request, &entry.route.endpoint.upstream_model)
        .map_err(|_| ApiError::invalid())?;
    crate::semantic::value::json_size(
        &projected,
        entry.route.endpoint.execution.request_body_limit,
    )
    .map_err(|_| ApiError::invalid())?;
    let deadline = Instant::now()
        + runtime.limits.exchange_timeout.min(Duration::from_millis(
            entry.route.endpoint.execution.timeout_ms,
        ));
    body::respond_producer(
        body::Producer::Speech {
            runtime: runtime.clone(),
            entry,
            request,
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
pub(super) async fn produce(
    runtime: Arc<Runtime>,
    entry: Arc<BoundSpeech>,
    request: crate::adapter::speech::Request,
    deadline: Instant,
    lane: &body::Lane<'_>,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    trace.begin_candidate(1);
    let result = produce_one(&runtime, &entry, &request, deadline, lane, trace).await;
    trace.end_candidate(result.as_ref().err().map(|e| e.code), false);
    result
}
async fn produce_one(
    runtime: &Runtime,
    entry: &BoundSpeech,
    request: &crate::adapter::speech::Request,
    deadline: Instant,
    lane: &body::Lane<'_>,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    trace.stage(Stage::Prepare);
    let (secret, grant) = entry.secret.resolve()?;
    if grant.is_some() {
        return Err(ApiError::upstream());
    }
    let prepared =
        crate::execution::speech::prepare(&entry.route, &entry.provider, &secret, request)
            .map_err(|_| ApiError::invalid())?;
    trace.stage(Stage::Connect);
    let upstream = runtime
        .transport
        .send(prepared, deadline.saturating_duration_since(Instant::now()))
        .await
        .map_err(ApiError::transport)?;
    let limit = runtime
        .limits
        .speech_bytes
        .min(runtime.limits.response_bytes)
        .min(entry.route.endpoint.execution.response_body_limit)
        .min(MAX_AUDIO_BYTES);
    let audio = if entry.route.endpoint.profile.upstream_sse() {
        let result = super::aliyun_speech::receive(
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
pub(super) async fn receive(
    mut upstream: reqwest::Response,
    task: &SpeechRequest,
    profile: crate::adapter::speech::Profile,
    limit: usize,
    trace: &mut Trace,
) -> Result<AudioArtifact, ApiError> {
    trace.stage(Stage::ResponseHead);
    trace.head(upstream.status().as_u16(), upstream.headers());
    if upstream.status().as_u16() != 200 {
        return Err(ApiError::status(upstream.status().as_u16()));
    }
    let headers = upstream.headers();
    let mut types = headers.get_all("content-type").iter();
    let media = types
        .next()
        .and_then(|v| v.to_str().ok())
        .ok_or_else(ApiError::upstream)?;
    let encoding = profile
        .reported_encoding(media, task)
        .map_err(|_| ApiError::upstream())?;
    if types.next().is_some() || headers.contains_key("content-encoding") {
        return Err(ApiError::upstream());
    }
    let limit = limit.min(MAX_AUDIO_BYTES);
    let mut lengths = headers.get_all("content-length").iter();
    let length = lengths
        .next()
        .map(|v| {
            let raw = v.to_str().map_err(|_| ApiError::upstream())?;
            if raw.is_empty() || !raw.bytes().all(|c| c.is_ascii_digit()) {
                return Err(ApiError::upstream());
            }
            raw.parse::<u64>().map_err(|_| ApiError::upstream())
        })
        .transpose()?;
    if lengths.next().is_some() || length.is_some_and(|n| n == 0 || n > limit as u64) {
        return Err(ApiError::upstream());
    }
    trace.stage(Stage::Intake);
    let mut bytes = Vec::new();
    while let Some(chunk) = upstream.chunk().await.map_err(|e| {
        if e.is_timeout() {
            ApiError::timeout()
        } else {
            ApiError::upstream()
        }
    })? {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(ApiError::upstream());
        }
        trace.received(chunk.len());
        bytes.extend_from_slice(&chunk);
    }
    trace.stage(Stage::Terminal);
    if length.is_some_and(|n| n != bytes.len() as u64) {
        return Err(ApiError::upstream());
    }
    AudioArtifact::new(bytes.into(), encoding).map_err(|_| ApiError::upstream())
}
