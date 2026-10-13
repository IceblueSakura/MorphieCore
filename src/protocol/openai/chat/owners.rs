//! Chat call ownership, a view of IR message envelopes. A Chat-shaped envelope is
//! an assistant message followed by the function calls it carries.
use crate::{protocol::CodecError, semantic::task::generation::*};
use std::collections::BTreeMap;

/// The content message when `group` has the Chat shape.
pub(in crate::protocol::openai) fn owner(
    group: &MessageEnvelope,
    items: &[(ItemId, Item)],
) -> Option<ItemId> {
    let (owner, calls) = group.members().split_first()?;
    let kind = |id: &ItemId| items.iter().find(|(item, _)| item == id).map(|(_, v)| v);
    (group.role() == MessageEnvelopeRole::Assistant
        && matches!(kind(owner), Some(Item::Message(m)) if m.role == MessageRole::Assistant)
        && calls
            .iter()
            .all(|id| matches!(kind(id), Some(Item::ToolCall(_)))))
    .then_some(*owner)
}

/// Call to content message, for every Chat-shaped envelope.
pub(in crate::protocol::openai) fn owners(
    items: &[(ItemId, Item)],
    groups: &[MessageEnvelope],
) -> BTreeMap<ItemId, ItemId> {
    groups
        .iter()
        .filter_map(|group| Some((owner(group, items)?, group)))
        .flat_map(|(owner, group)| group.members()[1..].iter().map(move |id| (*id, owner)))
        .collect()
}

/// Whether every Chat-shaped envelope holds its message and calls adjacently.
pub(in crate::protocol::openai) fn contiguous(
    items: &[(ItemId, Item)],
    groups: &[MessageEnvelope],
) -> bool {
    groups
        .iter()
        .filter(|group| owner(group, items).is_some())
        .all(|group| {
            let first = items.iter().position(|(id, _)| *id == group.members()[0]);
            first.is_some_and(|first| {
                group.members().iter().enumerate().all(|(offset, member)| {
                    items
                        .get(first + offset)
                        .is_some_and(|(id, _)| id == member)
                })
            })
        })
}

/// Envelopes for decoded Chat messages from `(call, message)` declarations.
pub(in crate::protocol::openai) fn envelopes(
    declarations: &[(ItemId, ItemId)],
) -> Result<Vec<MessageEnvelope>, CodecError> {
    let mut groups: Vec<(ItemId, Vec<ItemId>)> = vec![];
    for (call, owner) in declarations {
        match groups.iter_mut().find(|(id, _)| id == owner) {
            Some((_, members)) => members.push(*call),
            None => groups.push((*owner, vec![*owner, *call])),
        }
    }
    groups
        .into_iter()
        .map(|(owner, members)| {
            Ok(MessageEnvelope::new(
                GroupId::new(owner.scope(), owner.get()),
                MessageEnvelopeRole::Assistant,
                members,
            )?)
        })
        .collect()
}
