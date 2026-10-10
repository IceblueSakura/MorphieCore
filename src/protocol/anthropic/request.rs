//! Native request representation; validation never executes tools or rewrites schemas.
use super::{response::Block, shape::*};
use crate::{
    protocol::CodecError,
    semantic::{
        task::generation::{MAX_TOOLS, SchemaDocument},
        value::Presence,
    },
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Eq, PartialEq)]
pub enum TextContent {
    Text(String),
    Blocks(Vec<String>),
}
impl TextContent {
    pub(super) fn read(v: &Value) -> Result<Self, CodecError> {
        if v.is_string() {
            return Ok(Self::Text(string(v)?));
        }
        Ok(Self::Blocks(
            list(v)?
                .iter()
                .map(|v| {
                    let o = object(v, &["type", "text"])?;
                    if required(o, "type")?.as_str() != Some("text") {
                        return Err(CodecError::Unsupported("Anthropic text content".into()));
                    }
                    string(required(o, "text")?)
                })
                .collect::<Result<_, _>>()?,
        ))
    }
    pub(super) fn bytes(&self) -> Result<usize, CodecError> {
        let mut n = 0;
        match self {
            Self::Text(s) => charge(&mut n, text(s)?)?,
            Self::Blocks(a) => {
                count(a.len())?;
                for s in a {
                    charge(&mut n, text(s)?)?;
                }
            }
        }
        Ok(n)
    }
    pub(super) fn nodes(&self) -> usize {
        match self {
            Self::Text(_) => 1,
            Self::Blocks(a) => 1 + a.len() * 5,
        }
    }
    pub(super) fn wire(&self) -> Value {
        match self {
            Self::Text(s) => json!(s),
            Self::Blocks(a) => json!(
                a.iter()
                    .map(|s| json!({"type":"text","text":s}))
                    .collect::<Vec<_>>()
            ),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    User,
    Assistant,
}
impl Role {
    pub(super) fn read(v: &Value) -> Result<Self, CodecError> {
        match v.as_str() {
            Some("user") => Ok(Self::User),
            Some("assistant") => Ok(Self::Assistant),
            _ => Err(CodecError::Invalid("Anthropic role")),
        }
    }
    pub(super) fn wire(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}
#[derive(Clone, Eq, PartialEq)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<Block>),
}
#[derive(Clone, Eq, PartialEq)]
pub struct InputMessage {
    pub role: Role,
    pub content: MessageContent,
}
impl InputMessage {
    fn read(v: &Value) -> Result<Self, CodecError> {
        let o = object(v, &["role", "content"])?;
        let content = required(o, "content")?;
        Ok(Self {
            role: Role::read(required(o, "role")?)?,
            content: if content.is_string() {
                MessageContent::Text(string(content)?)
            } else {
                MessageContent::Blocks(
                    list(content)?
                        .iter()
                        .map(Block::read)
                        .collect::<Result<_, _>>()?,
                )
            },
        })
    }
    fn bytes(&self) -> Result<usize, CodecError> {
        let bytes = match &self.content {
            MessageContent::Text(s) => text(s)?,
            MessageContent::Blocks(a) => {
                count(a.len())?;
                let mut bytes = 0;
                for block in a {
                    if !block.permits(self.role) {
                        return Err(CodecError::Invalid("Anthropic block role"));
                    }
                    charge(&mut bytes, block.bytes(false)?)?;
                }
                bytes
            }
        };
        if bytes == 0 {
            return Err(CodecError::Invalid("Anthropic empty message"));
        }
        Ok(bytes)
    }
    fn wire(&self) -> Value {
        let content = match &self.content {
            MessageContent::Text(s) => json!(s),
            MessageContent::Blocks(a) => json!(a.iter().map(Block::wire).collect::<Vec<_>>()),
        };
        json!({"role":self.role.wire(),"content":content})
    }
}
#[derive(Clone, Eq, PartialEq)]
pub struct Tool {
    pub name: String,
    pub description: Presence<String>,
    pub input_schema: SchemaDocument,
    pub strict: Presence<bool>,
}
impl Tool {
    fn read(v: &Value) -> Result<Self, CodecError> {
        let o = object(v, &["name", "description", "input_schema", "strict"])?;
        Ok(Self {
            name: string(required(o, "name")?)?,
            description: presence(o, "description", string)?,
            input_schema: required(o, "input_schema")?.clone().into(),
            strict: presence(o, "strict", boolean)?,
        })
    }
    fn bytes(&self) -> Result<usize, CodecError> {
        name(&self.name, 128)?;
        nonnull(&self.description)?;
        nonnull(&self.strict)?;
        if self.strict == Presence::Value(true) {
            return Err(CodecError::Unsupported("Anthropic strict tools".into()));
        }
        if !self.input_schema.is_unversioned_local()
            || self.input_schema.root().get("type").and_then(Value::as_str) != Some("object")
        {
            return Err(CodecError::Unsupported("Anthropic object schema".into()));
        }
        let mut bytes = self.input_schema.validate()?;
        charge(&mut bytes, self.name.len())?;
        if let Presence::Value(s) = &self.description {
            charge(&mut bytes, text(s)?)?;
        }
        Ok(bytes)
    }
    fn wire(&self) -> Value {
        let mut v = json!({"name":self.name,"input_schema":self.input_schema.root()});
        let o = v.as_object_mut().expect("object");
        put(o, "description", &self.description, |s| json!(s));
        put(o, "strict", &self.strict, |b| json!(b));
        v
    }
}
#[derive(Clone, Eq, PartialEq)]
pub enum Selection {
    Auto,
    None,
    Any,
    Tool(String),
}
#[derive(Clone, Eq, PartialEq)]
pub struct ToolChoice {
    pub selection: Selection,
    pub disable_parallel_tool_use: Presence<bool>,
}
impl ToolChoice {
    fn read(v: &Value) -> Result<Self, CodecError> {
        let o = object(v, &["type", "name", "disable_parallel_tool_use"])?;
        let selection = match required(o, "type")?.as_str() {
            Some("tool") => Selection::Tool(string(required(o, "name")?)?),
            Some("auto") => Selection::Auto,
            Some("none") => Selection::None,
            Some("any") => Selection::Any,
            _ => return Err(CodecError::Unsupported("Anthropic tool choice".into())),
        };
        if !matches!(selection, Selection::Tool(_)) && o.contains_key("name") {
            return Err(CodecError::Invalid("Anthropic tool choice name"));
        }
        Ok(Self {
            selection,
            disable_parallel_tool_use: presence(o, "disable_parallel_tool_use", boolean)?,
        })
    }
    fn wire(&self) -> Value {
        let mut v = match &self.selection {
            Selection::Auto => json!({"type":"auto"}),
            Selection::None => json!({"type":"none"}),
            Selection::Any => json!({"type":"any"}),
            Selection::Tool(s) => json!({"type":"tool","name":s}),
        };
        put(
            v.as_object_mut().expect("object"),
            "disable_parallel_tool_use",
            &self.disable_parallel_tool_use,
            |b| json!(b),
        );
        v
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Display {
    Summarized,
    Omitted,
}
#[derive(Clone, Eq, PartialEq)]
pub struct Thinking {
    pub display: Presence<Display>,
}
impl Thinking {
    fn read(v: &Value) -> Result<Self, CodecError> {
        let o = object(v, &["type", "display"])?;
        if required(o, "type")?.as_str() != Some("adaptive") {
            return Err(CodecError::Unsupported("Anthropic thinking mode".into()));
        }
        Ok(Self {
            display: presence(o, "display", |v| match v.as_str() {
                Some("summarized") => Ok(Display::Summarized),
                Some("omitted") => Ok(Display::Omitted),
                _ => Err(CodecError::Unsupported("Anthropic display".into())),
            })?,
        })
    }
    fn wire(&self) -> Value {
        let mut v = json!({"type":"adaptive"});
        put(
            v.as_object_mut().expect("object"),
            "display",
            &self.display,
            |d| {
                json!(match d {
                    Display::Summarized => "summarized",
                    Display::Omitted => "omitted",
                })
            },
        );
        v
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Effort {
    Low,
    Medium,
    High,
}
impl Effort {
    fn read(v: &Value) -> Result<Self, CodecError> {
        let o = object(v, &["effort"])?;
        match required(o, "effort")?.as_str() {
            Some("low") => Ok(Self::Low),
            Some("medium") => Ok(Self::Medium),
            Some("high") => Ok(Self::High),
            _ => Err(CodecError::Unsupported("Anthropic effort".into())),
        }
    }
    fn wire(self) -> Value {
        json!({"effort":match self { Self::Low => "low", Self::Medium => "medium", Self::High => "high" }})
    }
}
#[derive(Clone, Eq, PartialEq)]
pub struct Request {
    pub model: String,
    pub max_tokens: u64,
    pub messages: Vec<InputMessage>,
    pub system: Presence<TextContent>,
    pub tools: Presence<Vec<Tool>>,
    pub tool_choice: Presence<ToolChoice>,
    pub thinking: Presence<Thinking>,
    pub output_config: Presence<Effort>,
    pub stream: Presence<bool>,
}
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AnthropicRequest([redacted])")
    }
}
impl Request {
    pub(super) fn read(v: &Value) -> Result<Self, CodecError> {
        let o = object(
            v,
            &[
                "model",
                "max_tokens",
                "messages",
                "system",
                "tools",
                "tool_choice",
                "thinking",
                "output_config",
                "stream",
            ],
        )?;
        let r = Self {
            model: string(required(o, "model")?)?,
            max_tokens: uint(required(o, "max_tokens")?)?,
            messages: list(required(o, "messages")?)?
                .iter()
                .map(InputMessage::read)
                .collect::<Result<_, _>>()?,
            system: presence(o, "system", TextContent::read)?,
            tools: presence(o, "tools", |v| {
                list(v)?.iter().map(Tool::read).collect::<Result<_, _>>()
            })?,
            tool_choice: presence(o, "tool_choice", ToolChoice::read)?,
            thinking: presence(o, "thinking", Thinking::read)?,
            output_config: presence(o, "output_config", Effort::read)?,
            stream: presence(o, "stream", boolean)?,
        };
        r.validate()?;
        Ok(r)
    }
    pub fn validate(&self) -> Result<(), CodecError> {
        label(&self.model)?;
        if self.max_tokens == 0 {
            return Err(CodecError::Invalid("Anthropic max_tokens"));
        }
        count(self.messages.len())?;
        if self.messages.first().map(|m| m.role) != Some(Role::User)
            || self.messages.last().map(|m| m.role) != Some(Role::User)
        {
            return Err(CodecError::Invalid("Anthropic user boundary"));
        }
        nonnull(&self.system)?;
        nonnull(&self.tools)?;
        nonnull(&self.tool_choice)?;
        nonnull(&self.thinking)?;
        nonnull(&self.output_config)?;
        nonnull(&self.stream)?;
        let mut bytes = self.model.len();
        // Conservative fixed-shape overhead bounds allocation before building wire Values.
        let mut nodes = 64;
        if let Presence::Value(s) = &self.system {
            charge(&mut bytes, s.bytes()?)?;
            node_charge(&mut nodes, s.nodes())?;
        }
        let mut names = BTreeSet::new();
        if let Presence::Value(tools) = &self.tools {
            if tools.len() > MAX_TOOLS {
                return Err(CodecError::Limit);
            }
            for tool in tools {
                charge(&mut bytes, tool.bytes()?)?;
                node_charge(&mut nodes, 16 + value_nodes(tool.input_schema.root())?)?;
                if !names.insert(tool.name.as_str()) {
                    return Err(CodecError::Invalid("Anthropic duplicate tool"));
                }
            }
        }
        if let Presence::Value(choice) = &self.tool_choice {
            nonnull(&choice.disable_parallel_tool_use)?;
            match &choice.selection {
                Selection::None if !choice.disable_parallel_tool_use.is_absent() => {
                    return Err(CodecError::Invalid("Anthropic none parallel control"));
                }
                Selection::Any if names.is_empty() => {
                    return Err(CodecError::Invalid("Anthropic required tools"));
                }
                Selection::Tool(s) => {
                    name(s, 128)?;
                    if !names.contains(s.as_str()) {
                        return Err(CodecError::Invalid("Anthropic selected tool"));
                    }
                }
                _ => {}
            }
        }
        let mut seen = BTreeSet::new();
        let mut pending = BTreeSet::new();
        let mut block_count = 0;
        for message in &self.messages {
            charge(&mut bytes, message.bytes()?)?;
            node_charge(&mut nodes, 8)?;
            let mut results = BTreeSet::new();
            let mut saw_text = false;
            if let MessageContent::Blocks(blocks) = &message.content {
                block_count += blocks.len();
                count(block_count)?;
                for block in blocks {
                    node_charge(&mut nodes, block.nodes()?)?;
                    match block {
                        Block::ToolUse { id, .. } => {
                            if !pending.is_empty() && message.role == Role::User {
                                return Err(CodecError::Invalid("Anthropic pending results"));
                            }
                            if !seen.insert(id.as_str()) {
                                return Err(CodecError::Invalid("Anthropic duplicate call"));
                            }
                        }
                        Block::ToolResult { tool_use_id, .. } => {
                            if saw_text
                                || !pending.contains(tool_use_id.as_str())
                                || !results.insert(tool_use_id.as_str())
                            {
                                return Err(CodecError::Invalid("Anthropic result reference"));
                            }
                        }
                        _ => {
                            saw_text = true;
                        }
                    }
                }
            }
            if !pending.is_empty() {
                if message.role != Role::User || results != pending {
                    return Err(CodecError::Invalid("Anthropic pending results"));
                }
                pending.clear();
            }
            if let MessageContent::Blocks(blocks) = &message.content {
                for block in blocks {
                    if let Block::ToolUse { id, .. } = block {
                        pending.insert(id.as_str());
                    }
                }
            }
        }
        if !pending.is_empty() {
            return Err(CodecError::Invalid("Anthropic pending results"));
        }
        Ok(())
    }
    pub(super) fn wire(&self) -> Value {
        let mut v = json!({"model":self.model,"max_tokens":self.max_tokens,"messages":self.messages.iter().map(InputMessage::wire).collect::<Vec<_>>()});
        let o = v.as_object_mut().expect("object");
        put(o, "system", &self.system, TextContent::wire);
        put(o, "tools", &self.tools, |a| {
            json!(a.iter().map(Tool::wire).collect::<Vec<_>>())
        });
        put(o, "tool_choice", &self.tool_choice, ToolChoice::wire);
        put(o, "thinking", &self.thinking, Thinking::wire);
        put(o, "output_config", &self.output_config, |e| e.wire());
        put(o, "stream", &self.stream, |b| json!(b));
        v
    }
}
