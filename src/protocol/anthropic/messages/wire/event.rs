//! One SSE payload: deltas and the events that carry them.
use crate::protocol::anthropic::messages::{
    shape::*,
    wire::{
        content::Block,
        envelope::{
            Message, StopReason, Usage, input_transformations, read_refusal, write_refusal,
        },
        request::Role,
    },
};
use crate::protocol::failure::CarrierReason;
use crate::{protocol::CodecError, semantic::value::Presence};
use serde_json::{Value, json};

#[derive(Clone, Eq, PartialEq)]
pub enum Delta {
    Text(String),
    Thinking(String),
    Signature(String),
    InputJson(String),
}
impl Delta {
    fn read(v: &Value) -> Result<Self, CodecError> {
        let kind = v
            .get("type")
            .and_then(Value::as_str)
            .ok_or(CodecError::invalid())?;
        let (key, constructor): (&str, fn(String) -> Self) = match kind {
            "text_delta" => ("text", Self::Text),
            "thinking_delta" => ("thinking", Self::Thinking),
            "signature_delta" => ("signature", Self::Signature),
            "input_json_delta" => ("partial_json", Self::InputJson),
            _ => {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
        };
        let o = object(v)?;
        Ok(constructor(string(required(
            o,
            match key {
                "text" => "text",
                "thinking" => "thinking",
                "signature" => "signature",
                _ => "partial_json",
            },
        )?)?))
    }
    fn bytes(&self) -> Result<usize, CodecError> {
        match self {
            Self::Text(s) | Self::Thinking(s) | Self::Signature(s) | Self::InputJson(s) => text(s),
        }
    }
    fn wire(&self) -> Value {
        match self {
            Self::Text(s) => json!({"type":"text_delta","text":s}),
            Self::Thinking(s) => json!({"type":"thinking_delta","thinking":s}),
            Self::Signature(s) => json!({"type":"signature_delta","signature":s}),
            Self::InputJson(s) => json!({"type":"input_json_delta","partial_json":s}),
        }
    }
}
#[derive(Clone, Eq, PartialEq)]
pub enum Event {
    MessageStart(Message),
    BlockStart {
        index: u64,
        block: Block,
    },
    BlockDelta {
        index: u64,
        delta: Delta,
    },
    BlockStop {
        index: u64,
    },
    MessageDelta {
        stop_reason: Presence<StopReason>,
        stop_sequence: Presence<String>,
        stop_details: Presence<crate::semantic::task::generation::RefusalDetails>,
        usage: Usage,
        input_transformations: bool,
    },
    MessageStop,
    Ping,
    Error {
        kind: String,
        message: String,
    },
}
impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AnthropicEvent([redacted])")
    }
}
impl Event {
    /// Event types with modeled content. Others are skipped by stream intake, as the
    /// Messages streaming guide asks clients to tolerate new event types.
    pub const KNOWN: &[&str] = &[
        "message_start",
        "content_block_start",
        "content_block_delta",
        "content_block_stop",
        "message_delta",
        "message_stop",
        "ping",
        "error",
    ];
    pub(in crate::protocol::anthropic::messages) fn read(v: &Value) -> Result<Self, CodecError> {
        let kind = v
            .get("type")
            .and_then(Value::as_str)
            .ok_or(CodecError::invalid())?;
        if !Self::KNOWN.contains(&kind) {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        let o = object(v)?;
        let e = match kind {
            "message_start" => Self::MessageStart(Message::read(required(o, "message")?, true)?),
            "content_block_start" => Self::BlockStart {
                index: uint(required(o, "index")?)?,
                block: Block::read(required(o, "content_block")?)?,
            },
            "content_block_delta" => Self::BlockDelta {
                index: uint(required(o, "index")?)?,
                delta: Delta::read(required(o, "delta")?)?,
            },
            "content_block_stop" => Self::BlockStop {
                index: uint(required(o, "index")?)?,
            },
            "message_delta" => {
                let d = object(required(o, "delta")?)?;
                Self::MessageDelta {
                    stop_reason: presence(d, "stop_reason", StopReason::read)?,
                    stop_sequence: presence(d, "stop_sequence", string)?,
                    stop_details: read_refusal(d)?,
                    usage: Usage::read(required(o, "usage")?)?,
                    input_transformations: input_transformations(o)?,
                }
            }
            "message_stop" => Self::MessageStop,
            "ping" => Self::Ping,
            "error" => {
                let d = object(required(o, "error")?)?;
                Self::Error {
                    kind: string(required(d, "type")?)?,
                    message: string(required(d, "message")?)?,
                }
            }
            _ => unreachable!("checked against KNOWN"),
        };
        e.validate()?;
        Ok(e)
    }
    pub fn validate(&self) -> Result<(), CodecError> {
        match self {
            Self::MessageStart(m) => m.validate(true)?,
            Self::BlockStart { index, block } => {
                Self::index(*index)?;
                if !block.permits(Role::Assistant) {
                    return Err(CodecError::invalid());
                }
                block.bytes(true)?;
            }
            Self::BlockDelta { index, delta } => {
                Self::index(*index)?;
                delta.bytes()?;
            }
            Self::BlockStop { index } => Self::index(*index)?,
            Self::MessageDelta {
                stop_reason,
                stop_sequence,
                stop_details,
                usage,
                ..
            } => {
                if let Some(details) = stop_details.value() {
                    details.validate()?;
                    if stop_reason
                        .value()
                        .is_some_and(|reason| *reason != StopReason::Refusal)
                    {
                        return Err(CodecError::invalid());
                    }
                }
                usage.validate()?;
                if !usage.cache_creation.is_absent() {
                    return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                }
                // The pinned MessageDeltaUsage has no service-tier carrier.
                // https://github.com/anthropics/anthropic-sdk-python/blob/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f/src/anthropic/types/message_delta_usage.py
                if !usage.service_class.is_absent() {
                    return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                }
                if stop_sequence.value().is_some()
                    != (*stop_reason == Presence::Value(StopReason::StopSequence))
                {
                    return Err(CodecError::invalid());
                }
                if let Presence::Value(s) = stop_sequence {
                    text(s)?;
                }
            }
            Self::Error { kind, message } => {
                text(kind)?;
                text(message)?;
            }
            Self::MessageStop | Self::Ping => {}
        }
        Ok(())
    }
    fn index(index: u64) -> Result<(), CodecError> {
        if index >= crate::semantic::task::generation::MAX_ITEMS as u64 {
            return Err(CodecError::Limit);
        }
        Ok(())
    }
    pub(in crate::protocol::anthropic::messages) fn wire(&self) -> Value {
        match self {
            Self::MessageStart(m) => json!({"type":"message_start","message":m.wire()}),
            Self::BlockStart { index, block } => {
                json!({"type":"content_block_start","index":index,"content_block":block.wire()})
            }
            Self::BlockDelta { index, delta } => {
                json!({"type":"content_block_delta","index":index,"delta":delta.wire()})
            }
            Self::BlockStop { index } => json!({"type":"content_block_stop","index":index}),
            Self::MessageDelta {
                stop_reason,
                stop_sequence,
                stop_details,
                usage,
                ..
            } => {
                let mut delta = json!({});
                let o = delta.as_object_mut().expect("object");
                put(o, "stop_reason", stop_reason, |s| json!(s.wire()));
                put(o, "stop_sequence", stop_sequence, |s| json!(s));
                write_refusal(o, stop_details);
                json!({"type":"message_delta","delta":delta,"usage":usage.wire()})
            }
            Self::MessageStop => json!({"type":"message_stop"}),
            Self::Ping => json!({"type":"ping"}),
            Self::Error { kind, message } => {
                json!({"type":"error","error":{"type":kind,"message":message}})
            }
        }
    }
}
