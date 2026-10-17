//! Native SSE for Messages clients from the shared stream events of any upstream.
//! Readable deltas stream; an opaque value (already sealed for the client) arrives
//! as one `signature_delta` when its thinking block closes. A reasoning block opens on
//! its first delta or at its end, so a redacted value can still open `redacted_thinking`.
use crate::semantic::{context::ServiceClass, value::Presence};
use crate::{
    protocol::CodecError,
    semantic::task::generation::{
        InputTokenRelation, InteractionProgress, ItemId, ItemKind, Outcome, OutputTokenRelation,
        PartId, PartKind, StreamEvent, StreamTerminal, Usage,
    },
};
use bytes::Bytes;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Text,
    Thinking,
    ToolUse,
}
pub struct ClientEventEncoder {
    losses: Vec<crate::lowering::generation::LossEvidence>,
    id: String,
    model: String,
    started: bool,
    finished: bool,
    rejected: bool,
    service_class: Presence<ServiceClass>,
    /// Wire index once the block has opened, kind, and a replay value seen at start.
    open: BTreeMap<ItemId, (Option<usize>, Kind, Option<String>)>,
    text_parts: BTreeMap<(ItemId, PartId), usize>,
    next: usize,
    calls: bool,
    progress: InteractionProgress,
    usage: Option<Usage>,
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
/// Messages usage: input excludes cache reads and reported cache writes.
/// An unreported write category stays absent; the remaining input keeps any such writes.
pub(super) fn usage_json(u: Option<&Usage>) -> Result<Value, CodecError> {
    let unsupported = || CodecError::Unsupported("Messages usage".into());
    let u = u.ok_or_else(unsupported)?;
    u.validate()?;
    let input = u.input_tokens.ok_or_else(unsupported)?;
    let input = match u.input_relation {
        InputTokenRelation::ExcludesCacheReadAndWrite => input,
        InputTokenRelation::IncludesCache => {
            let read = u.cached_input_tokens.ok_or_else(unsupported)?;
            let excluded = match u.input_cache_write_tokens {
                None => read,
                // IncludesCache does not promise disjoint cache-read and cache-write subsets.
                Some(write) if read != 0 && write != 0 => return Err(unsupported()),
                Some(write) => read.max(write),
            };
            input.checked_sub(excluded).ok_or_else(unsupported)?
        }
        InputTokenRelation::Unreported => return Err(unsupported()),
    };
    let output = u.output_tokens.ok_or_else(unsupported)?;
    let output = match u.output_relation {
        OutputTokenRelation::IncludesReasoning => output,
        OutputTokenRelation::ExcludesReasoning => output
            .checked_add(u.reasoning_tokens.ok_or_else(unsupported)?)
            .ok_or_else(unsupported)?,
        OutputTokenRelation::Unreported => return Err(unsupported()),
    };
    let mut v = json!({"input_tokens":input,"output_tokens":output});
    if let Some(read) = u.cached_input_tokens {
        v["cache_read_input_tokens"] = json!(read);
    }
    if let Some(write) = u.input_cache_write_tokens {
        v["cache_creation_input_tokens"] = json!(write);
    }
    if let Some(thinking) = u.reasoning_tokens {
        v["output_tokens_details"] = json!({ "thinking_tokens": thinking });
    }
    Ok(v)
}
impl ClientEventEncoder {
    pub fn new(id: &str, model: &str) -> Self {
        Self {
            losses: vec![],
            id: id.into(),
            model: model.into(),
            started: false,
            finished: false,
            rejected: false,
            service_class: Presence::Absent,
            open: BTreeMap::new(),
            text_parts: BTreeMap::new(),
            next: 0,
            calls: false,
            progress: InteractionProgress::Unreported,
            usage: None,
        }
    }
    pub(crate) fn reject(&mut self) {
        self.rejected = true;
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
                super::service::class_wire(*value)?;
            }
            if (self.started || !self.service_class.is_absent()) && self.service_class != class {
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
            let mut usage = json!({"input_tokens":0,"output_tokens":0});
            super::service::write_class(
                usage.as_object_mut().expect("usage object"),
                &self.service_class,
            )
            .expect("validated reported service");
            out.push(frame(&json!({"type":"message_start","message":{
                "id":self.id,"type":"message","role":"assistant","model":self.model,"content":[],
                "stop_reason":null,"stop_sequence":null,"usage":usage}})));
        }
    }
    pub fn encode(&mut self, event: &StreamEvent) -> Result<Vec<Bytes>, CodecError> {
        if self.finished || self.rejected {
            return Err(CodecError::Invalid("Messages stream closed"));
        }
        let mut out = vec![];
        match event {
            StreamEvent::Started => self.start(&mut out),
            StreamEvent::ItemStarted { item, kind, replay } => {
                self.start(&mut out);
                let (kind, block) = match kind {
                    ItemKind::Message { phase: None } => (Kind::Text, None),
                    // Opens on its first delta or at its end, once a redacted value is known.
                    ItemKind::Reasoning => (Kind::Thinking, None),
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
                        (
                            Kind::ToolUse,
                            Some(
                                json!({"type":"tool_use","id":call_id.as_str(),"name":name.as_str(),"input":{}}),
                            ),
                        )
                    }
                    _ => return Err(CodecError::Unsupported("Messages stream item".into())),
                };
                let index = block.map(|block| open_block(&mut self.next, &mut out, block));
                let signature = replay.as_ref().map(|r| r.value.as_str().to_owned());
                self.open.insert(*item, (index, kind, signature));
            }
            StreamEvent::PartStarted {
                item,
                part,
                kind: kind @ (PartKind::Text | PartKind::Refusal),
            } if self
                .open
                .get(item)
                .is_some_and(|(_, kind, _)| *kind == Kind::Text) =>
            {
                if *kind == PartKind::Refusal {
                    let loss = super::client::refusal_loss("event.encode");
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
                    .is_some_and(|(_, kind, _)| *kind == Kind::Text) =>
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
                let (index, kind, _) = self
                    .open
                    .get_mut(item)
                    .ok_or(CodecError::Invalid("Messages block"))?;
                let index = if *kind == Kind::Text {
                    *self
                        .text_parts
                        .get(&(*item, *part))
                        .ok_or(CodecError::Invalid("Messages text part"))?
                } else {
                    *index.get_or_insert_with(|| {
                        open_block(&mut self.next, &mut out, thinking_block())
                    })
                };
                let delta = match kind {
                    Kind::Text => json!({"type":"text_delta","text":fragment}),
                    Kind::Thinking => json!({"type":"thinking_delta","thinking":fragment}),
                    Kind::ToolUse => json!({"type":"input_json_delta","partial_json":fragment}),
                };
                out.push(frame(
                    &json!({"type":"content_block_delta","index":index,"delta":delta}),
                ));
            }
            StreamEvent::ItemFinished { item, replay, .. } => {
                let (index, kind, signature) = self
                    .open
                    .remove(item)
                    .ok_or(CodecError::Invalid("Messages block"))?;
                if kind == Kind::Text {
                    if self.text_parts.keys().any(|(owner, _)| owner == item) {
                        return Err(CodecError::Invalid("Messages open text part"));
                    }
                    return Ok(out);
                }
                let signature = replay
                    .as_ref()
                    .map(|r| r.value.as_str().to_owned())
                    .or(signature);
                let redacted =
                    index.is_none() && signature.as_deref().is_some_and(super::client::redacted);
                let index = match index {
                    Some(index) => index,
                    None if redacted => open_block(
                        &mut self.next,
                        &mut out,
                        json!({"type":"redacted_thinking","data":signature}),
                    ),
                    None => open_block(&mut self.next, &mut out, thinking_block()),
                };
                if kind == Kind::Thinking
                    && !redacted
                    && let Some(signature) = signature
                {
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
            StreamEvent::Usage(usage) => {
                let selected = super::client::project_usage(*usage)?;
                if selected != *usage {
                    let mut loss = super::client::usage_loss();
                    loss.direction = "event.encode";
                    if !self.losses.contains(&loss) {
                        self.losses.push(loss);
                    }
                }
                self.usage = Some(selected);
            }
            StreamEvent::Progress(progress) => self.progress = *progress,
            StreamEvent::UsageReport(report) => {
                report.validate()?;
                if report.scope == crate::semantic::task::generation::UsageScope::Operation
                    && report.stage == crate::semantic::task::generation::UsageStage::Final
                {
                    self.usage = report
                        .scalar_summary()
                        .map(super::client::project_usage)
                        .transpose()?;
                }
                let loss = crate::protocol::usage::report_loss("event.encode");
                if !self.losses.contains(&loss) {
                    self.losses.push(loss);
                }
            }
            StreamEvent::Terminal { terminal, details } => {
                self.start(&mut out);
                let outcome = match terminal {
                    StreamTerminal::Completed => Outcome::Completed,
                    StreamTerminal::Incomplete => Outcome::Incomplete,
                    StreamTerminal::Failed | StreamTerminal::Error => Outcome::Failed,
                    StreamTerminal::Cancelled => Outcome::Cancelled,
                };
                let (stop, sequence) =
                    super::client::terminal(outcome, self.progress, self.calls, details)?;
                // Close every started block by its wire index. Text items own no block;
                // a thinking item without content has not opened one yet.
                let mut started: Vec<usize> =
                    std::mem::take(&mut self.text_parts).into_values().collect();
                started.extend(
                    std::mem::take(&mut self.open)
                        .into_values()
                        .filter_map(|(index, _, _)| index),
                );
                started.sort_unstable();
                for index in started {
                    out.push(frame(&json!({"type":"content_block_stop","index":index})));
                }
                out.push(frame(&json!({"type":"message_delta",
                    "delta":{"stop_reason":stop,"stop_sequence":sequence},
                    "usage":usage_json(self.usage.as_ref())?})));
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
