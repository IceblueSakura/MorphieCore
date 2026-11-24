//! Authenticated static image admission and ordered, pre-send target selection.
mod transcode;

use super::{
    exchange::Receiver,
    lifecycle::{Admission, TaskOutput},
};
use crate::{
    ApiError, Credentials, ImageEntry, Runtime, StartupError, body,
    diagnostics::{Stage, Trace},
    ingress::admission,
    protocol::openai::images,
    semantic::value::{ImageFormat, Presence},
    topology::{CompiledTopology, images::ImageRoute},
};
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

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
fn profile_header(headers: &HeaderMap) -> Result<bool, ApiError> {
    let mut profiles = headers.get_all(PROFILE_HEADER).iter();
    match (profiles.next(), profiles.next()) {
        (None, None) => Ok(false),
        (Some(profile), None) if profile.as_bytes() == NATIVE_PROFILE.as_bytes() => Ok(true),
        _ => Err(ApiError::invalid()),
    }
}
fn profile(headers: &HeaderMap, limit: usize) -> Result<bool, ApiError> {
    admission::headers(headers, limit)?;
    profile_header(headers)
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
        match tokio::time::timeout_at(deadline, candidate.secret.availability_for_request())
            .await
            .map_err(|_| ApiError::timeout())?
        {
            Ok(revision) => {
                if let Some(revision) = revision {
                    runtime.executors.verified_material(scopes, revision);
                }
                if runtime
                    .executors
                    .unavailable(scopes, tokio::time::Instant::now())
                    .is_none()
                {
                    return Ok(candidate.clone());
                }
            }
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
pub(crate) async fn handle_edit(State(runtime): State<Arc<Runtime>>, request: Request) -> Response {
    ingress_edit(runtime, request)
        .await
        .unwrap_or_else(IntoResponse::into_response)
}
fn edit_upload_head(
    headers: &HeaderMap,
    limit: usize,
) -> Result<(String, Option<u64>, bool), ApiError> {
    let native = profile_header(headers)?;
    let mut types = headers.get_all("content-type").iter();
    let raw = types
        .next()
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            crate::error::ErrorKind::UnsupportedMediaType,
        ))?;
    if raw.len() > 256 || types.next().is_some() || headers.contains_key("content-encoding") {
        return Err(ApiError::invalid());
    }
    let media: mime::Mime = raw.parse().map_err(|_| ApiError::invalid())?;
    if media.essence_str() != "multipart/form-data" {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            crate::error::ErrorKind::UnsupportedMediaType,
        ));
    }
    let mut lengths = headers.get_all("content-length").iter();
    let length = lengths
        .next()
        .map(|v| {
            let raw = v.to_str().map_err(|_| ApiError::invalid())?;
            if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ApiError::invalid());
            }
            raw.parse::<u64>().map_err(|_| ApiError::invalid())
        })
        .transpose()?;
    if lengths.next().is_some() {
        return Err(ApiError::invalid());
    }
    if length.is_some_and(|n| n > limit as u64) {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            crate::error::ErrorKind::RequestTooLarge,
        ));
    }
    Ok((raw.into(), length, native))
}
async fn ingress(runtime: Arc<Runtime>, request: Request) -> Result<Response, ApiError> {
    let (admitted, native, bytes) =
        Admission::upload(&runtime, request, runtime.limits.request_bytes, profile).await?;
    let envelope = crate::protocol::json::decode(&bytes).map_err(|_| ApiError::invalid())?;
    let model = envelope
        .get("model")
        .and_then(serde_json::Value::as_str)
        .ok_or(ApiError::invalid())?;
    let candidates = runtime
        .images
        .get(model)
        .ok_or(ApiError::new(
            StatusCode::NOT_FOUND,
            crate::error::ErrorKind::ModelNotFound,
        ))?
        .clone();
    let upstream = candidates[0].route.endpoint.profile;
    if native && !upstream.supports_native_static() {
        return Err(ApiError::invalid());
    }
    let request = images::decode_request(&bytes).map_err(|_| ApiError::invalid())?;
    dispatch(admitted, runtime, &candidates, native, request).await
}
async fn ingress_edit(runtime: Arc<Runtime>, request: Request) -> Result<Response, ApiError> {
    let (admitted, (media, length, native), bytes) = Admission::upload(
        &runtime,
        request,
        runtime.limits.image_edit_request_bytes,
        edit_upload_head,
    )
    .await?;
    if length.is_some_and(|n| n != bytes.len() as u64) {
        return Err(ApiError::invalid());
    }
    let request = images::decode_edit_request(&media, bytes.into())
        .await
        .map_err(|err| match err {
            crate::protocol::CodecError::Limit => ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                crate::error::ErrorKind::RequestTooLarge,
            ),
            _ => ApiError::invalid(),
        })?;
    let candidates = runtime
        .images
        .get(&request.model)
        .ok_or(ApiError::new(
            StatusCode::NOT_FOUND,
            crate::error::ErrorKind::ModelNotFound,
        ))?
        .clone();
    let upstream = candidates[0].route.endpoint.profile;
    if native && !upstream.supports_native_static() {
        return Err(ApiError::invalid());
    }
    dispatch(admitted, runtime, &candidates, native, request).await
}
async fn dispatch(
    admitted: Admission,
    runtime: Arc<Runtime>,
    candidates: &[Arc<BoundImage>],
    native: bool,
    request: crate::protocol::image::Request,
) -> Result<Response, ApiError> {
    let upstream = candidates[0].route.endpoint.profile;
    let compatibility = !native && upstream.supports_native_static();
    let wire = if compatibility && upstream != crate::adapter::images::UpstreamAdapter::QwenImage {
        if request
            .task
            .format
            .value()
            .is_some_and(|format| *format != ImageFormat::Png)
        {
            return Err(ApiError::invalid());
        }
        if runtime.limits.image_ffmpeg.is_none() {
            return Err(ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                crate::error::ErrorKind::Unavailable,
            ));
        }
        let mut wire = request.clone();
        // The Provider selects its native format. Publication enforces PNG on
        // the immutable client request, not on this upstream-only projection.
        wire.task.format = Presence::Absent;
        Cow::Owned(wire)
    } else {
        Cow::Borrowed(&request)
    };
    let entry = select(&admitted, &runtime, candidates).await?;
    let prepared = crate::execution::images::prepare(&entry.route, &entry.provider, &wire)
        .map_err(|err| match err {
            crate::execution::AttemptError::Limit => ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                crate::error::ErrorKind::RequestTooLarge,
            ),
            _ => ApiError::invalid(),
        })?;
    drop(wire);
    admitted
        .respond(
            runtime,
            entry,
            prepared,
            Output {
                request,
                native,
                compatibility,
            },
        )
        .await
}
fn check_budget(
    response: &crate::semantic::task::image_generation::ImageGenerationResponse,
    image_bytes: usize,
    images_bytes: usize,
) -> Result<(), ApiError> {
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
    expected_inputs: usize,
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
        .decode_response(&bytes, expected_inputs)
        .map_err(|_| ApiError::upstream())
}
#[cfg(test)]
pub(crate) async fn receive_for_test(
    upstream: reqwest::Response,
    codec: crate::adapter::images::UpstreamAdapter,
    expected_inputs: usize,
    limit: usize,
    trace: &mut Trace,
) -> Result<crate::semantic::task::image_generation::ImageGenerationResponse, ApiError> {
    decode(
        Receiver::new(upstream, limit, trace)?,
        codec,
        expected_inputs,
        trace,
    )
    .await
}
struct Output {
    request: crate::protocol::image::Request,
    native: bool,
    compatibility: bool,
}
impl TaskOutput for Output {
    type Route = ImageRoute;

    fn response_type(&self) -> body::ResponseType {
        body::ResponseType::Json
    }

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
        let expected_inputs = self
            .request
            .task
            .edit()
            .map_or(0, |edit| edit.sources.len());
        let mut response = decode(
            receiver,
            entry.route.endpoint.profile,
            expected_inputs,
            trace,
        )
        .await?;
        if self.compatibility {
            check_budget(&response, limits.image_bytes, limits.images_bytes)?;
            for image in &mut response.images {
                transcode::png(
                    limits.image_ffmpeg.as_deref(),
                    image,
                    limits.image_bytes.min(limits.images_bytes),
                )
                .await?;
            }
        }
        if !self.native
            && self.request.task.format.value().is_none()
            && response.images.iter().any(|image| {
                image
                    .format
                    .value()
                    .is_some_and(|format| *format != ImageFormat::Png)
            })
        {
            return Err(ApiError::upstream());
        }
        response
            .validate_for(&self.request.task)
            .map_err(|_| ApiError::upstream())?;
        check_budget(&response, limits.image_bytes, limits.images_bytes)?;
        if (self.native || self.compatibility)
            && matches!(
                self.request.task.size.value(),
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
        let mut projected =
            crate::protocol::image::projection::project_response(&response, entry.route.accounting)
                .map_err(|_| ApiError::upstream())?;
        if self.compatibility && projected.response.created.value().is_none() {
            let prepared = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| ApiError::upstream())?;
            projected.response.created = Presence::Value(prepared.as_secs());
        }

        let value = if self.native {
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
