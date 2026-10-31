//! Reorder already validated content events, not source grammar or semantic facts.
//! Visible owners stay ahead of successors until their actual finish; text parts
//! have the same barrier. Decided omissions retain only bounded lifecycle IDs.
use super::*;
use crate::semantic::task::generation::{ItemLifecycle, MAX_ITEMS, MAX_TOTAL_BYTES};
use std::collections::VecDeque;

#[derive(Clone, Copy)]
enum Kind {
    Text,
    Thinking,
    Tool,
}
struct Part {
    events: VecDeque<Box<StreamEvent>>,
    closed: bool,
}
struct Owner {
    kind: Kind,
    omitted: bool,
    summary_parts: usize,
    summary_only: bool,
    summary_nonempty: bool,
    signature_redacted: Option<bool>,
    start: Option<Box<StreamEvent>>,
    events: VecDeque<Box<StreamEvent>>,
    parts: BTreeMap<PartId, Box<Part>>,
    order: VecDeque<PartId>,
    finish: Option<Box<StreamEvent>>,
}
// Box large values so spare map/deque capacity holds pointers, not event/owner
// payloads. Per-entry reserve also covers sparse map nodes and ordering slots.
const OWNER_BYTES: usize = size_of::<Owner>() + 512;
const PART_BYTES: usize = size_of::<Part>() + 512;
const EVENT_BYTES: usize = 2 * size_of::<StreamEvent>() + 64;
const OMITTED_BYTES: usize = size_of::<ItemId>() + 64;

fn pop<T>(queue: &mut VecDeque<T>) -> Option<T> {
    let value = queue.pop_front();
    // Live-event charges cover at most twice their slots. A drained but still
    // open owner must not retain an uncharged high-water allocation.
    if queue.capacity() > queue.len().saturating_mul(2) {
        queue.shrink_to_fit();
    }
    value
}

fn bytes(event: &StreamEvent) -> usize {
    let replay = match event {
        StreamEvent::ItemStarted { replay, .. } | StreamEvent::ItemFinished { replay, .. } => {
            replay.as_ref().map_or(0, |r| {
                r.value.as_str().len() + r.origin.as_ref().map_or(0, |o| o.as_str().len())
            })
        }
        _ => 0,
    };
    EVENT_BYTES
        + replay
        + match event {
            StreamEvent::ItemStarted {
                kind: ItemKind::ToolCall { call_id, name, .. },
                ..
            } => call_id.as_str().len() + name.as_str().len(),
            StreamEvent::Delta { fragment, .. } => fragment.len(),
            _ => 0,
        }
}

pub(super) enum Ready {
    Event(Box<StreamEvent>),
    OmitReasoning(ItemId),
    // Observed incomplete-owner or EOF wire closure, never semantic completion.
    ClosePart(ItemId, PartId),
    CloseOwner(ItemId),
}
#[derive(Default)]
pub(super) struct Schedule {
    owners: BTreeMap<ItemId, Box<Owner>>,
    omitted: BTreeSet<ItemId>,
    order: VecDeque<ItemId>,
    bytes: usize,
    events: usize,
}
impl Schedule {
    fn charge(&mut self, amount: usize) -> Result<(), CodecError> {
        self.bytes = self.bytes.checked_add(amount).ok_or(CodecError::Limit)?;
        if self.bytes > MAX_TOTAL_BYTES {
            return Err(CodecError::Limit);
        }
        Ok(())
    }
    fn charge_event(
        &mut self,
        event: &StreamEvent,
        limits: crate::protocol::stream::SseLimits,
    ) -> Result<(), CodecError> {
        if self.events >= limits.max_events {
            return Err(CodecError::Limit);
        }
        self.charge(bytes(event))?;
        self.events += 1;
        Ok(())
    }
    /// Return false for reports and other events that do not depend on content order.
    pub(super) fn push(
        &mut self,
        event: &StreamEvent,
        limits: crate::protocol::stream::SseLimits,
    ) -> Result<bool, CodecError> {
        if let StreamEvent::ItemStarted { item, kind, replay } = event {
            let kind = match kind {
                ItemKind::Message { phase: None } => Kind::Text,
                ItemKind::Reasoning => Kind::Thinking,
                ItemKind::ToolCall {
                    call_id,
                    name,
                    context,
                    ..
                } => {
                    if replay.is_some() {
                        return Err(CodecError::Unsupported(CarrierReason::Replay));
                    }
                    if !context.is_direct() || context.alias_domain.is_some() {
                        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                    }
                    super::super::projection::tool_labels(call_id.as_str(), name.as_str())?;
                    Kind::Tool
                }
                _ => return Err(CodecError::Unsupported(CarrierReason::Unavailable)),
            };
            if self.owners.contains_key(item) || self.omitted.contains(item) {
                return Err(CodecError::invalid());
            }
            if self.owners.len() + self.omitted.len() >= MAX_ITEMS {
                return Err(CodecError::Limit);
            }
            self.charge(OWNER_BYTES)?;
            self.charge_event(event, limits)?;
            self.order.push_back(*item);
            self.owners.insert(
                *item,
                Box::new(Owner {
                    kind,
                    omitted: false,
                    summary_parts: 0,
                    summary_only: true,
                    summary_nonempty: false,
                    signature_redacted: replay
                        .as_ref()
                        .filter(|r| !r.value.as_str().is_empty())
                        .map(|r| super::super::projection::redacted(r.value.as_str())),
                    start: Some(Box::new(event.clone())),
                    events: VecDeque::new(),
                    parts: BTreeMap::new(),
                    order: VecDeque::new(),
                    finish: None,
                }),
            );
            return Ok(true);
        }
        let (item, part) = match event {
            StreamEvent::PartStarted { item, part, .. }
            | StreamEvent::PartFinished { item, part }
            | StreamEvent::PartInterrupted { item, part }
            | StreamEvent::Delta { item, part, .. } => (*item, Some(*part)),
            StreamEvent::ItemFinished { item, .. } => (*item, None),
            _ => return Ok(false),
        };
        if self.omitted.contains(&item) {
            if matches!(event, StreamEvent::ItemFinished { .. }) {
                self.omitted.remove(&item);
                self.bytes -= OMITTED_BYTES;
            }
            return Ok(true);
        }
        let owner = self.owners.get(&item).ok_or(CodecError::invalid())?;
        if owner.finish.is_some() {
            return Err(CodecError::invalid());
        }
        if let StreamEvent::Delta {
            fragment, logprobs, ..
        } = event
        {
            if !logprobs.is_empty() {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
            if fragment.is_empty() {
                return Ok(true);
            }
        }
        if let StreamEvent::PartStarted { kind, .. } = event {
            match owner.kind {
                Kind::Thinking if !matches!(kind, PartKind::Summary | PartKind::ReasoningText) => {
                    return Err(CodecError::Unsupported(CarrierReason::Reasoning));
                }
                Kind::Text if !matches!(kind, PartKind::Text | PartKind::Refusal) => {
                    return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                }
                Kind::Tool
                    if !matches!(kind, PartKind::Arguments | PartKind::StructuredArguments) =>
                {
                    return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                }
                _ => {}
            }
        }
        let text = matches!(owner.kind, Kind::Text);
        if matches!(owner.kind, Kind::Tool)
            && matches!(
                event,
                StreamEvent::ItemFinished {
                    replay: Some(_),
                    ..
                }
            )
        {
            return Err(CodecError::Unsupported(CarrierReason::Replay));
        }
        if part.is_none()
            && text
            && owner.parts.values().any(|p| !p.closed)
            && !matches!(
                event,
                StreamEvent::ItemFinished {
                    status: ItemLifecycle::Incomplete,
                    ..
                }
            )
        {
            return Err(CodecError::invalid());
        }
        let owner = self.owners.get_mut(&item).expect("checked owner");
        if matches!(owner.kind, Kind::Thinking) {
            match event {
                StreamEvent::PartStarted { kind, .. } => {
                    owner.summary_parts += 1;
                    owner.summary_only &= *kind == PartKind::Summary;
                }
                StreamEvent::Delta { .. } => owner.summary_nonempty = true,
                StreamEvent::ItemFinished { replay, .. } => {
                    owner.signature_redacted = replay
                        .as_ref()
                        .filter(|replay| !replay.value.as_str().is_empty())
                        .map(|replay| super::super::projection::redacted(replay.value.as_str()));
                }
                _ => {}
            }
            let closed_empty = matches!(
                event,
                StreamEvent::PartFinished { .. } | StreamEvent::PartInterrupted { .. }
            ) && !owner.summary_nonempty;
            let finished_uncarried = matches!(event, StreamEvent::ItemFinished { .. })
                && !super::super::projection::reasoning_carried(
                    owner.signature_redacted,
                    owner.summary_parts,
                    owner.summary_only,
                    owner.summary_nonempty,
                );
            if !owner.omitted
                && (!owner.summary_only
                    || owner.summary_parts > 1
                    || owner.signature_redacted == Some(true) && owner.summary_parts != 0
                    || closed_empty
                    || finished_uncarried)
            {
                owner.omitted = true;
                for retained in owner.start.iter().chain(owner.events.iter()) {
                    self.bytes -= bytes(retained);
                    self.events -= 1;
                }
                owner.start = None;
                owner.events = VecDeque::new();
            }
            if owner.omitted && !matches!(event, StreamEvent::ItemFinished { .. }) {
                return Ok(true);
            }
        }
        self.charge_event(event, limits)?;
        if text && matches!(event, StreamEvent::PartStarted { .. }) {
            self.charge(PART_BYTES)?;
        }
        let owner = self.owners.get_mut(&item).expect("checked owner");
        if let Some(part) = part {
            if text {
                if matches!(event, StreamEvent::PartStarted { .. }) {
                    if owner.parts.contains_key(&part) {
                        return Err(CodecError::invalid());
                    }
                    if owner.parts.len() >= MAX_ITEMS {
                        return Err(CodecError::Limit);
                    }
                    owner.order.push_back(part);
                    owner.parts.insert(
                        part,
                        Box::new(Part {
                            events: VecDeque::new(),
                            closed: false,
                        }),
                    );
                }
                let pending = owner.parts.get_mut(&part).ok_or(CodecError::invalid())?;
                if pending.closed {
                    return Err(CodecError::invalid());
                }
                pending.closed = matches!(event, StreamEvent::PartFinished { .. });
                pending.events.push_back(Box::new(event.clone()));
            } else {
                owner.events.push_back(Box::new(event.clone()));
            }
        } else {
            owner.finish = Some(Box::new(event.clone()));
        }
        Ok(true)
    }
    pub(super) fn pop(&mut self, terminal: bool) -> Option<Ready> {
        if terminal {
            self.bytes -= self.omitted.len() * OMITTED_BYTES;
            self.omitted.clear();
        }
        let item = *self.order.front()?;
        let owner = self.owners.get_mut(&item).expect("ordered owner");
        if owner.omitted {
            let owner = self.owners.remove(&item).expect("ordered owner");
            if let Some(finish) = owner.finish {
                self.bytes -= bytes(&finish);
                self.events -= 1;
            } else if !terminal {
                self.omitted.insert(item);
                self.bytes += OMITTED_BYTES;
            }
            self.bytes -= OWNER_BYTES;
            pop(&mut self.order);
            return Some(Ready::OmitReasoning(item));
        }
        if matches!(owner.kind, Kind::Thinking) {
            // A later signature cannot retract a published block. Keep the
            // existing bounded owner queue until its carrier shape is known.
            if owner.finish.is_none() && !terminal {
                return None;
            }
            if !super::super::projection::reasoning_carried(
                owner.signature_redacted,
                owner.summary_parts,
                owner.summary_only,
                owner.summary_nonempty,
            ) {
                let owner = self.owners.remove(&item).expect("ordered owner");
                for event in owner
                    .start
                    .iter()
                    .chain(owner.events.iter())
                    .chain(owner.finish.iter())
                {
                    self.bytes -= bytes(event);
                    self.events -= 1;
                }
                self.bytes -= OWNER_BYTES;
                pop(&mut self.order);
                return Some(Ready::OmitReasoning(item));
            }
        }
        let close_parts = terminal
            || matches!(
                owner.finish.as_deref(),
                Some(StreamEvent::ItemFinished {
                    status: ItemLifecycle::Incomplete,
                    ..
                })
            );
        let event = if let Some(start) = owner.start.take() {
            start
        } else if let Some(event) = pop(&mut owner.events) {
            event
        } else {
            while let Some(part) = owner.order.front().copied() {
                let pending = owner.parts.get_mut(&part).expect("ordered part");
                if let Some(event) = pop(&mut pending.events) {
                    self.bytes -= bytes(&event);
                    self.events -= 1;
                    return Some(Ready::Event(event));
                }
                if !pending.closed && !close_parts {
                    return None;
                }
                let closed = pending.closed;
                pop(&mut owner.order);
                owner.parts.remove(&part);
                self.bytes -= PART_BYTES;
                if !closed {
                    return Some(Ready::ClosePart(item, part));
                }
            }
            if owner.finish.is_none() && !terminal {
                return None;
            }
            let finish = owner.finish.take();
            pop(&mut self.order);
            self.owners.remove(&item);
            self.bytes -= OWNER_BYTES;
            match finish {
                Some(event) => event,
                None => return Some(Ready::CloseOwner(item)),
            }
        };
        self.bytes -= bytes(&event);
        self.events -= 1;
        Some(Ready::Event(event))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_uncarried_reasoning_keeps_only_bounded_lifecycle_state() {
        let mut schedule = Schedule::default();
        let limits = crate::protocol::stream::SseLimits::default();
        let item = ItemId::new(1);
        let part = PartId::new(1);
        for event in [
            StreamEvent::ItemStarted {
                item,
                kind: ItemKind::Reasoning,
                replay: None,
            },
            StreamEvent::PartStarted {
                item,
                part,
                kind: PartKind::ReasoningText,
            },
        ] {
            schedule.push(&event, limits).unwrap();
        }
        assert!(matches!(schedule.pop(false), Some(Ready::OmitReasoning(id)) if id == item));
        let retained = schedule.bytes;
        let delta = StreamEvent::Delta {
            item,
            part,
            fragment: "x".into(),
            logprobs: vec![],
        };
        for _ in 0..200_000 {
            schedule.push(&delta, limits).unwrap();
        }
        assert_eq!(schedule.bytes, retained);
        assert_eq!(schedule.events, 0);
        assert!(schedule.pop(false).is_none());
        // The invisible owner does not hold back a visible successor.
        let next = ItemId::new(2);
        schedule
            .push(
                &StreamEvent::ItemStarted {
                    item: next,
                    kind: ItemKind::Message { phase: None },
                    replay: None,
                },
                limits,
            )
            .unwrap();
        assert!(matches!(schedule.pop(false), Some(Ready::Event(event))
            if matches!(*event, StreamEvent::ItemStarted { item, .. } if item == next)));
        schedule
            .push(
                &StreamEvent::ItemFinished {
                    item,
                    status: ItemLifecycle::Completed,
                    replay: None,
                },
                limits,
            )
            .unwrap();
        assert!(schedule.omitted.is_empty());
        assert!(schedule.bytes < retained + OWNER_BYTES);
    }

    #[test]
    fn small_blocked_queues_charge_their_allocated_slots() {
        let mut schedule = Schedule::default();
        let limits = crate::protocol::stream::SseLimits::default();
        schedule
            .push(
                &StreamEvent::ItemStarted {
                    item: ItemId::new(1),
                    kind: ItemKind::Message { phase: None },
                    replay: None,
                },
                limits,
            )
            .unwrap();
        for part in 1..=100 {
            schedule
                .push(
                    &StreamEvent::PartStarted {
                        item: ItemId::new(1),
                        part: PartId::new(part),
                        kind: PartKind::Text,
                    },
                    limits,
                )
                .unwrap();
        }
        let slots: usize = schedule.owners[&ItemId::new(1)]
            .parts
            .values()
            .map(|part| part.events.capacity() * size_of_val(part.events.front().unwrap()))
            .sum();
        assert!(
            schedule.bytes >= slots,
            "retained slot allocation exceeds its charged budget"
        );
    }

    #[test]
    fn draining_an_unfinished_part_releases_queued_slot_capacity() {
        let mut schedule = Schedule::default();
        let limits = crate::protocol::stream::SseLimits::default();
        for item in [1, 2] {
            schedule
                .push(
                    &StreamEvent::ItemStarted {
                        item: ItemId::new(item),
                        kind: ItemKind::Message { phase: None },
                        replay: None,
                    },
                    limits,
                )
                .unwrap();
        }
        schedule
            .push(
                &StreamEvent::PartStarted {
                    item: ItemId::new(2),
                    part: PartId::new(1),
                    kind: PartKind::Text,
                },
                limits,
            )
            .unwrap();
        for _ in 0..1024 {
            schedule
                .push(
                    &StreamEvent::Delta {
                        item: ItemId::new(2),
                        part: PartId::new(1),
                        fragment: "x".into(),
                        logprobs: vec![],
                    },
                    limits,
                )
                .unwrap();
        }
        schedule
            .push(
                &StreamEvent::ItemFinished {
                    item: ItemId::new(1),
                    status: crate::semantic::task::generation::ItemLifecycle::Completed,
                    replay: None,
                },
                limits,
            )
            .unwrap();
        while schedule.pop(false).is_some() {}
        let part = &schedule.owners[&ItemId::new(2)].parts[&PartId::new(1)];
        assert_eq!(
            part.events.capacity(),
            0,
            "empty live part must release its old high-water allocation"
        );
        assert!(!part.closed);
        assert_eq!(schedule.events, 0);
    }
}
