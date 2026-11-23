//! Authenticated static image admission and ordered, pre-send target selection.
use super::{
    exchange::Receiver,
    lifecycle::{Admission, TaskOutput},
};
use crate::{
    ApiError, Credentials, ImageEntry, Runtime, StartupError, body,
    diagnostics::{Stage, Trace},
    ingress::admission,
    protocol::openai::images,
    topology::{CompiledTopology, images::ImageRoute},
};
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use std::{collections::BTreeMap, sync::Arc};

pub(crate) const NATIVE_PROFILE: &str = "native-static-v1";
const PROFILE_HEADER: &str = "x-morphiecore-image-profile";
pub(crate) type BoundImage = super::BoundTask<ImageRoute>;
pub(crate) fn bind(
    topology: &CompiledTopology,
    entries: Vec<ImageEntry>,
    credentials: &Credentials,
    namespace: &[u8; 32],
) -> Result<BTreeMap<String, Vec<Arc<BoundImage>>>, StartupError> {
    if entries.len() > 64 {
        return Err(StartupError::Binding);
    }
    let mut bound = BTreeMap::new();
    for ImageEntry { model } in entries {
        let candidates = if let Some(configured) = credentials.image_candidates(&model) {
            let mut routes = topology.image_bindings(&model);
            configured
                .iter()
                .map(|(endpoint, account)| {
                    let route = routes
                        .find(|route| &route.endpoint.id == endpoint)
                        .ok_or(StartupError::Binding)?;
                    let credential = credentials.get(account).ok_or(StartupError::Credentials)?;
                    let mut target = super::bind_target(topology, route.clone(), credential)?;
                    target.health = Some(crate::routing::executors::Scopes::bind_image(
                        namespace,
                        &target.route.endpoint,
                        &target.secret.provenance(),
                    ));
                    Ok(Arc::new(target))
                })
                .collect::<Result<Vec<_>, StartupError>>()?
        } else {
            let route = topology.image_route(&model).ok_or(StartupError::Binding)?;
            let credential = credentials
                .task(&model, &route.endpoint.credential)
                .ok_or(StartupError::Credentials)?;
            vec![Arc::new(super::bind_target(
                topology,
                route.clone(),
                credential,
            )?)]
        };
        if candidates.is_empty() || bound.insert(model, candidates).is_some() {
            return Err(StartupError::Binding);
        }
    }
    Ok(bound)
}
fn profile(headers: &HeaderMap, limit: usize) -> Result<bool, ApiError> {
    admission::headers(headers, limit)?;
    let mut profiles = headers.get_all(PROFILE_HEADER).iter();
    match (profiles.next(), profiles.next()) {
        (None, None) => Ok(false),
        (Some(profile), None) if profile.as_bytes() == NATIVE_PROFILE.as_bytes() => Ok(true),
        _ => Err(ApiError::invalid()),
    }
}
async fn select(
    admitted: &Admission,
    runtime: &Runtime,
    candidates: &[Arc<BoundImage>],
) -> Result<Arc<BoundImage>, ApiError> {
    let mut unavailable = ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        crate::error::ErrorKind::Unavailable,
    );
    for candidate in candidates {
        let Some(scopes) = &candidate.health else {
            return Ok(candidate.clone());
        };
        let deadline = admitted.deadline(
            &runtime.limits,
            candidate.route.endpoint.execution.timeout_ms,
        );
        if tokio::time::Instant::now() >= deadline {
            return Err(ApiError::timeout());
        }
        if runtime
            .executors
            .unavailable(scopes, tokio::time::Instant::now())
            .is_some()
        {
            continue;
        }
        match tokio::time::timeout_at(deadline, candidate.secret.availability_for_request())
            .await
            .map_err(|_| ApiError::timeout())?
        {
            Ok(_) => return Ok(candidate.clone()),
            Err(error) if error.kind == crate::error::ErrorKind::CredentialUnavailable => {
                unavailable = error
            }
            Err(error) => return Err(error),
        }
    }
    Err(unavailable)
}
pub(crate) async fn handle(State(runtime): State<Arc<Runtime>>, request: Request) -> Response {
    ingress(runtime, request)
        .await
        .unwrap_or_else(IntoResponse::into_response)
}
async fn ingress(runtime: Arc<Runtime>, request: Request) -> Result<Response, ApiError> {
    let (admitted, native, bytes) =
        Admission::upload(&runtime, request, runtime.limits.request_bytes, profile).await?;
    let envelope = crate::protocol::json::decode(&bytes).map_err(|_| ApiError::invalid())?;
    let model = envelope
        .get("model")
        .and_then(serde_json::Value::as_str)
        .ok_or(ApiError::invalid())?;
    let candidates = runtime.images.get(model).ok_or(ApiError::new(
        StatusCode::NOT_FOUND,
        crate::error::ErrorKind::ModelNotFound,
    ))?;
    if native
        != candidates[0]
            .route
            .endpoint
            .profile
            .requires_native_static()
    {
        return Err(ApiError::invalid());
    }
    let request = images::decode_request(&bytes).map_err(|_| ApiError::invalid())?;
    let entry = select(&admitted, &runtime, candidates).await?;
    let prepared = crate::execution::images::prepare(&entry.route, &entry.provider, &request)
        .map_err(|_| ApiError::invalid())?;
    admitted
        .respond(runtime, entry, prepared, Output(request, native))
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
async fn decode(
    receiver: Receiver,
    codec: crate::adapter::images::UpstreamAdapter,
    trace: &mut Trace,
) -> Result<crate::semantic::task::image_generation::ImageGenerationResponse, ApiError> {
    receiver.require_media("application/json")?;
    let bytes = receiver.collect(trace).await.map_err(|error| {
        // Preserve this operation's existing public read-error policy while
        // the shared receiver retains the precise timeout cause internally.
        if error.kind == crate::error::ErrorKind::Timeout {
            ApiError::upstream()
        } else {
            error
        }
    })?;
    codec
        .decode_response(&bytes)
        .map_err(|_| ApiError::upstream())
}
#[cfg(test)]
pub(crate) async fn receive_for_test(
    upstream: reqwest::Response,
    codec: crate::adapter::images::UpstreamAdapter,
    limit: usize,
    trace: &mut Trace,
) -> Result<crate::semantic::task::image_generation::ImageGenerationResponse, ApiError> {
    decode(Receiver::new(upstream, limit, trace)?, codec, trace).await
}
struct Output(crate::protocol::image::Request, bool);
impl TaskOutput for Output {
    type Route = ImageRoute;
    const RESPONSE: body::ResponseType = body::ResponseType::Json;

    fn receive_limit(&self, _: &BoundImage, _: &crate::Limits, wire_limit: usize) -> usize {
        wire_limit.min(crate::semantic::value::JsonLimits::IMAGE_RESPONSE.bytes)
    }

    async fn publish(
        self,
        receiver: Receiver,
        entry: &BoundImage,
        limits: &crate::Limits,
        lane: &body::Lane<'_>,
        trace: &mut Trace,
    ) -> Result<(), ApiError> {
        let response = decode(receiver, entry.route.endpoint.profile, trace).await?;
        if self.0.task.format.value().is_none()
            && response.images.iter().any(|image| {
                image
                    .format
                    .value()
                    .is_some_and(|format| *format != crate::semantic::value::ImageFormat::Png)
            })
        {
            return Err(ApiError::upstream());
        }
        check_response(
            &response,
            &self.0.task,
            limits.image_bytes,
            limits.images_bytes,
        )?;
        if self.1
            && matches!(
                self.0.task.size.value(),
                Some(crate::semantic::task::image_generation::ImageSizeRequest::Exact(_))
            )
            && response
                .images
                .iter()
                .any(|image| image.size.value().is_none())
        {
            return Err(ApiError::upstream());
        }
        trace.stage(Stage::Projection);
        let projected =
            crate::protocol::image::projection::project_response(&response, entry.route.accounting)
                .map_err(|_| ApiError::upstream())?;

        let value = if self.1 {
            images::encode_native_static_response(&projected.response)
        } else {
            images::encode_response(&projected.response)
        }
        .map_err(|_| ApiError::upstream())?;
        crate::semantic::value::json_size(&value, limits.response_bytes)
            .map_err(|_| ApiError::upstream())?;
        let output = serde_json::to_vec(&value).map_err(|_| ApiError::upstream())?;
        body::send_final_frame(lane, output.into(), trace).await?;
        Ok(())
    }
}
