//! Standard request context and delivery intent, independent of wire dialects.
use super::value::Presence;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ContextError {
    #[error("invalid context: {0}")]
    Invalid(&'static str),
    #[error("context budget exceeded")]
    Limit,
    #[error("stateful execution is not admitted")]
    Stateful,
    #[error(transparent)]
    Generation(#[from] super::task::generation::GenerationError),
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceTier {
    Auto,
    Default,
    Flex,
    Fast,
    Priority,
    Scale,
    Ultrafast,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheRetention {
    InMemory,
    #[serde(rename = "24h")]
    Day,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheMode {
    Implicit,
    Explicit,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CacheTtl {
    #[serde(rename = "30m")]
    ThirtyMinutes,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheOptions {
    pub mode: CacheMode,
    pub ttl: CacheTtl,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub comparison_response_id: Presence<String>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub prewarm: Presence<bool>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheHints {
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub prompt_cache_key: Presence<String>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub prompt_cache_retention: Presence<CacheRetention>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub prompt_cache_options: Presence<CacheOptions>,
}
/// Client identity and safety metadata are not cache keys or session identities.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientIdentityHints {
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub safety_identifier: Presence<String>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub user: Presence<String>,
}
impl ClientIdentityHints {
    pub fn validate(&self) -> Result<(), ContextError> {
        for (value, max) in [(&self.safety_identifier, 64), (&self.user, 256)] {
            if value
                .value()
                .is_some_and(|s| s.chars().count() > max || s.len() > max * 4)
            {
                return Err(ContextError::Limit);
            }
        }
        Ok(())
    }
}
impl CacheHints {
    pub fn validate(&self) -> Result<(), ContextError> {
        for (value, max) in [(&self.prompt_cache_key, 256)] {
            if value
                .value()
                .is_some_and(|s| s.chars().count() > max || s.len() > max * 4)
            {
                return Err(ContextError::Limit);
            }
        }
        if let Some(options) = self.prompt_cache_options.value() {
            if options.prewarm == Presence::Null {
                return Err(ContextError::Invalid("prewarm"));
            }
            if options
                .comparison_response_id
                .value()
                .is_some_and(|s| s.len() > 256)
            {
                return Err(ContextError::Limit);
            }
        }
        super::value::json_size(self, super::task::generation::MAX_TEXT_BYTES)
            .map_err(|_| ContextError::Limit)?;
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExecutionHints {
    #[serde(flatten)]
    pub cache: CacheHints,
    #[serde(flatten)]
    pub identity: ClientIdentityHints,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub metadata: Presence<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub service_tier: Presence<ServiceTier>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub max_tool_calls: Presence<u64>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub store: Presence<bool>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub background: Presence<bool>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub previous_response_id: Presence<()>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub conversation: Presence<()>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub prompt: Presence<()>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub moderation: Presence<()>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub context_management: Presence<Vec<()>>,
}
impl ExecutionHints {
    pub fn validate(&self) -> Result<(), ContextError> {
        self.cache.validate()?;
        self.identity.validate()?;
        if self.store == Presence::Value(true)
            || self.background == Presence::Value(true)
            || self
                .context_management
                .value()
                .is_some_and(|v| !v.is_empty())
        {
            return Err(ContextError::Stateful);
        }
        if self.metadata.value().is_some_and(|m| {
            m.len() > 16
                || m.iter()
                    .any(|(k, v)| k.chars().count() > 64 || v.chars().count() > 512)
        }) {
            return Err(ContextError::Limit);
        }
        if self.max_tool_calls == Presence::Value(0) {
            return Err(ContextError::Invalid("max_tool_calls"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CacheDiagnostics {
    CacheHit,
    ComparisonResponseNotFound,
    Unavailable,
    CacheMiss {
        reason: CacheMissReason,
        cache_missed_tokens: u64,
        #[serde(default, skip_serializing_if = "Presence::is_absent")]
        comparison_reusable_tokens: Presence<u64>,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheMissReason {
    ModelChanged,
    PromptCacheKeyChanged,
    ToolsChanged,
    TextFormatChanged,
    ReasoningEffortChanged,
    VerbosityChanged,
    ContextCompacted,
    InputChanged,
    ServiceTierChanged,
}

/// Reported facts are distinct from request hints. Wire fidelity lives outside
/// this value and cannot restore deleted instructions or settings.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResponseContext {
    pub settings: Option<super::task::generation::GenerationSettings>,
    pub instruction_messages: Option<
        Vec<(
            super::task::generation::ItemId,
            super::task::generation::Item,
        )>,
    >,
    pub execution: ExecutionHints,
    pub completed_at: Presence<serde_json::Number>,
    pub cache_diagnostics: Presence<CacheDiagnostics>,
    pub system_fingerprint: Presence<String>,
}
impl ResponseContext {
    pub fn validate(&self) -> Result<(), ContextError> {
        use super::task::generation::{GenerationControls, GenerationRequest, Item};
        self.execution.validate()?;
        if self
            .system_fingerprint
            .value()
            .is_some_and(|s| s.is_empty() || s.len() > 256)
        {
            return Err(ContextError::Invalid("system fingerprint"));
        }
        if let Some(settings) = &self.settings {
            settings.validate()?;
        }
        if let Some(items) = &self.instruction_messages {
            if self
                .settings
                .as_ref()
                .is_some_and(|s| !s.instructions.is_absent())
                || items
                    .iter()
                    .any(|(_, item)| !matches!(item, Item::Instruction(_)))
            {
                return Err(ContextError::Invalid("instruction echoes"));
            }
            if !items.is_empty() {
                GenerationRequest::new(items.clone(), GenerationControls::default())?;
            }
        }
        if self
            .completed_at
            .value()
            .is_some_and(|n| !super::value::valid_timestamp(n))
        {
            return Err(ContextError::Invalid("completion timestamp"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StreamOptions {
    pub include_usage: Presence<bool>,
    pub include_obfuscation: Presence<bool>,
}
impl StreamOptions {
    pub fn usage(&self) -> bool {
        self.include_usage == Presence::Value(true)
    }
    pub fn obfuscation(&self) -> bool {
        self.include_obfuscation != Presence::Value(false)
    }
    pub fn validate(&self) -> Result<(), ContextError> {
        if self.include_usage == Presence::Null || self.include_obfuscation == Presence::Null {
            return Err(ContextError::Invalid("stream options"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeliveryIntent {
    pub stream: Presence<bool>,
    pub options: Presence<StreamOptions>,
}
impl DeliveryIntent {
    pub fn streaming(&self) -> bool {
        self.stream == Presence::Value(true)
    }
    pub fn validate(&self) -> Result<(), ContextError> {
        if let Some(options) = self.options.value() {
            if !self.streaming() {
                return Err(ContextError::Invalid("stream options without streaming"));
            }
            options.validate()?;
        }
        Ok(())
    }
}
