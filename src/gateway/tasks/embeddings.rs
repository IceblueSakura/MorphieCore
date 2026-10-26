//! Authenticated text embeddings use one bounded static exchange and the shared handoff owner.
use crate::gateway::{
    ApiError, Credentials, EmbeddingEntry, Runtime, StartupError, body,
    diagnostics::{Stage, Trace},
    ingress::{admission, capabilities},
};
use crate::{
    adapter::embeddings::{Encoding, Profile, Response as EmbeddingResponse},
    execution::embeddings::PreparedEmbedding,
    protocol::openai::embeddings as codec,
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
    let parsed = codec::parse_request(&bytes).map_err(|_| ApiError::invalid())?;
    let entry = runtime
        .embeddings
        .get(parsed.model())
        .ok_or(ApiError::new(StatusCode::NOT_FOUND, "model_not_found"))?
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
    trace.end_candidate(result.as_ref().err().map(|e| e.code), false);
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
    trace.stage(Stage::Prepare);
    trace.executor(entry.provider.id.as_str());
    let expectation = prepared.expectation.clone();
    let delivery = prepared.delivery.clone();
    let (secret, grant) = entry.secret.resolve_for_request().await?;
    if grant.is_some() {
        return Err(ApiError::upstream());
    }
    trace.embedding_receipts(&prepared.receipts);
    let prepared = prepared
        .authorize(&entry.provider, &secret)
        .map_err(|_| ApiError::invalid())?;
    trace.stage(Stage::Connect);
    if let Some(capture) = &trace.capture {
        capture.upstream_request(&prepared, trace.capture_attempt);
    }
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
        expectation.upstream_encoding,
    )
    .await?;
    expectation
        .validate_response(&response)
        .map_err(|_| ApiError::upstream())?;
    trace.stage(Stage::Projection);
    let projected = crate::lowering::embeddings::project_response(&response, &delivery)
        .map_err(|_| ApiError::upstream())?;
    trace.embedding_receipts(&projected.receipts);
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
    mut upstream: reqwest::Response,
    limit: usize,
    trace: &mut Trace,
    profile: Profile,
    encoding: Encoding,
) -> Result<EmbeddingResponse, ApiError> {
    trace.stage(Stage::ResponseHead);
    trace.head(upstream.status().as_u16(), upstream.headers());
    if let Some(capture) = &trace.capture {
        capture.head(
            crate::gateway::capture::Boundary::UpstreamResponse,
            Some(trace.capture_attempt),
            upstream.headers(),
            None,
        );
    }
    if upstream.status().as_u16() != 200 {
        let status = upstream.status().as_u16();
        let retry = crate::gateway::error::retry_after(upstream.headers());
        // The failed head is authoritative; a stalled optional error body must
        // not erase its status or Retry-After behind the operation deadline.
        return Err(ApiError::failure(status, b"", retry));
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
    let mut capture = trace.capture.as_ref().and_then(|c| {
        c.body(
            crate::gateway::capture::Boundary::UpstreamResponse,
            Some(trace.capture_attempt),
            "application/json",
        )
    });
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
    if let Some(capture) = &mut capture {
        capture.feed(&bytes);
        capture.finish();
    }
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
