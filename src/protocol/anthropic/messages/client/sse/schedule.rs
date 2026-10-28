//! Reorder already validated content events, not source grammar or semantic facts.
//! An owner stays ahead of its successors until its actual finish; text parts have
//! the same barrier independently. Only blocked events are retained between calls.
use super::*;
use crate::semantic::task::generation::{MAX_ITEMS, MAX_TOTAL_BYTES};
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
    summary_started: bool,
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
    // EOF-authorized wire closure; never synthesize semantic completion events.
    ClosePart(ItemId, PartId),
    CloseOwner(ItemId),
}
#[derive(Default)]
pub(super) struct Schedule {
    owners: BTreeMap<ItemId, Box<Owner>>,
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
            if self.owners.contains_key(item) {
                return Err(CodecError::invalid());
            }
            if self.owners.len() >= MAX_ITEMS {
                return Err(CodecError::Limit);
            }
            self.charge(OWNER_BYTES)?;
            self.charge_event(event, limits)?;
            self.order.push_back(*item);
            self.owners.insert(
                *item,
                Box::new(Owner {
                    kind,
                    summary_started: false,
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
            | StreamEvent::Delta { item, part, .. } => (*item, Some(*part)),
            StreamEvent::ItemFinished { item, .. } => (*item, None),
            _ => return Ok(false),
        };
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
                Kind::Thinking if *kind != PartKind::Summary || owner.summary_started => {
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
        if let StreamEvent::ItemFinished { replay, .. } = event
            && matches!(owner.kind, Kind::Thinking)
        {
            let signature = match replay {
                Some(replay) => (!replay.value.as_str().is_empty())
                    .then(|| super::super::projection::redacted(replay.value.as_str())),
                None => owner.signature_redacted,
            };
            if signature.is_none()
                || owner.summary_started && (!owner.summary_nonempty || signature == Some(true))
            {
                return Err(CodecError::Unsupported(CarrierReason::Replay));
            }
        }
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
        if part.is_none() && text && owner.parts.values().any(|p| !p.closed) {
            // Preserve explicit incomplete-item admission; terminal-only closure is separate.
            return Err(CodecError::invalid());
        }
        self.charge_event(event, limits)?;
        if text && matches!(event, StreamEvent::PartStarted { .. }) {
            self.charge(PART_BYTES)?;
        }
        let owner = self.owners.get_mut(&item).expect("checked owner");
        if matches!(owner.kind, Kind::Thinking) && matches!(event, StreamEvent::Delta { .. }) {
            owner.summary_nonempty = true;
        }
        if matches!(owner.kind, Kind::Thinking) && matches!(event, StreamEvent::PartStarted { .. })
        {
            owner.summary_started = true;
        }
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
        let item = *self.order.front()?;
        let owner = self.owners.get_mut(&item).expect("ordered owner");
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
                if !pending.closed && !terminal {
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
