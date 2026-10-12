//! Borrowed views of explicitly declared message ownership, never inferred turns.
use crate::semantic::task::generation::{
    CallReference, GenerationError, GenerationRequest, GenerationResponse, Item, ItemId,
    LocalScope, MAX_ITEMS, Message, MessageRole,
};
use std::collections::BTreeMap;

/// Caller-allocated group identity; neither a message nor a reported native ID.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct GroupId {
    scope: LocalScope,
    value: u64,
}
impl GroupId {
    pub const fn new(scope: LocalScope, value: u64) -> Self {
        Self { scope, value }
    }
    pub const fn scope(self) -> LocalScope {
        self.scope
    }
    pub const fn get(self) -> u64 {
        self.value
    }
}
pub(in crate::semantic::task::generation) fn validate_message_owners(
    items: &[(ItemId, Item)],
    owners: &BTreeMap<ItemId, ItemId>,
) -> Result<usize, GenerationError> {
    if owners.len() > MAX_ITEMS {
        return Err(GenerationError::Limit);
    }
    for (member, parent) in owners {
        let member_index = items
            .iter()
            .position(|(id, item)| id == member && matches!(item, Item::ToolCall(_)))
            .ok_or(GenerationError::InvalidMessageGroup)?;
        let parent_index = items
            .iter()
            .position(|(id, item)| {
                id == parent && matches!(item, Item::Message(m) if m.role == MessageRole::Assistant)
            })
            .ok_or(GenerationError::InvalidMessageGroup)?;
        if member_index <= parent_index {
            return Err(GenerationError::InvalidMessageGroup);
        }
        if matches!(&items[parent_index].1, Item::Message(m) if m.parts.iter().any(|part| matches!(part.content, crate::semantic::task::generation::ContentPart::Refusal(_))))
        {
            return Err(GenerationError::InvalidResponse);
        }
    }
    Ok(owners.len() * std::mem::size_of::<(ItemId, ItemId)>())
}
pub(in crate::semantic::task::generation) fn message_owners(
    declarations: Vec<(ItemId, ItemId)>,
) -> Result<BTreeMap<ItemId, ItemId>, GenerationError> {
    if declarations.len() > MAX_ITEMS {
        return Err(GenerationError::Limit);
    }
    let mut owners = BTreeMap::new();
    for (member, owner) in declarations {
        if owners.insert(member, owner).is_some() {
            return Err(GenerationError::InvalidMessageGroup);
        }
    }
    Ok(owners)
}
/// Derived references, not a second membership table or a copied body.
#[derive(Clone, Copy)]
pub struct MessageGroup<'a> {
    items: &'a [(ItemId, Item)],
    owners: &'a BTreeMap<ItemId, ItemId>,
    position: usize,
}
impl std::fmt::Debug for MessageGroup<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageGroup")
            .field("owner", &self.owner())
            .finish()
    }
}
impl<'a> MessageGroup<'a> {
    pub fn owner(&self) -> ItemId {
        self.items[self.position].0
    }
    pub fn message(&self) -> &'a Message {
        let Item::Message(message) = &self.items[self.position].1 else {
            unreachable!("validated assistant owner")
        };
        message
    }
    pub fn items(&self) -> Vec<&'a (ItemId, Item)> {
        let owner = self.owner();
        std::iter::once(&self.items[self.position])
            .chain(
                self.items
                    .iter()
                    .filter(|(id, _)| self.owners.get(id) == Some(&owner)),
            )
            .collect()
    }
    pub fn is_contiguous(&self) -> bool {
        self.items().iter().enumerate().all(|(offset, member)| {
            self.items
                .get(self.position + offset)
                .is_some_and(|candidate| candidate.0 == member.0)
        })
    }
    pub fn calls(&self) -> impl ExactSizeIterator<Item = CallReference<'a>> + use<'a> {
        let owner = self.owner();
        self.items
            .iter()
            .filter_map(|(id, item)| {
                if self.owners.get(id) != Some(&owner) {
                    return None;
                }
                let Item::ToolCall(call) = item else {
                    unreachable!("validated group member")
                };
                Some(CallReference {
                    item: *id,
                    call_id: call.call_id.as_str(),
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
    }
}
fn groups<'a>(
    items: &'a [(ItemId, Item)],
    owners: &'a BTreeMap<ItemId, ItemId>,
) -> impl Iterator<Item = MessageGroup<'a>> {
    items
        .iter()
        .enumerate()
        .filter_map(move |(position, (_, item))| {
            matches!(item, Item::Message(m) if m.role == MessageRole::Assistant).then_some(
                MessageGroup {
                    items,
                    owners,
                    position,
                },
            )
        })
}
impl GenerationRequest {
    pub fn message_groups(&self) -> impl Iterator<Item = MessageGroup<'_>> {
        groups(self.items(), self.message_owners())
    }
}
impl GenerationResponse {
    pub fn message_groups(&self) -> impl Iterator<Item = MessageGroup<'_>> {
        groups(self.items(), self.message_owners())
    }
}
