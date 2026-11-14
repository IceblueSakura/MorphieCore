//! Pure HTTP/SSE boundary for the stateless Responses profile.
//! Socket I/O, authentication, cancellation and commit remain caller responsibilities.
//! One consume call yields at most one framed payload; it never buffers a whole wire stream.
use crate::{
    protocol::{
        failure::ShapeReason,
        fidelity::FidelityRecords,
        openai::{
            CodecError, DecodedResponse, GenerationApi, ResponseMetadata, TargetContract,
            events::{EventDecoder, EventEncoder},
            obfuscation::{Obfuscation, charge_padding, pad_payload},
        },
        stream::{SseError, SseLimits, validate_http},
    },
    semantic::{
        task::generation::*,
        value::{ReplayOrigin, json_size},
    },
    transport::sse::{SseDecoder, SseEvent},
};
use bytes::Bytes;
use serde_json::{Value, json};
pub struct ResponsesSseDecoder {
    framing: SseDecoder,
    codec: EventDecoder,
    limits: SseLimits,
    wire_bytes: usize,
    events: usize,
    obfuscation_bytes: usize,
    done_marker: bool,
    closed: bool,
    rejected: bool,
    last_event: Option<crate::protocol::stream::SourceEvent>,
}
impl ResponsesSseDecoder {
    pub(crate) fn event_failure(&self) -> Option<crate::protocol::failure::SourceFailureEvidence> {
        self.codec.event_failure()
    }
    pub(crate) fn take_intake_evidence(
        &mut self,
    ) -> crate::protocol::conversion::generation::EvidenceDelta {
        self.codec.take_evidence()
    }
    pub fn new(
        status: u16,
        content_type: &str,
        limits: SseLimits,
        origin: Option<ReplayOrigin>,
    ) -> Result<Self, SseError> {
        limits.validate()?;
        validate_http(status, content_type)?;
        let mut codec = EventDecoder::new(GenerationApi::Responses);
        if let Some(origin) = origin {
            codec = codec.with_replay_origin(origin);
        }
        Ok(Self {
            framing: SseDecoder::new(limits.max_event_bytes),
            codec,
            limits,
            wire_bytes: 0,
            events: 0,
            obfuscation_bytes: 0,
            done_marker: false,
            closed: false,
            rejected: false,
            last_event: None,
        })
    }
    pub fn with_decoder(
        status: u16,
        content_type: &str,
        limits: SseLimits,
        codec: EventDecoder,
    ) -> Result<Self, SseError> {
        if codec.profile() != GenerationApi::Responses {
            return Err(CodecError::ProfileMismatch.into());
        }
        let mut decoder = Self::new(status, content_type, limits, None)?;
        decoder.codec = codec;
        Ok(decoder)
    }
    /// The caller retains and revisits unconsumed bytes, respecting downstream backpressure.
    pub fn consume(&mut self, chunk: &[u8]) -> Result<(usize, Vec<StreamEvent>), SseError> {
        if self.closed || self.rejected {
            return Err(SseError::Closed);
        }
        let result = (|| {
            let remaining = self.limits.max_wire_bytes.saturating_sub(self.wire_bytes);
            let available = chunk.len().min(remaining.saturating_add(1));
            let (event, n) = self.framing.push_until_event(&chunk[..available])?;
            self.wire_bytes = self.wire_bytes.checked_add(n).ok_or(SseError::Limit)?;
            if self.wire_bytes > self.limits.max_wire_bytes {
                return Err(SseError::Limit);
            }
            let events = if let Some(event) = event {
                self.frame(event)?
            } else {
                vec![]
            };
            Ok((n, events))
        })();
        if result.is_err() {
            self.rejected = true;
        }
        result
    }
    fn frame(&mut self, frame: SseEvent) -> Result<Vec<StreamEvent>, SseError> {
        self.last_event = Some(crate::protocol::stream::SourceEvent {
            ordinal: self.events.saturating_add(1),
            kind: event_kind(frame.event()),
            sequence: None,
        });
        if self.done_marker {
            return Err(SseError::Closed);
        }
        self.events += 1;
        if self.events > self.limits.max_events {
            return Err(SseError::Limit);
        }
        if frame.data().is_empty() {
            return Ok(vec![]);
        }
        if frame.data() == "[DONE]" && self.codec.permits_responses_done() {
            self.last_event.as_mut().expect("observed frame").kind = "done_marker";
            if frame
                .event()
                .is_some_and(|e| !e.is_empty() && e != "message")
            {
                return Err(SseError::EventType);
            }
            // Optional transport trailer only: it can never replace or create
            // the already validated semantic terminal.
            self.codec.finish()?;
            self.done_marker = true;
            return Ok(vec![]);
        }
        let mut payload = crate::protocol::json::decode(frame.data().as_bytes())?;
        let context = self.last_event.as_mut().expect("observed frame");
        context.sequence = payload.get("sequence_number").and_then(Value::as_u64);
        let body_kind = payload.get("type").and_then(Value::as_str);
        context.kind = if frame
            .event()
            .filter(|e| !e.is_empty())
            .is_some_and(|header| body_kind.is_some_and(|body| header != body))
        {
            "type_mismatch"
        } else {
            event_kind(body_kind.or(frame.event()))
        };
        let object = payload.as_object_mut().ok_or(CodecError::invalid())?;
        let declared = frame.event().filter(|s| !s.is_empty());
        match (declared, object.get("type")) {
            (Some(header), Some(Value::String(body))) if header == body => {}
            (Some(header), None) => {
                object.insert("type".into(), json!(header));
            }
            (None, Some(Value::String(_))) => {}
            _ => return Err(SseError::EventType),
        }
        if let Some(padding) = payload.get("obfuscation") {
            let bytes = padding.as_str().ok_or(CodecError::invalid())?.len();
            charge_padding(
                &mut self.obfuscation_bytes,
                bytes,
                self.limits.max_obfuscation_bytes,
            )?;
        }
        self.codec.validate_stream_payload(&payload)?;
        Ok(self.codec.push(&payload)?)
    }
    pub fn metadata(&self) -> Option<&ResponseMetadata> {
        self.codec.metadata()
    }
    pub(crate) fn last_event(&self) -> Option<crate::protocol::stream::SourceEvent> {
        self.last_event
    }
    pub fn ignored_events(&self) -> usize {
        self.codec.ignored_events()
    }
    pub fn fidelity(&self) -> &FidelityRecords {
        self.codec.fidelity()
    }
    /// A clean socket EOF is successful only after both framing and the typed terminal validate.
    pub fn finish(&mut self) -> Result<(), SseError> {
        if self.closed || self.rejected {
            return Err(SseError::Closed);
        }
        self.closed = true;
        let result = (|| {
            self.framing.finish_strict()?;
            self.codec.finish()?;
            Ok(())
        })();
        if result.is_err() {
            self.rejected = true;
        }
        result
    }
    pub fn materialize(&self) -> Result<DecodedResponse, SseError> {
        if !self.closed || self.rejected {
            return Err(SseError::Closed);
        }
        Ok(self.codec.materialize()?)
    }
}
fn event_kind(name: Option<&str>) -> &'static str {
    const KINDS: &[&str] = &[
        "response.created",
        "response.queued",
        "response.in_progress",
        "response.completed",
        "response.incomplete",
        "response.failed",
        "error",
        "response.output_item.added",
        "response.output_item.done",
        "response.content_part.added",
        "response.content_part.done",
        "response.output_text.delta",
        "response.output_text.done",
        "response.output_text.annotation.added",
        "response.refusal.delta",
        "response.refusal.done",
        "response.reasoning_summary_part.added",
        "response.reasoning_summary_part.done",
        "response.reasoning_summary_text.delta",
        "response.reasoning_summary_text.done",
        "response.reasoning_text.delta",
        "response.reasoning_text.done",
        "response.function_call_arguments.delta",
        "response.function_call_arguments.done",
        "response.custom_tool_call_input.delta",
        "response.custom_tool_call_input.done",
    ];
    match name {
        None => "missing",
        Some(name) => KINDS
            .iter()
            .copied()
            .find(|known| *known == name)
            .unwrap_or("unknown"),
    }
}

pub struct ResponsesSseEncoder {
    notice: Option<crate::protocol::diagnostics::AdaptationNotice>,
    codec: EventEncoder,
    limits: SseLimits,
    wire_bytes: usize,
    events: usize,
    obfuscation_bytes: usize,
    padding: Obfuscation,
    rejected: bool,
}
impl ResponsesSseEncoder {
    pub(crate) fn set_notice(&mut self, notice: crate::protocol::diagnostics::AdaptationNotice) {
        self.notice = Some(notice);
    }
    pub(crate) fn take_evidence(
        &mut self,
    ) -> crate::protocol::conversion::generation::EvidenceDelta {
        self.codec.take_evidence()
    }
    #[cfg(test)]
    pub(crate) fn projection(&self) -> &[crate::protocol::conversion::generation::Effect] {
        self.codec.projection()
    }
    pub fn new(
        metadata: ResponseMetadata,
        contract: TargetContract,
        limits: SseLimits,
        padding: Obfuscation,
    ) -> Result<Self, SseError> {
        limits.validate()?;
        metadata.context.validate().map_err(CodecError::from)?;
        Ok(Self {
            notice: None,
            codec: EventEncoder::new(GenerationApi::Responses, metadata, contract)?,
            limits,
            wire_bytes: 0,
            events: 0,
            obfuscation_bytes: 0,
            padding,
            rejected: false,
        })
    }
    pub fn update_metadata(&mut self, metadata: ResponseMetadata) -> Result<(), SseError> {
        if self.rejected {
            return Err(SseError::Closed);
        }
        let result = self.codec.update_metadata(metadata);
        if result.is_err() {
            self.rejected = true;
        }
        Ok(result?)
    }
    /// A gateway failure is not a model-reported failed Response. Preserve its
    /// classification in the standard error event and close this encoder.
    pub(crate) fn fail(&mut self, error: &Value) -> Result<Bytes, SseError> {
        if self.rejected {
            return Err(SseError::Closed);
        }
        self.rejected = true;
        if self.events >= self.limits.max_events {
            return Err(SseError::Limit);
        }
        let frame = error_frame(
            error,
            self.events,
            self.limits
                .max_event_bytes
                .min(self.limits.max_wire_bytes.saturating_sub(self.wire_bytes)),
            self.notice.as_ref(),
        )?;
        if frame.len() > self.limits.max_wire_bytes.saturating_sub(self.wire_bytes) {
            return Err(SseError::Limit);
        }
        Ok(frame)
    }
    pub fn encode(
        &mut self,
        event: &StreamEvent,
        fidelity: &FidelityRecords,
    ) -> Result<Vec<Bytes>, SseError> {
        if self.rejected {
            return Err(SseError::Closed);
        }
        let result = (|| {
            let values = self.codec.encode(event, fidelity)?;
            let mut notice = if matches!(event, StreamEvent::Terminal { .. }) {
                self.notice.take()
            } else {
                None
            };
            if let Some(notice) = &mut notice {
                notice.add_effects(self.codec.projection());
            }
            let mut frames = vec![];
            for mut value in values {
                self.events += 1;
                if self.events > self.limits.max_events {
                    return Err(SseError::Limit);
                }
                let typ = value["type"]
                    .as_str()
                    .ok_or(CodecError::invalid())?
                    .to_owned();
                if typ.ends_with(".delta")
                    && let Obfuscation::Seeded(seed) = &self.padding
                {
                    pad_payload(
                        &mut value,
                        seed,
                        self.events,
                        self.limits
                            .max_event_bytes
                            .saturating_sub(typ.len() + 16)
                            .min(MAX_TOTAL_BYTES),
                        &mut self.obfuscation_bytes,
                        self.limits.max_obfuscation_bytes,
                    )?;
                }
                crate::protocol::openai::responses::envelope::validate_stream_payload(&value)?;
                if matches!(
                    typ.as_str(),
                    "response.completed"
                        | "response.incomplete"
                        | "response.failed"
                        | "response.cancelled"
                ) && let Some(notice) = &notice
                {
                    let available = self
                        .limits
                        .max_event_bytes
                        .min(self.limits.max_wire_bytes.saturating_sub(self.wire_bytes))
                        .saturating_sub(typ.len() + 16);
                    notice.attach(&mut value, available, true);
                }
                let frame = encode_frame(&value, self.limits.max_event_bytes)?;
                self.wire_bytes = self
                    .wire_bytes
                    .checked_add(frame.len())
                    .ok_or(SseError::Limit)?;
                if self.wire_bytes > self.limits.max_wire_bytes {
                    return Err(SseError::Limit);
                }
                frames.push(frame);
            }
            Ok(frames)
        })();
        if result.is_err() {
            self.rejected = true;
        }
        result
    }
    pub fn finish(&self) -> Result<(), SseError> {
        if self.rejected {
            return Err(SseError::Closed);
        }
        Ok(self.codec.finish()?)
    }
}
pub(crate) fn error_frame(
    error: &Value,
    sequence: usize,
    max_bytes: usize,
    notice: Option<&crate::protocol::diagnostics::AdaptationNotice>,
) -> Result<Bytes, SseError> {
    let error = error.get("error").ok_or(CodecError::invalid())?;
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .ok_or(CodecError::invalid())?;
    let mut value = json!({
        "type":"error", "sequence_number":sequence,
        "code":error.get("code").and_then(Value::as_str),
        "message":message, "param":error.get("param").and_then(Value::as_str),
    });
    if let Some(notice) = notice {
        notice.attach(
            &mut value,
            max_bytes.saturating_sub("error".len() + 16),
            false,
        );
    }
    encode_frame(&value, max_bytes)
}
/// Encode exactly one payload with a matching event name and a complete LF delimiter.
pub fn encode_frame(payload: &Value, max_bytes: usize) -> Result<Bytes, SseError> {
    let typ = payload
        .get("type")
        .and_then(Value::as_str)
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
        })
        .ok_or(SseError::EventType)?;
    let prefix = format!("event: {typ}\ndata: ");
    let size = json_size(payload, max_bytes).map_err(|_| SseError::Limit)?;
    if prefix.len().saturating_add(size).saturating_add(2) > max_bytes {
        return Err(SseError::Limit);
    }
    let mut wire = Vec::with_capacity(prefix.len() + size + 2);
    wire.extend_from_slice(prefix.as_bytes());
    serde_json::to_writer(&mut wire, payload)
        .map_err(|_| CodecError::Invalid(ShapeReason::Json))?;
    wire.extend_from_slice(b"\n\n");
    Ok(Bytes::from(wire))
}
