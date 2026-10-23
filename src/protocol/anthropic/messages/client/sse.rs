//! Native SSE for Messages clients from the shared stream events of any upstream.
//! Readable deltas stream; an opaque value (already sealed for the client) arrives
//! as one `signature_delta` when its thinking block closes. A reasoning block opens on
//! its first delta or at its end, so a redacted value can still open `redacted_thinking`.
use crate::semantic::{context::ServiceClass, value::Presence};
use crate::{
    protocol::CodecError,
    semantic::task::generation::{
        InteractionProgress, ItemId, ItemKind, Outcome, PartId, PartKind, StreamEvent,
        StreamTerminal, UsageReport, UsageScope, UsageStage,
    },
};
use bytes::Bytes;
use serde_json::{Value, json};
use std::collections::BTreeMap;

enum OpenItem {
    Text,
    Thinking {
        index: Option<usize>,
        signature: Option<String>,
    },
    ToolUse {
        index: usize,
        /// SDK incremental JSON parsers reject a standalone whitespace prefix.
        prefix: String,
        arguments_started: bool,
    },
}
impl OpenItem {
    fn index(self) -> Option<usize> {
        match self {
            Self::Text => None,
            Self::Thinking { index, .. } => index,
            Self::ToolUse { index, .. } => Some(index),
        }
    }
}
pub struct ClientEventEncoder {
    losses: Vec<crate::lowering::generation::LossEvidence>,
    id: String,
    model: String,
    started: bool,
    seen_start: bool,
    finished: bool,
    rejected: bool,
    service_class: Presence<ServiceClass>,
    open: BTreeMap<ItemId, OpenItem>,
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
    attachment: Option<crate::protocol::attachments::Carrier>,
    limits: crate::protocol::stream::SseLimits,
    wire_bytes: usize,
    events: usize,
}
fn thinking_block() -> Value {
    json!({"type":"thinking","thinking":"","signature":""})
}
fn open_block(next: &mut usize, out: &mut Vec<Bytes>, block: Value) -> usize {
    let index = *next;
    *next += 1;
    out.push(frame(
        &json!({"type":"content_block_start","index":index,"content_block":block}),
    ));
    index
}
fn frame(v: &Value) -> Bytes {
    let kind = v["type"].as_str().unwrap_or("error");
    Bytes::from(format!("event: {kind}\ndata: {v}\n\n"))
}
impl ClientEventEncoder {
    pub fn new(id: &str, model: &str) -> Self {
        Self {
            losses: vec![],
            id: id.into(),
            model: model.into(),
            started: false,
            seen_start: false,
            finished: false,
            rejected: false,
            service_class: Presence::Absent,
            open: BTreeMap::new(),
            text_parts: BTreeMap::new(),
            next: 0,
            calls: false,
            incomplete_calls: false,
            progress: InteractionProgress::Unreported,
            usage: None,
            delivery_enabled: false,
            delivery_usage: None,
            opening_usage: None,
            attachment: None,
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
    pub(crate) fn set_attachment(
        &mut self,
        value: crate::protocol::attachments::Carrier,
    ) -> Result<(), CodecError> {
        if self.rejected || self.finished || self.attachment.is_some() {
            return Err(CodecError::Invalid("attachment carrier"));
        }
        self.attachment = Some(value);
        Ok(())
    }
    pub(crate) fn reject(&mut self) {
        self.rejected = true;
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
                return Err(CodecError::Invalid("Messages stream closed"));
            }
            if let Some(value) = class.value() {
                crate::protocol::anthropic::messages::service::class_wire(*value)?;
            }
            if (self.started || self.seen_start || !self.service_class.is_absent())
                && self.service_class != class
            {
                return Err(CodecError::Unsupported(
                    "Messages late service report".into(),
                ));
            }
            self.service_class = class;
            Ok(())
        })();
        if result.is_err() {
            self.rejected = true;
        }
        result
    }
    fn start(&mut self, out: &mut Vec<Bytes>) {
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
            out.push(frame(&json!({"type":"message_start","message":{
                "id":self.id,"type":"message","role":"assistant","model":self.model,"content":[],
                "stop_reason":null,"stop_sequence":null,"usage":usage}})));
        }
    }
    fn report(&mut self, source: &UsageReport, out: &mut Vec<Bytes>) -> Result<(), CodecError> {
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
                    return Err(CodecError::Unsupported(
                        "Messages late cache breakdown".into(),
                    ));
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
                    return Err(CodecError::Unsupported(
                        "Messages erased usage observation".into(),
                    ));
                }
            }
            self.usage = Some(wire);
            self.start(out);
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
                return Err(CodecError::Unsupported("Messages usage".into()));
            }
        }
        Ok(())
    }
    /// Render already reduced events. Raw argument strings are not JSON authority:
    /// callers must validate the EOF materialization before submitting its terminal,
    /// as ResponseDelivery does. This renderer is not source admission or a parser.
    pub fn encode(&mut self, event: &StreamEvent) -> Result<Vec<Bytes>, CodecError> {
        if self.finished || self.rejected {
            return Err(CodecError::Invalid("Messages stream closed"));
        }
        let result = self.encode_open(event).and_then(|frames| {
            for frame in &frames {
                self.events = self.events.checked_add(1).ok_or(CodecError::Limit)?;
                self.wire_bytes = self
                    .wire_bytes
                    .checked_add(frame.len())
                    .ok_or(CodecError::Limit)?;
                if frame.len() > self.limits.max_event_bytes
                    || self.events > self.limits.max_events
                    || self.wire_bytes > self.limits.max_wire_bytes
                {
                    return Err(CodecError::Limit);
                }
            }
            Ok(frames)
        });
        if result.is_err() {
            self.reject();
        }
        result
    }
    fn encode_open(&mut self, event: &StreamEvent) -> Result<Vec<Bytes>, CodecError> {
        if matches!(
            event,
            StreamEvent::PartInterrupted { .. } | StreamEvent::CompactionProgress { .. }
        ) {
            return Err(CodecError::Unsupported("Messages event carrier".into()));
        }
        let mut out = vec![];
        match event {
            // Wait for opening accounting or the first content owner, without buffering content.
            StreamEvent::Started => self.seen_start = true,
            StreamEvent::ItemStarted { item, kind, replay } => {
                self.start(&mut out);
                let block = match kind {
                    ItemKind::Message { phase: None } => OpenItem::Text,
                    // Opens on its first delta or at its end, once a redacted value is known.
                    ItemKind::Reasoning => OpenItem::Thinking {
                        index: None,
                        signature: replay.as_ref().map(|r| r.value.as_str().to_owned()),
                    },
                    ItemKind::ToolCall {
                        call_id,
                        name,
                        context,
                        ..
                    } => {
                        if !context.is_direct() || context.alias_domain.is_some() {
                            return Err(CodecError::Unsupported("Messages call context".into()));
                        }
                        self.calls = true;
                        OpenItem::ToolUse {
                            prefix: String::new(),
                            arguments_started: false,
                            index: open_block(
                                &mut self.next,
                                &mut out,
                                json!({
                                    "type": "tool_use",
                                    "id": call_id.as_str(),
                                    "name": name.as_str(),
                                    "input": {},
                                }),
                            ),
                        }
                    }
                    _ => return Err(CodecError::Unsupported("Messages stream item".into())),
                };
                self.open.insert(*item, block);
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
                let index = open_block(&mut self.next, &mut out, json!({"type":"text","text":""}));
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
                    .ok_or(CodecError::Invalid("Messages text part"))?;
                out.push(frame(&json!({"type":"content_block_stop","index":index})));
            }
            StreamEvent::Delta { logprobs, .. } if !logprobs.is_empty() => {
                return Err(CodecError::Unsupported("Messages probabilities".into()));
            }
            StreamEvent::LogprobsSnapshot { .. } => {
                return Err(CodecError::Unsupported("Messages probabilities".into()));
            }
            StreamEvent::TextMetadata { logprobs, .. } if !logprobs.is_absent() => {
                return Err(CodecError::Unsupported("Messages probabilities".into()));
            }
            StreamEvent::Delta {
                item,
                part,
                fragment,
                ..
            } if !fragment.is_empty() => {
                let block = self
                    .open
                    .get_mut(item)
                    .ok_or(CodecError::Invalid("Messages block"))?;
                let (index, delta) = match block {
                    OpenItem::Text => (
                        *self
                            .text_parts
                            .get(&(*item, *part))
                            .ok_or(CodecError::Invalid("Messages text part"))?,
                        json!({"type":"text_delta","text":fragment}),
                    ),
                    OpenItem::Thinking { index, .. } => (
                        *index.get_or_insert_with(|| {
                            open_block(&mut self.next, &mut out, thinking_block())
                        }),
                        json!({"type":"thinking_delta","thinking":fragment}),
                    ),
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
                                return Ok(out);
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
                out.push(frame(
                    &json!({"type":"content_block_delta","index":index,"delta":delta}),
                ));
            }
            StreamEvent::ItemFinished {
                item,
                replay,
                status,
            } => {
                let block = self
                    .open
                    .remove(item)
                    .ok_or(CodecError::Invalid("Messages block"))?;
                let (index, signature) = match block {
                    OpenItem::Text => {
                        if self.text_parts.keys().any(|(owner, _)| owner == item) {
                            return Err(CodecError::Invalid("Messages open text part"));
                        }
                        return Ok(out);
                    }
                    OpenItem::ToolUse { index, .. } => {
                        if *status == crate::semantic::task::generation::ItemLifecycle::Incomplete {
                            self.incomplete_calls = true;
                        }
                        out.push(frame(&json!({"type":"content_block_stop","index":index})));
                        return Ok(out);
                    }
                    OpenItem::Thinking { index, signature } => (index, signature),
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
                        &mut out,
                        json!({"type":"redacted_thinking","data":signature}),
                    ),
                    None => open_block(&mut self.next, &mut out, thinking_block()),
                };
                if !redacted && let Some(signature) = signature {
                    out.push(frame(&json!({"type":"content_block_delta","index":index,
                        "delta":{"type":"signature_delta","signature":signature}})));
                }
                out.push(frame(&json!({"type":"content_block_stop","index":index})));
            }
            StreamEvent::AnnotationAdded { .. } => {
                return Err(CodecError::Unsupported("Messages citations".into()));
            }
            StreamEvent::TextMetadata { annotations, .. } if !annotations.is_empty() => {
                return Err(CodecError::Unsupported("Messages citations".into()));
            }
            StreamEvent::PartStarted {
                kind: PartKind::Audio,
                ..
            }
            | StreamEvent::AudioDelta { .. } => {
                return Err(CodecError::Unsupported("Messages audio output".into()));
            }
            StreamEvent::SafetyReport(_) => {
                self.reject();
                return Err(CodecError::Unsupported("Messages safety report".into()));
            }
            StreamEvent::PartReplay { .. } => {
                return Err(CodecError::Unsupported("Messages replay attachment".into()));
            }
            StreamEvent::Usage(usage) => {
                self.report(&UsageReport::operation(*usage), &mut out)?;
            }
            StreamEvent::Progress(progress) => self.progress = *progress,
            StreamEvent::UsageReport(report) => {
                self.report(report, &mut out)?;
            }
            StreamEvent::Terminal { terminal, details } => {
                if self.incomplete_calls && (*terminal != StreamTerminal::Incomplete
                    || details.incomplete != Some(crate::semantic::task::generation::IncompleteReason::MaxOutputTokens))
                {
                    return Err(CodecError::Unsupported("Messages partial tool terminal".into()));
                }
                self.start(&mut out);
                let outcome = match terminal {
                    StreamTerminal::Completed => Outcome::Completed,
                    StreamTerminal::Incomplete => Outcome::Incomplete,
                    StreamTerminal::Failed | StreamTerminal::Error => Outcome::Failed,
                    StreamTerminal::Cancelled => Outcome::Cancelled,
                };
                let mut details = details.clone();
                details.validate(outcome)?;
                if !details.explanation.is_absent() {
                    let loss = super::projection::explanation_loss("event.encode");
                    if !self.losses.contains(&loss) {
                        self.losses.push(loss);
                    }
                    details.explanation = Presence::Absent;
                }
                let (stop, sequence) = crate::protocol::anthropic::messages::terminal::encode(
                    outcome,
                    self.progress,
                    self.calls,
                    &details,
                )?;
                // Close every started block by its wire index. Text items own no block;
                // a thinking item without content has not opened one yet.
                let mut started: Vec<usize> =
                    std::mem::take(&mut self.text_parts).into_values().collect();
                started.extend(
                    std::mem::take(&mut self.open)
                        .into_values()
                        .filter_map(OpenItem::index),
                );
                started.sort_unstable();
                for index in started {
                    out.push(frame(&json!({"type":"content_block_stop","index":index})));
                }
                if let Some(token) = self.attachment.take() {
                    if self.next >= crate::semantic::task::generation::MAX_ITEMS {
                        return Err(CodecError::Limit);
                    }
                    let index = open_block(&mut self.next, &mut out, thinking_block());
                    out.push(frame(&json!({
                        "type": "content_block_delta",
                        "index": index,
                        "delta": {"type": "signature_delta", "signature": token.as_str()},
                    })));
                    out.push(frame(&json!({"type":"content_block_stop","index":index})));
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
                let mut usage = wire.ok_or(CodecError::Invalid("Messages usage"))?.wire();
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
                out.push(frame(&event));
                out.push(frame(&json!({"type":"message_stop"})));
                self.finished = true;
            }
            _ => {}
        }
        Ok(out)
    }
    /// A terminal was encoded.
    pub fn finish(&self) -> Result<(), CodecError> {
        if self.finished && !self.rejected {
            Ok(())
        } else {
            Err(CodecError::Invalid("Messages stream unfinished"))
        }
    }
    pub(crate) fn losses(&self) -> &[crate::lowering::generation::LossEvidence] {
        &self.losses
    }
    /// End the stream with Anthropic's `error` event (`{"type":"error","error":…}`).
    pub fn fail(&mut self, error: &Value) -> Vec<Bytes> {
        self.finished = true;
        vec![frame(error)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::{
        task::generation::{ArgumentFormat, Usage},
        value::Text,
    };

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
