//! Authenticated Speech admission and bounded static binary delivery.
pub(crate) mod aliyun;
use crate::tasks::{
    exchange::{self, Receiver, SseStep},
    lifecycle::{Admission, TaskOutput},
};
use crate::{
    ApiError, Credentials, Runtime, SpeechEntry, StartupError, body,
    diagnostics::{Stage, Trace},
    ingress::admission,
    protocol::{openai::speech as codec, speech::StreamFormat},
    semantic::{
        task::speech_synthesis::{SpeechEvent, SpeechRequest},
        value::{AudioArtifact, AudioEncoding, MAX_AUDIO_BYTES},
    },
    topology::{CompiledTopology, speech::SpeechRoute},
};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::{collections::BTreeMap, sync::Arc};

pub(crate) type BoundSpeech = crate::tasks::BoundTask<SpeechRoute>;
pub(crate) fn bind(
    topology: &CompiledTopology,
    entries: Vec<SpeechEntry>,
    credentials: &Credentials,
) -> Result<BTreeMap<String, Arc<BoundSpeech>>, StartupError> {
    crate::tasks::bind(
        topology,
        entries,
        credentials,
        |entry: &SpeechEntry| topology.speech_route(&entry.model),
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
        .ok_or_else(ApiError::invalid)?;
    let entry = runtime
        .speech
        .get(model)
        .ok_or(ApiError::new(
            StatusCode::NOT_FOUND,
            crate::error::ErrorKind::ModelNotFound,
        ))?
        .clone();
    let request = codec::decode_request(&bytes).map_err(|_| ApiError::invalid())?;
    let profile = entry.route.endpoint.profile;
    let requested = request.task.requested_format();
    let (wire_format, needs_transcode) = match profile {
        crate::adapter::speech::UpstreamAdapter::Standard => (requested, false),
        crate::adapter::speech::UpstreamAdapter::AliyunMp3 => {
            (AudioEncoding::Mp3, requested != AudioEncoding::Mp3)
        }
        crate::adapter::speech::UpstreamAdapter::Xiaomi => {
            if crate::protocol::xiaomi_audio::FORMATS.contains(&requested) {
                (requested, false)
            } else {
                (AudioEncoding::Wav, true)
            }
        }
    };
    let native_sse = request.streaming() && profile.supports_streaming() && !needs_transcode;
    let wire = if needs_transcode || (request.streaming() && !native_sse) {
        let mut wire = request.clone();
        wire.task.format = Some(wire_format);
        wire.stream_format = if native_sse {
            Some(StreamFormat::Sse)
        } else {
            None
        };
        std::borrow::Cow::Owned(wire)
    } else {
        std::borrow::Cow::Borrowed(&request)
    };
    let prepared = crate::execution::speech::prepare(&entry.route, &entry.provider, &wire)
        .map_err(|_| ApiError::invalid())?;
    if needs_transcode && runtime.limits.ffmpeg.is_none() {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            crate::error::ErrorKind::Unavailable,
        ));
    }
    let wire_task = wire.into_owned().task;
    admitted
        .respond(
            runtime,
            entry,
            prepared,
            Output {
                request,
                wire_task,
                native_sse,
                needs_transcode,
            },
        )
        .await
}
async fn decode(
    receiver: Receiver,
    task: &SpeechRequest,
    profile: crate::adapter::speech::UpstreamAdapter,
    trace: &mut Trace,
) -> Result<AudioArtifact, ApiError> {
    let encoding = profile
        .reported_encoding(receiver.media_type(), task)
        .map_err(|_| ApiError::upstream())?;
    let bytes = receiver.collect(trace).await?;
    if profile == crate::adapter::speech::UpstreamAdapter::Xiaomi {
        let result = crate::protocol::xiaomi_audio::decode_speech(&bytes, task)
            .map_err(|_| ApiError::upstream())?;
        Ok(codec::project_result(&result)
            .map_err(|_| ApiError::upstream())?
            .audio)
    } else {
        AudioArtifact::new(bytes.into(), encoding).map_err(|_| ApiError::upstream())
    }
}
#[cfg(test)]
pub(crate) async fn receive_for_test(
    upstream: reqwest::Response,
    task: &SpeechRequest,
    profile: crate::adapter::speech::UpstreamAdapter,
    limit: usize,
    trace: &mut Trace,
) -> Result<AudioArtifact, ApiError> {
    decode(
        Receiver::new(upstream, limit.min(MAX_AUDIO_BYTES), trace)?,
        task,
        profile,
        trace,
    )
    .await
}
struct Output {
    request: crate::protocol::speech::Request,
    wire_task: SpeechRequest,
    native_sse: bool,
    needs_transcode: bool,
}
impl TaskOutput for Output {
    type Route = SpeechRoute;

    fn response_type(&self) -> body::ResponseType {
        if self.request.streaming() {
            body::ResponseType::EventStream
        } else {
            body::ResponseType::Audio
        }
    }

    fn receive_limit(
        &self,
        entry: &BoundSpeech,
        limits: &crate::Limits,
        wire_limit: usize,
    ) -> usize {
        if entry.route.endpoint.profile.upstream_sse() || self.native_sse {
            wire_limit
        } else {
            limits.speech_bytes.min(wire_limit).min(MAX_AUDIO_BYTES)
        }
    }

    async fn publish(
        self,
        receiver: Receiver,
        entry: &BoundSpeech,
        limits: &crate::Limits,
        lane: &body::Lane<'_>,
        trace: &mut Trace,
    ) -> Result<(), ApiError> {
        let wire_limit = limits
            .response_bytes
            .min(entry.route.endpoint.execution.response_body_limit);
        let audio_limit = limits.speech_bytes.min(wire_limit).min(MAX_AUDIO_BYTES);
        if self.native_sse {
            let map_event = |event: SpeechEvent| -> Result<SseStep, ApiError> {
                let is_done = matches!(event, SpeechEvent::Done { .. });
                let bytes = codec::encode_sse_event(&event).map_err(|_| ApiError::upstream())?;
                if is_done {
                    Ok(SseStep::HoldTerminal(bytes))
                } else {
                    Ok(SseStep::EmitDelta(bytes))
                }
            };
            if entry.route.endpoint.profile.upstream_sse() {
                let decoder = crate::protocol::aliyun_speech::Decoder::new_streaming(
                    audio_limit,
                    wire_limit,
                    limits.max_events,
                );
                return exchange::stream_sse(
                    receiver,
                    limits,
                    lane,
                    trace,
                    decoder,
                    |decoder, event| {
                        let value = exchange::parse_sse_json(event, &["result"], false)?;
                        match decoder
                            .push_stream(&value)
                            .map_err(|_| ApiError::upstream())?
                        {
                            None => Ok(SseStep::Ignore),
                            Some(step) => map_event(step),
                        }
                    },
                    |decoder| decoder.finish_stream().map_err(|_| ApiError::upstream()),
                )
                .await;
            }
            let decoder = codec::StreamDecoder::new(audio_limit, limits.max_events);
            return exchange::stream_sse(
                receiver,
                limits,
                lane,
                trace,
                decoder,
                |decoder, event| {
                    let value = exchange::parse_sse_json(
                        event,
                        &["speech.audio.delta", "speech.audio.done"],
                        true,
                    )?;
                    let step = decoder.push(&value).map_err(|_| ApiError::upstream())?;
                    map_event(step)
                },
                |decoder| decoder.finish().map_err(|_| ApiError::upstream()),
            )
            .await;
        }
        let mut audio = if entry.route.endpoint.profile.upstream_sse() {
            let result = aliyun::decode(
                receiver,
                audio_limit,
                wire_limit,
                limits.event_bytes,
                limits.max_events,
                trace,
            )
            .await?;
            codec::project_result(&result)
                .map_err(|_| ApiError::upstream())?
                .audio
        } else {
            decode(
                receiver,
                &self.wire_task,
                entry.route.endpoint.profile,
                trace,
            )
            .await?
        };
        if self.needs_transcode {
            audio = crate::tasks::transcode::speech_audio(
                limits.ffmpeg.as_deref(),
                audio,
                self.request.task.requested_format(),
                audio_limit,
            )
            .await?;
        }
        trace.stage(Stage::Projection);
        if self.request.streaming() {
            for chunk in audio.into_data().chunks(64 << 10) {
                let frame = codec::encode_sse_event(&SpeechEvent::AudioDelta {
                    audio: bytes::Bytes::copy_from_slice(chunk),
                })
                .map_err(|_| ApiError::upstream())?;
                body::send_frame(lane, frame, trace).await?;
            }
            let done = codec::encode_sse_event(&SpeechEvent::Done { usage: None })
                .map_err(|_| ApiError::upstream())?;
            body::send_final_frame(lane, done, trace).await?;
        } else {
            body::send_final_audio(lane, audio, trace).await?;
        }
        Ok(())
    }
}
