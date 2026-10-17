//! Format-bound opaque values; neither a scope label nor a token proves authenticity.
use crate::semantic::task::generation::{GenerationError, MAX_TEXT_BYTES};
use crate::semantic::value::Text;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayFormat {
    ResponsesEncrypted,
    ResponsesCompaction,
    GoogleInteractionsV1Thought,
    GoogleInteractionsV1Step,
    GoogleGenerateContentPart,
    AnthropicMessagesThinking,
    /// Safety-redacted thinking: opaque `data` with no readable parts.
    AnthropicMessagesRedactedThinking,
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
        let empty_compaction_opener =
            self.format() == ReplayFormat::ResponsesCompaction && self.replay_token().is_none();
        if self.as_str().is_empty() && !empty_compaction_opener
            || self.as_str().len() > MAX_TEXT_BYTES
        {
            return Err(GenerationError::Limit);
        }
        Ok(())
    }
    pub(crate) fn validate_compaction(&self) -> Result<(), GenerationError> {
        if self.format() != ReplayFormat::ResponsesCompaction {
            return Err(GenerationError::InvalidReplay);
        }
        self.validate()
    }
    pub(crate) fn validate_reasoning(&self) -> Result<(), GenerationError> {
        if !matches!(
            self.format(),
            ReplayFormat::ResponsesEncrypted
                | ReplayFormat::GoogleInteractionsV1Thought
                | ReplayFormat::GoogleGenerateContentPart
                | ReplayFormat::AnthropicMessagesThinking
                | ReplayFormat::AnthropicMessagesRedactedThinking
        ) {
            return Err(GenerationError::InvalidReplay);
        }
        self.validate()
    }
    pub(crate) fn fingerprint(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        hash.update([match self.format() {
            ReplayFormat::ResponsesEncrypted => 0,
            ReplayFormat::GoogleInteractionsV1Thought => 1,
            ReplayFormat::GoogleInteractionsV1Step => 2,
            ReplayFormat::GoogleGenerateContentPart => 3,
            ReplayFormat::AnthropicMessagesThinking => 4,
            ReplayFormat::AnthropicMessagesRedactedThinking => 5,
            ReplayFormat::ResponsesCompaction => 6,
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

/// Event-owned opaque value and intake origin. The item kind owns format admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemReplay {
    pub value: ReplayValue,
    pub origin: Option<crate::semantic::value::ReplayOrigin>,
}
impl ItemReplay {
    pub fn validate(
        &self,
        kind: &crate::semantic::task::generation::ItemKind,
    ) -> Result<(), GenerationError> {
        use crate::semantic::task::generation::ItemKind;
        match kind {
            ItemKind::Reasoning => self.value.validate_reasoning(),
            ItemKind::Compaction => self.value.validate_compaction(),
            _ => Err(GenerationError::InvalidReplay),
        }
    }
    pub fn permits(&self, target: Option<&crate::semantic::value::ReplayOrigin>) -> bool {
        self.origin.is_some() && self.origin.as_ref() == target
    }
}
