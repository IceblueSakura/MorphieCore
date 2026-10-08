//! Target-only assembly, never inferred native ownership.
use super::{generation::RepresentationError, projection::LossRule};
use crate::{protocol::openai::Profile, semantic::task::generation::*};
use std::collections::BTreeMap;

pub(super) fn project(
    items: &[(ItemId, Item)],
    groups: &[MessageEnvelope],
    owners: &BTreeMap<ItemId, ItemId>,
    profile: Profile,
) -> Result<Option<(Vec<MessageEnvelope>, LossRule)>, RepresentationError> {
    if profile == Profile::Responses {
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
        // Group-dependent opaque cannot be made independent by deleting its container.
        if items.iter().any(|(_, item)| {
            item.has_non_reasoning_replay()
                || matches!(item, Item::Reasoning(r) if r.replay.is_some())
        }) {
            return Err(RepresentationError::ReplayOrigin);
        }
        return Ok(Some((vec![], LossRule::OmitResponsesMessageEnvelopes)));
    }
    // Non-Chat native envelopes need their own target contract, not flattening.
    for group in groups {
        if group.chat_owner(items).is_none() {
            return Err(RepresentationError::MessageGrouping);
        }
    }
    let mut projected = groups.to_vec();
    let mut current = None;
    let mut additions: BTreeMap<ItemId, Vec<ItemId>> = BTreeMap::new();
    for (id, item) in items {
        match item {
            Item::Message(message) if message.role == MessageRole::Assistant => current = Some(*id),
            Item::ToolCall(_) => {
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
    if items.iter().any(|(_, item)| {
        item.has_non_reasoning_replay() || matches!(item, Item::Reasoning(r) if r.replay.is_some())
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
    Ok(Some((projected, LossRule::RegroupChatMessageItems)))
}
