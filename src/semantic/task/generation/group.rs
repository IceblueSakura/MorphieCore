//! Borrowed views of explicitly declared message ownership, never inferred turns.
use super::{
    CallReference, GenerationError, GenerationRequest, GenerationResponse, Item, ItemId,
    LocalScope, MAX_ITEMS, Message, MessageRole,
};
use std::collections::BTreeSet;

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

/// One assistant owner followed by its explicitly attached function calls.
/// Values remain in the final ordered items; no second membership table is stored.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MessageGroup<'a> {
    items: &'a [(ItemId, Item)],
}
impl<'a> MessageGroup<'a> {
    pub fn owner(&self) -> ItemId {
        self.items[0].0
    }
    pub fn message(&self) -> &'a Message {
        let Item::Message(message) = &self.items[0].1 else {
            unreachable!("groups are derived from validated assistant owners")
        };
        message
    }
    /// Owner and attached calls in final semantic order, not wire coordinates.
    pub fn items(&self) -> &'a [(ItemId, Item)] {
        self.items
    }
    pub fn calls(&self) -> impl ExactSizeIterator<Item = CallReference<'a>> + use<'a> {
        self.items[1..].iter().map(|(id, item)| {
            let Item::ToolCall(call) = item else {
                unreachable!("validated group member")
            };
            CallReference {
                item: *id,
                call_id: call.call_id.as_str(),
            }
        })
    }
}
fn groups(mut items: &[(ItemId, Item)]) -> impl Iterator<Item = MessageGroup<'_>> {
    std::iter::from_fn(move || {
        loop {
            let ((owner, item), tail) = items.split_first()?;
            if matches!(item, Item::Message(m) if m.role == MessageRole::Assistant) {
                let calls = tail.iter().take_while(|(_, item)| {
                matches!(item, Item::ToolCall(call) if call.message == Some(*owner))
            }).count();
                let (members, rest) = items.split_at(calls + 1);
                items = rest;
                return Some(MessageGroup { items: members });
            }
            items = tail;
        }
    })
}
impl GenerationRequest {
    /// Only declared membership is visible; standalone calls/reasoning stay independent.
    pub fn message_groups(&self) -> impl Iterator<Item = MessageGroup<'_>> {
        groups(self.items())
    }
}
impl GenerationResponse {
    /// Member lifecycle remains per item. This view never asserts turn completion.
    pub fn message_groups(&self) -> impl Iterator<Item = MessageGroup<'_>> {
        groups(self.items())
    }
}
