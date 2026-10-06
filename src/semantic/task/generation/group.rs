//! Borrowed views of explicitly declared message ownership, never inferred turns.
use super::{
    CallReference, GenerationError, GenerationRequest, GenerationResponse, Item, ItemId,
    LocalScope, MAX_ITEMS, Message, MessageRole,
};
use std::collections::{BTreeMap, BTreeSet};

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
/// The only writable replay membership. Shared members never duplicate bodies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayGroup {
    id: GroupId,
    members: Vec<ItemId>,
}
impl ReplayGroup {
    pub fn new(id: GroupId, members: Vec<ItemId>) -> Result<Self, GenerationError> {
        if members.len() > MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        if members.is_empty() || members.iter().collect::<BTreeSet<_>>().len() != members.len() {
            return Err(GenerationError::InvalidReplayGroup);
        }
        Ok(Self { id, members })
    }
    pub const fn id(&self) -> GroupId {
        self.id
    }
    pub fn members(&self) -> &[ItemId] {
        &self.members
    }
    pub(super) fn check_owners(&self, owners: &[ItemId]) -> Result<(), GenerationError> {
        let mut previous = None;
        for owner in &self.members {
            let index = owners
                .iter()
                .position(|id| id == owner)
                .ok_or(GenerationError::InvalidReplayGroup)?;
            if previous.is_some_and(|old| old >= index) {
                return Err(GenerationError::InvalidReplayGroup);
            }
            previous = Some(index);
        }
        Ok(())
    }
}
pub(super) fn validate_replay_groups(
    items: &[(ItemId, Item)],
    groups: &[ReplayGroup],
) -> Result<usize, GenerationError> {
    if groups.len() > MAX_ITEMS || items.len() > MAX_ITEMS {
        return Err(GenerationError::Limit);
    }
    let owners: Vec<_> = items.iter().map(|(owner, _)| *owner).collect();
    let mut ids = BTreeSet::new();
    let mut members = 0usize;
    for group in groups {
        if !ids.insert(group.id) {
            return Err(GenerationError::InvalidReplayGroup);
        }
        members = members
            .checked_add(group.members.len())
            .ok_or(GenerationError::Limit)?;
        if members > MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        group.check_owners(&owners)?;
    }
    Ok(std::mem::size_of_val(groups) + members * std::mem::size_of::<ItemId>())
}
/// A bounded borrowed view; finding a member never restores deleted content.
#[derive(Clone, Copy)]
pub struct ReplayGroupView<'a> {
    group: &'a ReplayGroup,
    items: &'a [(ItemId, Item)],
}
impl std::fmt::Debug for ReplayGroupView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplayGroupView")
            .field("declaration", &self.group)
            .finish()
    }
}
impl<'a> ReplayGroupView<'a> {
    pub const fn declaration(self) -> &'a ReplayGroup {
        self.group
    }
    pub fn items(self) -> impl ExactSizeIterator<Item = &'a (ItemId, Item)> + use<'a> {
        self.group.members.iter().map(move |owner| {
            self.items
                .iter()
                .find(|(id, _)| id == owner)
                .expect("validated replay member")
        })
    }
}
impl GenerationRequest {
    pub fn replay_group_views(&self) -> impl Iterator<Item = ReplayGroupView<'_>> {
        self.replay_groups().iter().map(|group| ReplayGroupView {
            group,
            items: self.items(),
        })
    }
    pub fn replay_groups_for(&self, owner: ItemId) -> impl Iterator<Item = &ReplayGroup> {
        self.replay_groups()
            .iter()
            .filter(move |group| group.members.contains(&owner))
    }
}
impl GenerationResponse {
    pub fn replay_group_views(&self) -> impl Iterator<Item = ReplayGroupView<'_>> {
        self.replay_groups().iter().map(|group| ReplayGroupView {
            group,
            items: self.items(),
        })
    }
    pub fn replay_groups_for(&self, owner: ItemId) -> impl Iterator<Item = &ReplayGroup> {
        self.replay_groups()
            .iter()
            .filter(move |group| group.members.contains(&owner))
    }
}

pub(super) fn validate_message_owners(
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
        if matches!(&items[parent_index].1, Item::Message(m) if m.parts.iter().any(|part| matches!(part.content, super::ContentPart::Refusal(_))))
        {
            return Err(GenerationError::InvalidResponse);
        }
    }
    Ok(owners.len() * std::mem::size_of::<(ItemId, ItemId)>())
}
pub(super) fn message_owners(
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
