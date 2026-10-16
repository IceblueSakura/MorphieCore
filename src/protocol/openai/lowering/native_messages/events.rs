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
    losses: Vec<crate::lowering::generation::LossEvidence>,
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
            losses: vec![],
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
    pub(crate) fn losses(&self) -> &[crate::lowering::generation::LossEvidence] {
        &self.losses
    }
    fn record(&mut self, loss: crate::lowering::generation::LossEvidence) {
        if !self.losses.contains(&loss) {
            self.losses.push(loss);
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
        self.record(super::envelope_loss(self.profile, "event.encode"));
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
        let checked = self.losses.len();
        let result = (|| {
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
                                    self.record(super::envelope_loss(self.profile, "event.encode"));
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
                        let projected = super::client_usage(*usage)?;
                        if let Some(loss) =
                            super::usage_loss(Some(*usage), projected, "event.encode")
                        {
                            self.record(loss);
                        }
                        let Some(projected) = projected else {
                            continue;
                        };
                        *usage = projected;
                    }
                    StreamEvent::MessageEnvelope(envelope) => {
                        let members: Vec<_> = self
                            .message
                            .into_iter()
                            .chain(self.calls.iter().copied())
                            .collect();
                        if self.profile == Profile::Responses || envelope.members() != members {
                            self.record(super::envelope_loss(self.profile, "event.encode"));
                        }
                        continue;
                    }
                    StreamEvent::Progress(_) => continue,
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
        })();
        if result.is_err() {
            self.losses.truncate(checked);
        }
        result
    }
}
