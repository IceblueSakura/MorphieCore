//! Explicit native message containers; result and replay edges remain independent.
use crate::semantic::task::generation::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageEnvelopeRole {
    User,
    Assistant,
    Tool,
}

/// Ordered references only. `before` anchors an empty container, never an array index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageEnvelope {
    id: GroupId,
    role: MessageEnvelopeRole,
    members: Vec<ItemId>,
    before: Option<ItemId>,
}
impl MessageEnvelope {
    pub fn new(
        id: GroupId,
        role: MessageEnvelopeRole,
        members: Vec<ItemId>,
    ) -> Result<Self, GenerationError> {
        if members.len() > MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        if members.is_empty() || members.iter().collect::<BTreeSet<_>>().len() != members.len() {
            return Err(GenerationError::InvalidMessageGroup);
        }
        Ok(Self {
            id,
            role,
            members,
            before: None,
        })
    }
    /// `None` places an empty container after all selected items.
    pub fn empty(id: GroupId, role: MessageEnvelopeRole, before: Option<ItemId>) -> Self {
        Self {
            id,
            role,
            members: vec![],
            before,
        }
    }
    pub fn id(&self) -> GroupId {
        self.id
    }
    pub fn role(&self) -> MessageEnvelopeRole {
        self.role
    }
    pub fn members(&self) -> &[ItemId] {
        &self.members
    }
    pub fn before(&self) -> Option<ItemId> {
        self.before
    }
}

/// Validated envelopes of one request or response. Protocol views, such as Chat's
/// call ownership, are derived at their boundary.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::semantic::task::generation) struct MessageEnvelopes {
    values: Vec<MessageEnvelope>,
}
impl MessageEnvelopes {
    pub fn values(&self) -> &[MessageEnvelope] {
        &self.values
    }
    pub fn new(
        items: &[(ItemId, Item)],
        values: Vec<MessageEnvelope>,
    ) -> Result<Self, GenerationError> {
        if values.len() > MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        let mut ids = BTreeSet::new();
        let mut members = BTreeSet::new();
        for group in &values {
            if !ids.insert(group.id) {
                return Err(GenerationError::InvalidMessageGroup);
            }
            let anchor = group
                .before
                .map(|before| {
                    items
                        .iter()
                        .position(|(id, _)| *id == before)
                        .ok_or(GenerationError::InvalidMessageGroup)
                })
                .transpose()?;
            let mut previous = None;
            for member in &group.members {
                if !members.insert(*member) {
                    return Err(GenerationError::InvalidMessageGroup);
                }
                if members.len() > MAX_ITEMS {
                    return Err(GenerationError::Limit);
                }
                let index = items
                    .iter()
                    .position(|(id, _)| id == member)
                    .ok_or(GenerationError::InvalidMessageGroup)?;
                if previous.is_some_and(|old| old >= index) || anchor.is_some_and(|a| a > index) {
                    return Err(GenerationError::InvalidMessageGroup);
                }
                let valid = match (&group.role, &items[index].1) {
                    (MessageEnvelopeRole::User, Item::Message(m)) => m.role == MessageRole::User,
                    (MessageEnvelopeRole::Assistant, Item::Message(m)) => {
                        m.role == MessageRole::Assistant
                    }
                    (
                        MessageEnvelopeRole::User | MessageEnvelopeRole::Tool,
                        Item::ToolResult(_) | Item::CustomResult(_),
                    ) => true,
                    (
                        MessageEnvelopeRole::Assistant,
                        Item::ToolCall(_)
                        | Item::CustomCall(_)
                        | Item::Reasoning(_)
                        | Item::ProviderTool(_),
                    ) => true,
                    _ => false,
                };
                if !valid {
                    return Err(GenerationError::InvalidMessageGroup);
                }
                previous = Some(index);
            }
        }
        Ok(Self { values })
    }
    pub fn validate(&self, items: &[(ItemId, Item)]) -> Result<usize, GenerationError> {
        Self::new(items, self.values.clone())?;
        Ok(std::mem::size_of_val(self.values.as_slice())
            + self
                .values
                .iter()
                .map(|g| std::mem::size_of_val(g.members.as_slice()))
                .sum::<usize>())
    }
    pub fn edited(&self, items: &[(ItemId, Item)]) -> Result<Self, GenerationError> {
        let mut values = self.values.clone();
        values.retain_mut(|group| {
            let was_empty = group.members.is_empty();
            group
                .members
                .retain(|id| items.iter().any(|(owner, _)| owner == id));
            was_empty || !group.members.is_empty()
        });
        Self::new(items, values)
    }
    pub fn revise(&mut self, old: ItemId, new: ItemId) {
        for group in &mut self.values {
            for member in &mut group.members {
                if *member == old {
                    *member = new;
                }
            }
            if group.before == Some(old) {
                group.before = Some(new);
            }
        }
    }
}
