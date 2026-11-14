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
        body::ResponseType::Json,
        runtime.limits.clone(),
        deadline,
        runtime.shutdown.clone(),
        permit,
        trace,
    )
    .await
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
            let projected = crate::protocol::image::projection::project_response(
                &response,
                entry.route.accounting,
            )
            .map_err(|_| ApiError::upstream())?;

            let value =
                images::encode_response(&projected.response).map_err(|_| ApiError::upstream())?;
            crate::semantic::value::json_size(&value, runtime.limits.response_bytes)
                .map_err(|_| ApiError::upstream())?;
            let output = serde_json::to_vec(&value).map_err(|_| ApiError::upstream())?;
            body::send_final_frame(lane, output.into(), trace).await?;
            Ok(())
        }
        .await;
        trace.end_candidate(result.as_ref().err().map(|e| e.diagnostic_code()), false);
        result
    }
}
