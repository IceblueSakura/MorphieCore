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
    text_owners: BTreeMap<ItemId, ItemId>,
    text_part: Option<(PartId, PartKind)>,
    text_parts: usize,
    text_metadata: bool,
    text_closed: bool,
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
            text_owners: BTreeMap::new(),
            text_part: None,
            text_parts: 0,
            text_metadata: false,
            text_closed: false,
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
    pub(crate) fn project(
        &mut self,
        events: &[StreamEvent],
    ) -> Result<Vec<StreamEvent>, CodecError> {
        let checked = self.losses.len();
        let result = (|| {
            let mut out = vec![];
            for event in events {
                let mut event = event.clone();
                let metadata = match &event {
                    StreamEvent::Delta { item, logprobs, .. } => {
                        Some((*item, !logprobs.is_empty()))
                    }
                    StreamEvent::LogprobsSnapshot { item, .. }
                    | StreamEvent::AnnotationAdded { item, .. } => Some((*item, true)),
                    StreamEvent::TextMetadata {
                        item,
                        annotations,
                        logprobs,
                        ..
                    } => Some((*item, !annotations.is_empty() || !logprobs.is_absent())),
                    _ => None,
                };
                if metadata.is_some_and(|(id, active)| active && self.text_owners.contains_key(&id))
                {
                    self.text_metadata = true;
                    if self.text_parts > 1 {
                        return Err(CodecError::Unsupported(
                            "native Chat text coalescing".into(),
                        ));
                    }
                }
                match &mut event {
                    StreamEvent::ItemStarted { item, kind, .. } => {
                        self.scope = item.scope();
                        match kind {
                            ItemKind::Reasoning if self.profile == Profile::Chat => {
                                if self.message.is_some() || !self.calls.is_empty() {
                                    return Err(order());
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
                                    self.close_text(&mut out);
                                    self.close_later(*item);
                                    self.calls.push(*item);
                                }
                            }
                            _ => {}
                        }
                    }
                    StreamEvent::PartStarted { item, part, kind } => {
                        if *kind == PartKind::StructuredArguments {
                            *kind = PartKind::Arguments;
                        }
                        if self.text_owners.contains_key(item) {
                            self.text_parts += 1;
                            if let Some((_, old_kind)) = self.text_part {
                                if self.text_metadata
                                    || *kind != PartKind::Text
                                    || old_kind != PartKind::Text
                                {
                                    return Err(CodecError::Unsupported(
                                        "native Chat text coalescing".into(),
                                    ));
                                }
                                self.record(super::envelope_loss(self.profile, "event.encode"));
                                continue;
                            }
                            self.text_part = Some((*part, *kind));
                        }
                        *item = self.text_owners.get(item).copied().unwrap_or(*item);
                    }
                    StreamEvent::ValueFinished { item, .. }
                    | StreamEvent::PartFinished { item, .. }
                        if self.text_owners.contains_key(item) =>
                    {
                        continue;
                    }
                    StreamEvent::Delta { item, part, .. }
                    | StreamEvent::LogprobsSnapshot { item, part, .. }
                    | StreamEvent::AnnotationAdded { item, part, .. }
                    | StreamEvent::TextMetadata { item, part, .. }
                    | StreamEvent::ValueFinished { item, part }
                    | StreamEvent::PartFinished { item, part } => {
                        if self.text_owners.contains_key(item) {
                            *part = self
                                .text_part
                                .ok_or(CodecError::Invalid("native text owner"))?
                                .0;
                        }
                        *item = self.text_owners.get(item).copied().unwrap_or(*item);
                    }
                    StreamEvent::ItemFinished { item, .. } if self.profile == Profile::Chat => {
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
                    StreamEvent::UsageReport(report) => {
                        report.validate()?;
                        if report.scope != UsageScope::Operation
                            || report.stage != UsageStage::Final
                            || report.accounting != UsageAccounting::Direct
                            || report.details != UsageDetails::default()
                        {
                            self.record(crate::protocol::usage::report_loss("event.encode"));
                        }
                        if report.scope != UsageScope::Operation
                            || report.stage != UsageStage::Final
                        {
                            continue;
                        }
                        let Some(source) = report.scalar_summary() else {
                            continue;
                        };
                        let projected = super::client_usage(source)?;
                        if let Some(loss) =
                            super::usage_loss(Some(source), projected, "event.encode")
                        {
                            self.record(loss);
                        }
                        let Some(usage) = projected else {
                            continue;
                        };
                        event = StreamEvent::Usage(usage);
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
                        self.close_text(&mut out);
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
    fn close_text(&mut self, out: &mut Vec<StreamEvent>) {
        if !self.text_closed
            && let Some((part, _)) = self.text_part
        {
            let item = self.message.expect("text message");
            out.push(StreamEvent::ValueFinished { item, part });
            out.push(StreamEvent::PartFinished { item, part });
            self.text_closed = true;
        }
    }
}
