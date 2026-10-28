//! Helpers shared by native (non-OpenAI) upstream protocols: trusted replay labels,
//! stateless request defaults and role grouping of client history. No wire shapes live here.
use crate::protocol::failure::CarrierReason;
use crate::{
    protocol::CodecError,
    semantic::{
        context::ExecutionHints,
        task::generation::*,
        value::{Presence, ReplayOrigin, Text},
    },
};
use std::borrow::Cow;

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
        return Err(CodecError::invalid());
    }
    Text::new(s, "native target label", 256).map_err(|_| CodecError::Limit)
}

/// Direct native codecs first verify issuer/dependencies, then omit finalized
/// reasoning in an incompatible carrier format before assembling target groups.
pub(crate) fn native_reasoning_history<'a>(
    request: &'a GenerationRequest,
    fidelity: &crate::protocol::fidelity::FidelityRecords,
    target: &ReplayTarget,
    formats: &[ReplayFormat],
) -> Result<
    (
        Cow<'a, GenerationRequest>,
        crate::protocol::fidelity::FidelityRecords,
        bool,
    ),
    CodecError,
> {
    request.validate()?;
    fidelity.require_native_replay(request, &target.origin())?;
    let mut omitted = Vec::new();
    for (id, item) in request.items() {
        if let Item::Reasoning(reasoning) = item
            && let Some(value) = &reasoning.replay
            && !formats.contains(&value.format())
        {
            if value.replay_token().is_none() {
                return Err(CodecError::Unsupported(CarrierReason::Reasoning));
            }
            omitted.push(*id);
        }
    }
    // Deletion validates envelopes before attachment checks, including empty anchors.
    let mut request = if omitted.is_empty() {
        Cow::Borrowed(request)
    } else {
        Cow::Owned(
            request
                .clone()
                .retain_items(|id, _| !omitted.contains(&id))?,
        )
    };
    let mut fidelity = fidelity.clone();
    let mut transferred = !omitted.is_empty();
    let foreign_attachments = request.items().iter().any(|(_, item)| match item {
        Item::Message(message) => message.parts.iter().any(|part| {
            part.replay
                .as_ref()
                .is_some_and(|value| !formats.contains(&value.format()))
        }),
        Item::ToolCall(call) => call
            .replay
            .as_ref()
            .is_some_and(|value| !formats.contains(&value.format())),
        _ => false,
    });
    if foreign_attachments {
        let mut items = request.items().to_vec();
        for (owner, item) in &mut items {
            match item {
                Item::Message(message) => {
                    for part in &mut message.parts {
                        if let Some(value) = &part.replay
                            && !formats.contains(&value.format())
                        {
                            if message.status != ItemLifecycle::Completed
                                || value.replay_token().is_none()
                            {
                                return Err(CodecError::Unsupported(CarrierReason::Replay));
                            }
                            part.replay = None;
                            fidelity.remove_part_replay(*owner, part.id);
                            transferred = true;
                        }
                    }
                }
                Item::ToolCall(call) => {
                    if let Some(value) = &call.replay
                        && !formats.contains(&value.format())
                    {
                        if call.status != ItemLifecycle::Completed || value.replay_token().is_none()
                        {
                            return Err(CodecError::Unsupported(CarrierReason::Replay));
                        }
                        call.replay = None;
                        fidelity.remove_replay(*owner);
                        fidelity.remove_response_item_id(*owner);
                        transferred = true;
                    }
                }
                _ => {}
            }
        }
        request = Cow::Owned(request.into_owned().with_items(items)?);
    }
    fidelity.retain_owners(request.items());
    Ok((request, fidelity, transferred))
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
/// Group client history by role. Instructions, and with `ordered_system` the tool
/// changes and configuration updates a Messages system message carries, stay outside.
pub(crate) fn group_client_history(
    request: &GenerationRequest,
    ordered_system: bool,
) -> Result<GenerationRequest, CodecError> {
    let outside_envelopes = |item: &Item| match item {
        Item::Instruction(_) => true,
        Item::ToolChange(_) | Item::ConfigurationUpdate(_) => ordered_system,
        _ => false,
    };
    let expected: Vec<_> = request
        .items()
        .iter()
        .filter(|(_, item)| !outside_envelopes(item))
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
    // Preserve known container boundaries when only part of the history has
    // envelopes. Empty containers need their explicit anchor, not guessed regrouping.
    if request
        .message_envelopes()
        .iter()
        .any(|g| g.members().is_empty())
    {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    let positions: std::collections::BTreeMap<_, _> = request
        .items()
        .iter()
        .enumerate()
        .map(|(position, (id, _))| (*id, position))
        .collect();
    let mut previous = None;
    for group in request.message_envelopes() {
        let mut member_position = None;
        for owner in group.members() {
            let position = *positions
                .get(owner)
                .ok_or(CodecError::Unsupported(CarrierReason::Unavailable))?;
            if previous.is_some_and(|p| position <= p)
                || member_position.is_some_and(|p| position != p + 1)
            {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
            previous = Some(position);
            member_position = Some(position);
        }
    }
    let starts: std::collections::BTreeSet<_> = request
        .message_envelopes()
        .iter()
        .filter_map(|g| g.members().first().copied())
        .collect();
    let ends: std::collections::BTreeSet<_> = request
        .message_envelopes()
        .iter()
        .filter_map(|g| g.members().last().copied())
        .collect();
    let mut groups: Vec<(MessageEnvelopeRole, Vec<ItemId>)> = vec![];
    let mut boundary = true;
    for (id, item) in request.items() {
        let role = match item {
            item if outside_envelopes(item) => {
                boundary = true;
                continue;
            }
            Item::Message(message) => match message.role {
                MessageRole::User => MessageEnvelopeRole::User,
                MessageRole::Assistant => MessageEnvelopeRole::Assistant,
            },
            Item::Reasoning(_) | Item::ToolCall(_) => MessageEnvelopeRole::Assistant,
            Item::ToolResult(_) => MessageEnvelopeRole::User,
            _ => {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
        };
        match groups.last_mut() {
            Some((last, members)) if *last == role && !boundary && !starts.contains(id) => {
                members.push(*id)
            }
            _ => groups.push((role, vec![*id])),
        }
        boundary = ends.contains(id);
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
