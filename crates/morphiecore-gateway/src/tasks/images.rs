//! Authenticated image admission, bounded transcoding, and ordered, pre-send target selection.

use super::{
    exchange::{self, Receiver, SseStep},
    lifecycle::{Admission, TaskOutput},
};
use crate::{
    ApiError, Credentials, ImageEntry, Runtime, StartupError, body,
    diagnostics::{Stage, Trace},
    ingress::admission,
    protocol::openai::images,
    semantic::{
        task::image_generation::{
            ImageBackgroundRequest, ImageQualityRequest, ImageSizeRequest, ImageStreamEvent,
            ImageStreamSettings,
        },
        value::{ImageFormat, Presence},
    },
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
fn edit_upload_head(headers: &HeaderMap, limit: usize) -> Result<(String, Option<u64>), ApiError> {
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
    Ok((raw.into(), length))
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
    let candidates = runtime
        .images
        .get(model)
        .ok_or(ApiError::new(
            StatusCode::NOT_FOUND,
            crate::error::ErrorKind::ModelNotFound,
        ))?
        .clone();
    let request = images::decode_request(&bytes).map_err(|_| ApiError::invalid())?;
    dispatch(admitted, runtime, &candidates, request).await
}
async fn ingress_edit(runtime: Arc<Runtime>, request: Request) -> Result<Response, ApiError> {
    let (admitted, (media, length), bytes) = Admission::upload(
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
    dispatch(admitted, runtime, &candidates, request).await
}
async fn dispatch(
    admitted: Admission,
    runtime: Arc<Runtime>,
    candidates: &[Arc<BoundImage>],
    request: crate::protocol::image::Request,
) -> Result<Response, ApiError> {
    let upstream = candidates[0].route.endpoint.profile;
    let compatibility = upstream.supports_native_static();
    let target_format = request
        .task
        .format
        .value()
        .copied()
        .unwrap_or(ImageFormat::Png);
    let target_compression = request.task.compression.value().copied();
    let needs_ffmpeg = compatibility
        && (upstream != crate::adapter::images::UpstreamAdapter::QwenImage
            || target_format != ImageFormat::Png
            || target_compression.is_some());
    let wire = if compatibility {
        let mut wire = request.clone();
        wire.delivery.stream = None;
        wire.task.partial_images = Presence::Absent;
        // The Provider selects its native format. Publication enforces the
        // client's target output_format/compression on the immutable request.
        wire.task.format = Presence::Absent;
        wire.task.compression = Presence::Absent;
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
    if needs_ffmpeg && runtime.limits.ffmpeg.is_none() {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            crate::error::ErrorKind::Unavailable,
        ));
    }
    admitted
        .respond(
            runtime,
            entry,
            prepared,
            Output {
                request,
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
    compatibility: bool,
}
impl TaskOutput for Output {
    type Route = ImageRoute;

    fn response_type(&self) -> body::ResponseType {
        if self.request.delivery.streaming() {
            body::ResponseType::EventStream
        } else {
            body::ResponseType::Json
        }
    }

    fn receive_limit(&self, entry: &BoundImage, _: &crate::Limits, wire_limit: usize) -> usize {
        if self.request.delivery.streaming() && entry.route.endpoint.profile.supports_streaming() {
            wire_limit
        } else {
            wire_limit.min(crate::semantic::value::JsonLimits::IMAGE_RESPONSE.bytes)
        }
    }

    async fn publish(
        self,
        receiver: Receiver,
        entry: &BoundImage,
        limits: &crate::Limits,
        lane: &body::Lane<'_>,
        trace: &mut Trace,
    ) -> Result<(), ApiError> {
        let is_edit = self.request.task.is_edit();
        let target_format = self
            .request
            .task
            .format
            .value()
            .copied()
            .unwrap_or(ImageFormat::Png);
        let target_compression = self.request.task.compression.value().copied();
        if self.request.delivery.streaming() && entry.route.endpoint.profile.supports_streaming() {
            let per_image_limit = limits.image_bytes.min(limits.images_bytes);
            let max_total_decoded_bytes = per_image_limit.saturating_mul(4);
            let allowed = if is_edit {
                ["image_edit.partial_image", "image_edit.completed"]
            } else {
                [
                    "image_generation.partial_image",
                    "image_generation.completed",
                ]
            };
            let decoder = images::StreamDecoder::new(
                is_edit,
                self.request.task.partial_images.value().copied(),
                target_format,
                per_image_limit,
                max_total_decoded_bytes,
                limits.max_events,
            );
            return exchange::stream_sse(
                receiver,
                limits,
                lane,
                trace,
                decoder,
                |decoder, event| {
                    let value = exchange::parse_sse_json(event, &allowed, true)?;
                    let step = decoder.push(&value).map_err(|_| ApiError::upstream())?;
                    let is_completed = matches!(step, ImageStreamEvent::Completed { .. });
                    let bytes = images::encode_sse_event(&step, is_edit)
                        .map_err(|_| ApiError::upstream())?;
                    if is_completed {
                        Ok(SseStep::HoldTerminal(bytes))
                    } else {
                        Ok(SseStep::EmitDelta(bytes))
                    }
                },
                |decoder| decoder.finish().map_err(|_| ApiError::upstream()),
            )
            .await;
        }
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
            let limit = limits.image_bytes.min(limits.images_bytes);
            for image in &mut response.images {
                crate::tasks::transcode::image(
                    limits.ffmpeg.as_deref(),
                    image,
                    target_format,
                    target_compression,
                    limit,
                )
                .await?;
            }
        }
        if self.request.task.format.value().is_none()
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
        if self.compatibility
            && matches!(
                self.request.task.size.value(),
                Some(ImageSizeRequest::Exact(_))
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

        if self.request.delivery.streaming() {
            let event_time = projected
                .response
                .created
                .value()
                .copied()
                .ok_or_else(ApiError::upstream)?;
            let image = projected
                .response
                .images
                .into_iter()
                .next()
                .ok_or_else(ApiError::upstream)?;
            let size = match image.size.value() {
                Some(exact) => ImageSizeRequest::Exact(*exact),
                None => ImageSizeRequest::Auto,
            };
            let settings = ImageStreamSettings {
                format: target_format,
                size,
                quality: ImageQualityRequest::Auto,
                background: ImageBackgroundRequest::Auto,
            };
            let event = ImageStreamEvent::Completed {
                data: image.data,
                event_time,
                settings,
                safety_identifier: Presence::Absent,
                usage: None,
            };
            let sse =
                images::encode_sse_event(&event, is_edit).map_err(|_| ApiError::upstream())?;
            if sse.len() > limits.response_bytes {
                return Err(ApiError::upstream());
            }
            body::send_final_frame(lane, sse, trace).await?;
            return Ok(());
        }

        let value =
            images::encode_response(&projected.response).map_err(|_| ApiError::upstream())?;
        crate::semantic::value::json_size(&value, limits.response_bytes)
            .map_err(|_| ApiError::upstream())?;
        let output = serde_json::to_vec(&value).map_err(|_| ApiError::upstream())?;
        body::send_final_frame(lane, output.into(), trace).await?;
        Ok(())
    }
}
