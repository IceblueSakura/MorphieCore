//! Incremental counterpart of the native JSON projection; no body aggregation here.
use crate::{
    protocol::{CodecError, openai::Profile},
    semantic::task::generation::*,
};
use std::collections::BTreeMap;

pub(crate) struct EventProjection {
    profile: Profile,
    message: Option<ItemId>,
    reasoning: Option<ItemId>,
    text_owners: BTreeMap<ItemId, ItemId>,
    pending: Vec<ItemId>,
    tools: bool,
    scope: LocalScope,
}
impl EventProjection {
    pub(crate) fn new(profile: Profile) -> Self {
        Self {
            profile,
            message: None,
            reasoning: None,
            text_owners: BTreeMap::new(),
            pending: vec![],
            tools: false,
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
                            if self.reasoning.is_some() || self.message.is_some() || self.tools {
                                return Err(CodecError::Unsupported(
                                    "native block order on Chat".into(),
                                ));
                            }
                            self.reasoning = Some(*item);
                        }
                        ItemKind::Message { .. } if self.profile == Profile::Chat => {
                            if self.tools {
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
                        ItemKind::ToolCall {
                            format, message, ..
                        } => {
                            *format = ArgumentFormat::Raw;
                            if self.profile == Profile::Chat {
                                *message = Some(self.owner(&mut out));
                                self.close_later(*item);
                                self.tools = true;
                            }
                        }
                        _ => {}
                    }
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
                    if Some(*item) != self.reasoning {
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
                    self.owner(&mut out);
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
                }
                _ => {}
            }
            out.push(event);
        }
        Ok(out)
    }
}
