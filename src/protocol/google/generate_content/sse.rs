//! `streamGenerateContent?alt=sse` intake: each frame is one `GenerateContentResponse`
//! fragment. Known fields are strict and unknown ones ignored; the aggregate is the reduced
//! event state, released only at strict EOF.
//! Source: <https://ai.google.dev/api/generate-content>.
use super::{accounting::usage, replay::Signed};
use crate::{
    protocol::{
        CodecError, DecodedResponse, ResponseMetadata,
        fidelity::FidelityRecords,
        native::ReplayTarget,
        stream::{SseError, SseLimits},
    },
    semantic::{
        task::generation::*,
        value::{JsonLimits, Text, parse_json},
    },
    transport::sse::SseDecoder,
};
use serde_json::{Map, Value};

enum OpenKind {
    Thought { signature: Option<String> },
    Text { signature: Option<String> },
}
struct Open {
    item: ItemId,
    part: PartId,
    kind: OpenKind,
    part_started: bool,
}
pub struct GenerationSseDecoder {
    framing: SseDecoder,
    limits: SseLimits,
    wire_bytes: usize,
    events: usize,
    scope: LocalScope,
    target: ReplayTarget,
    state: StreamState,
    fidelity: FidelityRecords,
    metadata: Option<ResponseMetadata>,
    open: Option<Open>,
    members: Vec<ItemId>,
    calls: Vec<String>,
    usage: Option<UsageReport>,
    finished: bool,
    closed: bool,
    rejected: bool,
}
pub(super) fn invalid(what: &'static str) -> SseError {
    CodecError::Invalid(what).into()
}
fn unsupported(what: &str) -> SseError {
    CodecError::Unsupported(format!("Gemini {what}")).into()
}
fn text(value: &str) -> Result<Text, SseError> {
    Text::allowing_empty(value, "Gemini text", MAX_TEXT_BYTES).map_err(|_| SseError::Limit)
}
pub(super) fn object<'a>(
    v: &'a Value,
    what: &'static str,
) -> Result<&'a Map<String, Value>, SseError> {
    v.as_object().ok_or_else(|| invalid(what))
}
fn string<'a>(o: &'a Map<String, Value>, key: &'static str) -> Result<Option<&'a str>, SseError> {
    match o.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(invalid(key)),
    }
}
/// Stops that end the turn honestly; the rest are failures of this attempt.
fn finish_reason(reason: &str) -> Result<Option<IncompleteReason>, SseError> {
    Ok(match reason {
        "STOP" => None,
        "MAX_TOKENS" => Some(IncompleteReason::MaxOutputTokens),
        "SAFETY"
        | "BLOCKLIST"
        | "PROHIBITED_CONTENT"
        | "SPII"
        | "IMAGE_SAFETY"
        | "IMAGE_PROHIBITED_CONTENT" => Some(IncompleteReason::ContentFilter),
        "RECITATION" | "IMAGE_RECITATION" => Some(IncompleteReason::Recitation),
        // The request lacked a signature the gateway should have restored.
        "MISSING_THOUGHT_SIGNATURE" => return Err(CodecError::StreamFailure(400).into()),
        "PUP_LIMITED_DISABLED" => return Err(CodecError::StreamFailure(403).into()),
        _ => return Err(CodecError::StreamFailure(500).into()),
    })
}
impl GenerationSseDecoder {
    pub fn new(
        status: u16,
        media: &str,
        limits: SseLimits,
        scope: LocalScope,
        target: ReplayTarget,
    ) -> Result<Self, SseError> {
        limits.validate()?;
        crate::protocol::stream::validate_http(status, media)?;
        Ok(Self {
            framing: SseDecoder::new(limits.max_event_bytes),
            limits,
            wire_bytes: 0,
            events: 0,
            scope,
            target,
            state: StreamState::new(),
            fidelity: FidelityRecords::default(),
            metadata: None,
            open: None,
            members: vec![],
            calls: vec![],
            usage: None,
            finished: false,
            closed: false,
            rejected: false,
        })
    }
    pub fn metadata(&self) -> Option<&ResponseMetadata> {
        self.metadata.as_ref()
    }
    pub fn fidelity(&self) -> &FidelityRecords {
        &self.fidelity
    }
    /// Consume at most one frame. The caller owns and revisits the unconsumed suffix.
    pub fn consume(&mut self, chunk: &[u8]) -> Result<(usize, Vec<StreamEvent>), SseError> {
        if self.closed || self.rejected {
            return Err(SseError::Closed);
        }
        let result = (|| {
            let remaining = self.limits.max_wire_bytes.saturating_sub(self.wire_bytes);
            let available = chunk.len().min(remaining.saturating_add(1));
            let (frame, used) = self.framing.push_until_event(&chunk[..available])?;
            self.wire_bytes = self.wire_bytes.checked_add(used).ok_or(SseError::Limit)?;
            if self.wire_bytes > self.limits.max_wire_bytes {
                return Err(SseError::Limit);
            }
            let mut out = vec![];
            if let Some(frame) = frame {
                self.events = self.events.checked_add(1).ok_or(SseError::Limit)?;
                if self.events > self.limits.max_events {
                    return Err(SseError::Limit);
                }
                let value = parse_json(frame.data().as_bytes(), JsonLimits::ENVELOPE).map_err(
                    |e| match e {
                        crate::semantic::value::JsonError::Limit => SseError::Limit,
                        crate::semantic::value::JsonError::Invalid => invalid("Gemini JSON"),
                    },
                )?;
                self.chunk(&value, &mut out)?;
            }
            Ok((used, out))
        })();
        if result.is_err() {
            self.rejected = true;
        }
        result
    }
    fn emit(&mut self, event: StreamEvent, out: &mut Vec<StreamEvent>) -> Result<(), SseError> {
        self.state =
            reduce(std::mem::take(&mut self.state), event.clone()).map_err(CodecError::from)?;
        out.push(event);
        Ok(())
    }
    fn next_ids(&mut self) -> Result<(ItemId, PartId), SseError> {
        if self.members.len() >= MAX_ITEMS {
            return Err(SseError::Limit);
        }
        let n = self.members.len() as u64 + 1;
        let item = ItemId::scoped(self.scope, n);
        self.members.push(item);
        Ok((item, PartId::scoped(self.scope, n * (MAX_ITEMS as u64 + 1))))
    }
    fn chunk(&mut self, value: &Value, out: &mut Vec<StreamEvent>) -> Result<(), SseError> {
        let o = object(value, "Gemini chunk")?;
        if let Some(error) = o.get("error") {
            let code = object(error, "Gemini error")?
                .get("code")
                .and_then(Value::as_u64)
                .and_then(|c| u16::try_from(c).ok())
                .filter(|c| (400..600).contains(c))
                .unwrap_or(500);
            return Err(CodecError::StreamFailure(code).into());
        }
        if self.finished {
            return Err(invalid("Gemini chunk after finish"));
        }
        let id = string(o, "responseId")?;
        let model = string(o, "modelVersion")?;
        if let Some(metadata) = &self.metadata {
            if id.is_some_and(|id| Some(id) != metadata.id.as_deref())
                || model.is_some_and(|model| model != metadata.model)
            {
                return Err(invalid("Gemini response identity changed"));
            }
        } else {
            let metadata = ResponseMetadata {
                storage: None,
                id: Some(id.ok_or_else(|| invalid("responseId"))?.into()),
                model: model.ok_or_else(|| invalid("modelVersion"))?.into(),
                created: None,
                context: Default::default(),
                instruction_fidelity: Default::default(),
            };
            metadata.validate()?;
            self.metadata = Some(metadata);
            self.emit(StreamEvent::Started, out)?;
        }
        if let Some(v) = o.get("usageMetadata") {
            self.usage = Some(usage(v)?);
        }
        if let Some(feedback) = o.get("promptFeedback")
            && let Some(report) =
                super::safety::report(object(feedback, "promptFeedback")?, SafetySubject::Input)?
        {
            let blocked = report
                .reason
                .is_some_and(|reason| reason != SafetyBlockReason::Unspecified);
            if blocked
                && (!self.members.is_empty()
                    || o.get("candidates")
                        .is_some_and(|v| !v.as_array().is_some_and(Vec::is_empty)))
            {
                return Err(invalid("blocked prompt with candidates"));
            }
            self.emit(StreamEvent::SafetyReport(report), out)?;
            if blocked {
                return self.terminal(Some(IncompleteReason::ContentFilter), out);
            }
        }
        let candidates = match o.get("candidates") {
            None => return Ok(()),
            Some(v) => v.as_array().ok_or_else(|| invalid("candidates"))?,
        };
        let candidate = match candidates.as_slice() {
            [] => return Ok(()),
            [one] => object(one, "candidate")?,
            _ => return Err(unsupported("candidate count")),
        };
        if candidate
            .get("index")
            .is_some_and(|i| i.as_u64() != Some(0))
        {
            return Err(invalid("candidate index"));
        }
        if let Some(metadata) = candidate.get("citationMetadata").filter(|v| !v.is_null()) {
            let metadata = object(metadata, "citationMetadata")?;
            if metadata
                .get("citationSources")
                .is_some_and(|v| v.as_array().is_none_or(|sources| !sources.is_empty()))
            {
                return Err(unsupported("citationMetadata"));
            }
        }
        if candidate
            .get("groundingMetadata")
            .is_some_and(|v| !v.is_null())
        {
            return Err(unsupported("groundingMetadata"));
        }
        let mut probabilities = candidate
            .get("logprobsResult")
            .map(super::logprobs::read)
            .transpose()?
            .unwrap_or_default();
        if let Some(value) = candidate.get("avgLogprobs")
            && !value.as_f64().is_some_and(|n| n.is_finite() && n <= 0.0)
        {
            return Err(invalid("avgLogprobs"));
        }
        if !probabilities.is_empty() {
            let parts = candidate
                .get("content")
                .and_then(|content| content.get("parts"))
                .and_then(Value::as_array)
                .ok_or_else(|| unsupported("logprob owner"))?;
            let [part] = parts.as_slice() else {
                return Err(unsupported("logprob owner"));
            };
            let part = object(part, "part")?;
            let fragment = string(part, "text")?
                .filter(|text| !text.is_empty())
                .ok_or_else(|| unsupported("logprob owner"))?;
            // Only chunk-local, unambiguous text reports are selected. Do not
            // guess whether a mixed or cumulative report belongs to this delta.
            if part
                .get("thought")
                .is_some_and(|thought| thought != &Value::Bool(false))
                || probabilities
                    .iter()
                    .map(|p| p.token.as_str())
                    .collect::<String>()
                    != fragment
            {
                return Err(unsupported("logprob alignment"));
            }
        }
        if let Some(content) = candidate.get("content") {
            let content = object(content, "content")?;
            if string(content, "role")?.is_some_and(|r| r != "model") {
                return Err(invalid("content role"));
            }
            if let Some(parts) = content.get("parts") {
                for (index, part) in parts
                    .as_array()
                    .ok_or_else(|| invalid("parts"))?
                    .iter()
                    .enumerate()
                {
                    // Parts in one frame have explicit boundaries. Only the first
                    // part may continue the last fragment of the previous frame.
                    if index > 0 {
                        self.close(out)?;
                    }
                    self.part(
                        object(part, "part")?,
                        std::mem::take(&mut probabilities),
                        out,
                    )?;
                }
            }
        }
        if let Some(reason) = string(candidate, "finishReason")? {
            if let Some(report) = super::safety::report(candidate, SafetySubject::Output)? {
                self.emit(StreamEvent::SafetyReport(report), out)?;
            }
            let incomplete = finish_reason(reason)?;
            return self.terminal(incomplete, out);
        }
        if let Some(report) = super::safety::report(candidate, SafetySubject::Output)? {
            self.emit(StreamEvent::SafetyReport(report), out)?;
        }
        Ok(())
    }
    fn part(
        &mut self,
        p: &Map<String, Value>,
        probabilities: Vec<Logprob>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), SseError> {
        const UNMODELED: &[&str] = &[
            "inlineData",
            "fileData",
            "functionResponse",
            "executableCode",
            "codeExecutionResult",
        ];
        if let Some(key) = UNMODELED.iter().find(|k| p.contains_key(**k)) {
            return Err(unsupported(&format!("part {key}")));
        }
        let signature = match string(p, "thoughtSignature")? {
            Some("") => return Err(invalid("thoughtSignature")),
            other => other.map(str::to_owned),
        };
        let thought = match p.get("thought") {
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err(invalid("thought")),
        };
        match (string(p, "text")?, p.get("functionCall")) {
            (Some(fragment), None) if thought => {
                if !matches!(
                    self.open,
                    Some(Open {
                        // A signature closes its part; later text has another owner.
                        kind: OpenKind::Thought { signature: None },
                        ..
                    })
                ) {
                    self.close(out)?;
                    let (item, part) = self.next_ids()?;
                    self.emit(
                        StreamEvent::ItemStarted {
                            item,
                            kind: ItemKind::Reasoning,
                            replay: None,
                        },
                        out,
                    )?;
                    self.open = Some(Open {
                        item,
                        part,
                        kind: OpenKind::Thought { signature: None },
                        part_started: false,
                    });
                }
                if let Some(Open {
                    kind: OpenKind::Thought { signature: kept },
                    ..
                }) = &mut self.open
                    && signature.is_some()
                {
                    *kept = signature;
                }
                self.delta(fragment, PartKind::Summary, vec![], out)
            }
            (Some(fragment), None) => {
                if !fragment.is_empty() || signature.is_some() {
                    if !matches!(
                        self.open,
                        Some(Open {
                            kind: OpenKind::Text { .. },
                            ..
                        })
                    ) {
                        self.close(out)?;
                        let (item, part) = self.next_ids()?;
                        self.emit(
                            StreamEvent::ItemStarted {
                                item,
                                kind: ItemKind::Message { phase: None },
                                replay: None,
                            },
                            out,
                        )?;
                        self.open = Some(Open {
                            item,
                            part,
                            kind: OpenKind::Text { signature: None },
                            part_started: false,
                        });
                    }
                    self.delta(fragment, PartKind::Text, probabilities, out)?;
                }
                if let Some(signature) = signature {
                    let open = self.open.as_mut().expect("text opened");
                    let (item, part) = (open.item, open.part);
                    if !open.part_started {
                        open.part_started = true;
                        self.emit(
                            StreamEvent::PartStarted {
                                item,
                                part,
                                kind: PartKind::Text,
                            },
                            out,
                        )?;
                    }
                    self.open.as_mut().expect("text opened").kind = OpenKind::Text {
                        signature: Some(signature),
                    };
                    self.close(out)?;
                }
                Ok(())
            }
            (None, Some(call)) if !thought => {
                self.close(out)?;
                let call = object(call, "functionCall")?;
                // Gemini 3 always returns call IDs; the gateway mints none.
                let id = string(call, "id")?
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| invalid("functionCall.id"))?;
                let name = string(call, "name")?
                    .filter(|n| !n.is_empty())
                    .ok_or_else(|| invalid("functionCall.name"))?;
                let args = match call.get("args") {
                    None => Value::Object(Map::new()),
                    Some(v @ Value::Object(_)) => v.clone(),
                    Some(_) => return Err(invalid("functionCall.args")),
                };
                if self.calls.iter().any(|c| c == id) {
                    return Err(invalid("duplicate Gemini call"));
                }
                self.calls.push(id.into());
                let replay = if let Some(signature) = signature {
                    Some(ItemReplay {
                        value: ReplayValue::final_value(
                            ReplayFormat::GoogleGenerateContentSignature,
                            text(&signature)?,
                        ),
                        origin: Some(self.target.origin()),
                    })
                } else {
                    None
                };
                let signed = replay.is_some();
                let (item, part) = self.next_ids()?;
                self.emit(
                    StreamEvent::ItemStarted {
                        item,
                        kind: ItemKind::ToolCall {
                            format: ArgumentFormat::Json,
                            call_id: text(id)?,
                            name: text(name)?,
                            context: Default::default(),
                        },
                        replay: None,
                    },
                    out,
                )?;
                let arguments =
                    serde_json::to_string(&args).map_err(|_| invalid("functionCall.args"))?;
                for event in [
                    StreamEvent::PartStarted {
                        item,
                        part,
                        kind: PartKind::StructuredArguments,
                    },
                    StreamEvent::Delta {
                        item,
                        part,
                        fragment: arguments,
                        logprobs: vec![],
                    },
                    StreamEvent::ValueFinished { item, part },
                    StreamEvent::PartFinished { item, part },
                    StreamEvent::ItemFinished {
                        item,
                        status: ItemLifecycle::Completed,
                        replay,
                    },
                ] {
                    self.emit(event, out)?;
                }
                if signed {
                    let Item::ToolCall(call) = self
                        .state
                        .item(item)
                        .and_then(|i| i.snapshot())
                        .map_err(CodecError::from)?
                    else {
                        return Err(invalid("Gemini call replay owner"));
                    };
                    self.fidelity
                        .record_call_replay(item, &call, Some(self.target.origin()))?;
                }
                Ok(())
            }
            _ => Err(invalid("Gemini part data")),
        }
    }
    fn delta(
        &mut self,
        fragment: &str,
        kind: PartKind,
        logprobs: Vec<Logprob>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), SseError> {
        if fragment.is_empty() {
            return Ok(());
        }
        let open = self.open.as_mut().ok_or_else(|| invalid("Gemini part"))?;
        let (item, part) = (open.item, open.part);
        let start = !open.part_started;
        open.part_started = true;
        if start {
            self.emit(StreamEvent::PartStarted { item, part, kind }, out)?;
        }
        self.emit(
            StreamEvent::Delta {
                item,
                part,
                fragment: fragment.into(),
                logprobs,
            },
            out,
        )
    }
    fn finish_reasoning(
        &mut self,
        item: ItemId,
        value: Signed,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), SseError> {
        let value = ReplayValue::final_value(
            ReplayFormat::GoogleGenerateContentPart,
            Text::new(value.encode(), "Gemini signature", MAX_TEXT_BYTES)
                .map_err(|_| SseError::Limit)?,
        );
        self.emit(
            StreamEvent::ItemFinished {
                item,
                status: ItemLifecycle::Completed,
                replay: Some(ItemReplay {
                    value,
                    origin: Some(self.target.origin()),
                }),
            },
            out,
        )?;
        let Item::Reasoning(reasoning) = self
            .state
            .item(item)
            .and_then(|i| i.snapshot())
            .map_err(CodecError::from)?
        else {
            return Err(invalid("Gemini reasoning owner"));
        };
        self.fidelity
            .record_replay(item, &reasoning, Some(self.target.origin()))?;
        Ok(())
    }
    fn close(&mut self, out: &mut Vec<StreamEvent>) -> Result<(), SseError> {
        let Some(open) = self.open.take() else {
            return Ok(());
        };
        if open.part_started {
            self.emit(
                StreamEvent::ValueFinished {
                    item: open.item,
                    part: open.part,
                },
                out,
            )?;
            if let OpenKind::Text {
                signature: Some(signature),
            } = &open.kind
            {
                self.emit(
                    StreamEvent::PartReplay {
                        item: open.item,
                        part: open.part,
                        replay: ItemReplay {
                            value: ReplayValue::final_value(
                                ReplayFormat::GoogleGenerateContentSignature,
                                text(signature)?,
                            ),
                            origin: Some(self.target.origin()),
                        },
                    },
                    out,
                )?;
            }
            self.emit(
                StreamEvent::PartFinished {
                    item: open.item,
                    part: open.part,
                },
                out,
            )?;
        }
        let signed_text = matches!(open.kind, OpenKind::Text { signature: Some(_) });
        match open.kind {
            OpenKind::Thought { signature } => {
                self.finish_reasoning(open.item, Signed { signature }, out)
            }
            OpenKind::Text { .. } => self.emit(
                StreamEvent::ItemFinished {
                    item: open.item,
                    status: ItemLifecycle::Completed,
                    replay: None,
                },
                out,
            ),
        }?;
        if signed_text {
            let Item::Message(message) = self
                .state
                .item(open.item)
                .and_then(|i| i.snapshot())
                .map_err(CodecError::from)?
            else {
                return Err(invalid("Gemini text replay owner"));
            };
            self.fidelity.record_part_replay(
                open.item,
                message.status,
                0,
                &message.parts[0],
                Some(self.target.origin()),
            )?;
        }
        Ok(())
    }
    fn terminal(
        &mut self,
        incomplete: Option<IncompleteReason>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), SseError> {
        self.close(out)?;
        self.finished = true;
        let group = GroupId::new(self.scope, 1);
        let envelope = if self.members.is_empty() {
            MessageEnvelope::empty(group, MessageEnvelopeRole::Assistant, None)
        } else {
            MessageEnvelope::new(group, MessageEnvelopeRole::Assistant, self.members.clone())
                .map_err(CodecError::from)?
        };
        self.emit(StreamEvent::MessageEnvelope(envelope), out)?;
        if let Some(usage) = self.usage.take() {
            self.emit(StreamEvent::UsageReport(usage), out)?;
        }
        let (terminal, progress) = match &incomplete {
            Some(_) => (StreamTerminal::Incomplete, InteractionProgress::Unreported),
            None if self.calls.is_empty() => {
                (StreamTerminal::Completed, InteractionProgress::TurnFinished)
            }
            None => (
                StreamTerminal::Completed,
                InteractionProgress::AwaitingToolResults,
            ),
        };
        if progress != InteractionProgress::Unreported {
            self.emit(StreamEvent::Progress(progress), out)?;
        }
        self.emit(
            StreamEvent::Terminal {
                terminal,
                details: TerminalDetails {
                    error: None,
                    completion: incomplete
                        .is_none()
                        .then_some(CompletionCause::NaturalOrStopSequence),
                    incomplete,
                },
            },
            out,
        )
    }
    /// Strict transport closure after a finish reason releases the aggregate.
    pub fn finish(&mut self) -> Result<DecodedResponse, SseError> {
        if self.closed || self.rejected {
            return Err(SseError::Closed);
        }
        self.closed = true;
        let result = (|| {
            self.framing.finish_strict()?;
            if !self.finished {
                return Err(invalid("Gemini stream without finish reason"));
            }
            Ok(DecodedResponse {
                semantic: materialize(&self.state).map_err(CodecError::from)?,
                fidelity: self.fidelity.clone(),
                metadata: self
                    .metadata
                    .clone()
                    .ok_or_else(|| invalid("Gemini metadata"))?,
            })
        })();
        if result.is_err() {
            self.rejected = true;
        }
        result
    }
}
