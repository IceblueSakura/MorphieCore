//! Text, reasoning, tool and media blocks shared by requests, responses and events.
//! No stream reducer or IR mapping lives here.
use crate::protocol::anthropic::messages::{shape::*, wire::request::Role};
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
    /// Arrives whole in `content_block_start`; there is no delta type.
    RedactedThinking(String),
    ToolUse {
        id: String,
        name: String,
        input: StructuredValue,
        caller: Presence<Caller>,
    },
    /// Native SSE aggregation only. JSON and request history have no carrier.
    PartialToolUse {
        id: String,
        name: String,
        input: String,
        caller: Presence<Caller>,
    },
    ToolResult {
        tool_use_id: String,
        content: crate::protocol::anthropic::messages::ResultContent,
        is_error: Presence<bool>,
    },
    Media(crate::protocol::anthropic::messages::Media),
    /// Request-only system blocks that change tool availability from this position.
    /// https://platform.claude.com/docs/en/build-with-claude/mid-conversation-system-messages
    ToolAddition(super::ToolTarget),
    ToolRemoval(String),
}
impl std::fmt::Debug for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AnthropicBlock([redacted])")
    }
}
impl Block {
    pub(in crate::protocol::anthropic::messages) fn read(v: &Value) -> Result<Self, CodecError> {
        let kind = v
            .get("type")
            .and_then(Value::as_str)
            .ok_or(CodecError::Invalid("Anthropic block type"))?;
        Ok(match kind {
            "text" => {
                let o = object(v)?;
                uncited(o)?;
                Self::Text(string(required(o, "text")?)?)
            }
            "thinking" => {
                let o = object(v)?;
                Self::Thinking {
                    thinking: string(required(o, "thinking")?)?,
                    signature: string(required(o, "signature")?)?,
                }
            }
            "redacted_thinking" => Self::RedactedThinking(string(required(object(v)?, "data")?)?),
            "tool_use" => {
                let o = object(v)?;
                reject_toolset(o)?;
                let input = required(o, "input")?;
                if !input.is_object() {
                    return Err(CodecError::Invalid("Anthropic tool input object"));
                }
                Self::ToolUse {
                    id: string(required(o, "id")?)?,
                    name: string(required(o, "name")?)?,
                    input: StructuredValue::new(input.clone())?,
                    caller: presence(o, "caller", |v| {
                        let o = object(v)?;
                        if required(o, "type")?.as_str() != Some("direct") {
                            return Err(CodecError::Unsupported("Anthropic caller".into()));
                        }
                        Ok(Caller::Direct)
                    })?,
                }
            }
            "tool_result" => {
                let o = object(v)?;
                reject_toolset(o)?;
                let is_error = presence(o, "is_error", boolean)?;
                nonnull(&is_error)?;
                Self::ToolResult {
                    tool_use_id: string(required(o, "tool_use_id")?)?,
                    // Optional in the pinned ToolResultBlockParam; absence is no content.
                    content: match o.get("content") {
                        None => crate::protocol::anthropic::messages::ResultContent::Blocks(vec![]),
                        Some(v) => crate::protocol::anthropic::messages::ResultContent::read(v)?,
                    },
                    is_error,
                }
            }
            "image" | "document" => {
                Self::Media(crate::protocol::anthropic::messages::Media::read(kind, v)?)
            }
            "tool_addition" => Self::ToolAddition(super::ToolTarget::read(required(
                closed(v, &["type", "tool", "cache_control"])?,
                "tool",
            )?)?),
            "tool_removal" => {
                let tool = required(closed(v, &["type", "tool", "cache_control"])?, "tool")?;
                match super::ToolTarget::read(tool)? {
                    super::ToolTarget::Reference(name) => Self::ToolRemoval(name),
                    super::ToolTarget::Definition(_) => {
                        return Err(CodecError::Invalid("Anthropic tool removal"));
                    }
                }
            }
            _ => return Err(CodecError::Unsupported("Anthropic block type".into())),
        })
    }
    pub(in crate::protocol::anthropic::messages) fn permits(&self, role: Role) -> bool {
        matches!(
            (self, role),
            (Self::Text(_), _)
                | (Self::ToolResult { .. } | Self::Media(_), Role::User)
                | (Self::ToolAddition(_) | Self::ToolRemoval(_), Role::System)
                | (
                    Self::Thinking { .. } | Self::RedactedThinking(_) | Self::ToolUse { .. },
                    Role::Assistant
                )
        )
    }
    pub(in crate::protocol::anthropic::messages) fn bytes(
        &self,
        opening: bool,
    ) -> Result<usize, CodecError> {
        let mut bytes = 0;
        match self {
            Self::PartialToolUse {
                id,
                name: tool,
                input,
                ..
            } => {
                name(id, 64)?;
                name(tool, 128)?;
                charge(&mut bytes, id.len() + tool.len() + text(input)?)?;
            }
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
            Self::RedactedThinking(data) => {
                if data.is_empty() {
                    return Err(CodecError::Invalid("Anthropic redacted thinking"));
                }
                charge(&mut bytes, text(data)?)?;
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
                    crate::semantic::value::json_size(
                        input.value(),
                        crate::semantic::task::generation::MAX_TEXT_BYTES,
                    )
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
            Self::Media(media) => charge(&mut bytes, media.bytes()?)?,
            Self::ToolAddition(target) => charge(&mut bytes, target.bytes()?)?,
            Self::ToolRemoval(tool) => {
                name(tool, 128)?;
                charge(&mut bytes, tool.len())?;
            }
        }
        Ok(bytes)
    }
    pub(in crate::protocol::anthropic::messages) fn nodes(&self) -> Result<usize, CodecError> {
        Ok(16
            + match self {
                Self::ToolUse { input, .. } => value_nodes(input.value())?,
                Self::ToolResult { content, .. } => content.nodes(),
                Self::ToolAddition(super::ToolTarget::Definition(tool)) => tool.nodes()?,
                _ => 0,
            })
    }
    pub(in crate::protocol::anthropic::messages) fn wire(&self) -> Value {
        match self {
            Self::Text(s) => json!({"type":"text","text":s}),
            Self::PartialToolUse { .. } => unreachable!("partial tool input has no JSON carrier"),
            Self::Thinking {
                thinking,
                signature,
            } => json!({"type":"thinking","thinking":thinking,"signature":signature}),
            Self::RedactedThinking(data) => json!({"type":"redacted_thinking","data":data}),
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
            Self::Media(media) => media.wire(),
            Self::ToolAddition(target) => json!({"type":"tool_addition","tool":target.wire()}),
            Self::ToolRemoval(name) => {
                json!({"type":"tool_removal","tool":{"type":"tool_reference","name":name}})
            }
            Self::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                let mut v = json!({
                    "type": "tool_result",
                    "tool_use_id": tool_use_id,
                    "content": content.wire(),
                });
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

fn reject_toolset(o: &serde_json::Map<String, Value>) -> Result<(), CodecError> {
    // Dispatch identity belongs to this block, never to its argument properties.
    // Neither null nor an empty namespace has a proven omission equivalence.
    if !presence(o, "toolset_name", string)?.is_absent() {
        return Err(CodecError::Unsupported("Anthropic toolset identity".into()));
    }
    Ok(())
}
