//! Helpers shared by native (non-OpenAI) upstream protocols: trusted replay labels,
//! stateless request defaults and role grouping of client history. No wire shapes live here.
use crate::{
    protocol::CodecError,
    semantic::{
        context::ExecutionHints,
        task::generation::*,
        value::{Presence, ReplayOrigin, Text},
    },
};

/// Trusted compatibility labels, never an upstream URL, credential locator or client hint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayTarget {
    provider: Text,
    model: Text,
    origin: ReplayOrigin,
}
impl ReplayTarget {
    pub fn new(provider: &str, model: &str) -> Result<Self, CodecError> {
        Ok(Self {
            provider: label(provider)?,
            model: label(model)?,
            origin: crate::protocol::replay_envelope::issuer(provider, model),
        })
    }
    pub fn provider(&self) -> &str {
        self.provider.as_str()
    }
    pub fn model(&self) -> &str {
        self.model.as_str()
    }
    pub(crate) fn with_origin(mut self, origin: ReplayOrigin) -> Self {
        self.origin = origin;
        self
    }
    pub(crate) fn origin(&self) -> ReplayOrigin {
        self.origin.clone()
    }
}
fn label(s: &str) -> Result<Text, CodecError> {
    if s.is_empty() || s.len() > 256 || s.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(CodecError::Invalid("native target label"));
    }
    Text::new(s, "native target label", 256).map_err(|_| CodecError::Limit)
}

/// Direct native codecs first verify issuer/dependencies, then omit complete
/// reasoning in an incompatible carrier format before assembling target groups.
pub(crate) fn native_reasoning_history(
    request: &GenerationRequest,
    fidelity: &crate::protocol::fidelity::FidelityRecords,
    target: &ReplayTarget,
    formats: &[ReplayFormat],
) -> Result<
    (
        GenerationRequest,
        crate::protocol::fidelity::FidelityRecords,
        bool,
    ),
    CodecError,
> {
    request.validate()?;
    fidelity.require_reasoning_replay(request, &target.origin())?;
    let mut omitted = Vec::new();
    for (id, item) in request.items() {
        if let Item::Reasoning(reasoning) = item
            && let Some(value) = &reasoning.replay
            && !formats.contains(&value.format())
        {
            if reasoning.status != ItemLifecycle::Completed || value.replay_token().is_none() {
                return Err(CodecError::Unsupported("partial foreign reasoning".into()));
            }
            omitted.push(*id);
        }
    }
    let request = request
        .clone()
        .retain_items(|id, _| !omitted.contains(&id))?;
    let mut fidelity = fidelity.clone();
    fidelity.retain_owners(request.items());
    Ok((request, fidelity, !omitted.is_empty()))
}

/// Explicit stateless defaults do not ask the upstream for storage or context.
pub(crate) fn normalize_stateless_defaults(context: &mut ExecutionHints) {
    if context.store == Presence::Value(false) {
        context.store = Presence::Absent;
    }
    if context.background == Presence::Value(false) {
        context.background = Presence::Absent;
    }
    for value in [
        &mut context.previous_response_id,
        &mut context.conversation,
        &mut context.prompt,
        &mut context.moderation,
    ] {
        if *value == Presence::Null {
            *value = Presence::Absent;
        }
    }
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
