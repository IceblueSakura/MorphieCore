//! Bounded native aggregation. No semantic output or replay is complete before strict EOF.
use super::{Block, Delta, Event, Message, Profile};
use crate::{
    protocol::{
        CodecError,
        openai::sse::{SseError, SseLimits},
    },
    semantic::{
        task::generation::{MAX_ITEMS, MAX_TEXT_BYTES, MAX_TOTAL_BYTES, StructuredValue},
        value::{JsonLimits, parse_json},
    },
    transport::sse::{SseDecoder, SseEvent},
};
struct OpenBlock {
    block: Block,
    json: Option<String>,
    signature_started: bool,
}
pub struct MessageSseDecoder {
    framing: SseDecoder,
    limits: SseLimits,
    wire_bytes: usize,
    events: usize,
    state_bytes: usize,
    message: Option<Message>,
    open: Option<OpenBlock>,
    ending: bool,
    stopped: bool,
    closed: bool,
    rejected: bool,
}
fn invalid() -> SseError {
    CodecError::Invalid("Anthropic stream ordering or closure").into()
}
fn append(target: &mut String, value: &str) -> Result<(), SseError> {
    if target
        .len()
        .checked_add(value.len())
        .is_none_or(|n| n > MAX_TEXT_BYTES)
    {
        return Err(SseError::Limit);
    }
    target.push_str(value);
    Ok(())
}
impl MessageSseDecoder {
    pub fn new(status: u16, content_type: &str, limits: SseLimits) -> Result<Self, SseError> {
        limits.validate()?;
        crate::protocol::openai::sse::validate_http(status, content_type)?;
        Ok(Self {
            framing: SseDecoder::new(limits.max_event_bytes),
            limits,
            wire_bytes: 0,
            events: 0,
            state_bytes: 0,
            message: None,
            open: None,
            ending: false,
            stopped: false,
            closed: false,
            rejected: false,
        })
    }
    /// Consume at most one frame. The caller owns and revisits the unconsumed suffix.
    pub fn consume(&mut self, chunk: &[u8]) -> Result<usize, SseError> {
        if self.closed || self.rejected {
            return Err(SseError::Closed);
        }
        let result = (|| {
            let remaining = self.limits.max_wire_bytes.saturating_sub(self.wire_bytes);
            let available = chunk.len().min(remaining.saturating_add(1));
            let (frame, used) = self.framing.push_until_event(&chunk[..available])?;
            self.wire_bytes = self.wire_bytes.checked_add(used).ok_or(SseError::Limit)?;
            if self.wire_bytes > self.limits.max_wire_bytes {
                return Err(SseError::Limit);
            }
            if let Some(frame) = frame {
                self.frame(frame)?;
            }
            Ok(used)
        })();
        if result.is_err() {
            self.reject();
        }
        result
    }
    fn frame(&mut self, frame: SseEvent) -> Result<(), SseError> {
        if self.stopped {
            return Err(SseError::Closed);
        }
        self.events = self.events.checked_add(1).ok_or(SseError::Limit)?;
        if self.events > self.limits.max_events {
            return Err(SseError::Limit);
        }
        let event = Profile::AdaptiveTextTools.decode_event(
            frame.event().ok_or(SseError::EventType)?,
            frame.data().as_bytes(),
        )?;
        self.event(event)
    }
    fn charge(&mut self, bytes: usize) -> Result<(), SseError> {
        self.state_bytes = self.state_bytes.checked_add(bytes).ok_or(SseError::Limit)?;
        if self.state_bytes > MAX_TOTAL_BYTES {
            return Err(SseError::Limit);
        }
        Ok(())
    }
    fn event(&mut self, event: Event) -> Result<(), SseError> {
        match event {
            Event::Ping => {}
            Event::Error { .. } => return Err(CodecError::Invalid("Anthropic stream error").into()),
            Event::MessageStart(message) => {
                if self.message.is_some() {
                    return Err(invalid());
                }
                self.charge(message.id.len() + message.model.len())?;
                self.message = Some(message);
            }
            Event::BlockStart { index, block } => {
                let message = self.message.as_ref().ok_or_else(invalid)?;
                if self.ending || self.open.is_some() || index as usize != message.content.len() {
                    return Err(invalid());
                }
                if message.content.len() >= MAX_ITEMS {
                    return Err(SseError::Limit);
                }
                if matches!(&block,Block::ToolUse {input,..} if !input.value().as_object().is_some_and(|o|o.is_empty()))
                {
                    return Err(CodecError::Invalid("Anthropic opening tool input").into());
                }
                self.charge(block.bytes(true)?)?;
                let signature_started =
                    matches!(&block,Block::Thinking {signature,..} if !signature.is_empty());
                self.open = Some(OpenBlock {
                    block,
                    json: None,
                    signature_started,
                });
            }
            Event::BlockDelta { index, delta } => {
                if self.ending
                    || index as usize != self.message.as_ref().ok_or_else(invalid)?.content.len()
                {
                    return Err(invalid());
                }
                let bytes = match &delta {
                    Delta::Text(v)
                    | Delta::Thinking(v)
                    | Delta::Signature(v)
                    | Delta::InputJson(v) => v.len(),
                };
                self.charge(bytes)?;
                let open = self.open.as_mut().ok_or_else(invalid)?;
                match (&mut open.block, delta) {
                    (Block::Text(text), Delta::Text(value)) => append(text, &value)?,
                    (Block::Thinking { thinking, .. }, Delta::Thinking(value))
                        if !open.signature_started =>
                    {
                        append(thinking, &value)?
                    }
                    (Block::Thinking { signature, .. }, Delta::Signature(value)) => {
                        open.signature_started = true;
                        append(signature, &value)?;
                    }
                    (Block::ToolUse { .. }, Delta::InputJson(value)) => {
                        append(open.json.get_or_insert_with(String::new), &value)?
                    }
                    _ => return Err(invalid()),
                }
            }
            Event::BlockStop { index } => {
                let message = self.message.as_mut().ok_or_else(invalid)?;
                if index as usize != message.content.len() {
                    return Err(invalid());
                }
                let mut open = self.open.take().ok_or_else(invalid)?;
                if let Block::ToolUse { input, .. } = &mut open.block
                    && let Some(json) = open.json
                {
                    let value = parse_json(json.as_bytes(), JsonLimits::STRUCTURED)
                        .map_err(|_| CodecError::Invalid("Anthropic final tool JSON"))?;
                    if !value.is_object() {
                        return Err(CodecError::Invalid("Anthropic final tool object").into());
                    }
                    *input = StructuredValue::new(value).map_err(CodecError::from)?;
                }
                open.block.bytes(false)?;
                message.content.push(open.block);
            }
            Event::MessageDelta {
                stop_reason,
                stop_sequence,
                usage,
            } => {
                if self.open.is_some() {
                    return Err(invalid());
                }
                let message = self.message.as_mut().ok_or_else(invalid)?;
                self.ending = true;
                if !stop_reason.is_absent() {
                    if message.stop_reason.value().is_some() && message.stop_reason != stop_reason {
                        return Err(invalid());
                    }
                    message.stop_reason = stop_reason;
                }
                if !stop_sequence.is_absent() {
                    message.stop_sequence = stop_sequence;
                }
                if !usage.input_tokens.is_absent() {
                    message.usage.input_tokens = usage.input_tokens;
                }
                if !usage.cache_creation_input_tokens.is_absent() {
                    message.usage.cache_creation_input_tokens = usage.cache_creation_input_tokens;
                }
                if !usage.cache_read_input_tokens.is_absent() {
                    message.usage.cache_read_input_tokens = usage.cache_read_input_tokens;
                }
                // Native counts are cumulative snapshots, never deltas to sum.
                message.usage.output_tokens = usage.output_tokens;
            }
            Event::MessageStop => {
                if self.open.is_some() || !self.ending {
                    return Err(invalid());
                }
                self.message.as_ref().ok_or_else(invalid)?.validate(false)?;
                self.stopped = true;
            }
        }
        Ok(())
    }
    /// Strict transport closure, not message_stop alone, releases the final value.
    pub fn finish(&mut self) -> Result<Message, SseError> {
        if self.closed || self.rejected {
            return Err(SseError::Closed);
        }
        self.closed = true;
        let result = (|| {
            self.framing.finish_strict()?;
            if !self.stopped || self.open.is_some() {
                return Err(invalid());
            }
            let message = self.message.take().ok_or_else(invalid)?;
            message.validate(false)?;
            Ok(message)
        })();
        if result.is_err() {
            self.reject();
        }
        result
    }
    fn reject(&mut self) {
        self.rejected = true;
        self.message = None;
        self.open = None;
    }
}
