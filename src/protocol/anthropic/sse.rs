//! Shared native wire reducer; only strict EOF exposes the complete aggregated message.
use super::{Block, Delta, Event, Message, Profile};
use crate::{
    protocol::{
        CodecError,
        stream::{SseError, SseLimits},
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
/// Status of each documented error type
/// (https://docs.anthropic.com/en/api/errors), for classification only.
pub(super) fn error_status(kind: &str) -> u16 {
    match kind {
        "invalid_request_error" => 400,
        "authentication_error" => 401,
        "billing_error" => 402,
        "permission_error" => 403,
        "not_found_error" => 404,
        "request_too_large" => 413,
        "rate_limit_error" => 429,
        "overloaded_error" => 529,
        _ => 500,
    }
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
        crate::protocol::stream::validate_http(status, content_type)?;
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
        self.consume_event(chunk).map(|(used, _)| used)
    }
    /// Expose only an event accepted by the native ordering/closure reducer.
    pub(super) fn consume_event(
        &mut self,
        chunk: &[u8],
    ) -> Result<(usize, Option<Event>), SseError> {
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
            let event = frame.map(|frame| self.frame(frame)).transpose()?.flatten();
            Ok((used, event))
        })();
        if result.is_err() {
            self.reject();
        }
        result
    }
    fn frame(&mut self, frame: SseEvent) -> Result<Option<Event>, SseError> {
        if self.stopped {
            return Err(SseError::Closed);
        }
        self.events = self.events.checked_add(1).ok_or(SseError::Limit)?;
        if self.events > self.limits.max_events {
            return Err(SseError::Limit);
        }
        let name = frame.event().ok_or(SseError::EventType)?;
        if !Event::KNOWN.contains(&name) {
            return Ok(None);
        }
        let event = Profile::AdaptiveTextTools.decode_event(name, frame.data().as_bytes())?;
        self.event(event.clone())?;
        Ok(Some(event))
    }
    pub(super) fn message(&self) -> Option<&Message> {
        self.message.as_ref()
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
            Event::Error { kind, .. } => {
                return Err(CodecError::StreamFailure(error_status(&kind)).into());
            }
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
                if let Block::ToolUse { input, .. } = &block
                    && !input.value().as_object().is_some_and(|o| o.is_empty())
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
                    if message.stop_sequence.value().is_some()
                        && message.stop_sequence != stop_sequence
                    {
                        return Err(invalid());
                    }
                    message.stop_sequence = stop_sequence;
                }
                // Optional null counters mean no update, not erasure of prior reports.
                if usage.input_tokens.value().is_some() {
                    message.usage.input_tokens = usage.input_tokens;
                }
                if usage.cache_creation_input_tokens.value().is_some() {
                    message.usage.cache_creation_input_tokens = usage.cache_creation_input_tokens;
                }
                if usage.cache_read_input_tokens.value().is_some() {
                    message.usage.cache_read_input_tokens = usage.cache_read_input_tokens;
                }
                if usage.thinking_tokens.value().is_some() {
                    message.usage.thinking_tokens = usage.thinking_tokens;
                }
                if usage.server_tool_use.value().is_some() {
                    message.usage.server_tool_use = usage.server_tool_use;
                }
                // Native counts are cumulative snapshots, never deltas to sum.
                message.usage.output_tokens = usage.output_tokens;
                message.usage.validate()?;
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
