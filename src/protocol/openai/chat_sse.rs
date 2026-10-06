//! Chat data-only SSE over shared bounded framing, strict JSON and Generation event codecs.
//! EOF is not DONE. Body I/O, cancellation and downstream commit belong to the caller.
use super::{
    CodecError, DecodedResponse, Profile, ResponseMetadata, chat_envelope,
    events::{EventDecoder, EventEncoder},
    sse::{Obfuscation, SseError, SseLimits, charge_padding, pad_payload, validate_http},
};
use crate::semantic::context::StreamOptions;
use crate::{
    lowering::generation::GenerationRepresentationContract,
    protocol::fidelity::FidelityRecords,
    semantic::task::generation::{MAX_TOTAL_BYTES, StreamEvent},
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
    pub fn new(status: u16, content_type: &str, limits: SseLimits) -> Result<Self, SseError> {
        limits.validate()?;
        validate_http(status, content_type)?;
        Ok(Self {
            framing: SseDecoder::new(limits.max_event_bytes),
            codec: EventDecoder::new(Profile::Chat),
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
        if codec.profile() != Profile::Chat {
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
        let mut payload = super::json::decode(frame.data().as_bytes())?;
        let object = payload
            .as_object_mut()
            .ok_or(CodecError::Invalid("Chat SSE payload"))?;
        if let Some(padding) = object.remove("obfuscation") {
            let n = padding
                .as_str()
                .ok_or(CodecError::Invalid("obfuscation"))?
                .len();
            charge_padding(
                &mut self.padding_bytes,
                n,
                self.limits.max_obfuscation_bytes,
            )?;
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
                return Err(SseError::Codec(CodecError::Invalid("EOF before Chat DONE")));
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
    codec: EventEncoder,
    limits: SseLimits,
    options: StreamOptions,
    padding: Obfuscation,
    wire_bytes: usize,
    events: usize,
    padding_bytes: usize,
    usage_seen: bool,
    done: bool,
    rejected: bool,
}
impl ChatSseEncoder {
    /// Enabled obfuscation requires a caller-supplied seed; no random/credential source is used here.
    pub fn new(
        metadata: ResponseMetadata,
        contract: GenerationRepresentationContract,
        limits: SseLimits,
        options: StreamOptions,
        padding: Obfuscation,
    ) -> Result<Self, SseError> {
        limits.validate()?;
        options.validate().map_err(CodecError::from)?;
        if options.obfuscation() != matches!(padding, Obfuscation::Seeded(_)) {
            return Err(SseError::Codec(CodecError::Invalid("Chat padding policy")));
        }
        if metadata.created.as_u64().is_none() {
            return Err(SseError::Codec(CodecError::Invalid("Chat created")));
        }
        Ok(Self {
            codec: EventEncoder::new(Profile::Chat, metadata)?.with_contract(contract),
            limits,
            options,
            padding,
            wire_bytes: 0,
            events: 0,
            padding_bytes: 0,
            usage_seen: false,
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
            if terminal && self.options.usage() && !self.usage_seen {
                return Err(SseError::Codec(CodecError::Invalid(
                    "missing requested Chat usage",
                )));
            }
            let values = self.codec.encode(event, source)?;
            if matches!(event, StreamEvent::Usage(_)) {
                self.usage_seen = true;
            }
            let mut frames = vec![];
            for mut value in values {
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
