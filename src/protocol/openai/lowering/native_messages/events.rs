//! Incremental counterpart of the native JSON projection; no body aggregation here.
use crate::{
    protocol::{CodecError, openai::Profile},
    semantic::task::generation::*,
};
use std::collections::BTreeMap;

fn order() -> CodecError {
    CodecError::Unsupported("native block order on Chat".into())
}
pub(crate) struct EventProjection {
    profile: Profile,
    message: Option<ItemId>,
    reasoning: Vec<ItemId>,
    /// A reasoning item opened after content, held until it proves to be anchored.
    late: Option<StreamEvent>,
    text_owners: BTreeMap<ItemId, ItemId>,
    pending: Vec<ItemId>,
    /// Chat calls, grouped under the carrier message at the terminal.
    calls: Vec<ItemId>,
    scope: LocalScope,
}
impl EventProjection {
    pub(crate) fn new(profile: Profile) -> Self {
        Self {
            profile,
            message: None,
            reasoning: vec![],
            late: None,
            text_owners: BTreeMap::new(),
            pending: vec![],
            calls: vec![],
            scope: LocalScope::new(2),
        }
    }
    fn close_later(&mut self, item: ItemId) {
        if !self.pending.contains(&item) {
            self.pending.push(item);
        }
    }
    fn owner(&mut self, out: &mut Vec<StreamEvent>) -> ItemId {
        if let Some(owner) = self.message {
            return owner;
        }
        // Native block IDs are 1..=MAX_ITEMS. This is only a target carrier.
        let owner = ItemId::scoped(self.scope, MAX_ITEMS as u64 + 1);
        self.message = Some(owner);
        out.push(StreamEvent::ItemStarted {
            item: owner,
            kind: ItemKind::Message { phase: None },
            replay: None,
        });
        self.close_later(owner);
        owner
    }
    fn late_item(&self) -> Option<ItemId> {
        match &self.late {
            Some(StreamEvent::ItemStarted { item, .. }) => Some(*item),
            _ => None,
        }
    }
    pub(crate) fn project(
        &mut self,
        events: &[StreamEvent],
    ) -> Result<Vec<StreamEvent>, CodecError> {
        let mut out = vec![];
        for event in events {
            let mut event = event.clone();
            match &mut event {
                StreamEvent::ItemStarted { item, kind, .. } => {
                    self.scope = item.scope();
                    match kind {
                        ItemKind::Reasoning if self.profile == Profile::Chat => {
                            if self.message.is_some() || !self.calls.is_empty() {
                                if self.late.is_some() {
                                    return Err(order());
                                }
                                self.late = Some(event.clone());
                                continue;
                            }
                            self.reasoning.push(*item);
                        }
                        ItemKind::Message { .. } if self.profile == Profile::Chat => {
                            if !self.calls.is_empty() {
                                return Err(CodecError::Unsupported(
                                    "native block order on Chat".into(),
                                ));
                            }
                            let owner = *self.message.get_or_insert(*item);
                            self.text_owners.insert(*item, owner);
                            self.close_later(owner);
                            if owner != *item {
                                continue;
                            }
                        }
                        ItemKind::ToolCall { format, .. } => {
                            *format = ArgumentFormat::Raw;
                            if self.profile == Profile::Chat {
                                self.owner(&mut out);
                                self.close_later(*item);
                                self.calls.push(*item);
                            }
                        }
                        _ => {}
                    }
                }
                StreamEvent::PartStarted { item, .. } | StreamEvent::Delta { item, .. }
                    if self.late_item() == Some(*item) =>
                {
                    return Err(order());
                }
                StreamEvent::ItemFinished {
                    item,
                    replay: Some(replay),
                    ..
                } if self.late_item() == Some(*item) => {
                    let anchored = super::anchored(&ReasoningItem {
                        parts: vec![],
                        status: ItemLifecycle::Completed,
                        replay: Some(replay.value.clone()),
                    });
                    if !anchored {
                        return Err(order());
                    }
                    out.extend(self.late.take());
                    self.reasoning.push(*item);
                }
                StreamEvent::PartStarted { item, kind, .. } => {
                    if *kind == PartKind::StructuredArguments {
                        *kind = PartKind::Arguments;
                    }
                    *item = self.text_owners.get(item).copied().unwrap_or(*item);
                }
                StreamEvent::Delta { item, .. }
                | StreamEvent::ValueFinished { item, .. }
                | StreamEvent::PartFinished { item, .. } => {
                    *item = self.text_owners.get(item).copied().unwrap_or(*item);
                }
                StreamEvent::ItemFinished { item, .. } if self.profile == Profile::Chat => {
                    if self.late_item() == Some(*item) {
                        return Err(order());
                    }
                    if !self.reasoning.contains(item) {
                        continue;
                    }
                }
                StreamEvent::Usage(usage) => {
                    let Some(projected) = super::client_usage(*usage)? else {
                        continue;
                    };
                    *usage = projected;
                }
                StreamEvent::Progress(_) | StreamEvent::MessageEnvelope(_) => continue,
                StreamEvent::Terminal { terminal, .. } if self.profile == Profile::Chat => {
                    let owner = self.owner(&mut out);
                    for item in self.pending.drain(..) {
                        out.push(StreamEvent::ItemFinished {
                            item,
                            status: if *terminal == StreamTerminal::Incomplete {
                                ItemLifecycle::Incomplete
                            } else {
                                ItemLifecycle::Completed
                            },
                            replay: None,
                        });
                    }
                    if !self.calls.is_empty() {
                        let mut members = vec![owner];
                        members.extend(&self.calls);
                        out.push(StreamEvent::MessageEnvelope(MessageEnvelope::new(
                            GroupId::new(owner.scope(), owner.get()),
                            MessageEnvelopeRole::Assistant,
                            members,
                        )?));
                    }
                }
                _ => {}
            }
            out.push(event);
        }
        Ok(out)
    }
}
