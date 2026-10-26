//! Target-only assembly, never inferred native ownership.
use crate::{
    protocol::openai::{
        GenerationApi,
        lowering::{RepresentationError, projection::ProjectionRule},
    },
    semantic::task::generation::*,
};
use std::collections::BTreeMap;

pub(super) fn project(
    items: &[(ItemId, Item)],
    groups: &[MessageEnvelope],
    profile: GenerationApi,
) -> Result<Option<(Vec<MessageEnvelope>, ProjectionRule)>, RepresentationError> {
    if profile == GenerationApi::Responses {
        if groups.is_empty() {
            return Ok(None);
        }
        for group in groups {
            let positions: Vec<_> = group
                .members()
                .iter()
                .map(|id| {
                    items
                        .iter()
                        .position(|(owner, _)| owner == id)
                        .ok_or(RepresentationError::MessageGrouping)
                })
                .collect::<Result<_, _>>()?;
            if positions.windows(2).any(|w| w[1] != w[0] + 1) {
                return Err(RepresentationError::MessageGrouping);
            }
        }
        // An unrelated reasoning owner survives envelope omission unchanged.
        // Final lowering still checks its scope and value.
        if items.iter().any(|(id, item)| {
            matches!(item, Item::Reasoning(r) if r.replay.is_some()
                && groups.iter().any(|group| group.members().contains(id)))
        }) {
            return Err(RepresentationError::ReplayOrigin);
        }
        return Ok(Some((
            vec![],
            ProjectionRule::OmitResponsesMessageEnvelopes,
        )));
    }
    // Non-Chat native envelopes need their own target contract, not flattening.
    for group in groups {
        if crate::protocol::openai::chat::owners::owner(group, items).is_none() {
            return Err(RepresentationError::MessageGrouping);
        }
    }
    let owners = crate::protocol::openai::chat::owners::owners(items, groups);
    let mut projected = groups.to_vec();
    let mut current = None;
    let mut additions: BTreeMap<ItemId, Vec<ItemId>> = BTreeMap::new();
    for (id, item) in items {
        match item {
            Item::Message(message) if message.role == MessageRole::Assistant => {
                current = (message.participant.is_none()
                    || groups.iter().any(|group| group.members().contains(id)))
                .then_some(*id);
            }
            Item::ToolCall(_) | Item::CustomCall(_) => {
                if let Some(owner) = owners.get(id) {
                    if current != Some(*owner) {
                        return Err(RepresentationError::MessageGrouping);
                    }
                } else if let Some(owner) = current {
                    // Do not extend a reported container with an independent item.
                    if !groups.iter().any(|g| g.members().contains(&owner)) {
                        additions.entry(owner).or_default().push(*id);
                    } else {
                        current = None;
                    }
                }
            }
            _ => current = None,
        }
    }
    if additions.is_empty() {
        return Ok(None);
    }
    if items.iter().any(|(id, item)| {
        matches!(item, Item::Reasoning(r) if r.replay.is_some()
            && additions.iter().any(|(owner, calls)| owner == id || calls.contains(id)))
    }) {
        return Err(RepresentationError::ReplayOrigin);
    }
    for (owner, calls) in additions {
        let mut value = owner.get();
        let id = loop {
            let id = GroupId::new(owner.scope(), value);
            if !projected.iter().any(|g| g.id() == id) {
                break id;
            }
            value = value
                .checked_add(1)
                .ok_or(RepresentationError::MessageGrouping)?;
        };
        let mut members = vec![owner];
        members.extend(calls);
        projected.push(MessageEnvelope::new(
            id,
            MessageEnvelopeRole::Assistant,
            members,
        )?);
    }
    Ok(Some((projected, ProjectionRule::RegroupChatMessageItems)))
}
