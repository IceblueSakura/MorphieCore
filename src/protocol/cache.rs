//! Explicit upstream cache carriers and routing affinity keys.
//!
//! # Privacy and Scoping
//! - **No Shared State**: This module does not manage cache storage, session data, or
//!   routing execution.
//! - **Private Identifiers**: Cache keys and caller conversation identities are hashed
//!   via XXH3-128 (`affinity_key`) and kept private; raw identifiers never cross upstream
//!   boundaries unless supported by an explicit target carrier.
use super::openai::CodecError;
use crate::{
    protocol::failure::CarrierReason,
    semantic::{
        context::{ConversationContext, ExecutionHints},
        value::Text,
    },
};
use xxhash_rust::xxh3::Xxh3;

fn affinity_key(context: &ConversationContext, domain: &[u8]) -> String {
    grouping_key(
        context.id().as_bytes(),
        context.is_conversation_scoped() as u8,
        domain,
    )
}
fn grouping_key(id: &[u8], kind: u8, domain: &[u8]) -> String {
    let mut hash = Xxh3::new();
    hash.update(domain);
    hash.update(&[kind]);
    hash.update(&(id.len() as u32).to_le_bytes());
    hash.update(id);
    format!("{:032x}", hash.digest128())
}

/// Provider-local grouping, independent of credentials, models and process lifetime.
/// Source tags distinguish declared conversations, independent requests, prefixes and keys.
#[derive(Clone, Copy)]
pub(crate) enum SessionNamespace {
    Siwc,
    OpenCodeGo,
    OpenRouter,
}
impl SessionNamespace {
    fn domain(self) -> &'static [u8] {
        match self {
            Self::Siwc => b"MorphieCore.siwc-session.v2\0",
            Self::OpenCodeGo => b"MorphieCore.go-session.v2\0",
            Self::OpenRouter => b"MorphieCore.openrouter-session.v2\0",
        }
    }
    pub(crate) fn cache_key(self, key: &str) -> String {
        grouping_key(key.as_bytes(), 3, self.domain())
    }
}

/// Candidate-local optimization input, never caller conversation identity.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct InferredAffinity(pub(crate) [u8; 16]);
impl std::fmt::Debug for InferredAffinity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InferredAffinity([redacted])")
    }
}

/// Explicit upstream cache carriers supported by a model target.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheTargetPolicy {
    /// Target supports native cache keys.
    pub key: bool,
    /// Target supports cache retention settings.
    pub retention: bool,
    /// Target supports cache options.
    pub options: bool,
    /// Target supports explicit session IDs.
    pub session_id: bool,
}

impl CacheTargetPolicy {
    pub const fn all() -> Self {
        Self {
            key: true,
            retention: true,
            options: true,
            session_id: true,
        }
    }
    pub fn intersect(&mut self, other: Self) {
        self.key &= other.key;
        self.retention &= other.retention;
        self.options &= other.options;
        self.session_id &= other.session_id;
    }
    pub(crate) fn infer_key(
        self,
        context: &mut ExecutionHints,
        conversation: Option<&ConversationContext>,
        inferred: Option<InferredAffinity>,
    ) {
        if self.key && context.cache.prompt_cache_key.is_none() {
            let key = if let Some(group) = conversation.filter(|c| c.is_conversation_scoped()) {
                Some(affinity_key(group, b"MorphieCore.prompt-cache.v2\0"))
            } else {
                inferred.map(|group| grouping_key(&group.0, 2, b"MorphieCore.prompt-cache.v2\0"))
            };
            if let Some(key) = key {
                context.cache.prompt_cache_key = Some(key);
            }
        }
    }
    /// Internal target projection. Callers select which grouping sources their carrier admits.
    pub(crate) fn session_id(
        self,
        conversation: Option<&ConversationContext>,
        session: Option<&CacheSession>,
        inferred: Option<InferredAffinity>,
        namespace: SessionNamespace,
    ) -> Result<String, CodecError> {
        if !self.session_id {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        match (conversation, session) {
            (Some(group), None) if !group.is_conversation_scoped() && inferred.is_some() => Ok(
                grouping_key(&inferred.expect("checked").0, 2, namespace.domain()),
            ),
            (Some(group), None) => Ok(affinity_key(group, namespace.domain())),
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
