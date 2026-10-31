//! Native SSE for Messages clients from the shared stream events of any upstream.
//! Reasoning owners wait in the bounded schedule until their carrier is known.
//! An opaque value (already sealed for the client) arrives as one `signature_delta`.
//! A reasoning block opens on
//! its first delta or at its end, so a redacted value can still open `redacted_thinking`.
use crate::{
    protocol::{
        CodecError,
        failure::{CarrierReason, ShapeReason},
    },
    semantic::{
        context::ServiceClass,
        task::generation::{
            InteractionProgress, ItemId, ItemKind, Outcome, PartId, PartKind, StreamEvent,
            StreamTerminal, UsageReport, UsageScope, UsageStage,
        },
        value::Presence,
    },
};
use bytes::Bytes;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
mod schedule;

enum OpenItem {
    Text,
    Thinking {
        index: Option<usize>,
        signature: Option<String>,
        parts: usize,
        active_part: Option<PartId>,
        bytes: usize,
    },
    ToolUse {
        index: usize,
        /// SDK incremental JSON parsers reject a standalone whitespace prefix.
        prefix: String,
        arguments_started: bool,
    },
}
pub struct ClientEventEncoder {
    schedule: schedule::Schedule,
    notice: Option<crate::protocol::diagnostics::AdaptationNotice>,
    losses: Vec<crate::protocol::conversion::generation::Effect>,
    id: String,
    model: String,
    started: bool,
    seen_start: bool,
    finished: bool,
    rejected: bool,
    service_class: Presence<ServiceClass>,
    open: BTreeMap<ItemId, OpenItem>,
    owners_seen: BTreeSet<ItemId>,
    text_parts: BTreeMap<(ItemId, PartId), usize>,
    next: usize,
    calls: bool,
    incomplete_calls: bool,
    progress: InteractionProgress,
    usage: Option<crate::protocol::anthropic::messages::Usage>,
    delivery_enabled: bool,
    delivery_usage: Option<crate::semantic::task::generation::Usage>,
    /// Already published optional observations cannot be erased by absent delta fields.
    opening_usage: Option<crate::protocol::anthropic::messages::Usage>,
    limits: crate::protocol::stream::SseLimits,
    wire_bytes: usize,
    events: usize,
}
fn thinking_block() -> Value {
    json!({"type":"thinking","thinking":"","signature":""})
}
// Bound each frame before serializing and each batch while draining queued owners.
struct Frames {
    values: Vec<Bytes>,
    limits: crate::protocol::stream::SseLimits,
    events: usize,
    wire_bytes: usize,
}
impl Frames {
    fn new(encoder: &ClientEventEncoder) -> Self {
        Self {
            values: vec![],
            limits: encoder.limits,
            events: encoder.events,
            wire_bytes: encoder.wire_bytes,
        }
    }
    fn available(&self) -> usize {
        self.limits
            .max_event_bytes
            .min(self.limits.max_wire_bytes.saturating_sub(self.wire_bytes))
    }
    fn push(&mut self, value: &Value) -> Result<(), CodecError> {
        use crate::protocol::failure::Resource;
        if self.events >= self.limits.max_events {
            return Err(CodecError::Resource(Resource::Events));
        }
        let kind = value["type"].as_str().ok_or(CodecError::invalid())?;
        let overhead = kind
            .len()
            .checked_add(16)
            .ok_or(CodecError::Resource(Resource::Bytes))?;
        let available = self
            .available()
            .checked_sub(overhead)
            .ok_or(CodecError::Resource(Resource::Bytes))?;
        let size = crate::semantic::value::json_size(value, available)
            .map_err(|_| CodecError::Resource(Resource::Bytes))?;
        self.values
            .push(Bytes::from(format!("event: {kind}\ndata: {value}\n\n")));
        self.events += 1;
        self.wire_bytes += overhead + size;
        Ok(())
    }
}
fn open_block(next: &mut usize, out: &mut Frames, block: Value) -> Result<usize, CodecError> {
    if *next >= crate::semantic::task::generation::MAX_ITEMS {
        return Err(CodecError::Limit);
    }
    let index = *next;
    *next += 1;
    out.push(&json!({"type":"content_block_start","index":index,"content_block":block}))?;
    Ok(index)
}
impl ClientEventEncoder {
    pub(crate) fn set_notice(&mut self, notice: crate::protocol::diagnostics::AdaptationNotice) {
        self.notice = Some(notice);
    }
    pub fn new(id: &str, model: &str) -> Self {
        Self {
            schedule: schedule::Schedule::default(),
            notice: None,
            losses: vec![],
            id: id.into(),
            model: model.into(),
            started: false,
            seen_start: false,
            finished: false,
            rejected: false,
            service_class: Presence::Absent,
            open: BTreeMap::new(),
            owners_seen: BTreeSet::new(),
            text_parts: BTreeMap::new(),
            next: 0,
            calls: false,
            incomplete_calls: false,
            progress: InteractionProgress::Unreported,
            usage: None,
            delivery_enabled: false,
            delivery_usage: None,
            opening_usage: None,
            limits: Default::default(),
            wire_bytes: 0,
            events: 0,
        }
    }
    pub(crate) fn with_limits(
        mut self,
        limits: crate::protocol::stream::SseLimits,
    ) -> Result<Self, crate::protocol::stream::SseError> {
        limits.validate()?;
        self.limits = limits;
        Ok(self)
    }
    pub(crate) fn reject(&mut self) {
        self.rejected = true;
        self.schedule = schedule::Schedule::default();
        self.open.clear();
        self.text_parts.clear();
    }
    pub(crate) fn update_delivery(
        &mut self,
        delivery: Option<&crate::protocol::GenerationDelivery>,
    ) {
        self.delivery_enabled = delivery.is_some();
        self.delivery_usage = delivery.and_then(|d| d.usage);
    }
    /// Messages reports the assigned service in opening usage, not a late delta.
    pub fn update_service_class(
        &mut self,
        class: Presence<ServiceClass>,
    ) -> Result<(), CodecError> {
        let result = (|| {
            if self.rejected || self.finished {
                return Err(CodecError::invalid());
            }
            if let Some(value) = class.value() {
                crate::protocol::anthropic::messages::service::class_wire(*value)?;
            }
            if (self.started || self.seen_start || !self.service_class.is_absent())
                && self.service_class != class
            {
                return Err(CodecError::Unsupported(CarrierReason::Field(
                    crate::protocol::conversion::generation::Field::ServiceClass,
                )));
            }
            self.service_class = class;
            Ok(())
        })();
        if result.is_err() {
            self.reject();
        }
        result
    }
    fn start(&mut self, out: &mut Frames) -> Result<(), CodecError> {
        if !self.started {
            self.started = true;
            let mut usage = match &self.usage {
                Some(wire) => {
                    let value = wire.wire();
                    self.opening_usage = Some(wire.clone());
                    value
                }
                None => json!({"input_tokens":0,"output_tokens":0}),
            };
            crate::protocol::anthropic::messages::service::write_class(
                usage.as_object_mut().expect("usage object"),
                &self.service_class,
            )
            .expect("validated reported service");
            out.push(&json!({"type":"message_start","message":{
                "id":self.id,"type":"message","role":"assistant","model":self.model,"content":[],
                "stop_reason":null,"stop_sequence":null,"usage":usage}}))?;
        }
        Ok(())
    }
    fn report(&mut self, source: &UsageReport, out: &mut Frames) -> Result<(), CodecError> {
        source.validate()?;
        let selected = match crate::protocol::anthropic::messages::accounting::project(source) {
            Ok(selected) => selected,
            // An incomplete checkpoint need not have a representable delivery view yet.
            Err(_) if source.stage == UsageStage::Checkpoint => None,
            Err(CodecError::Unsupported(_)) if self.delivery_enabled => None,
            Err(error) => return Err(error),
        };
        for loss in super::projection::report_losses(source, selected.as_ref(), "event.encode") {
            if !self.losses.contains(&loss) {
                self.losses.push(loss);
            }
        }
        if let Some(selected) = selected {
            let wire = crate::protocol::anthropic::messages::accounting::wire(
                &selected,
                Presence::Absent,
            )?;
            if self.started {
                if wire.cache_creation.value()
                    != self
                        .opening_usage
                        .as_ref()
                        .and_then(|u| u.cache_creation.value())
                {
                    return Err(CodecError::Unsupported(CarrierReason::Usage));
                }
                if let Some(opening) = &self.opening_usage
                    && (opening.cache_read_input_tokens.value().is_some()
                        && wire.cache_read_input_tokens.value().is_none()
                        || opening.cache_creation_input_tokens.value().is_some()
                            && wire.cache_creation_input_tokens.value().is_none()
                        || opening.thinking_tokens.value().is_some()
                            && wire.thinking_tokens.value().is_none()
                        || opening.server_tool_use.value().is_some()
                            && wire.server_tool_use.value().is_none())
                {
                    return Err(CodecError::Unsupported(CarrierReason::Usage));
                }
            }
            self.usage = Some(wire);
            self.start(out)?;
        } else if source.scope == UsageScope::Operation {
            self.usage = None;
            let published_details = self.opening_usage.as_ref().is_some_and(|usage| {
                usage.cache_read_input_tokens.value().is_some()
                    || usage.cache_creation_input_tokens.value().is_some()
                    || usage.thinking_tokens.value().is_some()
                    || usage.server_tool_use.value().is_some()
                    || usage.cache_creation.value().is_some()
            });
            // Mandatory scalar completion cannot retract an optional observation
            // already retained by SDK accumulators, or resurrect an old checkpoint.
            if source.stage == UsageStage::Final && (!self.delivery_enabled || published_details) {
                return Err(CodecError::Unsupported(CarrierReason::Usage));
            }
        }
        Ok(())
    }
    /// Render already reduced events. Raw argument strings are not JSON authority:
    /// callers must validate the EOF materialization before submitting its terminal,
    /// as ResponseDelivery does. This renderer is not source admission or a parser.
    pub fn encode(&mut self, event: &StreamEvent) -> Result<Vec<Bytes>, CodecError> {
        if self.finished || self.rejected {
            return Err(CodecError::invalid());
        }
        let checkpoint = self.losses.len();
        let result = (|| {
            let queued = self.schedule.push(event, self.limits)?;
            let terminal = matches!(event, StreamEvent::Terminal { .. });
            let mut out = Frames::new(self);
            while let Some(ready) = self.schedule.pop(terminal) {
                match ready {
                    schedule::Ready::Event(event) => self.encode_open(&event, &mut out)?,
                    schedule::Ready::OmitReasoning(item) => {
                        self.losses.push(super::projection::reasoning_omission(
                            item,
                            ("event.project").into(),
                        ));
                    }
                    schedule::Ready::ClosePart(item, part) => {
                        let index = self
                            .text_parts
                            .remove(&(item, part))
                            .ok_or(CodecError::invalid())?;
                        out.push(&json!({"type":"content_block_stop","index":index}))?;
                    }
                    schedule::Ready::CloseOwner(item) => {
                        match self.open.remove(&item).ok_or(CodecError::invalid())? {
                            OpenItem::Thinking { .. } => {
                                return Err(CodecError::Unsupported(CarrierReason::Replay));
                            }
                            OpenItem::ToolUse { index, .. } => {
                                out.push(&json!({"type":"content_block_stop","index":index}))?
                            }
                            OpenItem::Text => {}
                        }
                    }
                }
            }
            if !queued {
                self.encode_open(event, &mut out)?;
            }
            self.events = out.events;
            self.wire_bytes = out.wire_bytes;
            Ok(out.values)
        })();
        if result.is_err() {
            self.losses.truncate(checkpoint);
            self.reject();
        }
        result
    }
    fn encode_open(&mut self, event: &StreamEvent, out: &mut Frames) -> Result<(), CodecError> {
        if matches!(event, StreamEvent::CompactionProgress { .. }) {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        match event {
            // Wait for opening accounting or the first content owner, without buffering content.
            StreamEvent::Started => self.seen_start = true,
            StreamEvent::ItemStarted { item, kind, replay } => {
                if replay.is_some() && matches!(kind, ItemKind::ToolCall { .. }) {
                    return Err(CodecError::Unsupported(CarrierReason::Replay));
                }
                if self.owners_seen.len() >= crate::semantic::task::generation::MAX_ITEMS {
                    return Err(CodecError::Limit);
                }
                if !self.owners_seen.insert(*item) {
                    return Err(CodecError::invalid());
                }
                self.start(out)?;
                let block = match kind {
                    ItemKind::Message { phase: None } => OpenItem::Text,
                    // Opens on its first delta or at its end, once a redacted value is known.
                    ItemKind::Reasoning => OpenItem::Thinking {
                        index: None,
                        signature: replay.as_ref().map(|r| r.value.as_str().to_owned()),
                        parts: 0,
                        active_part: None,
                        bytes: 0,
                    },
                    ItemKind::ToolCall {
                        call_id,
                        name,
                        context,
                        ..
                    } => {
                        if !context.is_direct() || context.alias_domain.is_some() {
                            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                        }
                        super::projection::tool_labels(call_id.as_str(), name.as_str())?;
                        self.calls = true;
                        OpenItem::ToolUse {
                            prefix: String::new(),
                            arguments_started: false,
                            index: open_block(
                                &mut self.next,
                                out,
                                json!({
                                    "type": "tool_use",
                                    "id": call_id.as_str(),
                                    "name": name.as_str(),
                                    "input": {},
                                }),
                            )?,
                        }
                    }
                    _ => {
                        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                    }
                };
                self.open.insert(*item, block);
            }
            StreamEvent::PartStarted { item, part, kind }
                if matches!(self.open.get(item), Some(OpenItem::Thinking { .. })) =>
            {
                let Some(OpenItem::Thinking {
                    parts, active_part, ..
                }) = self.open.get_mut(item)
                else {
                    unreachable!()
                };
                if active_part.is_some() || *parts > 0 || *kind != PartKind::Summary {
                    // Arrival-order concatenation would change interleaved part text.
                    return Err(CodecError::Unsupported(CarrierReason::Reasoning));
                }
                *parts += 1;
                *active_part = Some(*part);
            }
            StreamEvent::PartFinished { item, part }
            | StreamEvent::PartInterrupted { item, part }
                if matches!(self.open.get(item), Some(OpenItem::Thinking { .. })) =>
            {
                let Some(OpenItem::Thinking { active_part, .. }) = self.open.get_mut(item) else {
                    unreachable!()
                };
                if active_part.take() != Some(*part) {
                    return Err(CodecError::invalid());
                }
            }
            StreamEvent::PartInterrupted { .. } => {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
            StreamEvent::PartStarted {
                item,
                part,
                kind: kind @ (PartKind::Text | PartKind::Refusal),
            } if self
                .open
                .get(item)
                .is_some_and(|block| matches!(block, OpenItem::Text)) =>
            {
                if *kind == PartKind::Refusal {
                    let loss = super::projection::refusal_loss("event.encode");
                    if !self.losses.contains(&loss) {
                        self.losses.push(loss);
                    }
                }
                let index = open_block(&mut self.next, out, json!({"type":"text","text":""}))?;
                self.text_parts.insert((*item, *part), index);
            }
            StreamEvent::PartFinished { item, part }
                if self
                    .open
                    .get(item)
                    .is_some_and(|block| matches!(block, OpenItem::Text)) =>
            {
                let index = self
                    .text_parts
                    .remove(&(*item, *part))
                    .ok_or(CodecError::invalid())?;
                out.push(&json!({"type":"content_block_stop","index":index}))?;
            }
            StreamEvent::Delta { logprobs, .. } if !logprobs.is_empty() => {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
            StreamEvent::LogprobsSnapshot { logprobs, .. } if !logprobs.is_empty() => {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
            StreamEvent::TextMetadata { logprobs, .. }
                if !logprobs.is_absent()
                    && !matches!(logprobs, Presence::Value(probs) if probs.is_empty()) =>
            {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
            StreamEvent::Delta {
                item,
                part,
                fragment,
                ..
            } if !fragment.is_empty() => {
                let block = self.open.get_mut(item).ok_or(CodecError::invalid())?;
                let (index, delta) = match block {
                    OpenItem::Text => (
                        *self
                            .text_parts
                            .get(&(*item, *part))
                            .ok_or(CodecError::invalid())?,
                        json!({"type":"text_delta","text":fragment}),
                    ),
                    OpenItem::Thinking {
                        index,
                        active_part,
                        bytes,
                        ..
                    } => {
                        if *active_part != Some(*part) {
                            return Err(CodecError::invalid());
                        }
                        *bytes = bytes.saturating_add(fragment.len());
                        if *bytes > crate::semantic::task::generation::MAX_TEXT_BYTES {
                            return Err(CodecError::Limit);
                        }
                        if index.is_none() {
                            *index = Some(open_block(&mut self.next, out, thinking_block())?);
                        }
                        (
                            index.expect("opened thinking"),
                            json!({"type":"thinking_delta","thinking":fragment}),
                        )
                    }
                    OpenItem::ToolUse {
                        index,
                        prefix,
                        arguments_started,
                    } => {
                        if !*arguments_started {
                            if prefix.len().saturating_add(fragment.len())
                                > self.limits.max_event_bytes
                            {
                                return Err(CodecError::Limit);
                            }
                            prefix.push_str(fragment);
                            if prefix
                                .bytes()
                                .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
                            {
                                return Ok(());
                            }
                            *arguments_started = true;
                            (
                                *index,
                                json!({"type":"input_json_delta","partial_json":std::mem::take(prefix)}),
                            )
                        } else {
                            (
                                *index,
                                json!({"type":"input_json_delta","partial_json":fragment}),
                            )
                        }
                    }
                };
                out.push(&json!({"type":"content_block_delta","index":index,"delta":delta}))?;
            }
            StreamEvent::ItemFinished {
                item,
                replay,
                status,
            } => {
                let block = self.open.remove(item).ok_or(CodecError::invalid())?;
                let (index, signature) = match block {
                    OpenItem::Text => {
                        if self.text_parts.keys().any(|(owner, _)| owner == item) {
                            return Err(CodecError::invalid());
                        }
                        return Ok(());
                    }
                    OpenItem::ToolUse { index, .. } => {
                        if replay.is_some() {
                            return Err(CodecError::Unsupported(CarrierReason::Replay));
                        }
                        if *status == crate::semantic::task::generation::ItemLifecycle::Incomplete {
                            self.incomplete_calls = true;
                        }
                        out.push(&json!({"type":"content_block_stop","index":index}))?;
                        return Ok(());
                    }
                    OpenItem::Thinking {
                        index,
                        signature,
                        parts,
                        bytes,
                        ..
                    } => {
                        let signed = replay
                            .as_ref()
                            .map(|r| r.value.as_str())
                            .or(signature.as_deref());
                        if signed.is_none_or(str::is_empty) {
                            return Err(CodecError::Unsupported(CarrierReason::Replay));
                        }
                        if signed.is_some_and(|s| {
                            parts > 0 && (bytes == 0 || super::projection::redacted(s))
                        }) {
                            return Err(CodecError::Unsupported(CarrierReason::Replay));
                        }
                        (index, signature)
                    }
                };
                let signature = replay
                    .as_ref()
                    .map(|r| r.value.as_str().to_owned())
                    .or(signature);
                let redacted = index.is_none()
                    && signature
                        .as_deref()
                        .is_some_and(super::projection::redacted);
                let index = match index {
                    Some(index) => index,
                    None if redacted => open_block(
                        &mut self.next,
                        out,
                        json!({"type":"redacted_thinking","data":signature}),
                    )?,
                    None => open_block(&mut self.next, out, thinking_block())?,
                };
                if !redacted && let Some(signature) = signature {
                    out.push(&json!({"type":"content_block_delta","index":index,
                        "delta":{"type":"signature_delta","signature":signature}}))?;
                }
                out.push(&json!({"type":"content_block_stop","index":index}))?;
            }
            StreamEvent::AnnotationAdded { .. } => {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
            StreamEvent::TextMetadata { annotations, .. } if !annotations.is_empty() => {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
            StreamEvent::PartStarted {
                kind: PartKind::Audio,
                ..
            }
            | StreamEvent::AudioDelta { .. } => {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
            StreamEvent::SafetyReport(_) => {
                self.reject();
                return Err(CodecError::Unsupported(CarrierReason::Safety));
            }
            StreamEvent::PartReplay { .. } => {
                return Err(CodecError::Unsupported(CarrierReason::Replay));
            }
            StreamEvent::Usage(usage) => {
                self.report(&UsageReport::operation(*usage), out)?;
            }
            StreamEvent::Progress(progress) => self.progress = *progress,
            StreamEvent::UsageReport(report) => {
                self.report(report, out)?;
            }
            StreamEvent::Terminal { terminal, details } => {
                if self.incomplete_calls && (*terminal != StreamTerminal::Incomplete
                    || details.incomplete != Some(crate::semantic::task::generation::IncompleteReason::MaxOutputTokens))
                {
                    return Err(CodecError::Unsupported(CarrierReason::Terminal));
                }
                self.start(out)?;
                let outcome = match terminal {
                    StreamTerminal::Completed => Outcome::Completed,
                    StreamTerminal::Incomplete => Outcome::Incomplete,
                    StreamTerminal::Failed | StreamTerminal::Error => Outcome::Failed,
                    StreamTerminal::Cancelled => Outcome::Cancelled,
                };
                let mut details = details.clone();
                details.validate(outcome)?;
                if details.explanation.value().is_some() {
                    let loss = super::projection::explanation_loss("event.encode");
                    if !self.losses.contains(&loss) {
                        self.losses.push(loss);
                    }
                }
                details.explanation = Presence::Absent;
                let (stop, sequence) = crate::protocol::anthropic::messages::terminal::encode(
                    outcome,
                    self.progress,
                    self.calls,
                    &details,
                )?;
                if !self.open.is_empty() || !self.text_parts.is_empty() {
                    return Err(CodecError::invalid());
                }
                let mut wire = self.usage.clone();
                if let Some(counts) = self.delivery_usage {
                    let completed =
                        crate::protocol::anthropic::messages::accounting::delivery_wire(counts)?;
                    if let Some(wire) = &mut wire {
                        wire.input_tokens = completed.input_tokens;
                        wire.output_tokens = completed.output_tokens;
                    } else {
                        wire = Some(completed);
                    }
                }
                let mut usage = wire.ok_or(CodecError::Invalid(ShapeReason::Usage))?.wire();
                // TTL detail has only an opening carrier in the pinned delta schema.
                usage
                    .as_object_mut()
                    .expect("usage")
                    .remove("cache_creation");
                let mut event = json!({"type":"message_delta",
                    "delta":{"stop_reason":stop.wire(),"stop_sequence":sequence},
                    "usage":usage});
                crate::protocol::anthropic::messages::wire::envelope::write_refusal(
                    event["delta"].as_object_mut().expect("message delta"),
                    &details.refusal,
                );
                if let Some(mut notice) = self.notice.take() {
                    notice.add_effects(&self.losses);
                    let stop_bytes =
                        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".len();
                    let available = out
                        .available()
                        .saturating_sub("message_delta".len() + 16 + stop_bytes);
                    notice.attach(&mut event, available, false);
                }
                out.push(&event)?;
                out.push(&json!({"type":"message_stop"}))?;
                self.finished = true;
            }
            _ => {}
        }
        Ok(())
    }
    /// A terminal was encoded.
    pub fn finish(&self) -> Result<(), CodecError> {
        if self.finished && !self.rejected {
            Ok(())
        } else {
            Err(CodecError::invalid())
        }
    }
    pub(crate) fn take_evidence(
        &mut self,
    ) -> crate::protocol::conversion::generation::EvidenceDelta {
        std::mem::take(&mut self.losses).into_iter().collect()
    }
    #[cfg(test)]
    pub(crate) fn losses(&self) -> &[crate::protocol::conversion::generation::Effect] {
        &self.losses
    }
    /// End the stream with Anthropic's `error` event (`{"type":"error","error":…}`).
    pub fn fail(&mut self, error: &Value) -> Result<Vec<Bytes>, CodecError> {
        if self.finished || self.rejected {
            return Err(CodecError::invalid());
        }
        self.finished = true;
        let mut value = error.clone();
        if value.get("type").and_then(Value::as_str) != Some("error") {
            self.reject();
            return Err(CodecError::invalid());
        }
        if let Some(notice) = self.notice.take() {
            notice.attach(
                &mut value,
                self.limits
                    .max_event_bytes
                    .min(self.limits.max_wire_bytes.saturating_sub(self.wire_bytes))
                    .saturating_sub("error".len() + 16),
                false,
            );
        }
        let mut out = Frames::new(self);
        let result = out.push(&value).map(|()| out.values);
        self.reject();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::{
        task::generation::{ArgumentFormat, StreamState, Usage, reduce},
        value::Text,
    };

    #[test]
    fn thinking_waits_for_an_observed_signature_or_is_omitted() {
        use crate::semantic::task::generation::{
            ItemLifecycle, ItemReplay, ReplayFormat, ReplayValue,
        };
        let item = ItemId::new(1);
        let part = PartId::new(1);
        for signed in [false, true] {
            let mut encoder = ClientEventEncoder::new("r", "m");
            for event in [
                StreamEvent::ItemStarted {
                    item,
                    kind: ItemKind::Reasoning,
                    replay: None,
                },
                StreamEvent::PartStarted {
                    item,
                    part,
                    kind: PartKind::Summary,
                },
                StreamEvent::Delta {
                    item,
                    part,
                    fragment: "thought".into(),
                    logprobs: vec![],
                },
                StreamEvent::PartFinished { item, part },
            ] {
                let frames = encoder.encode(&event).unwrap();
                assert!(frames.is_empty(), "an undecided owner cannot publish");
            }
            let event = StreamEvent::ItemFinished {
                item,
                status: ItemLifecycle::Completed,
                replay: signed.then(|| ItemReplay {
                    value: ReplayValue::final_value(
                        ReplayFormat::AnthropicMessagesThinking,
                        Text::new("signature", "test", 64).unwrap(),
                    ),
                    origin: None,
                }),
            };
            if signed {
                let frames = encoder.encode(&event).unwrap();
                let wire = frames
                    .iter()
                    .map(|f| std::str::from_utf8(f).unwrap())
                    .collect::<String>();
                assert!(
                    wire.find("signature_delta").unwrap()
                        < wire.find("content_block_stop").unwrap()
                );
            } else {
                assert!(encoder.encode(&event).unwrap().is_empty());
                assert!(encoder.finish().is_err());
                let losses = encoder.take_evidence();
                assert_eq!(losses.len(), 1);
                assert_eq!(losses[0].rule(), "messages.omit-uncarried-reasoning.v1");
            }
        }
        let mut encoder = ClientEventEncoder::new("r", "m");
        encoder
            .encode(&StreamEvent::ItemStarted {
                item,
                kind: ItemKind::Reasoning,
                replay: None,
            })
            .unwrap();
        assert!(
            encoder
                .encode(&StreamEvent::PartStarted {
                    item,
                    part,
                    kind: PartKind::Text
                })
                .is_err()
        );
    }

    #[test]
    fn late_block_starts_wait_without_reversing_source_owner_order() {
        let first = ItemId::new(1);
        let second = ItemId::new(2);
        let tool = StreamEvent::ItemStarted {
            item: second,
            kind: ItemKind::ToolCall {
                format: ArgumentFormat::Raw,
                call_id: Text::new("c", "id", 32).unwrap(),
                name: Text::new("read", "name", 32).unwrap(),
                context: Default::default(),
            },
            replay: None,
        };
        let text = StreamEvent::PartStarted {
            item: first,
            part: PartId::new(1),
            kind: PartKind::Text,
        };
        for thinking in [false, true] {
            let owner = StreamEvent::ItemStarted {
                item: first,
                kind: if thinking {
                    ItemKind::Reasoning
                } else {
                    ItemKind::Message { phase: None }
                },
                replay: None,
            };
            let part = if thinking {
                StreamEvent::PartStarted {
                    item: first,
                    part: PartId::new(1),
                    kind: PartKind::Summary,
                }
            } else {
                text.clone()
            };
            let mut events = vec![StreamEvent::Started, owner, tool.clone(), part];
            if thinking {
                events.push(StreamEvent::Delta {
                    item: first,
                    part: PartId::new(1),
                    fragment: "plan".into(),
                    logprobs: vec![],
                });
            }
            let mut source = StreamState::new();
            for event in &events {
                source = reduce(source, event.clone()).expect("valid source prefix");
            }
            let mut encoder = ClientEventEncoder::new("r", "m");
            for event in &events[..events.len() - 1] {
                encoder.encode(event).unwrap();
            }
            assert!(encoder.encode(events.last().unwrap()).is_ok());
            assert!(encoder.finish().is_err());
        }
        let mut encoder = ClientEventEncoder::new("r", "m");
        let mut starts = vec![];
        for event in [
            StreamEvent::Started,
            StreamEvent::ItemStarted {
                item: first,
                kind: ItemKind::Message { phase: None },
                replay: None,
            },
            text,
            tool,
        ] {
            for bytes in encoder.encode(&event).unwrap() {
                let wire = std::str::from_utf8(&bytes).unwrap();
                let value: Value = serde_json::from_str(
                    wire.lines().find_map(|s| s.strip_prefix("data: ")).unwrap(),
                )
                .unwrap();
                if value["type"] == "content_block_start" {
                    starts.push((
                        value["index"].clone(),
                        value["content_block"]["type"].clone(),
                    ));
                }
            }
        }
        assert_eq!(starts, vec![(json!(0), json!("text"))]);
    }

    fn blocked(kind: ItemKind, limits: crate::protocol::stream::SseLimits) -> ClientEventEncoder {
        let mut encoder = ClientEventEncoder::new("r", "m")
            .with_limits(limits)
            .unwrap();
        encoder
            .encode(&StreamEvent::ItemStarted {
                item: ItemId::new(1),
                kind: ItemKind::Message { phase: None },
                replay: None,
            })
            .unwrap();
        encoder
            .encode(&StreamEvent::PartStarted {
                item: ItemId::new(1),
                part: PartId::new(1),
                kind: PartKind::Text,
            })
            .unwrap();
        assert!(
            encoder
                .encode(&StreamEvent::ItemStarted {
                    item: ItemId::new(2),
                    kind,
                    replay: None,
                })
                .unwrap()
                .is_empty()
        );
        encoder
    }
    fn tool() -> ItemKind {
        ItemKind::ToolCall {
            format: ArgumentFormat::Raw,
            call_id: Text::new("c", "id", 32).unwrap(),
            name: Text::new("lookup", "name", 32).unwrap(),
            context: Default::default(),
        }
    }
    fn blocked_delta(s: &str) -> StreamEvent {
        StreamEvent::Delta {
            item: ItemId::new(2),
            part: PartId::new(2),
            fragment: s.into(),
            logprobs: vec![],
        }
    }

    #[test]
    fn blocked_content_limits_and_failure_release_pending_events() {
        let mut bounded = blocked(
            tool(),
            crate::protocol::stream::SseLimits {
                max_events: 6,
                ..Default::default()
            },
        );
        for _ in 0..1000 {
            assert!(bounded.encode(&blocked_delta("")).unwrap().is_empty());
        }
        for _ in 0..5 {
            assert!(bounded.encode(&blocked_delta("x")).unwrap().is_empty());
        }
        assert_eq!(bounded.encode(&blocked_delta("x")), Err(CodecError::Limit));
        assert!(bounded.schedule.pop(true).is_none());
        assert!(bounded.open.is_empty());
        assert!(bounded.encode(&blocked_delta("")).is_err());
        assert!(bounded.finish().is_err());

        let mut bytes = blocked(tool(), Default::default());
        let payload = "x".repeat(crate::semantic::task::generation::MAX_TEXT_BYTES);
        assert!(bytes.encode(&blocked_delta(&payload)).unwrap().is_empty());
        assert_eq!(
            bytes.encode(&blocked_delta(&payload)),
            Err(CodecError::Limit)
        );
        assert!(bytes.schedule.pop(true).is_none());

        let mut failed = blocked(tool(), Default::default());
        failed.encode(&blocked_delta("{")).unwrap();
        let frames = failed
            .fail(&json!({"type":"error","error":{"type":"api_error","message":"synthetic"}}))
            .unwrap();
        assert_eq!(frames.len(), 1);
        assert!(
            std::str::from_utf8(&frames[0])
                .unwrap()
                .starts_with("event: error\n")
        );
        assert!(failed.schedule.pop(true).is_none());
        assert!(failed.finish().is_err());
    }

    #[test]
    fn uncarriable_reasoning_is_omitted_without_blocking_visible_content() {
        use crate::semantic::task::generation::{
            ItemLifecycle, ItemReplay, ReplayFormat, ReplayValue,
        };
        let mut encoder = blocked(ItemKind::Reasoning, Default::default());
        for number in [2, 3] {
            let part = PartId::new(number);
            for event in [
                StreamEvent::PartStarted {
                    item: ItemId::new(2),
                    part,
                    kind: PartKind::Summary,
                },
                StreamEvent::Delta {
                    item: ItemId::new(2),
                    part,
                    fragment: "plan".into(),
                    logprobs: vec![],
                },
                StreamEvent::ValueFinished {
                    item: ItemId::new(2),
                    part,
                },
                StreamEvent::PartFinished {
                    item: ItemId::new(2),
                    part,
                },
            ] {
                assert!(encoder.encode(&event).unwrap().is_empty());
            }
        }
        assert!(
            encoder
                .encode(&StreamEvent::ItemFinished {
                    item: ItemId::new(2),
                    status: ItemLifecycle::Completed,
                    replay: Some(ItemReplay {
                        value: ReplayValue::final_value(
                            ReplayFormat::AnthropicMessagesThinking,
                            Text::new("signature", "test", 64).unwrap()
                        ),
                        origin: None
                    }),
                })
                .unwrap()
                .is_empty()
        );
        let visible = encoder
            .encode(&StreamEvent::Delta {
                item: ItemId::new(1),
                part: PartId::new(1),
                fragment: "retained".into(),
                logprobs: vec![],
            })
            .unwrap();
        assert!(
            visible
                .iter()
                .any(|frame| std::str::from_utf8(frame).unwrap().contains("retained"))
        );
        encoder
            .encode(&StreamEvent::PartFinished {
                item: ItemId::new(1),
                part: PartId::new(1),
            })
            .unwrap();
        assert!(
            encoder
                .encode(&StreamEvent::ItemFinished {
                    item: ItemId::new(1),
                    status: ItemLifecycle::Completed,
                    replay: None,
                })
                .unwrap()
                .is_empty()
        );
        let losses = encoder.take_evidence();
        assert_eq!(
            losses
                .iter()
                .filter(|e| e.rule() == "messages.omit-uncarried-reasoning.v1")
                .count(),
            1
        );
        assert!(encoder.schedule.pop(true).is_none());
        assert!(
            encoder.finish().is_err(),
            "omission cannot manufacture a terminal"
        );
    }

    #[test]
    fn queued_redacted_thinking_opens_only_after_its_predecessor_closes() {
        use crate::semantic::task::generation::{
            ItemLifecycle, ItemReplay, ReplayFormat, ReplayValue,
        };
        let mut encoder = blocked(ItemKind::Reasoning, Default::default());
        let signature = crate::protocol::replay_envelope::seal(
            &crate::protocol::replay_envelope::CLIENT_ORIGIN,
            ReplayFormat::AnthropicMessagesRedactedThinking,
            "synthetic-redacted",
        )
        .unwrap();
        assert!(
            encoder
                .encode(&StreamEvent::ItemFinished {
                    item: ItemId::new(2),
                    status: ItemLifecycle::Completed,
                    replay: Some(ItemReplay {
                        origin: None,
                        value: ReplayValue::final_value(
                            ReplayFormat::AnthropicMessagesRedactedThinking,
                            signature.clone(),
                        )
                    }),
                })
                .unwrap()
                .is_empty()
        );
        encoder
            .encode(&StreamEvent::PartFinished {
                item: ItemId::new(1),
                part: PartId::new(1),
            })
            .unwrap();
        let frames = encoder
            .encode(&StreamEvent::ItemFinished {
                item: ItemId::new(1),
                status: ItemLifecycle::Completed,
                replay: None,
            })
            .unwrap();
        let events: Vec<Value> = frames
            .iter()
            .map(|frame| {
                serde_json::from_str(
                    std::str::from_utf8(frame)
                        .unwrap()
                        .lines()
                        .find_map(|line| line.strip_prefix("data: "))
                        .unwrap(),
                )
                .unwrap()
            })
            .collect();
        assert_eq!(
            events,
            vec![
                json!({"type":"content_block_start","index":1,"content_block":{"type":"redacted_thinking","data":signature.as_str()}}),
                json!({"type":"content_block_stop","index":1}),
            ]
        );
    }

    #[test]
    fn queued_drain_checks_wire_limits_and_escaped_frame_size() {
        use crate::protocol::failure::Resource;
        let limits = crate::protocol::stream::SseLimits {
            max_wire_bytes: 1024,
            max_event_bytes: 512,
            ..Default::default()
        };
        let mut encoder = blocked(tool(), limits);
        for _ in 0..20 {
            encoder.encode(&blocked_delta("x")).unwrap();
        }
        encoder
            .encode(&StreamEvent::PartFinished {
                item: ItemId::new(1),
                part: PartId::new(1),
            })
            .unwrap();
        assert_eq!(
            encoder.encode(&StreamEvent::ItemFinished {
                item: ItemId::new(1),
                status: crate::semantic::task::generation::ItemLifecycle::Completed,
                replay: None,
            }),
            Err(CodecError::Resource(Resource::Bytes))
        );
        assert!(encoder.schedule.pop(true).is_none());
        let mut escaped = blocked(tool(), limits);
        assert_eq!(
            escaped.encode(&StreamEvent::Delta {
                item: ItemId::new(1),
                part: PartId::new(1),
                fragment: "\u{1}".repeat(100),
                logprobs: vec![],
            }),
            Err(CodecError::Resource(Resource::Bytes))
        );
    }

    #[test]
    fn scalar_completion_does_not_erase_published_optional_accounting() {
        let mut encoder = ClientEventEncoder::new("r", "m");
        encoder.update_delivery(Some(&crate::protocol::GenerationDelivery::default()));
        let mut counts = Usage::operation(10, 5, 15);
        counts.cached_input_tokens = Some(2);
        let mut opening = UsageReport::operation(counts);
        opening.stage = UsageStage::Checkpoint;
        encoder.encode(&StreamEvent::UsageReport(opening)).unwrap();
        counts.cached_input_tokens = None;
        assert!(
            encoder
                .encode(&StreamEvent::UsageReport(UsageReport::operation(counts)))
                .is_err()
        );
        assert!(encoder.finish().is_err());
    }

    #[test]
    fn tool_whitespace_prefix_is_bounded_and_preserved_with_its_first_token() {
        let start = StreamEvent::ItemStarted {
            item: ItemId::new(1),
            kind: ItemKind::ToolCall {
                format: ArgumentFormat::Raw,
                call_id: Text::new("c", "id", 64).unwrap(),
                name: Text::new("lookup", "name", 64).unwrap(),
                context: Default::default(),
            },
            replay: None,
        };
        let delta = |fragment: &str| StreamEvent::Delta {
            item: ItemId::new(1),
            part: PartId::new(1),
            fragment: fragment.into(),
            logprobs: vec![],
        };
        let mut encoder = ClientEventEncoder::new("r", "m");
        encoder.encode(&start).unwrap();
        for prefix in [" ", "\n\t"] {
            assert!(encoder.encode(&delta(prefix)).unwrap().is_empty());
        }
        let mut arguments = String::new();
        for fragment in ["{\"x\":", "1}"] {
            for frame in encoder.encode(&delta(fragment)).unwrap() {
                let text = std::str::from_utf8(&frame).unwrap();
                let data = text
                    .lines()
                    .find_map(|line| line.strip_prefix("data: "))
                    .unwrap();
                let event: Value = serde_json::from_str(data).unwrap();
                arguments.push_str(event["delta"]["partial_json"].as_str().unwrap());
            }
        }
        assert_eq!(arguments, " \n\t{\"x\":1}");
        let mut bounded = ClientEventEncoder::new("r", "m")
            .with_limits(crate::protocol::stream::SseLimits {
                max_event_bytes: 512,
                ..Default::default()
            })
            .unwrap();
        bounded.encode(&start).unwrap();
        assert!(bounded.encode(&delta(&" ".repeat(256))).unwrap().is_empty());
        assert_eq!(
            bounded.encode(&delta(&" ".repeat(257))),
            Err(CodecError::Limit)
        );
        assert!(bounded.encode(&delta("{}")).is_err());
        assert!(bounded.finish().is_err());
    }
}
