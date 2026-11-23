//! Authenticated image admission and a single bounded static exchange.
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
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::{collections::BTreeMap, sync::Arc};

pub(crate) type BoundImage = super::BoundTask<ImageRoute>;
pub(crate) fn bind(
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
        .ok_or(ApiError::invalid())?;
    let entry = runtime
        .images
        .get(model)
        .ok_or(ApiError::new(
            StatusCode::NOT_FOUND,
            crate::error::ErrorKind::ModelNotFound,
        ))?
        .clone();
    let request = images::decode_request(&bytes).map_err(|_| ApiError::invalid())?;
    let prepared = crate::execution::images::prepare(&entry.route, &entry.provider, &request)
        .map_err(|_| ApiError::invalid())?;
    admitted
        .respond(runtime, entry, prepared, Output(request))
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
struct Output(crate::protocol::image::Request);
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
        trace.stage(Stage::Projection);
        let projected =
            crate::protocol::image::projection::project_response(&response, entry.route.accounting)
                .map_err(|_| ApiError::upstream())?;

        let value =
            images::encode_response(&projected.response).map_err(|_| ApiError::upstream())?;
        crate::semantic::value::json_size(&value, limits.response_bytes)
            .map_err(|_| ApiError::upstream())?;
        let output = serde_json::to_vec(&value).map_err(|_| ApiError::upstream())?;
        body::send_final_frame(lane, output.into(), trace).await?;
        Ok(())
    }
}
