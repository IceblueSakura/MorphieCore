//! Retained response envelope: stop reason, usage accounting and the message itself.
use crate::protocol::anthropic::messages::{
    shape::*,
    wire::{content::Block, request::Role},
};
use crate::{protocol::CodecError, semantic::value::Presence};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
    ModelContextWindowExceeded,
    /// HTTP 200 with `stop_details`; the details are not carried.
    Refusal,
}
impl StopReason {
    /// `pause_turn` needs server tools, which no binding enables.
    pub(in crate::protocol::anthropic::messages) fn read(v: &Value) -> Result<Self, CodecError> {
        match v.as_str() {
            Some("end_turn") => Ok(Self::EndTurn),
            Some("tool_use") => Ok(Self::ToolUse),
            Some("max_tokens") => Ok(Self::MaxTokens),
            Some("stop_sequence") => Ok(Self::StopSequence),
            Some("model_context_window_exceeded") => Ok(Self::ModelContextWindowExceeded),
            Some("refusal") => Ok(Self::Refusal),
            _ => Err(CodecError::Unsupported("Anthropic stop reason".into())),
        }
    }
    pub(in crate::protocol::anthropic::messages) fn wire(self) -> &'static str {
        match self {
            Self::EndTurn => "end_turn",
            Self::ToolUse => "tool_use",
            Self::MaxTokens => "max_tokens",
            Self::StopSequence => "stop_sequence",
            Self::ModelContextWindowExceeded => "model_context_window_exceeded",
            Self::Refusal => "refusal",
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheCreation {
    pub ephemeral_5m_input_tokens: u64,
    pub ephemeral_1h_input_tokens: u64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerToolUsage {
    pub web_search_requests: u64,
    pub web_fetch_requests: u64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Usage {
    pub service_class: Presence<crate::semantic::context::ServiceClass>,
    pub input_tokens: Presence<u64>,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: Presence<u64>,
    pub cache_read_input_tokens: Presence<u64>,
    pub cache_creation: Presence<CacheCreation>,
    pub server_tool_use: Presence<ServerToolUsage>,
    /// `output_tokens_details.thinking_tokens`, a subset of `output_tokens`.
    /// https://github.com/anthropics/anthropic-sdk-python/blob/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f/src/anthropic/types/output_tokens_details.py
    pub thinking_tokens: Presence<u64>,
}
impl Usage {
    pub(in crate::protocol::anthropic::messages) fn read(v: &Value) -> Result<Self, CodecError> {
        let o = object(v)?;
        let usage = Self {
            service_class: crate::protocol::anthropic::messages::service::read_class(o)?,
            input_tokens: presence(o, "input_tokens", uint)?,
            output_tokens: uint(required(o, "output_tokens")?)?,
            cache_creation_input_tokens: presence(o, "cache_creation_input_tokens", uint)?,
            cache_read_input_tokens: presence(o, "cache_read_input_tokens", uint)?,
            cache_creation: presence(o, "cache_creation", |value| {
                let fields = object(value)?;
                Ok(CacheCreation {
                    ephemeral_5m_input_tokens: uint(required(
                        fields,
                        "ephemeral_5m_input_tokens",
                    )?)?,
                    ephemeral_1h_input_tokens: uint(required(
                        fields,
                        "ephemeral_1h_input_tokens",
                    )?)?,
                })
            })?,
            server_tool_use: presence(o, "server_tool_use", |value| {
                let fields = object(value)?;
                Ok(ServerToolUsage {
                    web_search_requests: uint(required(fields, "web_search_requests")?)?,
                    web_fetch_requests: uint(required(fields, "web_fetch_requests")?)?,
                })
            })?,
            thinking_tokens: presence(o, "output_tokens_details", |v| {
                uint(required(object(v)?, "thinking_tokens")?)
            })?,
        };
        usage.validate()?;
        Ok(usage)
    }
    pub(in crate::protocol::anthropic::messages) fn validate(&self) -> Result<(), CodecError> {
        crate::protocol::anthropic::messages::accounting::report(
            self,
            crate::semantic::task::generation::UsageStage::Final,
        )
        .validate()?;
        Ok(())
    }
    pub(in crate::protocol::anthropic::messages) fn wire(&self) -> Value {
        let mut v = json!({"output_tokens":self.output_tokens});
        let o = v.as_object_mut().expect("object");
        put(o, "input_tokens", &self.input_tokens, |n| json!(n));
        crate::protocol::anthropic::messages::service::write_class(o, &self.service_class)
            .expect("validated reported service");
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
        put(
            o,
            "output_tokens_details",
            &self.thinking_tokens,
            |n| json!({ "thinking_tokens": n }),
        );
        put(o, "cache_creation", &self.cache_creation, |counts| {
            json!({
                "ephemeral_5m_input_tokens": counts.ephemeral_5m_input_tokens,
                "ephemeral_1h_input_tokens": counts.ephemeral_1h_input_tokens,
            })
        });
        put(o, "server_tool_use", &self.server_tool_use, |counts| {
            json!({
                "web_search_requests": counts.web_search_requests,
                "web_fetch_requests": counts.web_fetch_requests,
            })
        });
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
    /// The upstream reported changes to its request input. Entries index the upstream
    /// request, so they are never forwarded.
    pub input_transformations: bool,
}
/// Validates `input_transformations` entries; unknown entry types are ignored, as the
/// API documents. True when any entry is reported.
/// https://github.com/anthropics/anthropic-sdk-python/blob/bcd55fe1d3f0f1240765f8cef04295b67337f55a/src/anthropic/types/beta/beta_message.py
pub(crate) fn input_transformations(
    o: &serde_json::Map<String, Value>,
) -> Result<bool, CodecError> {
    let Some(list) = o.get("input_transformations").filter(|v| !v.is_null()) else {
        return Ok(false);
    };
    let list = list
        .as_array()
        .ok_or(CodecError::Invalid("Anthropic input_transformations"))?;
    count(list.len())?;
    for entry in list {
        let entry = object(entry)?;
        let kind = string(required(entry, "type")?)?;
        if matches!(
            kind.as_str(),
            "thinking_dropped" | "thinking_mismatch_allowed"
        ) {
            string(required(entry, "path")?)?;
        }
    }
    Ok(!list.is_empty())
}
impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AnthropicMessage([redacted])")
    }
}
impl Message {
    pub(in crate::protocol::anthropic::messages) fn read(
        v: &Value,
        opening: bool,
    ) -> Result<Self, CodecError> {
        let o = object(v)?;
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
            input_transformations: input_transformations(o)?,
        };
        m.validate(opening)?;
        Ok(m)
    }
    pub fn validate(&self, opening: bool) -> Result<(), CodecError> {
        label(&self.id)?;
        self.usage.validate()?;
        if let Some(class) = self.usage.service_class.value() {
            crate::protocol::anthropic::messages::service::class_wire(*class)?;
        }
        label(&self.model)?;
        count(self.content.len())?;
        let matched = self.stop_reason == Presence::Value(StopReason::StopSequence);
        if !matches!(self.usage.input_tokens, Presence::Value(_))
            || self.stop_sequence.value().is_some() != matched
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
        if let Presence::Value(s) = &self.stop_sequence {
            charge(&mut bytes, text(s)?)?;
        }
        let mut nodes = 64;
        let mut calls = std::collections::BTreeSet::new();
        for block in &self.content {
            let partial = matches!(block, Block::PartialToolUse { .. });
            if partial && self.stop_reason != Presence::Value(StopReason::MaxTokens) {
                return Err(CodecError::Invalid("Anthropic partial tool terminal"));
            }
            if !block.permits(Role::Assistant) && !partial {
                return Err(CodecError::Invalid("Anthropic output block"));
            }
            charge(&mut bytes, block.bytes(false)?)?;
            node_charge(&mut nodes, block.nodes()?)?;
            if let Block::ToolUse { id, .. } | Block::PartialToolUse { id, .. } = block
                && !calls.insert(id)
            {
                return Err(CodecError::Invalid("Anthropic duplicate call"));
            }
        }
        if !opening
            && (self.stop_reason == Presence::Value(StopReason::ToolUse) && calls.is_empty()
                || matched && !calls.is_empty()
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
            storage: None,
            id: Some(self.id.clone()),
            model: self.model.clone(),
            created: None,
            context: crate::semantic::context::ResponseContext {
                reports: crate::semantic::context::ResponseReports::single(
                    crate::semantic::context::ResponseReport {
                        service_class: self.usage.service_class.clone(),
                        ..Default::default()
                    },
                ),
                ..Default::default()
            },
            instruction_fidelity: Default::default(),
        };
        metadata.validate()?;
        Ok(metadata)
    }
    pub(in crate::protocol::anthropic::messages) fn wire(&self) -> Value {
        let mut v = json!({"id":self.id,"type":"message","role":"assistant","model":self.model,
            "content":self.content.iter().map(Block::wire).collect::<Vec<_>>(),"usage":self.usage.wire()});
        let o = v.as_object_mut().expect("object");
        put(o, "stop_reason", &self.stop_reason, |s| json!(s.wire()));
        put(o, "stop_sequence", &self.stop_sequence, |s| json!(s));
        v
    }
}
