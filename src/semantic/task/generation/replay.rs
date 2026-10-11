//! Format-bound opaque values; neither a scope label nor a token proves authenticity.
use super::{GenerationError, MAX_TEXT_BYTES};
use crate::semantic::value::Text;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayFormat {
    ResponsesEncrypted,
    GoogleInteractionsV1Thought,
    GoogleInteractionsV1Step,
    GoogleGenerateContentPart,
    AnthropicMessagesThinking,
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
    pub(crate) fn validate_reasoning(&self) -> Result<(), GenerationError> {
        if !matches!(
            self.format(),
            ReplayFormat::ResponsesEncrypted
                | ReplayFormat::GoogleInteractionsV1Thought
                | ReplayFormat::AnthropicMessagesThinking
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

impl super::Item {
    pub(crate) fn has_non_reasoning_replay(&self) -> bool {
        match self {
            Self::Message(m) => m.parts.iter().any(|p| p.replay.is_some()),
            Self::ToolCall(c) => c.context.replay.is_some(),
            Self::ProviderTool(p) => p.replay.is_some(),
            _ => false,
        }
    }
    pub(crate) fn has_assistant_media(&self) -> bool {
        matches!(self, Self::Message(m) if m.role == super::MessageRole::Assistant
            && m.parts.iter().any(|p| matches!(p.content, super::ContentPart::Resource(_))))
    }
}
/// A borrowed locator, never a second writable attachment table.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ReplayOwner {
    Item(super::ItemId),
    Part {
        item: super::ItemId,
        part: super::PartId,
    },
}
impl ReplayOwner {
    pub fn item(self) -> super::ItemId {
        match self {
            Self::Item(item) | Self::Part { item, .. } => item,
        }
    }
    pub fn value(self, items: &[(super::ItemId, super::Item)]) -> Option<&ReplayValue> {
        use super::Item;
        if items.len() > super::MAX_ITEMS {
            return None;
        }
        let mut candidates = items.iter().filter(|(id, _)| *id == self.item());
        let (_, item) = candidates.next()?;
        if candidates.next().is_some() {
            return None;
        }
        match (self, item) {
            (Self::Item(_), Item::Reasoning(r)) => r.replay.as_ref(),
            (Self::Item(_), Item::ToolCall(c)) => c.context.replay.as_ref(),
            (Self::Item(_), Item::ProviderTool(p)) => p.replay.as_ref(),
            (Self::Part { part, .. }, Item::Message(m)) => {
                if m.parts.len() > super::MAX_ITEMS {
                    return None;
                }
                let mut parts = m.parts.iter().filter(|p| p.id == part);
                let value = parts.next()?;
                if parts.next().is_some() {
                    return None;
                }
                value.replay.as_ref()
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ReplayDependency {
    Item(super::RequestDependencyProof),
    Part([u8; 32]),
}
impl ReplayDependency {
    pub(crate) fn capture(
        owner: ReplayOwner,
        request: &super::GenerationRequest,
    ) -> Result<Self, GenerationError> {
        request.validate()?;
        match owner {
            ReplayOwner::Item(id) => Ok(Self::Item(super::RequestDependencyProof::capture(
                request,
                super::HistoryDependency::Owners(vec![id]),
                super::SettingsDependency::None,
            )?)),
            ReplayOwner::Part { item, part } => Ok(Self::Part(super::dependency::part_dependency(
                request, item, part,
            )?)),
        }
    }
    pub(crate) fn check(&self, owner: ReplayOwner, request: &super::GenerationRequest) -> bool {
        match self {
            Self::Item(proof) => proof.check(request).is_ok(),
            Self::Part(_) => Self::capture(owner, request).is_ok_and(|value| value == *self),
        }
    }
}
