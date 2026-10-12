//! Native SSE for Messages clients from the shared stream events of any upstream.
//! Readable deltas stream; an opaque value (already sealed for the client) arrives
//! as one `signature_delta` when its thinking block closes.
use crate::{
    protocol::CodecError,
    semantic::task::generation::{
        IncompleteReason, InputTokenRelation, ItemId, ItemKind, StreamEvent, StreamTerminal, Usage,
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
    open: BTreeMap<ItemId, (usize, Kind, Option<String>)>,
    next: usize,
    calls: bool,
    usage: Option<Usage>,
}
fn frame(v: &Value) -> Bytes {
    let kind = v["type"].as_str().unwrap_or("error");
    Bytes::from(format!("event: {kind}\ndata: {v}\n\n"))
}
/// Messages usage: input excludes cache reads and writes.
pub(super) fn usage_json(u: Option<&Usage>) -> Value {
    let Some(u) = u else {
        return json!({"input_tokens":0,"output_tokens":0});
    };
    let read = u.cached_input_tokens.unwrap_or(0);
    let write = u.input_cache_write_tokens.unwrap_or(0);
    let input = u.input_tokens.unwrap_or(0);
    let input = match u.input_relation {
        InputTokenRelation::IncludesCache => input.saturating_sub(read).saturating_sub(write),
        _ => input,
    };
    let mut v = json!({"input_tokens":input,"output_tokens":u.output_tokens.unwrap_or(0)});
    if let Some(read) = u.cached_input_tokens {
        v["cache_read_input_tokens"] = json!(read);
    }
    if let Some(write) = u.input_cache_write_tokens {
        v["cache_creation_input_tokens"] = json!(write);
    }
    v
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
                    ItemKind::Message { .. } => (Kind::Text, json!({"type":"text","text":""})),
                    ItemKind::Reasoning => (
                        Kind::Thinking,
                        json!({"type":"thinking","thinking":"","signature":""}),
                    ),
                    ItemKind::ToolCall { call_id, name, .. } => {
                        self.calls = true;
                        (
                            Kind::ToolUse,
                            json!({"type":"tool_use","id":call_id.as_str(),"name":name.as_str(),"input":{}}),
                        )
                    }
                    _ => return Err(CodecError::Unsupported("Messages stream item".into())),
                };
                let index = self.next;
                self.next += 1;
                let signature = replay.as_ref().map(|r| r.value.as_str().to_owned());
                self.open.insert(*item, (index, kind, signature));
                out.push(frame(
                    &json!({"type":"content_block_start","index":index,"content_block":block}),
                ));
            }
            StreamEvent::Delta { item, fragment, .. } if !fragment.is_empty() => {
                let (index, kind, _) = self
                    .open
                    .get(item)
                    .ok_or(CodecError::Invalid("Messages block"))?;
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
                if kind == Kind::Thinking
                    && let Some(signature) = signature
                {
                    out.push(frame(&json!({"type":"content_block_delta","index":index,
                        "delta":{"type":"signature_delta","signature":signature}})));
                }
                out.push(frame(&json!({"type":"content_block_stop","index":index})));
            }
            StreamEvent::Usage(usage) => self.usage = Some(*usage),
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
                    "usage":usage_json(self.usage.as_ref())})));
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
