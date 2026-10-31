//! Standard request context and delivery intent, independent of wire dialects.
mod reports;
use super::value::Presence;
pub use reports::{ReportPhase, ResponseReport, ResponseReports};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ContextError {
    #[error("invalid context: {0}")]
    Invalid(ContextReason),
    #[error("context budget exceeded")]
    Limit,
    #[error("stateful execution is not admitted")]
    Stateful,
    #[error(transparent)]
    Generation(#[from] super::task::generation::GenerationError),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextReason {
    Conversation,
    CacheOptions,
    MaxToolCalls,
    SystemFingerprint,
    InstructionEchoes,
    CompletionTimestamp,
    StreamOptions,
    StreamOptionsWithoutStreaming,
}
impl std::fmt::Display for ContextReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
/// Caller-owned logical grouping, not stored history, authentication or replay authority.
#[derive(Clone, Eq, PartialEq)]
pub struct ConversationContext {
    id: String,
    conversation_scoped: bool,
}
impl std::fmt::Debug for ConversationContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConversationContext([redacted])")
    }
}
impl ConversationContext {
    pub fn conversation(id: &str) -> Result<Self, ContextError> {
        Self::new(id, true)
    }
    /// An explicit request-local group; pure callers allocate the ID themselves.
    pub fn independent_request(id: &str) -> Result<Self, ContextError> {
        Self::new(id, false)
    }
    fn new(id: &str, conversation_scoped: bool) -> Result<Self, ContextError> {
        if id.is_empty() || id.len() > 256 || id.bytes().any(|b| !b.is_ascii_graphic() || b == b',')
        {
            return Err(ContextError::Invalid(ContextReason::Conversation));
        }
        Ok(Self {
            id: id.into(),
            conversation_scoped,
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn is_conversation_scoped(&self) -> bool {
        self.conversation_scoped
    }
}

/// Complete execution policies, not interchangeable native tier names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServicePolicy {
    ProjectConfigured,
    StandardProcessing,
    FlexibleProcessing,
    FastProcessing,
    PriorityProcessing,
    ScaleProcessing,
    UltraFastProcessing,
    PreferCommittedCapacity,
    StandardCapacityOnly,
}
/// An upstream-reported service class, never inferred from request intent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceClass {
    ConfiguredProcessing,
    StandardProcessing,
    FlexibleProcessing,
    FastProcessing,
    PriorityProcessing,
    ScaleProcessing,
    UltraFastProcessing,
    StandardCapacity,
    PriorityCapacity,
    BatchProcessing,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CacheRetention {
    InMemory,
    Day,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CacheMode {
    Implicit,
    Explicit,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CacheTtl {
    ThirtyMinutes,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CacheOptions {
    pub mode: Option<CacheMode>,
    pub ttl: Option<CacheTtl>,
    pub comparison_response_id: Option<String>,
    pub prewarm: Option<bool>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CacheHints {
    pub prompt_cache_key: Option<String>,
    pub prompt_cache_retention: Option<CacheRetention>,
    pub prompt_cache_options: Presence<CacheOptions>,
}
/// Client identity and safety metadata are not cache keys or session identities.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ClientIdentityHints {
    pub safety_identifier: Option<String>,
    pub user: Option<String>,
}
impl ClientIdentityHints {
    pub fn validate(&self) -> Result<(), ContextError> {
        for (value, max) in [(&self.safety_identifier, 128), (&self.user, 512)] {
            if value
                .as_ref()
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
        if self
            .prompt_cache_key
            .as_ref()
            .is_some_and(|s| s.chars().count() > 256 || s.len() > 1024)
        {
            return Err(ContextError::Limit);
        }
        if let Some(options) = self.prompt_cache_options.value()
            && options
                .comparison_response_id
                .as_ref()
                .is_some_and(|s| s.len() > 256)
        {
            return Err(ContextError::Limit);
        }
        // Fixed field cardinality and the individual text bounds fit this budget.
        const {
            assert!(super::task::generation::MAX_TEXT_BYTES >= 8192);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExecutionHints {
    pub cache: CacheHints,
    pub identity: ClientIdentityHints,
    pub metadata: Presence<BTreeMap<String, String>>,
    pub max_tool_calls: Presence<u64>,
    pub store: Presence<bool>,
    pub background: Presence<bool>,
}

pub(crate) fn validate_metadata(
    metadata: Option<&BTreeMap<String, String>>,
) -> Result<(), ContextError> {
    if metadata.is_some_and(|m| {
        m.len() > 16
            || m.iter()
                .any(|(k, v)| k.chars().count() > 64 || v.chars().count() > 512)
    }) {
        return Err(ContextError::Limit);
    }
    Ok(())
}

/// Dynamic map storage and escaped-text working space; the container is charged
/// by its owner. Share the same accounting for reported and request-echo maps.
pub(crate) fn metadata_retained_bytes(metadata: Option<&BTreeMap<String, String>>) -> usize {
    metadata.map_or(0, |map| {
        map.iter().fold(0usize, |bytes, (key, value)| {
            bytes
                .saturating_add(1024)
                .saturating_add(key.capacity().max(key.len().saturating_mul(64)))
                .saturating_add(value.capacity().max(value.len().saturating_mul(64)))
        })
    })
}
impl ExecutionHints {
    /// Conservative retained storage plus escaped-text working space, without
    /// choosing a protocol's field names or serialization.
    pub fn retained_bytes(&self) -> usize {
        fn text(value: &String) -> usize {
            value.capacity().max(value.len().saturating_mul(64))
        }
        let mut bytes = std::mem::size_of_val(self);
        for value in [
            self.cache.prompt_cache_key.as_ref(),
            self.cache
                .prompt_cache_options
                .value()
                .and_then(|o| o.comparison_response_id.as_ref()),
            self.identity.safety_identifier.as_ref(),
            self.identity.user.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            bytes = bytes.saturating_add(text(value));
        }
        bytes.saturating_add(metadata_retained_bytes(self.metadata.value()))
    }
    pub fn validate(&self) -> Result<(), ContextError> {
        self.cache.validate()?;
        self.identity.validate()?;
        if self.store == Presence::Value(true) || self.background == Presence::Value(true) {
            return Err(ContextError::Stateful);
        }
        validate_metadata(self.metadata.value())?;
        if self.max_tool_calls == Presence::Value(0) {
            return Err(ContextError::Invalid(ContextReason::MaxToolCalls));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CacheDiagnostics {
    CacheHit,
    ComparisonResponseNotFound,
    Unavailable,
    CacheMiss {
        reason: CacheMissReason,
        cache_missed_tokens: u64,
        comparison_reusable_tokens: Presence<u64>,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
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
    pub reports: ResponseReports,
    /// Reported text-only instructions; no independent resource table is needed.
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
    pub fn settings(&self) -> Option<&super::task::generation::GenerationSettings> {
        self.reports.latest().and_then(|r| r.settings.as_ref())
    }
    pub fn service_class(&self) -> &Presence<ServiceClass> {
        self.reports
            .latest()
            .map_or(&Presence::Absent, |r| &r.service_class)
    }
    pub fn validate(&self) -> Result<(), ContextError> {
        use super::task::generation::{GenerationControls, GenerationRequest, Item};
        self.execution.validate()?;
        if self
            .system_fingerprint
            .value()
            .is_some_and(|s| s.is_empty() || s.len() > 256)
        {
            return Err(ContextError::Invalid(ContextReason::SystemFingerprint));
        }
        self.reports.validate()?;
        if let Some(items) = &self.instruction_messages {
            if self.reports.iter().any(|r| {
                r.settings
                    .as_ref()
                    .is_some_and(|s| !s.instructions.is_absent())
            }) || items
                .iter()
                .any(|(_, item)| !matches!(item, Item::Instruction(_)))
            {
                return Err(ContextError::Invalid(ContextReason::InstructionEchoes));
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
            return Err(ContextError::Invalid(ContextReason::CompletionTimestamp));
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
            return Err(ContextError::Invalid(ContextReason::StreamOptions));
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
                return Err(ContextError::Invalid(
                    ContextReason::StreamOptionsWithoutStreaming,
                ));
            }
            options.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_metadata_storage_charges_spare_tree_slots() {
        let empty = ExecutionHints::default();
        let sparse = ExecutionHints {
            metadata: Presence::Value(BTreeMap::from([(String::new(), String::new())])),
            ..Default::default()
        };
        sparse.validate().unwrap();
        // The pinned toolchain's B-tree leaf has eleven slots, even for one key.
        let slots = 11 * std::mem::size_of::<(String, String)>();
        assert!(sparse.retained_bytes() - empty.retained_bytes() >= slots + 128);
    }
}
