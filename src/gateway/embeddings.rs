//! Authenticated text embeddings use one bounded static exchange and the shared handoff owner.
use super::{
    ApiError, Credentials, EmbeddingEntry, Runtime, StartupError, admission, body, credentials,
    diagnostics::{Stage, Trace},
};
use crate::{
    adapter::embeddings::{Profile, Request as EmbeddingRequest, Response as EmbeddingResponse},
    protocol::openai::embeddings as codec,
    provider::{CredentialKind, ProviderDefinition},
    topology::{CompiledTopology, embeddings::EmbeddingRoute},
};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::time::Instant;

pub(super) struct BoundEmbedding {
    route: EmbeddingRoute,
    provider: ProviderDefinition,
    secret: credentials::Source,
}
pub(super) fn bind(
    topology: &CompiledTopology,
    entries: Vec<EmbeddingEntry>,
    credentials: &Credentials,
) -> Result<BTreeMap<String, Arc<BoundEmbedding>>, StartupError> {
    if entries.len() > 64 {
        return Err(StartupError::Binding);
    }
    let mut result = BTreeMap::new();
    for entry in entries {
        let route = topology
            .embedding_route(&entry.model)
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
                Arc::new(BoundEmbedding {
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
        _=runtime.shutdown.cancelled()=>return Err(ApiError::shutdown()),
        result=tokio::time::timeout(runtime.limits.body_timeout,admission::collect(request.into_body(),runtime.limits.request_bytes))
            =>result.map_err(|_|ApiError::new(StatusCode::REQUEST_TIMEOUT,"request_timeout"))??,
    };
    let request = codec::decode_request(&bytes).map_err(|_| ApiError::invalid())?;
    let entry = runtime
        .embeddings
        .get(&request.model)
        .ok_or(ApiError::new(StatusCode::NOT_FOUND, "model_not_found"))?
        .clone();
    entry
        .route
        .endpoint
        .contract
        .admit(&request)
        .map_err(|_| ApiError::invalid())?;
    let value = entry
        .route
        .endpoint
        .profile
        .encode_request(&request, &entry.route.endpoint.upstream_model)
        .map_err(|_| ApiError::invalid())?;
    crate::semantic::value::json_size(&value, entry.route.endpoint.execution.request_body_limit)
        .map_err(|_| ApiError::invalid())?;
    let deadline = Instant::now()
        + runtime.limits.exchange_timeout.min(Duration::from_millis(
            entry.route.endpoint.execution.timeout_ms,
        ));
    body::respond_producer(
        body::Producer::Embedding {
            runtime: runtime.clone(),
            entry,
            request,
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
pub(super) async fn produce(
    runtime: Arc<Runtime>,
    entry: Arc<BoundEmbedding>,
    request: EmbeddingRequest,
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
    entry: &BoundEmbedding,
    request: &EmbeddingRequest,
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
        crate::execution::embeddings::prepare(&entry.route, &entry.provider, &secret, request)
            .map_err(|_| ApiError::invalid())?;
    trace.stage(Stage::Connect);
    let upstream = runtime
        .transport
        .send(prepared, deadline.saturating_duration_since(Instant::now()))
        .await
        .map_err(ApiError::transport)?;
    let response = receive(
        upstream,
        runtime
            .limits
            .response_bytes
            .min(entry.route.endpoint.execution.response_body_limit),
        trace,
        entry.route.endpoint.profile,
    )
    .await?;
    response
        .validate_for(&request.task)
        .map_err(|_| ApiError::upstream())?;
    trace.stage(Stage::Projection);
    let projected = crate::lowering::embeddings::project_response(&response)
        .map_err(|_| ApiError::upstream())?;
    let value = codec::encode_response(&projected.response).map_err(|_| ApiError::upstream())?;
    crate::semantic::value::json_size(&value, runtime.limits.response_bytes)
        .map_err(|_| ApiError::upstream())?;
    let output = serde_json::to_vec(&value).map_err(|_| ApiError::upstream())?;
    let mut lifecycle = crate::execution::Lifecycle::new();
    body::send_frame(lane, output.into(), trace).await?;
    lifecycle.commit().map_err(|_| ApiError::upstream())?;
    lifecycle.terminal().map_err(|_| ApiError::upstream())?;
    Ok(())
}
pub(super) async fn receive(
    mut upstream: reqwest::Response,
    limit: usize,
    trace: &mut Trace,
    profile: Profile,
) -> Result<EmbeddingResponse, ApiError> {
    trace.stage(Stage::ResponseHead);
    trace.head(upstream.status().as_u16(), upstream.headers());
    if upstream.status().as_u16() != 200 {
        return Err(ApiError::status(upstream.status().as_u16()));
    }
    let limit = limit.min(crate::semantic::value::JsonLimits::ENVELOPE.bytes);
    let headers = upstream.headers();
    let mut types = headers.get_all("content-type").iter();
    let media: mime::Mime = types
        .next()
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .ok_or_else(ApiError::upstream)?;
    if types.next().is_some()
        || media.essence_str() != "application/json"
        || headers.contains_key("content-encoding")
        || media
            .params()
            .any(|(k, v)| k != mime::CHARSET || v != mime::UTF_8)
    {
        return Err(ApiError::upstream());
    }
    let mut lengths = headers.get_all("content-length").iter();
    let length = lengths
        .next()
        .map(|v| {
            let raw = v.to_str().map_err(|_| ApiError::upstream())?;
            if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
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
    profile
        .decode_response(&bytes)
        .map_err(|_| ApiError::upstream())
}
