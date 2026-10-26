//! Chat data-only SSE over shared bounded framing, strict JSON and Generation event codecs.
//! EOF is not DONE. Body I/O, cancellation and downstream commit belong to the caller.
use crate::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            CodecError, DecodedResponse, GenerationApi, ResponseMetadata, TargetContract,
            chat::envelope as chat_envelope,
            events::{EventDecoder, EventEncoder},
            obfuscation::{Obfuscation, charge_padding, pad_payload},
        },
        stream::{SseError, SseLimits, validate_http},
    },
    semantic::{
        context::StreamOptions,
        task::generation::{MAX_TOTAL_BYTES, StreamEvent},
    },
    transport::sse::{SseDecoder, SseEvent},
};
use bytes::Bytes;

pub struct ChatSseDecoder {
    framing: SseDecoder,
    codec: EventDecoder,
    limits: SseLimits,
    wire_bytes: usize,
    events: usize,
    padding_bytes: usize,
    done: bool,
    closed: bool,
    rejected: bool,
}
impl ChatSseDecoder {
    pub(crate) fn take_intake_evidence(
        &mut self,
    ) -> crate::protocol::conversion::generation::EvidenceDelta {
        self.codec.take_evidence()
    }
    pub fn new(status: u16, content_type: &str, limits: SseLimits) -> Result<Self, SseError> {
        limits.validate()?;
        validate_http(status, content_type)?;
        Ok(Self {
            framing: SseDecoder::new(limits.max_event_bytes),
            codec: EventDecoder::new(GenerationApi::Chat),
            limits,
            wire_bytes: 0,
            events: 0,
            padding_bytes: 0,
            done: false,
            closed: false,
            rejected: false,
        })
    }
    pub fn with_decoder(
        status: u16,
        content_type: &str,
        limits: SseLimits,
        codec: EventDecoder,
    ) -> Result<Self, SseError> {
        if codec.profile() != GenerationApi::Chat {
            return Err(CodecError::ProfileMismatch.into());
        }
        let mut decoder = Self::new(status, content_type, limits)?;
        decoder.codec = codec;
        Ok(decoder)
    }
    pub fn metadata(&self) -> Option<&ResponseMetadata> {
        self.codec.metadata()
    }
    pub fn fidelity(&self) -> &FidelityRecords {
        self.codec.fidelity()
    }
    /// Consume at most one event; the caller must revisit the unconsumed suffix.
    pub fn consume(&mut self, chunk: &[u8]) -> Result<(usize, Vec<StreamEvent>), SseError> {
        if self.closed || self.rejected {
            return Err(SseError::Closed);
        }
        let result = (|| {
            let remaining = self.limits.max_wire_bytes.saturating_sub(self.wire_bytes);
            let available = chunk.len().min(remaining.saturating_add(1));
            let (frame, n) = self.framing.push_until_event(&chunk[..available])?;
            self.wire_bytes = self.wire_bytes.checked_add(n).ok_or(SseError::Limit)?;
            if self.wire_bytes > self.limits.max_wire_bytes {
                return Err(SseError::Limit);
            }
            let events = if let Some(frame) = frame {
                self.frame(frame)?
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
        if self.done {
            return Err(SseError::Closed);
        }
        self.events += 1;
        if self.events > self.limits.max_events {
            return Err(SseError::Limit);
        }
        if frame
            .event()
            .is_some_and(|s| !s.is_empty() && s != "message")
        {
            return Err(SseError::EventType);
        }
        if frame.data().is_empty() {
            return Ok(vec![]);
        }
        if frame.data() == "[DONE]" {
            let events = self.codec.done()?;
            self.done = true;
            return Ok(events);
        }
        let mut payload = crate::protocol::json::decode(frame.data().as_bytes())?;
        let object = payload.as_object_mut().ok_or(CodecError::invalid())?;
        if let Some(padding) = object.remove("obfuscation") {
            let n = padding.as_str().ok_or(CodecError::invalid())?.len();
            charge_padding(
                &mut self.padding_bytes,
                n,
                self.limits.max_obfuscation_bytes,
            )?;
        }
        // OpenAI-compatible servers report in-band failures as an `error` object,
        // with or without chunk identity fields; it is never chunk content.
        if let Some(error) = object.get("error").filter(|error| !error.is_null()) {
            let field = |name| error.get(name).and_then(serde_json::Value::as_str);
            let evidence = crate::protocol::openai::terminal::reported_failure(
                error.get("code").and_then(serde_json::Value::as_u64),
                field("code"),
                field("type"),
                field("message").or(error.as_str()),
            );
            return Err(CodecError::StreamFailure(evidence).into());
        }
        chat_envelope::headers(&payload, "chat.completion.chunk")?;
        Ok(self.codec.push(&payload)?)
    }
    pub fn finish(&mut self) -> Result<(), SseError> {
        if self.closed || self.rejected {
            return Err(SseError::Closed);
        }
        self.closed = true;
        let result = (|| {
            self.framing.finish_strict()?;
            if !self.done {
                return Err(SseError::Codec(CodecError::invalid()));
            }
            Ok(self.codec.finish()?)
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

pub struct ChatSseEncoder {
    notice: Option<crate::protocol::diagnostics::AdaptationNotice>,
    codec: EventEncoder,
    limits: SseLimits,
    options: StreamOptions,
    padding: Obfuscation,
    wire_bytes: usize,
    events: usize,
    padding_bytes: usize,
    done: bool,
    rejected: bool,
}
impl ChatSseEncoder {
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
    /// Enabled obfuscation requires a caller-supplied seed; no random/credential source is used here.
    pub fn new(
        metadata: ResponseMetadata,
        contract: TargetContract,
        limits: SseLimits,
        options: StreamOptions,
        padding: Obfuscation,
    ) -> Result<Self, SseError> {
        limits.validate()?;
        options.validate().map_err(CodecError::from)?;
        if options.obfuscation() != matches!(padding, Obfuscation::Seeded(_)) {
            return Err(SseError::Codec(CodecError::invalid()));
        }
        if metadata
            .created
            .as_ref()
            .and_then(serde_json::Number::as_u64)
            .is_none()
        {
            return Err(SseError::Codec(CodecError::invalid()));
        }
        Ok(Self {
            notice: None,
            codec: EventEncoder::new(GenerationApi::Chat, metadata)?.with_contract(contract),
            limits,
            options,
            padding,
            wire_bytes: 0,
            events: 0,
            padding_bytes: 0,
            done: false,
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
    pub fn encode(
        &mut self,
        event: &StreamEvent,
        source: &FidelityRecords,
    ) -> Result<Vec<Bytes>, SseError> {
        if self.rejected {
            return Err(SseError::Closed);
        }
        let result = (|| {
            if self.done {
                return Err(SseError::Closed);
            }
            let terminal = matches!(event, StreamEvent::Terminal { .. });
            // Pure codecs preserve unknown usage. Gateway delivery supplies its
            // explicit completed view; neither path may replace a real terminal.
            // https://developers.openai.com/api/reference/resources/chat/completions
            let values = self.codec.encode(event, source)?;
            let mut notice = if terminal { self.notice.take() } else { None };
            if let Some(notice) = &mut notice {
                notice.add_effects(self.codec.projection());
            }
            let final_json = values.iter().rposition(|value| {
                !(value.get("usage").is_some_and(|u| !u.is_null()) && !self.options.usage())
            });
            let mut frames = vec![];
            for (index, mut value) in values.into_iter().enumerate() {
                // Both standard empty-choice and repeated-finish adapters emit
                // usage in a separate terminal tail. Honor the delivery option.
                if value.get("usage").is_some_and(|v| !v.is_null()) && !self.options.usage() {
                    continue;
                }
                chat_envelope::headers(&value, "chat.completion.chunk")?;
                if let Obfuscation::Seeded(seed) = &self.padding {
                    pad_payload(
                        &mut value,
                        seed,
                        self.events + 1,
                        self.limits
                            .max_event_bytes
                            .saturating_sub(8)
                            .min(MAX_TOTAL_BYTES),
                        &mut self.padding_bytes,
                        self.limits.max_obfuscation_bytes,
                    )?;
                }
                if final_json == Some(index)
                    && let Some(notice) = &notice
                {
                    let reserve = if terminal {
                        b"data: [DONE]\n\n".len()
                    } else {
                        0
                    };
                    let available = self
                        .limits
                        .max_event_bytes
                        .min(
                            self.limits
                                .max_wire_bytes
                                .saturating_sub(self.wire_bytes)
                                .saturating_sub(reserve),
                        )
                        .saturating_sub(8);
                    notice.attach(&mut value, available, false);
                }
                // Charge all frame budgets before allocating the serialized bytes.
                let size = crate::semantic::value::json_size(
                    &value,
                    self.limits
                        .max_event_bytes
                        .saturating_sub(8)
                        .min(MAX_TOTAL_BYTES),
                )
                .map_err(|_| SseError::Limit)?;
                self.charge(size + 8)?;
                let bytes = Bytes::from(format!("data: {value}\n\n"));
                frames.push(bytes);
            }
            if terminal {
                let done = Bytes::from_static(b"data: [DONE]\n\n");
                self.charge(done.len())?;
                frames.push(done);
                self.done = true;
            }
            Ok(frames)
        })();
        if result.is_err() {
            self.rejected = true;
        }
        result
    }
    pub(crate) fn fail(&mut self, error: &serde_json::Value) -> Result<Bytes, SseError> {
        if self.rejected || self.done {
            return Err(SseError::Closed);
        }
        let result = (|| {
            let mut value = error.clone();
            if let Some(notice) = self.notice.take() {
                notice.attach(
                    &mut value,
                    self.limits
                        .max_event_bytes
                        .min(self.limits.max_wire_bytes.saturating_sub(self.wire_bytes))
                        .saturating_sub(8),
                    false,
                );
            }
            let size = crate::semantic::value::json_size(
                &value,
                self.limits.max_event_bytes.saturating_sub(8),
            )
            .map_err(|_| SseError::Limit)?;
            self.charge(size + 8)?;
            Ok(Bytes::from(format!("data: {value}\n\n")))
        })();
        self.rejected = true;
        result
    }
    fn charge(&mut self, frame_bytes: usize) -> Result<(), SseError> {
        self.events += 1;
        self.wire_bytes = self
            .wire_bytes
            .checked_add(frame_bytes)
            .ok_or(SseError::Limit)?;
        if self.events > self.limits.max_events
            || frame_bytes > self.limits.max_event_bytes
            || self.wire_bytes > self.limits.max_wire_bytes
        {
            return Err(SseError::Limit);
        }
        Ok(())
    }
    pub fn finish(&self) -> Result<(), SseError> {
        if self.rejected || !self.done {
            return Err(SseError::Closed);
        }
        Ok(self.codec.finish()?)
    }
}
