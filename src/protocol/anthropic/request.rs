//! Native request representation; validation never executes tools or rewrites schemas.
use super::{
    media::{MAX_IMAGES, Media, ResultBlock, ResultContent},
    response::Block,
    shape::*,
};
use crate::{
    protocol::CodecError,
    semantic::{
        task::generation::{MAX_TOOLS, PrefixCacheTtl, SchemaDocument},
        value::Presence,
    },
};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

/// A request-local owner, independent of the response block representation.
#[derive(Clone, Eq, PartialEq)]
pub struct CacheMarked<T> {
    pub content: T,
    pub cache: Option<PrefixCacheTtl>,
}
impl<T> CacheMarked<T> {
    pub(super) fn read(
        value: &Value,
        read: impl FnOnce(&Value) -> Result<T, CodecError>,
    ) -> Result<Self, CodecError> {
        let cache = presence(
            object(value)?,
            "cache_control",
            CacheControl::read_breakpoint,
        )?;
        nonnull(&cache)?;
        Ok(Self {
            content: read(value)?,
            cache: cache.value().copied(),
        })
    }
    pub(super) fn wire(&self, encode: impl FnOnce(&T) -> Value) -> Value {
        let mut value = encode(&self.content);
        if let Some(ttl) = self.cache {
            value["cache_control"] = CacheControl::breakpoint_wire(&ttl);
        }
        value
    }
}
#[derive(Clone, Eq, PartialEq)]
pub enum TextContent {
    Text(String),
    Blocks(Vec<CacheMarked<String>>),
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
                    CacheMarked::read(v, |v| {
                        let o = closed(v, &["type", "text", "citations", "cache_control"])?;
                        uncited(o)?;
                        if required(o, "type")?.as_str() != Some("text") {
                            return Err(CodecError::Unsupported("Anthropic text content".into()));
                        }
                        string(required(o, "text")?)
                    })
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
                    charge(
                        &mut n,
                        text(&s.content)? + usize::from(s.cache.is_some()) * 32,
                    )?;
                }
            }
        }
        Ok(n)
    }
    pub(super) fn nodes(&self) -> usize {
        match self {
            Self::Text(_) => 1,
            Self::Blocks(a) => 1 + a.len() * 10,
        }
    }
    pub(super) fn wire(&self) -> Value {
        match self {
            Self::Text(s) => json!(s),
            Self::Blocks(a) => json!(
                a.iter()
                    .map(|s| s.wire(|s| json!({"type":"text","text":s})))
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
    Blocks(Vec<CacheMarked<Block>>),
}
#[derive(Clone, Eq, PartialEq)]
pub struct InputMessage {
    pub role: Role,
    pub content: MessageContent,
}
impl InputMessage {
    fn read(v: &Value) -> Result<Self, CodecError> {
        let o = closed(v, &["role", "content"])?;
        let content = required(o, "content")?;
        Ok(Self {
            role: Role::read(required(o, "role")?)?,
            content: if content.is_string() {
                MessageContent::Text(string(content)?)
            } else {
                MessageContent::Blocks(
                    list(content)?
                        .iter()
                        .map(|v| CacheMarked::read(v, Block::read))
                        .collect::<Result<_, _>>()?,
                )
            },
        })
    }
    fn bytes(&self) -> Result<usize, CodecError> {
        let mut has_content = false;
        let bytes = match &self.content {
            MessageContent::Text(s) => {
                has_content = !s.is_empty();
                text(s)?
            }
            MessageContent::Blocks(a) => {
                count(a.len())?;
                let mut bytes = 0;
                for block in a {
                    if !block.content.permits(self.role)
                        || block.cache.is_some()
                            && matches!(
                                block.content,
                                Block::Thinking { .. } | Block::RedactedThinking(_)
                            )
                    {
                        return Err(CodecError::Invalid("Anthropic block role"));
                    }
                    let content_bytes = block.content.bytes(false)?;
                    has_content |= content_bytes > 0;
                    charge(
                        &mut bytes,
                        content_bytes + usize::from(block.cache.is_some()) * 32,
                    )?;
                }
                bytes
            }
        };
        if !has_content {
            return Err(CodecError::Invalid("Anthropic empty message"));
        }
        Ok(bytes)
    }
    fn wire(&self) -> Value {
        let content = match &self.content {
            MessageContent::Text(s) => json!(s),
            MessageContent::Blocks(a) => {
                json!(a.iter().map(|b| b.wire(Block::wire)).collect::<Vec<_>>())
            }
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
    pub cache_control: Presence<PrefixCacheTtl>,
}
impl Tool {
    pub(super) fn read(v: &Value) -> Result<Self, CodecError> {
        let o = closed(
            v,
            &[
                "name",
                "description",
                "input_schema",
                "strict",
                "cache_control",
            ],
        )?;
        Ok(Self {
            name: string(required(o, "name")?)?,
            description: presence(o, "description", string)?,
            input_schema: required(o, "input_schema")?.clone().into(),
            strict: presence(o, "strict", boolean)?,
            cache_control: presence(o, "cache_control", CacheControl::read_breakpoint)?,
        })
    }
    pub(super) fn bytes(&self) -> Result<usize, CodecError> {
        name(&self.name, 128)?;
        nonnull(&self.description)?;
        nonnull(&self.strict)?;
        nonnull(&self.cache_control)?;
        if self.input_schema.root().get("type").and_then(Value::as_str) != Some("object") {
            return Err(CodecError::Unsupported("Anthropic object schema".into()));
        }
        let mut bytes = self.input_schema.validate()?;
        if self.cache_control.value().is_some() {
            charge(&mut bytes, 32)?;
        }
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
        put(
            o,
            "cache_control",
            &self.cache_control,
            CacheControl::breakpoint_wire,
        );
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
    pub(super) fn read(v: &Value) -> Result<Self, CodecError> {
        let o = closed(v, &["type", "name", "disable_parallel_tool_use"])?;
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
impl Display {
    pub(super) fn read(v: &Value) -> Result<Self, CodecError> {
        match v.as_str() {
            Some("summarized") => Ok(Self::Summarized),
            Some("omitted") => Ok(Self::Omitted),
            _ => Err(CodecError::Invalid("Anthropic display")),
        }
    }
}
#[derive(Clone, Eq, PartialEq)]
pub enum Thinking {
    Disabled,
    Adaptive { display: Presence<Display> },
}
impl Thinking {
    fn read(v: &Value) -> Result<Self, CodecError> {
        let o = object(v)?;
        match required(o, "type")?.as_str() {
            Some("disabled") => {
                closed(v, &["type"])?;
                Ok(Self::Disabled)
            }
            Some("adaptive") => {
                let o = closed(v, &["type", "display"])?;
                let display = presence(o, "display", Display::read)?;
                nonnull(&display)?;
                Ok(Self::Adaptive { display })
            }
            _ => Err(CodecError::Unsupported("Anthropic thinking mode".into())),
        }
    }
    fn wire(&self) -> Value {
        let Self::Adaptive { display } = self else {
            return json!({"type":"disabled"});
        };
        let mut v = json!({"type":"adaptive"});
        put(
            v.as_object_mut().expect("object"),
            "display",
            display,
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
    XHigh,
    Max,
}
impl Effort {
    fn read(v: &Value) -> Result<Self, CodecError> {
        match v.as_str() {
            Some("low") => Ok(Self::Low),
            Some("medium") => Ok(Self::Medium),
            Some("high") => Ok(Self::High),
            Some("xhigh") => Ok(Self::XHigh),
            Some("max") => Ok(Self::Max),
            _ => Err(CodecError::Unsupported("Anthropic effort".into())),
        }
    }
    fn wire(self) -> Value {
        json!(match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
        })
    }
}
/// Effort and the output format are independent settings of one object.
/// The format is always a constrained JSON schema; it has no name or description.
/// https://platform.claude.com/docs/en/build-with-claude/structured-outputs
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputConfig {
    pub effort: Presence<Effort>,
    pub format: Presence<SchemaDocument>,
}
impl OutputConfig {
    pub(super) fn read(v: &Value) -> Result<Self, CodecError> {
        let o = closed(v, &["effort", "format"])?;
        Ok(Self {
            effort: presence(o, "effort", Effort::read)?,
            format: presence(o, "format", |v| {
                let o = closed(v, &["type", "schema"])?;
                if required(o, "type")?.as_str() != Some("json_schema") {
                    return Err(CodecError::Unsupported("Anthropic output format".into()));
                }
                let schema = SchemaDocument::from(required(o, "schema")?.clone());
                schema.validate()?;
                Ok(schema)
            })?,
        })
    }
    fn wire(&self) -> Value {
        let mut v = Map::new();
        put(&mut v, "effort", &self.effort, |e| e.wire());
        put(
            &mut v,
            "format",
            &self.format,
            |s| json!({"type":"json_schema","schema":s.root()}),
        );
        Value::Object(v)
    }
}
/// Top-level automatic caching: the API places and advances the breakpoint.
/// Source: https://platform.claude.com/docs/en/build-with-claude/prompt-caching
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheControl {
    Ephemeral,
    OneHour,
}
impl CacheControl {
    pub(super) fn read_breakpoint(v: &Value) -> Result<PrefixCacheTtl, CodecError> {
        let o = closed(v, &["type", "ttl"])?;
        if required(o, "type")?.as_str() != Some("ephemeral") {
            return Err(CodecError::Unsupported("Anthropic cache type".into()));
        }
        match o.get("ttl") {
            None => Ok(PrefixCacheTtl::FiveMinutes),
            Some(Value::String(ttl)) if ttl == "5m" => Ok(PrefixCacheTtl::FiveMinutes),
            Some(Value::String(ttl)) if ttl == "1h" => Ok(PrefixCacheTtl::OneHour),
            _ => Err(CodecError::Invalid("Anthropic cache TTL")),
        }
    }
    fn breakpoint_wire(ttl: &PrefixCacheTtl) -> Value {
        match ttl {
            PrefixCacheTtl::FiveMinutes => json!({"type":"ephemeral"}),
            PrefixCacheTtl::OneHour => json!({"type":"ephemeral","ttl":"1h"}),
        }
    }
    pub(super) fn read(v: &Value) -> Result<Self, CodecError> {
        Ok(Self::from_ttl(Self::read_breakpoint(v)?))
    }
    pub(super) fn from_ttl(ttl: PrefixCacheTtl) -> Self {
        match ttl {
            PrefixCacheTtl::FiveMinutes => Self::Ephemeral,
            PrefixCacheTtl::OneHour => Self::OneHour,
        }
    }
    pub(super) fn ttl(self) -> PrefixCacheTtl {
        match self {
            Self::Ephemeral => PrefixCacheTtl::FiveMinutes,
            Self::OneHour => PrefixCacheTtl::OneHour,
        }
    }
    fn wire(self) -> Value {
        Self::breakpoint_wire(&self.ttl())
    }
}
/// Empty text cannot own an explicit or automatic cache breakpoint.
/// https://platform.claude.com/docs/en/build-with-claude/prompt-caching#what-cannot-be-cached
pub(super) fn text_cache_eligible(
    text: &str,
    cache: Option<PrefixCacheTtl>,
) -> Result<bool, CodecError> {
    if text.is_empty() && cache.is_some() {
        return Err(CodecError::Invalid("Anthropic empty text cache owner"));
    }
    Ok(!text.is_empty())
}
/// Messages admits four ordered prefix breakpoints; longer TTLs must come first.
/// https://platform.claude.com/docs/en/build-with-claude/prompt-caching
pub(super) fn validate_cache_breakpoints(
    ttls: impl IntoIterator<Item = PrefixCacheTtl>,
) -> Result<(), CodecError> {
    let ttls = ttls.into_iter();
    let mut short = false;
    for (index, ttl) in ttls.enumerate() {
        if index >= 4 {
            return Err(CodecError::Limit);
        }
        match ttl {
            PrefixCacheTtl::FiveMinutes => short = true,
            PrefixCacheTtl::OneHour if short => {
                return Err(CodecError::Invalid("Anthropic cache TTL order"));
            }
            PrefixCacheTtl::OneHour => {}
        }
    }
    Ok(())
}
pub(super) fn validate_automatic_cache(
    breakpoints: &mut Vec<PrefixCacheTtl>,
    last_cache: Option<PrefixCacheTtl>,
    automatic: Option<PrefixCacheTtl>,
) -> Result<(), CodecError> {
    if let Some(ttl) = automatic {
        // The API rejects four explicit markers with automatic caching enabled,
        // independently of its same-TTL placement no-op.
        // https://platform.claude.com/docs/en/build-with-claude/prompt-caching#automatic-caching
        if breakpoints.len() >= 4 {
            return Err(CodecError::Limit);
        }
        match last_cache {
            Some(last) if last != ttl => {
                return Err(CodecError::Invalid(
                    "Anthropic automatic cache TTL conflict",
                ));
            }
            Some(_) => {}
            None => breakpoints.push(ttl),
        }
    }
    validate_cache_breakpoints(breakpoints.iter().copied())
}
#[derive(Clone, Eq, PartialEq)]
pub struct Request {
    pub model: String,
    pub max_tokens: u64,
    pub stop_sequences: Vec<String>,
    pub messages: Vec<InputMessage>,
    pub system: Presence<TextContent>,
    pub tools: Presence<Vec<Tool>>,
    pub tool_choice: Presence<ToolChoice>,
    pub thinking: Presence<Thinking>,
    pub output_config: Presence<OutputConfig>,
    pub cache_control: Presence<CacheControl>,
    pub stream: Presence<bool>,
}
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AnthropicRequest([redacted])")
    }
}
impl Request {
    pub(super) fn read(v: &Value) -> Result<Self, CodecError> {
        let o = closed(
            v,
            &[
                "model",
                "max_tokens",
                "stop_sequences",
                "messages",
                "system",
                "tools",
                "tool_choice",
                "thinking",
                "output_config",
                "cache_control",
                "stream",
            ],
        )?;
        let r = Self {
            model: string(required(o, "model")?)?,
            max_tokens: uint(required(o, "max_tokens")?)?,
            stop_sequences: crate::protocol::generation_controls::stops(
                o.get("stop_sequences"),
                false,
                crate::semantic::task::generation::MAX_ITEMS,
            )?
            .iter()
            .map(|s| s.as_str().into())
            .collect(),
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
            output_config: presence(o, "output_config", OutputConfig::read)?,
            cache_control: presence(o, "cache_control", CacheControl::read)?,
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
        if let Presence::Value(Thinking::Adaptive { display }) = &self.thinking {
            nonnull(display)?;
        }
        nonnull(&self.output_config)?;
        nonnull(&self.cache_control)?;
        nonnull(&self.stream)?;
        let mut breakpoints = self
            .tools
            .value()
            .into_iter()
            .flatten()
            .filter_map(|tool| tool.cache_control.value().copied())
            .collect::<Vec<_>>();
        let mut last_cache = self
            .tools
            .value()
            .and_then(|tools| tools.last())
            .and_then(|tool| tool.cache_control.value().copied());
        match self.system.value() {
            Some(TextContent::Text(s)) if !s.is_empty() => last_cache = None,
            Some(TextContent::Blocks(blocks)) => {
                for block in blocks {
                    if text_cache_eligible(&block.content, block.cache)? {
                        last_cache = block.cache;
                    }
                    breakpoints.extend(block.cache);
                }
            }
            _ => {}
        }
        for message in &self.messages {
            match &message.content {
                MessageContent::Text(s) => {
                    if text_cache_eligible(s, None)? {
                        last_cache = None;
                    }
                }
                MessageContent::Blocks(blocks) => {
                    for block in blocks {
                        if let Block::ToolResult {
                            content: ResultContent::Blocks(parts),
                            ..
                        } = &block.content
                        {
                            for part in parts {
                                if let ResultBlock::Text(s) = &part.content {
                                    text_cache_eligible(s, part.cache)?;
                                }
                                breakpoints.extend(part.cache);
                            }
                        }
                        let eligible = match &block.content {
                            Block::Text(s) => text_cache_eligible(s, block.cache)?,
                            Block::Thinking { .. } | Block::RedactedThinking(_) => false,
                            _ => true,
                        };
                        if eligible {
                            last_cache = block.cache;
                        }
                        breakpoints.extend(block.cache);
                    }
                }
            }
        }
        validate_automatic_cache(
            &mut breakpoints,
            last_cache,
            self.cache_control.value().map(|c| c.ttl()),
        )?;
        let mut bytes = self.model.len();
        count(self.stop_sequences.len())?;
        for stop in &self.stop_sequences {
            if stop.is_empty() {
                return Err(CodecError::Invalid("Anthropic stop sequence"));
            }
            charge(&mut bytes, text(stop)?)?;
        }
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
                node_charge(&mut nodes, 24 + value_nodes(tool.input_schema.root())?)?;
                if !names.insert(tool.name.as_str()) {
                    return Err(CodecError::Invalid("Anthropic duplicate tool"));
                }
            }
        }
        if let Presence::Value(config) = &self.output_config {
            nonnull(&config.effort)?;
            nonnull(&config.format)?;
            if let Presence::Value(schema) = &config.format {
                charge(&mut bytes, schema.validate()?)?;
                node_charge(&mut nodes, value_nodes(schema.root())?)?;
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
        let mut image_count = 0;
        for message in &self.messages {
            charge(&mut bytes, message.bytes()?)?;
            node_charge(&mut nodes, 8)?;
            let mut results = BTreeSet::new();
            let mut saw_text = false;
            if let MessageContent::Blocks(blocks) = &message.content {
                block_count += blocks.len();
                count(block_count)?;
                for block in blocks {
                    let block = &block.content;
                    image_count += match block {
                        Block::Media(Media::Image { .. }) => 1,
                        Block::ToolResult {
                            content: ResultContent::Blocks(parts),
                            ..
                        } => parts
                            .iter()
                            .filter(|p| {
                                matches!(p.content, ResultBlock::Media(Media::Image { .. }))
                            })
                            .count(),
                        _ => 0,
                    };
                    if image_count > MAX_IMAGES {
                        return Err(CodecError::Limit);
                    }
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
                    if let Block::ToolUse { id, .. } = &block.content {
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
        if !self.stop_sequences.is_empty() {
            o.insert("stop_sequences".into(), json!(self.stop_sequences));
        }
        put(o, "system", &self.system, TextContent::wire);
        put(o, "tools", &self.tools, |a| {
            json!(a.iter().map(Tool::wire).collect::<Vec<_>>())
        });
        put(o, "tool_choice", &self.tool_choice, ToolChoice::wire);
        put(o, "thinking", &self.thinking, Thinking::wire);
        put(o, "output_config", &self.output_config, OutputConfig::wire);
        put(o, "cache_control", &self.cache_control, |c| c.wire());
        put(o, "stream", &self.stream, |b| json!(b));
        v
    }
}
