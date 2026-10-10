//! Native blocks, envelopes and individual events. No stream reducer or IR mapping lives here.
use super::{
    request::{Role, TextContent},
    shape::*,
};
use crate::{
    protocol::CodecError,
    semantic::{task::generation::StructuredValue, value::Presence},
};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Caller {
    Direct,
}
#[derive(Clone, Eq, PartialEq)]
pub enum Block {
    Text(String),
    Thinking {
        thinking: String,
        signature: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: StructuredValue,
        caller: Presence<Caller>,
    },
    ToolResult {
        tool_use_id: String,
        content: TextContent,
        is_error: Presence<bool>,
    },
}
impl std::fmt::Debug for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AnthropicBlock([redacted])")
    }
}
impl Block {
    pub(super) fn read(v: &Value) -> Result<Self, CodecError> {
        let kind = v
            .get("type")
            .and_then(Value::as_str)
            .ok_or(CodecError::Invalid("Anthropic block type"))?;
        Ok(match kind {
            "text" => {
                let o = object(v, &["type", "text"])?;
                Self::Text(string(required(o, "text")?)?)
            }
            "thinking" => {
                let o = object(v, &["type", "thinking", "signature"])?;
                Self::Thinking {
                    thinking: string(required(o, "thinking")?)?,
                    signature: string(required(o, "signature")?)?,
                }
            }
            "tool_use" => {
                let o = object(v, &["type", "id", "name", "input", "caller"])?;
                let input = required(o, "input")?;
                if !input.is_object() {
                    return Err(CodecError::Invalid("Anthropic tool input object"));
                }
                Self::ToolUse {
                    id: string(required(o, "id")?)?,
                    name: string(required(o, "name")?)?,
                    input: StructuredValue::new(input.clone())?,
                    caller: presence(o, "caller", |v| {
                        let o = object(v, &["type"])?;
                        if required(o, "type")?.as_str() != Some("direct") {
                            return Err(CodecError::Unsupported("Anthropic caller".into()));
                        }
                        Ok(Caller::Direct)
                    })?,
                }
            }
            "tool_result" => {
                let o = object(v, &["type", "tool_use_id", "content", "is_error"])?;
                Self::ToolResult {
                    tool_use_id: string(required(o, "tool_use_id")?)?,
                    content: TextContent::read(required(o, "content")?)?,
                    is_error: presence(o, "is_error", boolean)?,
                }
            }
            _ => return Err(CodecError::Unsupported("Anthropic block type".into())),
        })
    }
    pub(super) fn permits(&self, role: Role) -> bool {
        matches!(
            (self, role),
            (Self::Text(_), _)
                | (Self::ToolResult { .. }, Role::User)
                | (
                    Self::Thinking { .. } | Self::ToolUse { .. },
                    Role::Assistant
                )
        )
    }
    pub(super) fn bytes(&self, opening: bool) -> Result<usize, CodecError> {
        let mut bytes = 0;
        match self {
            Self::Text(s) => charge(&mut bytes, text(s)?)?,
            Self::Thinking {
                thinking,
                signature,
            } => {
                if !opening && signature.is_empty() {
                    return Err(CodecError::Invalid("Anthropic thinking signature"));
                }
                charge(&mut bytes, text(thinking)?)?;
                charge(&mut bytes, text(signature)?)?;
            }
            Self::ToolUse {
                id,
                name: tool,
                input,
                ..
            } => {
                name(id, 64)?;
                name(tool, 128)?;
                if !input.value().is_object() {
                    return Err(CodecError::Invalid("Anthropic tool input object"));
                }
                charge(&mut bytes, id.len() + tool.len())?;
                charge(
                    &mut bytes,
                    crate::semantic::value::json_size(input.value(), 1 << 20)
                        .map_err(|_| CodecError::Limit)?,
                )?;
            }
            Self::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                name(tool_use_id, 64)?;
                nonnull(is_error)?;
                charge(&mut bytes, tool_use_id.len())?;
                charge(&mut bytes, content.bytes()?)?;
            }
        }
        Ok(bytes)
    }
    pub(super) fn nodes(&self) -> Result<usize, CodecError> {
        Ok(16
            + match self {
                Self::ToolUse { input, .. } => value_nodes(input.value())?,
                Self::ToolResult { content, .. } => content.nodes(),
                _ => 0,
            })
    }
    pub(super) fn wire(&self) -> Value {
        match self {
            Self::Text(s) => json!({"type":"text","text":s}),
            Self::Thinking {
                thinking,
                signature,
            } => json!({"type":"thinking","thinking":thinking,"signature":signature}),
            Self::ToolUse {
                id,
                name,
                input,
                caller,
            } => {
                let mut v = json!({"type":"tool_use","id":id,"name":name,"input":input.value()});
                put(
                    v.as_object_mut().expect("object"),
                    "caller",
                    caller,
                    |_| json!({"type":"direct"}),
                );
                v
            }
            Self::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                let mut v = json!({"type":"tool_result","tool_use_id":tool_use_id,"content":content.wire()});
                put(
                    v.as_object_mut().expect("object"),
                    "is_error",
                    is_error,
                    |b| json!(b),
                );
                v
            }
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
}
impl StopReason {
    fn read(v: &Value) -> Result<Self, CodecError> {
        match v.as_str() {
            Some("end_turn") => Ok(Self::EndTurn),
            Some("tool_use") => Ok(Self::ToolUse),
            Some("max_tokens") => Ok(Self::MaxTokens),
            _ => Err(CodecError::Unsupported("Anthropic stop reason".into())),
        }
    }
    fn wire(self) -> &'static str {
        match self {
            Self::EndTurn => "end_turn",
            Self::ToolUse => "tool_use",
            Self::MaxTokens => "max_tokens",
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Usage {
    pub input_tokens: Presence<u64>,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: Presence<u64>,
    pub cache_read_input_tokens: Presence<u64>,
}
impl Usage {
    fn read(v: &Value) -> Result<Self, CodecError> {
        let o = object(
            v,
            &[
                "input_tokens",
                "output_tokens",
                "cache_creation_input_tokens",
                "cache_read_input_tokens",
            ],
        )?;
        Ok(Self {
            input_tokens: presence(o, "input_tokens", uint)?,
            output_tokens: uint(required(o, "output_tokens")?)?,
            cache_creation_input_tokens: presence(o, "cache_creation_input_tokens", uint)?,
            cache_read_input_tokens: presence(o, "cache_read_input_tokens", uint)?,
        })
    }
    fn wire(&self) -> Value {
        let mut v = json!({"output_tokens":self.output_tokens});
        let o = v.as_object_mut().expect("object");
        put(o, "input_tokens", &self.input_tokens, |n| json!(n));
        put(
            o,
            "cache_creation_input_tokens",
            &self.cache_creation_input_tokens,
            |n| json!(n),
        );
        put(
            o,
            "cache_read_input_tokens",
            &self.cache_read_input_tokens,
            |n| json!(n),
        );
        v
    }
}
#[derive(Clone, Eq, PartialEq)]
pub struct Message {
    pub id: String,
    pub model: String,
    pub content: Vec<Block>,
    pub stop_reason: Presence<StopReason>,
    pub stop_sequence: Presence<String>,
    pub usage: Usage,
}
impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AnthropicMessage([redacted])")
    }
}
impl Message {
    pub(super) fn read(v: &Value, opening: bool) -> Result<Self, CodecError> {
        let o = object(
            v,
            &[
                "id",
                "type",
                "role",
                "model",
                "content",
                "stop_reason",
                "stop_sequence",
                "usage",
            ],
        )?;
        if required(o, "type")?.as_str() != Some("message")
            || required(o, "role")?.as_str() != Some("assistant")
        {
            return Err(CodecError::Invalid("Anthropic message envelope"));
        }
        let m = Self {
            id: string(required(o, "id")?)?,
            model: string(required(o, "model")?)?,
            content: list(required(o, "content")?)?
                .iter()
                .map(Block::read)
                .collect::<Result<_, _>>()?,
            stop_reason: presence(o, "stop_reason", StopReason::read)?,
            stop_sequence: presence(o, "stop_sequence", string)?,
            usage: Usage::read(required(o, "usage")?)?,
        };
        m.validate(opening)?;
        Ok(m)
    }
    pub fn validate(&self, opening: bool) -> Result<(), CodecError> {
        label(&self.id)?;
        label(&self.model)?;
        count(self.content.len())?;
        if !matches!(self.usage.input_tokens, Presence::Value(_))
            || self.stop_sequence.value().is_some()
        {
            return Err(CodecError::Invalid("Anthropic message reports"));
        }
        if opening {
            if self.stop_reason != Presence::Null || !self.content.is_empty() {
                return Err(CodecError::Invalid("Anthropic message start"));
            }
        } else if !matches!(self.stop_reason, Presence::Value(_)) {
            return Err(CodecError::Invalid("Anthropic final stop"));
        }
        let mut bytes = self.id.len() + self.model.len();
        let mut nodes = 64;
        let mut calls = std::collections::BTreeSet::new();
        for block in &self.content {
            if !block.permits(Role::Assistant) {
                return Err(CodecError::Invalid("Anthropic output block"));
            }
            charge(&mut bytes, block.bytes(false)?)?;
            node_charge(&mut nodes, block.nodes()?)?;
            if let Block::ToolUse { id, .. } = block
                && !calls.insert(id)
            {
                return Err(CodecError::Invalid("Anthropic duplicate call"));
            }
        }
        if !opening
            && (self.stop_reason == Presence::Value(StopReason::ToolUse) && calls.is_empty()
                || self.stop_reason == Presence::Value(StopReason::EndTurn) && !calls.is_empty())
        {
            return Err(CodecError::Invalid("Anthropic stop/call combination"));
        }
        Ok(())
    }
    /// Derived envelope facts only; no timestamp, instruction echo or replay proof is invented.
    pub fn reported_metadata(&self) -> Result<crate::protocol::ResponseMetadata, CodecError> {
        self.validate(false)?;
        let metadata = crate::protocol::ResponseMetadata {
            id: self.id.clone(),
            model: self.model.clone(),
            created: None,
            context: Default::default(),
            instruction_fidelity: Default::default(),
        };
        metadata.validate()?;
        Ok(metadata)
    }
    pub(super) fn wire(&self) -> Value {
        let mut v = json!({"id":self.id,"type":"message","role":"assistant","model":self.model,
            "content":self.content.iter().map(Block::wire).collect::<Vec<_>>(),"usage":self.usage.wire()});
        let o = v.as_object_mut().expect("object");
        put(o, "stop_reason", &self.stop_reason, |s| json!(s.wire()));
        put(o, "stop_sequence", &self.stop_sequence, |s| json!(s));
        v
    }
}
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
            .ok_or(CodecError::Invalid("Anthropic delta type"))?;
        let (key, constructor): (&str, fn(String) -> Self) = match kind {
            "text_delta" => ("text", Self::Text),
            "thinking_delta" => ("thinking", Self::Thinking),
            "signature_delta" => ("signature", Self::Signature),
            "input_json_delta" => ("partial_json", Self::InputJson),
            _ => return Err(CodecError::Unsupported("Anthropic delta type".into())),
        };
        let o = object(v, &["type", key])?;
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
        usage: Usage,
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
    pub(super) fn read(v: &Value) -> Result<Self, CodecError> {
        let kind = v
            .get("type")
            .and_then(Value::as_str)
            .ok_or(CodecError::Invalid("Anthropic event type"))?;
        let allowed: &[&str] = match kind {
            "message_start" => &["type", "message"],
            "content_block_start" => &["type", "index", "content_block"],
            "content_block_delta" => &["type", "index", "delta"],
            "content_block_stop" => &["type", "index"],
            "message_delta" => &["type", "delta", "usage"],
            "message_stop" | "ping" => &["type"],
            "error" => &["type", "error"],
            _ => return Err(CodecError::Unsupported("Anthropic event type".into())),
        };
        let o = object(v, allowed)?;
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
                let d = object(required(o, "delta")?, &["stop_reason", "stop_sequence"])?;
                Self::MessageDelta {
                    stop_reason: presence(d, "stop_reason", StopReason::read)?,
                    stop_sequence: presence(d, "stop_sequence", string)?,
                    usage: Usage::read(required(o, "usage")?)?,
                }
            }
            "message_stop" => Self::MessageStop,
            "ping" => Self::Ping,
            "error" => {
                let d = object(required(o, "error")?, &["type", "message"])?;
                Self::Error {
                    kind: string(required(d, "type")?)?,
                    message: string(required(d, "message")?)?,
                }
            }
            _ => unreachable!("closed event grammar"),
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
                    return Err(CodecError::Invalid("Anthropic start block"));
                }
                block.bytes(true)?;
            }
            Self::BlockDelta { index, delta } => {
                Self::index(*index)?;
                delta.bytes()?;
            }
            Self::BlockStop { index } => Self::index(*index)?,
            Self::MessageDelta { stop_sequence, .. } => {
                if stop_sequence.value().is_some() {
                    return Err(CodecError::Invalid("Anthropic stop_sequence"));
                }
            }
            Self::Error { kind, message } => {
                if ![
                    "invalid_request_error",
                    "authentication_error",
                    "permission_error",
                    "not_found_error",
                    "rate_limit_error",
                    "api_error",
                    "overloaded_error",
                ]
                .contains(&kind.as_str())
                {
                    return Err(CodecError::Unsupported("Anthropic error type".into()));
                }
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
    pub(super) fn wire(&self) -> Value {
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
                usage,
            } => {
                let mut delta = json!({});
                let o = delta.as_object_mut().expect("object");
                put(o, "stop_reason", stop_reason, |s| json!(s.wire()));
                put(o, "stop_sequence", stop_sequence, |s| json!(s));
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
