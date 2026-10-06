//! Format-bound opaque values; neither a scope label nor a token proves authenticity.
use super::{GenerationError, MAX_TEXT_BYTES};
use crate::semantic::value::Text;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayFormat {
    ResponsesEncrypted,
    GoogleInteractionsV1Thought,
}
#[derive(Clone, Eq, PartialEq)]
pub enum ReplayValue {
    Partial { format: ReplayFormat, value: Text },
    Final { format: ReplayFormat, value: Text },
}
impl std::fmt::Debug for ReplayValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(match self {
            Self::Partial { .. } => "Partial",
            Self::Final { .. } => "Final",
        })
        .field("format", &self.format())
        .field("value", &"[REDACTED]")
        .finish()
    }
}
impl ReplayValue {
    pub fn partial(format: ReplayFormat, value: Text) -> Self {
        Self::Partial { format, value }
    }
    pub fn final_value(format: ReplayFormat, value: Text) -> Self {
        Self::Final { format, value }
    }
    pub fn format(&self) -> ReplayFormat {
        match self {
            Self::Partial { format, .. } | Self::Final { format, .. } => *format,
        }
    }
    pub fn validate(&self) -> Result<(), GenerationError> {
        if self.as_str().is_empty() || self.as_str().len() > MAX_TEXT_BYTES {
            return Err(GenerationError::Limit);
        }
        Ok(())
    }
    pub(crate) fn fingerprint(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        hash.update([match self.format() {
            ReplayFormat::ResponsesEncrypted => 0,
            ReplayFormat::GoogleInteractionsV1Thought => 1,
        }]);
        hash.update([u8::from(self.replay_token().is_some())]);
        hash.update(self.as_str().as_bytes());
        hash.finalize().into()
    }
    pub fn replay_token(&self) -> Option<&str> {
        match self {
            Self::Partial { .. } => None,
            Self::Final { value, .. } => Some(value.as_str()),
        }
    }
    pub fn as_str(&self) -> &str {
        match self {
            Self::Partial { value, .. } | Self::Final { value, .. } => value.as_str(),
        }
    }
}
