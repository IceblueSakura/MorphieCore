//! Borrowed views of explicitly declared message ownership, never inferred turns.
use super::{
    CallReference, GenerationRequest, GenerationResponse, Item, ItemId, Message, MessageRole,
};

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
