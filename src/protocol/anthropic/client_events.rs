//! Native SSE for Messages clients from the shared stream events of any upstream.
//! Readable deltas stream; an opaque value (already sealed for the client) arrives
//! as one `signature_delta` when its thinking block closes. A reasoning block opens on
//! its first delta or at its end, so a redacted value can still open `redacted_thinking`.
use crate::{
    protocol::CodecError,
    semantic::task::generation::{
        IncompleteReason, InputTokenRelation, ItemId, ItemKind, OutputTokenRelation, StreamEvent,
        StreamTerminal, Usage,
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
    id: String,
    model: String,
    started: bool,
    finished: bool,
    /// Wire index once the block has opened, kind, and a replay value seen at start.
    open: BTreeMap<ItemId, (Option<usize>, Kind, Option<String>)>,
    next: usize,
    calls: bool,
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
/// Messages usage: input excludes cache reads and writes.
pub(super) fn usage_json(u: Option<&Usage>) -> Result<Value, CodecError> {
    let unsupported = || CodecError::Unsupported("Messages usage".into());
    let u = u.ok_or_else(unsupported)?;
    u.validate()?;
    let input = u.input_tokens.ok_or_else(unsupported)?;
    let input = match u.input_relation {
        InputTokenRelation::ExcludesCacheReadAndWrite => input,
        InputTokenRelation::IncludesCache => {
            let read = u.cached_input_tokens.ok_or_else(unsupported)?;
            let write = u.input_cache_write_tokens.ok_or_else(unsupported)?;
            // IncludesCache does not promise disjoint cache-read and cache-write subsets.
            if read != 0 && write != 0 {
                return Err(unsupported());
            }
            input.checked_sub(read.max(write)).ok_or_else(unsupported)?
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
    Ok(v)
}
impl ClientEventEncoder {
    pub fn new(id: &str, model: &str) -> Self {
        Self {
            id: id.into(),
            model: model.into(),
            started: false,
            finished: false,
            open: BTreeMap::new(),
            next: 0,
            calls: false,
            usage: None,
        }
    }
    fn start(&mut self, out: &mut Vec<Bytes>) {
        if !self.started {
            self.started = true;
            out.push(frame(&json!({"type":"message_start","message":{
                "id":self.id,"type":"message","role":"assistant","model":self.model,"content":[],
                "stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":0,"output_tokens":0}}})));
        }
    }
    pub fn encode(&mut self, event: &StreamEvent) -> Result<Vec<Bytes>, CodecError> {
        if self.finished {
            return Err(CodecError::Invalid("Messages stream closed"));
        }
        let mut out = vec![];
        match event {
            StreamEvent::Started => self.start(&mut out),
            StreamEvent::ItemStarted { item, kind, replay } => {
                self.start(&mut out);
                let (kind, block) = match kind {
                    ItemKind::Message { .. } => {
                        (Kind::Text, Some(json!({"type":"text","text":""})))
                    }
                    // Opens on its first delta or at its end, once a redacted value is known.
                    ItemKind::Reasoning => (Kind::Thinking, None),
                    ItemKind::ToolCall { call_id, name, .. } => {
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
            StreamEvent::Delta { item, fragment, .. } if !fragment.is_empty() => {
                let (index, kind, _) = self
                    .open
                    .get_mut(item)
                    .ok_or(CodecError::Invalid("Messages block"))?;
                let index = *index
                    .get_or_insert_with(|| open_block(&mut self.next, &mut out, thinking_block()));
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
            StreamEvent::Usage(usage) => {
                usage_json(Some(usage))?;
                self.usage = Some(*usage);
            }
            StreamEvent::Terminal { terminal, details } => {
                self.start(&mut out);
                let stop = match (terminal, &details.incomplete) {
                    (StreamTerminal::Completed, _) if self.calls => "tool_use",
                    (StreamTerminal::Completed, _) => "end_turn",
                    (StreamTerminal::Incomplete, Some(IncompleteReason::MaxOutputTokens)) => {
                        "max_tokens"
                    }
                    (StreamTerminal::Incomplete, Some(IncompleteReason::ContentFilter)) => {
                        "refusal"
                    }
                    _ => return Err(CodecError::Unsupported("Messages terminal".into())),
                };
                for (_, (index, _, _)) in std::mem::take(&mut self.open) {
                    out.push(frame(&json!({"type":"content_block_stop","index":index})));
                }
                out.push(frame(&json!({"type":"message_delta",
                    "delta":{"stop_reason":stop,"stop_sequence":null},
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
        if self.finished {
            Ok(())
        } else {
            Err(CodecError::Invalid("Messages stream unfinished"))
        }
    }
    /// End the stream with Anthropic's `error` event (`{"type":"error","error":…}`).
    pub fn fail(&mut self, error: &Value) -> Vec<Bytes> {
        self.finished = true;
        vec![frame(error)]
    }
}
