//! Incremental counterpart of the native JSON projection; no body aggregation here.
use crate::{
    protocol::{CodecError, failure::CarrierReason, openai::GenerationApi},
    semantic::task::generation::*,
};
use std::collections::{BTreeMap, VecDeque};

struct TextPart {
    key: (ItemId, PartId),
    message_order: usize,
    deltas: Vec<StreamEvent>,
    finished: bool,
}

fn order() -> CodecError {
    CodecError::Unsupported(CarrierReason::Unavailable)
}
pub(crate) struct EventProjection {
    losses: Vec<crate::protocol::conversion::generation::Effect>,
    profile: GenerationApi,
    phase_only: bool,
    message: Option<ItemId>,
    reasoning: Vec<ItemId>,
    text_owners: BTreeMap<ItemId, ItemId>,
    text_messages: Vec<(ItemId, bool)>,
    active_text_message: usize,
    text_part: Option<(PartId, PartKind)>,
    text_parts: usize,
    /// Later parts wait for earlier parts to finish. Source reduction bounds their
    /// total retained text; arrival order is not message-content order.
    text_queue: VecDeque<TextPart>,
    buffered_text_bytes: usize,
    text_metadata: bool,
    /// One final metadata snapshot for a non-coalescible text part, emitted only
    /// after its held ValueFinished. Plain absent metadata needs no separate event.
    text_metadata_end: Option<StreamEvent>,
    text_closed: bool,
    pending: Vec<ItemId>,
    /// Chat calls, grouped under the carrier message at the terminal.
    calls: Vec<ItemId>,
    scope: LocalScope,
}
impl EventProjection {
    pub(crate) fn take_evidence(&mut self) -> Vec<crate::protocol::conversion::generation::Effect> {
        std::mem::take(&mut self.losses)
    }
    pub(crate) fn new(profile: GenerationApi) -> Self {
        Self {
            losses: vec![],
            profile,
            phase_only: false,
            message: None,
            reasoning: vec![],
            text_owners: BTreeMap::new(),
            text_messages: vec![],
            active_text_message: 0,
            text_part: None,
            text_parts: 0,
            text_queue: VecDeque::new(),
            buffered_text_bytes: 0,
            text_metadata: false,
            text_metadata_end: None,
            text_closed: false,
            pending: vec![],
            calls: vec![],
            scope: LocalScope::new(2),
        }
    }
    #[cfg(test)]
    pub(crate) fn effects(&self) -> &[crate::protocol::conversion::generation::Effect] {
        &self.losses
    }
    pub(crate) fn for_phase() -> Self {
        Self {
            phase_only: true,
            ..Self::new(GenerationApi::Chat)
        }
    }
    pub(crate) fn needs_source_check(&self) -> bool {
        self.phase_only
    }
    fn record(&mut self, loss: crate::protocol::conversion::generation::Effect) {
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
        self.record(super::envelope_loss(self.profile, "event.project"));
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
                        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                    }
                }
                match &mut event {
                    StreamEvent::ItemStarted { item, kind, .. } => {
                        self.scope = item.scope();
                        match kind {
                            ItemKind::Reasoning if self.profile == GenerationApi::Chat => {
                                if self.message.is_some() || !self.calls.is_empty() {
                                    return Err(order());
                                }
                                self.reasoning.push(*item);
                            }
                            ItemKind::Message { phase } if self.profile == GenerationApi::Chat => {
                                if self.phase_only && phase.take().is_some() {
                                    self.record(crate::protocol::phase::delivery_omission(
                                        "event.project",
                                    ));
                                }
                                if !self.calls.is_empty() {
                                    return Err(CodecError::Unsupported(
                                        CarrierReason::Unavailable,
                                    ));
                                }
                                let owner = *self.message.get_or_insert(*item);
                                if self.text_messages.len() >= MAX_ITEMS {
                                    return Err(CodecError::Limit);
                                }
                                self.text_messages.push((*item, false));
                                self.text_owners.insert(*item, owner);
                                self.close_later(owner);
                                if owner != *item {
                                    self.record(super::envelope_loss(
                                        self.profile,
                                        "event.project",
                                    ));
                                    continue;
                                }
                            }
                            ItemKind::ToolCall { format, .. } => {
                                *format = ArgumentFormat::Raw;
                                if self.profile == GenerationApi::Chat {
                                    self.owner(&mut out);
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
                            if self.text_parts >= MAX_ITEMS {
                                return Err(CodecError::Limit);
                            }
                            let message_order = self
                                .text_messages
                                .iter()
                                .position(|(id, _)| id == item)
                                .ok_or(CodecError::invalid())?;
                            let position = self
                                .text_queue
                                .iter()
                                .position(|p| p.message_order > message_order)
                                .unwrap_or(self.text_queue.len());
                            self.text_queue.insert(
                                position,
                                TextPart {
                                    key: (*item, *part),
                                    message_order,
                                    deltas: vec![],
                                    finished: false,
                                },
                            );
                            self.text_parts += 1;
                            if let Some((_, old_kind)) = self.text_part {
                                if self.text_metadata
                                    || *kind != PartKind::Text
                                    || old_kind != PartKind::Text
                                {
                                    return Err(CodecError::Unsupported(
                                        CarrierReason::Unavailable,
                                    ));
                                }
                                self.record(super::envelope_loss(self.profile, "event.project"));
                                continue;
                            }
                            self.text_part = Some((*part, *kind));
                        }
                        *item = self.text_owners.get(item).copied().unwrap_or(*item);
                    }
                    StreamEvent::PartFinished { item, part }
                        if self.text_owners.contains_key(item) =>
                    {
                        let pending = self
                            .text_queue
                            .iter_mut()
                            .find(|p| p.key == (*item, *part))
                            .ok_or(CodecError::invalid())?;
                        pending.finished = true;
                        self.flush_text(&mut out);
                        continue;
                    }
                    StreamEvent::ValueFinished { item, .. }
                        if self.text_owners.contains_key(item) =>
                    {
                        continue;
                    }
                    StreamEvent::TextMetadata {
                        item,
                        part,
                        annotations,
                        logprobs,
                    } if self.text_owners.contains_key(item) => {
                        if !annotations.is_empty() || !logprobs.is_absent() {
                            *part = self.text_part.ok_or(CodecError::invalid())?.0;
                            *item = self.text_owners[item];
                            self.text_metadata_end = Some(event);
                        }
                        continue;
                    }
                    StreamEvent::Delta {
                        item,
                        part,
                        fragment,
                        ..
                    } if self.text_owners.contains_key(item)
                        && self.text_queue.front().is_some_and(|p| {
                            p.key != (*item, *part) || p.message_order != self.active_text_message
                        }) =>
                    {
                        self.buffered_text_bytes = self
                            .buffered_text_bytes
                            .checked_add(
                                fragment
                                    .len()
                                    .saturating_add(std::mem::size_of::<StreamEvent>()),
                            )
                            .filter(|bytes| *bytes <= MAX_TOTAL_BYTES)
                            .ok_or(CodecError::Limit)?;
                        self.text_queue
                            .iter_mut()
                            .find(|p| p.key == (*item, *part))
                            .ok_or(CodecError::invalid())?
                            .deltas
                            .push(event);
                        continue;
                    }
                    StreamEvent::Delta { item, part, .. }
                    | StreamEvent::LogprobsSnapshot { item, part, .. }
                    | StreamEvent::AnnotationAdded { item, part, .. }
                    | StreamEvent::TextMetadata { item, part, .. }
                    | StreamEvent::ValueFinished { item, part }
                    | StreamEvent::PartFinished { item, part } => {
                        if self.text_owners.contains_key(item) {
                            *part = self.text_part.ok_or(CodecError::invalid())?.0;
                        }
                        *item = self.text_owners.get(item).copied().unwrap_or(*item);
                    }
                    StreamEvent::ItemFinished { item, .. }
                        if self.profile == GenerationApi::Chat =>
                    {
                        if let Some((_, finished)) =
                            self.text_messages.iter_mut().find(|(id, _)| id == item)
                        {
                            *finished = true;
                            while self
                                .text_messages
                                .get(self.active_text_message)
                                .is_some_and(|(_, done)| *done)
                            {
                                self.active_text_message += 1;
                            }
                            self.flush_text(&mut out);
                        }
                        if !self.reasoning.contains(item) {
                            continue;
                        }
                    }
                    StreamEvent::Usage(usage) if !self.phase_only => {
                        let projected = super::client_usage(*usage)?;
                        for loss in super::usage_loss(Some(*usage), projected, "event.project") {
                            self.record(loss);
                        }
                        let Some(projected) = projected else {
                            continue;
                        };
                        *usage = projected;
                    }
                    StreamEvent::UsageReport(report) if !self.phase_only => {
                        report.validate()?;
                        if report.has_unrepresented_details() {
                            self.record(crate::protocol::usage::report_loss("event.project"));
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
                        for loss in super::usage_loss(Some(source), projected, "event.project") {
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
                        if self.profile == GenerationApi::Responses || envelope.members() != members
                        {
                            self.record(super::envelope_loss(self.profile, "event.project"));
                        }
                        continue;
                    }
                    StreamEvent::Progress(_) if !self.phase_only => continue,
                    StreamEvent::Terminal { terminal, .. }
                        if self.profile == GenerationApi::Chat =>
                    {
                        let owner = self.owner(&mut out);
                        self.close_text(&mut out)?;
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
    fn close_text(&mut self, out: &mut Vec<StreamEvent>) -> Result<(), CodecError> {
        if !self.text_queue.is_empty() {
            return Err(order());
        }
        if !self.text_closed
            && let Some((part, _)) = self.text_part
        {
            let item = self.message.expect("text message");
            out.push(StreamEvent::ValueFinished { item, part });
            out.extend(self.text_metadata_end.take());
            out.push(StreamEvent::PartFinished { item, part });
            self.text_closed = true;
        }
        Ok(())
    }
    fn flush_text(&mut self, out: &mut Vec<StreamEvent>) {
        while let Some(next) = self.text_queue.front_mut() {
            if next.message_order > self.active_text_message {
                break;
            }
            for mut event in std::mem::take(&mut next.deltas) {
                if let StreamEvent::Delta {
                    item,
                    part,
                    fragment,
                    ..
                } = &mut event
                {
                    self.buffered_text_bytes -= fragment.len() + std::mem::size_of::<StreamEvent>();
                    *item = self.message.expect("text message");
                    *part = self.text_part.expect("text part").0;
                }
                out.push(event);
            }
            if !next.finished {
                break;
            }
            self.text_queue.pop_front();
        }
    }
}
