//! OpenAI context DTOs. Semantic values never serialize themselves as wire fields.
use crate::semantic::{context as semantic, value::Presence};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn map_presence<'a, T, U>(value: &'a Presence<T>, map: impl FnOnce(&'a T) -> U) -> Presence<U> {
    match value {
        Presence::Absent => Presence::Absent,
        Presence::Null => Presence::Null,
        Presence::Value(value) => Presence::Value(map(value)),
    }
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

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheOptions {
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub mode: Presence<CacheMode>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub ttl: Presence<CacheTtl>,
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

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientIdentityHints {
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub safety_identifier: Presence<String>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub user: Presence<String>,
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

impl From<&semantic::CacheRetention> for CacheRetention {
    fn from(value: &semantic::CacheRetention) -> Self {
        match value {
            semantic::CacheRetention::InMemory => Self::InMemory,
            semantic::CacheRetention::Day => Self::Day,
        }
    }
}

impl From<&semantic::CacheMode> for CacheMode {
    fn from(value: &semantic::CacheMode) -> Self {
        match value {
            semantic::CacheMode::Implicit => Self::Implicit,
            semantic::CacheMode::Explicit => Self::Explicit,
        }
    }
}

impl From<&semantic::CacheTtl> for CacheTtl {
    fn from(value: &semantic::CacheTtl) -> Self {
        match value {
            semantic::CacheTtl::ThirtyMinutes => Self::ThirtyMinutes,
        }
    }
}

impl From<&semantic::CacheMissReason> for CacheMissReason {
    fn from(value: &semantic::CacheMissReason) -> Self {
        match value {
            semantic::CacheMissReason::ModelChanged => Self::ModelChanged,
            semantic::CacheMissReason::PromptCacheKeyChanged => Self::PromptCacheKeyChanged,
            semantic::CacheMissReason::ToolsChanged => Self::ToolsChanged,
            semantic::CacheMissReason::TextFormatChanged => Self::TextFormatChanged,
            semantic::CacheMissReason::ReasoningEffortChanged => Self::ReasoningEffortChanged,
            semantic::CacheMissReason::VerbosityChanged => Self::VerbosityChanged,
            semantic::CacheMissReason::ContextCompacted => Self::ContextCompacted,
            semantic::CacheMissReason::InputChanged => Self::InputChanged,
            semantic::CacheMissReason::ServiceTierChanged => Self::ServiceTierChanged,
        }
    }
}

impl From<&semantic::CacheOptions> for CacheOptions {
    fn from(value: &semantic::CacheOptions) -> Self {
        Self {
            mode: map_presence(&value.mode, Into::into),
            ttl: map_presence(&value.ttl, Into::into),
            comparison_response_id: value.comparison_response_id.clone(),
            prewarm: value.prewarm.clone(),
        }
    }
}

impl From<&semantic::CacheHints> for CacheHints {
    fn from(value: &semantic::CacheHints) -> Self {
        Self {
            prompt_cache_key: value.prompt_cache_key.clone(),
            prompt_cache_retention: map_presence(&value.prompt_cache_retention, Into::into),
            prompt_cache_options: map_presence(&value.prompt_cache_options, Into::into),
        }
    }
}

impl From<&semantic::ClientIdentityHints> for ClientIdentityHints {
    fn from(value: &semantic::ClientIdentityHints) -> Self {
        Self {
            safety_identifier: value.safety_identifier.clone(),
            user: value.user.clone(),
        }
    }
}

impl From<&semantic::ExecutionHints> for ExecutionHints {
    fn from(value: &semantic::ExecutionHints) -> Self {
        Self {
            cache: (&value.cache).into(),
            identity: (&value.identity).into(),
            metadata: value.metadata.clone(),
            max_tool_calls: value.max_tool_calls.clone(),
            store: value.store.clone(),
            background: value.background.clone(),
            previous_response_id: value.previous_response_id.clone(),
            conversation: value.conversation.clone(),
            prompt: value.prompt.clone(),
            moderation: value.moderation.clone(),
            context_management: value.context_management.clone(),
        }
    }
}

impl From<&semantic::CacheDiagnostics> for CacheDiagnostics {
    fn from(value: &semantic::CacheDiagnostics) -> Self {
        match value {
            semantic::CacheDiagnostics::CacheHit => Self::CacheHit,
            semantic::CacheDiagnostics::ComparisonResponseNotFound => {
                Self::ComparisonResponseNotFound
            }
            semantic::CacheDiagnostics::Unavailable => Self::Unavailable,
            semantic::CacheDiagnostics::CacheMiss {
                reason,
                cache_missed_tokens,
                comparison_reusable_tokens,
            } => Self::CacheMiss {
                reason: reason.into(),
                cache_missed_tokens: *cache_missed_tokens,
                comparison_reusable_tokens: comparison_reusable_tokens.clone(),
            },
        }
    }
}

impl From<&CacheRetention> for semantic::CacheRetention {
    fn from(value: &CacheRetention) -> Self {
        match value {
            CacheRetention::InMemory => Self::InMemory,
            CacheRetention::Day => Self::Day,
        }
    }
}

impl From<&CacheMode> for semantic::CacheMode {
    fn from(value: &CacheMode) -> Self {
        match value {
            CacheMode::Implicit => Self::Implicit,
            CacheMode::Explicit => Self::Explicit,
        }
    }
}

impl From<&CacheTtl> for semantic::CacheTtl {
    fn from(value: &CacheTtl) -> Self {
        match value {
            CacheTtl::ThirtyMinutes => Self::ThirtyMinutes,
        }
    }
}

impl From<&CacheMissReason> for semantic::CacheMissReason {
    fn from(value: &CacheMissReason) -> Self {
        match value {
            CacheMissReason::ModelChanged => Self::ModelChanged,
            CacheMissReason::PromptCacheKeyChanged => Self::PromptCacheKeyChanged,
            CacheMissReason::ToolsChanged => Self::ToolsChanged,
            CacheMissReason::TextFormatChanged => Self::TextFormatChanged,
            CacheMissReason::ReasoningEffortChanged => Self::ReasoningEffortChanged,
            CacheMissReason::VerbosityChanged => Self::VerbosityChanged,
            CacheMissReason::ContextCompacted => Self::ContextCompacted,
            CacheMissReason::InputChanged => Self::InputChanged,
            CacheMissReason::ServiceTierChanged => Self::ServiceTierChanged,
        }
    }
}

impl From<&CacheOptions> for semantic::CacheOptions {
    fn from(value: &CacheOptions) -> Self {
        Self {
            mode: map_presence(&value.mode, Into::into),
            ttl: map_presence(&value.ttl, Into::into),
            comparison_response_id: value.comparison_response_id.clone(),
            prewarm: value.prewarm.clone(),
        }
    }
}

impl From<&CacheHints> for semantic::CacheHints {
    fn from(value: &CacheHints) -> Self {
        Self {
            prompt_cache_key: value.prompt_cache_key.clone(),
            prompt_cache_retention: map_presence(&value.prompt_cache_retention, Into::into),
            prompt_cache_options: map_presence(&value.prompt_cache_options, Into::into),
        }
    }
}

impl From<&ClientIdentityHints> for semantic::ClientIdentityHints {
    fn from(value: &ClientIdentityHints) -> Self {
        Self {
            safety_identifier: value.safety_identifier.clone(),
            user: value.user.clone(),
        }
    }
}

impl From<&ExecutionHints> for semantic::ExecutionHints {
    fn from(value: &ExecutionHints) -> Self {
        Self {
            cache: (&value.cache).into(),
            identity: (&value.identity).into(),
            metadata: value.metadata.clone(),
            max_tool_calls: value.max_tool_calls.clone(),
            store: value.store.clone(),
            background: value.background.clone(),
            previous_response_id: value.previous_response_id.clone(),
            conversation: value.conversation.clone(),
            prompt: value.prompt.clone(),
            moderation: value.moderation.clone(),
            context_management: value.context_management.clone(),
        }
    }
}

impl From<&CacheDiagnostics> for semantic::CacheDiagnostics {
    fn from(value: &CacheDiagnostics) -> Self {
        match value {
            CacheDiagnostics::CacheHit => Self::CacheHit,
            CacheDiagnostics::ComparisonResponseNotFound => Self::ComparisonResponseNotFound,
            CacheDiagnostics::Unavailable => Self::Unavailable,
            CacheDiagnostics::CacheMiss {
                reason,
                cache_missed_tokens,
                comparison_reusable_tokens,
            } => Self::CacheMiss {
                reason: reason.into(),
                cache_missed_tokens: *cache_missed_tokens,
                comparison_reusable_tokens: comparison_reusable_tokens.clone(),
            },
        }
    }
}
