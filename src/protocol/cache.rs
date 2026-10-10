//! Explicit upstream cache carriers. No cache storage, session state or routing decisions.
use super::openai::CodecError;
use crate::semantic::{
    context::{ConversationContext, ExecutionHints},
    value::{Presence, Text},
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
pub struct CacheProjection {
    pub key: bool,
    pub retention: bool,
    pub options: bool,
    pub session_id: bool,
}
impl CacheProjection {
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
    pub fn project(self, context: &mut ExecutionHints) -> Result<(), CodecError> {
        context.cache.validate()?;
        // Key and retention are advisory; absence leaves automatic caching to the Provider.
        if !self.key {
            context.cache.prompt_cache_key = Presence::Absent;
        }
        if !self.retention {
            context.cache.prompt_cache_retention = Presence::Absent;
        }
        // Explicit cache modes/prewarm are behavioral controls, not inactive hints.
        if !self.options && context.cache.prompt_cache_options.value().is_some() {
            return Err(CodecError::Unsupported("cache options".into()));
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
            return Err(CodecError::Unsupported("Go session carrier".into()));
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
            _ => Err(CodecError::Invalid("request grouping")),
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
            return Err(CodecError::Invalid("session_id"));
        }
        Ok(Self(
            Text::new(value, "session_id", 1024).map_err(|_| CodecError::Invalid("session_id"))?,
        ))
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
    pub(crate) fn header_value(&self) -> Result<&str, CodecError> {
        if !self.as_str().is_ascii() {
            return Err(CodecError::Invalid("session header"));
        }
        Ok(self.as_str())
    }
}
