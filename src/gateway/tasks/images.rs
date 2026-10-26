//! Authenticated image admission and a single bounded static exchange.
use crate::{
    gateway::{
        ApiError, Credentials, ImageEntry, Runtime, StartupError, body,
        diagnostics::{Stage, Trace},
        ingress::admission,
    },
    protocol::openai::images,
    topology::{CompiledTopology, images::ImageRoute},
};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::time::Instant;

pub(in crate::gateway) type BoundImage = super::BoundTask<ImageRoute>;
pub(in crate::gateway) fn bind(
    topology: &CompiledTopology,
    entries: Vec<ImageEntry>,
    credentials: &Credentials,
) -> Result<BTreeMap<String, Arc<BoundImage>>, StartupError> {
    super::bind(
        topology,
        entries,
        credentials,
        |entry: &ImageEntry| topology.image_route(&entry.model),
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
        result=tokio::time::timeout(runtime.limits.body_timeout,admission::collect(request.into_body(),runtime.limits.request_bytes))=>result.map_err(|_|ApiError::new(StatusCode::REQUEST_TIMEOUT,crate::gateway::error::ErrorKind::RequestTimeout))??,
    };
    let envelope = crate::protocol::json::decode(&bytes).map_err(|_| ApiError::invalid())?;
    let model = envelope
        .get("model")
        .and_then(serde_json::Value::as_str)
        .ok_or(ApiError::invalid())?;
    let entry = runtime
        .images
        .get(model)
        .ok_or(ApiError::new(
            StatusCode::NOT_FOUND,
            crate::gateway::error::ErrorKind::ModelNotFound,
        ))?
        .clone();
    let request = images::decode_request(&bytes).map_err(|_| ApiError::invalid())?;
    let prepared = crate::execution::images::prepare(&entry.route, &entry.provider, &request)
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
    entry: Arc<BoundImage>,
    request: crate::protocol::image::Request,
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
fn check_response(
    response: &crate::semantic::task::image_generation::ImageGenerationResponse,
    request: &crate::semantic::task::image_generation::ImageGenerationRequest,
    image_bytes: usize,
    images_bytes: usize,
) -> Result<(), ApiError> {
    response
        .validate_for(request)
        .map_err(|_| ApiError::upstream())?;
    if response
        .images
        .iter()
        .any(|image| image.data.decoded_bytes() > image_bytes)
        || response.decoded_bytes().map_err(|_| ApiError::upstream())? > images_bytes
    {
        return Err(ApiError::upstream());
    }
    Ok(())
}
async fn produce_one(
    runtime: &Runtime,
    entry: &BoundImage,
    request: &crate::protocol::image::Request,
    prepared: crate::execution::task::PreparedTaskRequest,
    deadline: Instant,
    lane: &body::Lane<'_>,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    let upstream = super::exchange::send(runtime, entry, prepared, deadline, trace).await?;
    let limit = runtime
        .limits
        .response_bytes
        .min(entry.route.endpoint.execution.response_body_limit)
        .min(crate::semantic::value::JsonLimits::IMAGE_RESPONSE.bytes);
    let response = receive(upstream, entry.route.endpoint.profile, limit, trace).await?;
    check_response(
        &response,
        &request.task,
        runtime.limits.image_bytes,
        runtime.limits.images_bytes,
    )?;
    trace.stage(Stage::Projection);
    let projected =
        crate::protocol::image::projection::project_response(&response, entry.route.accounting)
            .map_err(|_| ApiError::upstream())?;
    trace.image_accounting(response.usage.value(), projected.loss);
    let value = images::encode_response(&projected.response).map_err(|_| ApiError::upstream())?;
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
    codec: crate::adapter::images::UpstreamAdapter,
    limit: usize,
    trace: &mut Trace,
) -> Result<crate::semantic::task::image_generation::ImageGenerationResponse, ApiError> {
    let receiver = super::exchange::Receiver::new(upstream, limit, trace)?;
    receiver.require_media("application/json")?;
    let bytes = receiver.collect(trace).await.map_err(|error| {
        // Preserve this operation's existing public read-error policy while
        // the shared receiver retains the precise timeout cause internally.
        if error.kind == crate::gateway::error::ErrorKind::Timeout {
            ApiError::upstream()
        } else {
            error
        }
    })?;
    codec
        .decode_response(&bytes)
        .map_err(|_| ApiError::upstream())
}
/// One bounded upstream exchange behind the response body.
struct Exchange {
    runtime: Arc<Runtime>,
    entry: Arc<BoundImage>,
    prepared: crate::execution::task::PreparedTaskRequest,
    request: crate::protocol::image::Request,
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
