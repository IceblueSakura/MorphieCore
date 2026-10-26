//! Authenticated text embeddings use one bounded static exchange and the shared handoff owner.
use crate::{
    adapter::embeddings::UpstreamAdapter,
    execution::embeddings::PreparedEmbedding,
    gateway::{
        ApiError, Credentials, EmbeddingEntry, Runtime, StartupError, body,
        diagnostics::{Stage, Trace},
        ingress::{admission, capabilities},
    },
    protocol::{
        embedding::{Encoding, Response as EmbeddingResponse},
        openai::embeddings as codec,
    },
    topology::{CompiledTopology, embeddings::EmbeddingRoute},
};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::time::Instant;

pub(in crate::gateway) type BoundEmbedding = super::BoundTask<EmbeddingRoute>;
pub(in crate::gateway) fn bind(
    topology: &CompiledTopology,
    entries: Vec<EmbeddingEntry>,
    credentials: &Credentials,
) -> Result<BTreeMap<String, Arc<BoundEmbedding>>, StartupError> {
    super::bind(
        topology,
        entries,
        credentials,
        |entry: &EmbeddingEntry| topology.embedding_route(&entry.model),
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
        _=runtime.shutdown.cancelled()=>return Err(ApiError::shutdown()),
        result=tokio::time::timeout(runtime.limits.body_timeout,admission::collect(request.into_body(),runtime.limits.request_bytes))
            =>result.map_err(|_|ApiError::new(StatusCode::REQUEST_TIMEOUT,crate::gateway::error::ErrorKind::RequestTimeout))??,
    };
    let parsed = codec::parse_request(&bytes).map_err(|_| ApiError::invalid())?;
    let entry = runtime
        .embeddings
        .get(parsed.model())
        .ok_or(ApiError::new(
            StatusCode::NOT_FOUND,
            crate::gateway::error::ErrorKind::ModelNotFound,
        ))?
        .clone();
    let request = parsed
        .into_request(entry.route.endpoint.contract.tokenizer)
        .map_err(|_| ApiError::invalid())?;
    let prepared = crate::execution::embeddings::prepare(&entry.route, &entry.provider, &request)
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
    .map(capabilities::notice)
}
async fn produce(
    runtime: Arc<Runtime>,
    entry: Arc<BoundEmbedding>,
    prepared: PreparedEmbedding,
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
    entry: &BoundEmbedding,
    prepared: PreparedEmbedding,
    deadline: Instant,
    lane: &body::Lane<'_>,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    let expectation = prepared.expectation;
    let delivery = prepared.delivery;
    trace.embedding_adaptations(&prepared.adaptations);
    let upstream = super::exchange::send(runtime, entry, prepared.request, deadline, trace).await?;
    let response = receive(
        upstream,
        runtime
            .limits
            .response_bytes
            .min(entry.route.endpoint.execution.response_body_limit),
        trace,
        entry.route.endpoint.profile,
        expectation.upstream_encoding,
    )
    .await?;
    expectation
        .validate_response(&response)
        .map_err(|_| ApiError::upstream())?;
    trace.stage(Stage::Projection);
    let projected = crate::protocol::embedding::projection::project_response(&response, &delivery)
        .map_err(|_| ApiError::upstream())?;
    trace.embedding_adaptations(&projected.adaptations);
    let value = codec::encode_response_as(&projected.response, delivery.encoding)
        .map_err(|_| ApiError::upstream())?;
    crate::semantic::value::json_size(&value, runtime.limits.response_bytes)
        .map_err(|_| ApiError::upstream())?;
    let output = serde_json::to_vec(&value).map_err(|_| ApiError::upstream())?;
    let mut lifecycle = crate::execution::Lifecycle::new();
    body::send_frame(lane, output.into(), trace).await?;
    lifecycle.commit().map_err(|_| ApiError::upstream())?;
    lifecycle.terminal().map_err(|_| ApiError::upstream())?;
    Ok(())
}
pub(in crate::gateway) async fn receive(
    upstream: reqwest::Response,
    limit: usize,
    trace: &mut Trace,
    profile: UpstreamAdapter,
    encoding: Encoding,
) -> Result<EmbeddingResponse, ApiError> {
    let receiver = super::exchange::Receiver::new(
        upstream,
        limit.min(crate::semantic::value::JsonLimits::ENVELOPE.bytes),
        trace,
    )?;
    receiver.require_media("application/json")?;
    let bytes = receiver.collect(trace).await?;
    profile
        .decode_response_as(&bytes, encoding)
        .map_err(|_| ApiError::upstream())
}
/// One bounded upstream exchange behind the response body.
struct Exchange {
    runtime: Arc<Runtime>,
    entry: Arc<BoundEmbedding>,
    prepared: PreparedEmbedding,
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
