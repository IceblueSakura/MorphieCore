//! An existing input base, not a cache hint or a resource lifecycle operation.
use crate::semantic::{task::generation::GenerationError, value::Text};
use std::time::SystemTime;

#[derive(Clone, Eq, PartialEq)]
pub struct CachedContextReference {
    id: Text,
    /// An absent resource report does not promise unlimited retention.
    expires_at: Option<SystemTime>,
}
impl std::fmt::Debug for CachedContextReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CachedContextReference([redacted])")
    }
}
impl CachedContextReference {
    pub fn new(id: Text, expires_at: Option<SystemTime>) -> Result<Self, GenerationError> {
        if id.as_str().is_empty() || id.as_str().len() > 256 {
            return Err(GenerationError::InvalidResource);
        }
        Ok(Self { id, expires_at })
    }
    pub fn id(&self) -> &str {
        self.id.as_str()
    }
    pub fn expires_at(&self) -> Option<SystemTime> {
        self.expires_at
    }
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.id.as_str().len()
    }
    pub fn check_at(&self, now: SystemTime) -> Result<(), GenerationError> {
        if self.expires_at.is_some_and(|end| now >= end) {
            return Err(GenerationError::InvalidResource);
        }
        Ok(())
    }
}
