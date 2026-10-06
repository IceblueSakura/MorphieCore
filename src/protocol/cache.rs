//! Explicit upstream cache carriers. No cache storage, session state or routing decisions.
use super::openai::CodecError;
use crate::semantic::{
    context::ExecutionHints,
    value::{Presence, Text},
};
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
}
/// Caller-supplied Provider cache/observability grouping. Never derived from user/key/replay.
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
}
