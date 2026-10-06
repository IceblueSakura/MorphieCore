//! Authenticated image admission and a single bounded static exchange.
#[cfg(test)]
#[path = "image_tests.rs"]
mod tests;
use super::{
    ApiError, Credentials, ImageEntry, Runtime, StartupError, admission, body, credentials,
    diagnostics::{Stage, Trace},
};
use crate::{
    protocol::openai::images,
    provider::{CredentialKind, ProviderDefinition},
    topology::{CompiledTopology, images::ImageRoute},
};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::time::Instant;

pub(super) struct BoundImage {
    route: ImageRoute,
    provider: ProviderDefinition,
    secret: credentials::Source,
}
pub(super) fn bind(
    topology: &CompiledTopology,
    entries: Vec<ImageEntry>,
    credentials: &Credentials,
) -> Result<BTreeMap<String, Arc<BoundImage>>, StartupError> {
    if entries.len() > 64 {
        return Err(StartupError::Binding);
    }
    let mut result = BTreeMap::new();
    for entry in entries {
        let route = topology
            .image_route(&entry.model)
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
                Arc::new(BoundImage {
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
        result=tokio::time::timeout(runtime.limits.body_timeout,admission::collect(request.into_body(),runtime.limits.request_bytes))=>result.map_err(|_|ApiError::new(StatusCode::REQUEST_TIMEOUT,"request_timeout"))??,
    };
    let envelope =
        crate::protocol::openai::json::decode(&bytes).map_err(|_| ApiError::invalid())?;
    let model = envelope
        .get("model")
        .and_then(serde_json::Value::as_str)
        .ok_or(ApiError::invalid())?;
    let entry = runtime
        .images
        .get(model)
        .ok_or(ApiError::new(StatusCode::NOT_FOUND, "model_not_found"))?
        .clone();
    let request = images::decode_request(&bytes).map_err(|_| ApiError::invalid())?;
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
        body::Producer::Image {
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
    entry: Arc<BoundImage>,
    request: crate::adapter::images::Request,
    deadline: Instant,
    lane: &body::Lane<'_>,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    trace.begin_candidate(1);
    let result = produce_one(&runtime, &entry, &request, deadline, lane, trace).await;
    trace.end_candidate(result.as_ref().err().map(|e| e.code), false);
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
    request: &crate::adapter::images::Request,
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
        crate::execution::images::prepare(&entry.route, &entry.provider, &secret, request)
            .map_err(|_| ApiError::invalid())?;
    trace.stage(Stage::Connect);
    let mut upstream = runtime
        .transport
        .send(prepared, deadline.saturating_duration_since(Instant::now()))
        .await
        .map_err(ApiError::transport)?;
    trace.stage(Stage::ResponseHead);
    trace.head(upstream.status().as_u16(), upstream.headers());
    if upstream.status().as_u16() != 200 {
        return Err(ApiError::status(upstream.status().as_u16()));
    }
    let mut types = upstream.headers().get_all("content-type").iter();
    let media = types
        .next()
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<mime::Mime>().ok())
        .ok_or(ApiError::upstream())?;
    if types.next().is_some()
        || media.type_() != mime::APPLICATION
        || media.subtype() != mime::JSON
        || media
            .params()
            .any(|(k, v)| k == mime::CHARSET && v != mime::UTF_8)
        || upstream.headers().contains_key("content-encoding")
    {
        return Err(ApiError::upstream());
    }
    let limit = runtime
        .limits
        .response_bytes
        .min(entry.route.endpoint.execution.response_body_limit)
        .min(crate::semantic::value::JsonLimits::IMAGE_RESPONSE.bytes);
    if upstream.content_length().is_some_and(|n| n > limit as u64) {
        return Err(ApiError::upstream());
    }
    trace.stage(Stage::Intake);
    let mut bytes = Vec::new();
    while let Some(chunk) = upstream.chunk().await.map_err(|_| ApiError::upstream())? {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(ApiError::upstream());
        }
        trace.received(chunk.len());
        bytes.extend_from_slice(&chunk);
    }
    // Only strict transport EOF plus every complete artifact can be published.
    trace.stage(Stage::Terminal);
    let response = entry
        .route
        .endpoint
        .profile
        .decode_response(&bytes)
        .map_err(|_| ApiError::upstream())?;
    check_response(
        &response,
        &request.task,
        runtime.limits.image_bytes,
        runtime.limits.images_bytes,
    )?;
    trace.stage(Stage::Projection);
    let projected = crate::lowering::images::project_response(&response, entry.route.accounting)
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
