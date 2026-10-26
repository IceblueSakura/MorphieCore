//! Explicit upstream cache carriers. No cache storage, session state or routing decisions.
use super::openai::CodecError;
use crate::{
    protocol::failure::CarrierReason,
    semantic::{
        context::{CacheRetention, ConversationContext, ExecutionHints},
        value::{Presence, Text},
    },
};
use sha2::{Digest, Sha256};

fn affinity_key(context: &ConversationContext, domain: &[u8]) -> String {
    grouping_key(
        context.id().as_bytes(),
        context.is_conversation_scoped() as u8,
        domain,
    )
}
fn grouping_key(id: &[u8], kind: u8, domain: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update([kind]);
    hash.update((id.len() as u32).to_le_bytes());
    hash.update(id);
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
/// Candidate-local optimization input, never caller conversation identity.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct InferredAffinity(pub(crate) [u8; 16]);
impl std::fmt::Debug for InferredAffinity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InferredAffinity([redacted])")
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheTargetPolicy {
    pub key: bool,
    pub retention: bool,
    /// Trusted opt-in after reviewing this target's default storage policy.
    /// Only the extended-cache preference may be omitted; this is not a carrier.
    pub omit_day_retention: bool,
    pub options: bool,
    pub session_id: bool,
}
/// Omitted advisory execution hints. Fixed field labels only; cache values and
/// caller identifiers stay private.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum HintRule {
    OmitKey,
    OmitInactiveOptions,
    OmitIdentityHints,
    OmitDayRetention,
    OmitLegacyUserBucketing,
}
impl HintRule {
    pub const fn name(self) -> &'static str {
        match self {
            Self::OmitKey => "cache.omit-key.v1",
            Self::OmitInactiveOptions => "cache.omit-inactive-options.v1",
            Self::OmitIdentityHints => "generation.omit-identity-hints.v1",
            Self::OmitDayRetention => "cache.omit-day-retention.v1",
            Self::OmitLegacyUserBucketing => "cache.omit-legacy-user-bucketing.v1",
        }
    }
    pub(crate) fn effect(self) -> crate::protocol::conversion::generation::Effect {
        use crate::protocol::conversion::generation::{Change, ConversionBoundary, Effect, Field};
        let (field, change) = match self {
            Self::OmitIdentityHints => (Field::Identity, Change::Omit),
            Self::OmitLegacyUserBucketing => (Field::IdentityBucket, Change::Omit),
            Self::OmitInactiveOptions => (Field::CacheOptions, Change::Normalize),
            Self::OmitKey => (Field::CacheKey, Change::Omit),
            Self::OmitDayRetention => (Field::CacheRetention, Change::Omit),
        };
        Effect::new(
            self.name(),
            field,
            ConversionBoundary::RequestProject,
            change,
        )
    }
    pub(crate) fn evidence(self) -> Option<crate::protocol::conversion::generation::Effect> {
        Some(self.effect())
    }
}
impl CacheTargetPolicy {
    /// Call only after successful projection; active options still reject.
    pub fn applied_rules(self, context: &ExecutionHints) -> Vec<HintRule> {
        [
            (
                self.omits_day_retention(context),
                HintRule::OmitDayRetention,
            ),
            (
                !self.key && !context.cache.prompt_cache_key.is_absent(),
                HintRule::OmitKey,
            ),
            (
                !self.options && !context.cache.prompt_cache_options.is_absent(),
                HintRule::OmitInactiveOptions,
            ),
        ]
        .into_iter()
        .filter_map(|(lost, rule)| lost.then_some(rule))
        .collect()
    }
    pub const fn all() -> Self {
        Self {
            key: true,
            retention: true,
            omit_day_retention: false,
            options: true,
            session_id: true,
        }
    }
    pub fn intersect(&mut self, other: Self) {
        self.key &= other.key;
        self.retention &= other.retention;
        // Either trusted declaration may select the loss. False is absence of
        // approval, not a veto; repeated carrier intersections are idempotent.
        self.omit_day_retention |= other.omit_day_retention;
        self.options &= other.options;
        self.session_id &= other.session_id;
    }
    pub fn project(self, context: &mut ExecutionHints) -> Result<(), CodecError> {
        let mut projected = context.clone();
        self.project_inner(&mut projected)?;
        *context = projected;
        Ok(())
    }
    pub(crate) fn omits_day_retention(self, context: &ExecutionHints) -> bool {
        !self.retention
            && self.omit_day_retention
            && context.cache.prompt_cache_retention == Presence::Value(CacheRetention::Day)
    }
    fn project_inner(self, context: &mut ExecutionHints) -> Result<(), CodecError> {
        context.cache.validate()?;
        // Shared hints also hold nullable reports; target projection admits requests only.
        if context.cache.prompt_cache_options == Presence::Null {
            return Err(CodecError::invalid());
        }
        // Never erase in_memory or infer a default storage policy from automatic caching.
        if !self.retention && !context.cache.prompt_cache_retention.is_absent() {
            if !self.omits_day_retention(context) {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
            context.cache.prompt_cache_retention = Presence::Absent;
        }
        // The grouping key alone is advisory.
        if !self.key {
            context.cache.prompt_cache_key = Presence::Absent;
        }
        // Explicit cache modes/prewarm are behavioral controls, not inactive hints.
        if !self.options
            && context
                .cache
                .prompt_cache_options
                .value()
                .is_some_and(|options| {
                    options != &crate::semantic::context::CacheOptions::default()
                })
        {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        if !self.options {
            context.cache.prompt_cache_options = Presence::Absent;
        }
        Ok(())
    }
    pub(crate) fn project_request(
        self,
        context: &mut ExecutionHints,
        conversation: Option<&ConversationContext>,
        inferred: Option<InferredAffinity>,
    ) -> Result<(), CodecError> {
        self.project(context)?;
        if self.key && context.cache.prompt_cache_key.is_absent() {
            let key = if let Some(group) = conversation.filter(|c| c.is_conversation_scoped()) {
                Some(affinity_key(group, b"MorphieCore.prompt-cache.v1\0"))
            } else {
                inferred.map(|group| grouping_key(&group.0, 2, b"MorphieCore.prompt-cache.v1\0"))
            };
            if let Some(key) = key {
                context.cache.prompt_cache_key = Presence::Value(key);
            }
        }
        Ok(())
    }
    /// Internal target projection. Cache keys never identify a conversation.
    pub(crate) fn opencode_session(
        self,
        conversation: Option<&ConversationContext>,
        session: Option<&CacheSession>,
        inferred: Option<InferredAffinity>,
    ) -> Result<String, CodecError> {
        if !self.session_id {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        match (conversation, session) {
            (Some(group), None) if !group.is_conversation_scoped() && inferred.is_some() => {
                Ok(grouping_key(
                    &inferred.expect("checked").0,
                    2,
                    b"MorphieCore.go-session.v1\0",
                ))
            }
            (Some(group), None) => Ok(affinity_key(group, b"MorphieCore.go-session.v1\0")),
            (None, Some(session)) => Ok(session.header_value()?.into()),
            _ => Err(CodecError::invalid()),
        }
    }
}
/// Independent caller-supplied grouping. Key projection never populates this request field.
#[derive(Clone, Eq, PartialEq)]
pub struct CacheSession(Text);
impl std::fmt::Debug for CacheSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CacheSession([redacted])")
    }
}
impl CacheSession {
    pub fn new(value: &str) -> Result<Self, CodecError> {
        if value.chars().count() > 256 || value.chars().any(char::is_control) {
            return Err(CodecError::invalid());
        }
        Ok(Self(
            Text::new(value, "session_id", 1024).map_err(|_| CodecError::invalid())?,
        ))
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
    pub(crate) fn header_value(&self) -> Result<&str, CodecError> {
        if !self.as_str().is_ascii() {
            return Err(CodecError::invalid());
        }
        Ok(self.as_str())
    }
}
