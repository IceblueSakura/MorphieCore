//! Helpers shared by native (non-OpenAI) upstream protocols: trusted replay labels and
//! role grouping of client history. No wire shapes live here.
use crate::{
    protocol::CodecError,
    semantic::{
        task::generation::*,
        value::{ReplayOrigin, Text},
    },
};

/// Trusted compatibility labels, never an upstream URL, credential locator or client hint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayTarget {
    provider: Text,
    model: Text,
}
impl ReplayTarget {
    pub fn new(provider: &str, model: &str) -> Result<Self, CodecError> {
        Ok(Self {
            provider: label(provider)?,
            model: label(model)?,
        })
    }
    pub fn provider(&self) -> &str {
        self.provider.as_str()
    }
    pub fn model(&self) -> &str {
        self.model.as_str()
    }
    pub(crate) fn origin(&self) -> ReplayOrigin {
        crate::protocol::replay_envelope::issuer(self.provider(), self.model())
    }
}
fn label(s: &str) -> Result<Text, CodecError> {
    if s.is_empty() || s.len() > 256 || s.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(CodecError::Invalid("native target label"));
    }
    Text::new(s, "native target label", 256).map_err(|_| CodecError::Limit)
}

/// Assemble target messages from ordered items, without claiming source ownership:
/// consecutive assistant output (messages, reasoning, calls) forms one assistant turn,
/// user messages and tool results one user turn.
pub(crate) fn group_client_history(
    request: &GenerationRequest,
) -> Result<GenerationRequest, CodecError> {
    let expected: Vec<_> = request
        .items()
        .iter()
        .filter(|(_, item)| !matches!(item, Item::Instruction(_)))
        .map(|(id, _)| *id)
        .collect();
    let actual: Vec<_> = request
        .message_envelopes()
        .iter()
        .flat_map(|group| group.members().iter().copied())
        .collect();
    if actual == expected {
        return Ok(request.clone());
    }
    let mut groups: Vec<(MessageEnvelopeRole, Vec<ItemId>)> = vec![];
    for (id, item) in request.items() {
        let role = match item {
            Item::Instruction(_) => continue,
            Item::Message(message) => match message.role {
                MessageRole::User => MessageEnvelopeRole::User,
                MessageRole::Assistant => MessageEnvelopeRole::Assistant,
            },
            Item::Reasoning(_) | Item::ToolCall(_) => MessageEnvelopeRole::Assistant,
            Item::ToolResult(_) => MessageEnvelopeRole::User,
            _ => return Err(CodecError::Unsupported("native semantic carrier".into())),
        };
        match groups.last_mut() {
            Some((last, members)) if *last == role => members.push(*id),
            _ => groups.push((role, vec![*id])),
        }
    }
    let groups = groups
        .into_iter()
        .map(|(role, members)| {
            MessageEnvelope::new(
                GroupId::new(members[0].scope(), members[0].get()),
                role,
                members,
            )
        })
        .collect::<Result<_, _>>()?;
    Ok(request.clone().with_message_envelopes(groups)?)
}
