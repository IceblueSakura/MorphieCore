//! Bounded Interactions step intake; terminals are released only after strict EOF.
//! Source: <https://ai.google.dev/gemini-api/docs/interactions/streaming>.
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

enum Kind {
    Thought,
    Text,
    Call { initial: Value },
}
struct Open {
    index: u64,
    item: ItemId,
    part: PartId,
    kind: Kind,
    started: bool,
    signature: Option<String>,
}
pub struct GenerationSseDecoder {
    framing: SseDecoder,
    limits: SseLimits,
    bytes: usize,
    frames: usize,
    scope: LocalScope,
    target: ReplayTarget,
    metadata: Option<ResponseMetadata>,
    state: StreamState,
    fidelity: FidelityRecords,
    open: Option<Open>,
    // Source ordinals include empty output steps, unlike semantic members.
    steps: Vec<Option<ItemId>>,
    members: Vec<ItemId>,
    calls: Vec<String>,
    completed: bool,
    done: bool,
    closed: bool,
    rejected: bool,
}
pub(super) fn invalid(what: &'static str) -> SseError {
    CodecError::Invalid(what).into()
}
pub(super) fn object<'a>(
    v: &'a Value,
    what: &'static str,
) -> Result<&'a Map<String, Value>, SseError> {
    v.as_object().ok_or_else(|| invalid(what))
}
pub(super) fn string<'a>(
    o: &'a Map<String, Value>,
    key: &'static str,
) -> Result<Option<&'a str>, SseError> {
    match o.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        _ => Err(invalid(key)),
    }
}
pub(super) fn required<'a>(
    o: &'a Map<String, Value>,
    key: &'static str,
) -> Result<&'a str, SseError> {
    string(o, key)?
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid(key))
}
pub(super) fn field<'a>(
    o: &'a Map<String, Value>,
    key: &'static str,
) -> Result<&'a Value, SseError> {
    o.get(key).ok_or_else(|| invalid(key))
}
fn check_lifecycle(i: &Map<String, Value>) -> Result<(), SseError> {
    // Service reports still need a selected source baseline and mapping.
    check_continuation(i)?;
    if i.get("service_tier").is_some_and(|v| !v.is_null()) {
        return Err(CodecError::Unsupported("Interactions service_tier".into()).into());
    }
    Ok(())
}
fn check_continuation(o: &Map<String, Value>) -> Result<(), SseError> {
    if let Some(value) = o.get("continuation_token").filter(|v| !v.is_null()) {
        if !value.is_string() {
            return Err(invalid("continuation_token"));
        }
        return Err(CodecError::Unsupported("Interactions continuation_token".into()).into());
    }
    Ok(())
}
fn status(o: &Map<String, Value>) -> Result<&str, SseError> {
    match required(o, "status")? {
        value @ ("in_progress" | "requires_action" | "completed" | "failed" | "cancelled"
        | "incomplete") => Ok(value),
        // SDK additions do not activate background or budget-resumption support.
        "queued" | "budget_exceeded" => {
            Err(CodecError::Unsupported("Interactions status".into()).into())
        }
        _ => Err(invalid("Interactions status")),
    }
}
pub(super) fn check_annotations(p: &Map<String, Value>) -> Result<(), SseError> {
    if p.get("annotations")
        .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
    {
        return Err(CodecError::Unsupported("Interactions annotations".into()).into());
    }
    Ok(())
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
            bytes: 0,
            frames: 0,
            scope,
            target,
            metadata: None,
            state: StreamState::new(),
            fidelity: Default::default(),
            open: None,
            steps: vec![],
            members: vec![],
            calls: vec![],
            completed: false,
            done: false,
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
    pub fn consume(&mut self, chunk: &[u8]) -> Result<(usize, Vec<StreamEvent>), SseError> {
        if self.closed || self.rejected {
            return Err(SseError::Closed);
        }
        let result = (|| {
            let available = chunk.len().min(
                self.limits
                    .max_wire_bytes
                    .saturating_sub(self.bytes)
                    .saturating_add(1),
            );
            let (frame, n) = self.framing.push_until_event(&chunk[..available])?;
            self.bytes = self.bytes.checked_add(n).ok_or(SseError::Limit)?;
            if self.bytes > self.limits.max_wire_bytes {
                return Err(SseError::Limit);
            }
            let mut out = vec![];
            if let Some(frame) = frame {
                self.frames = self.frames.checked_add(1).ok_or(SseError::Limit)?;
                if self.frames > self.limits.max_events {
                    return Err(SseError::Limit);
                }
                if self.done {
                    return Err(invalid("Interactions data after done"));
                }
                if frame.data() == "[DONE]" {
                    if !self.completed || frame.event().is_some_and(|e| e != "done") {
                        return Err(invalid("Interactions premature done"));
                    }
                    self.done = true;
                } else {
                    let v =
                        parse_json(frame.data().as_bytes(), JsonLimits::ENVELOPE).map_err(|e| {
                            match e {
                                crate::semantic::value::JsonError::Limit => SseError::Limit,
                                _ => invalid("Interactions JSON"),
                            }
                        })?;
                    let o = object(&v, "Interactions event")?;
                    let kind = required(o, "event_type")?;
                    if frame.event().is_some_and(|e| e != kind) {
                        return Err(invalid("Interactions event identity"));
                    }
                    string(o, "event_id")?;
                    self.event(kind, o, &mut out)?;
                }
            }
            Ok((n, out))
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
    fn ids(&mut self) -> Result<(ItemId, PartId), SseError> {
        if self.members.len() >= MAX_ITEMS {
            return Err(SseError::Limit);
        }
        let n = self.members.len() as u64 + 1;
        let item = ItemId::scoped(self.scope, n);
        self.members.push(item);
        Ok((item, PartId::scoped(self.scope, n * (MAX_ITEMS as u64 + 1))))
    }
    fn check_identity(&self, o: &Map<String, Value>, key: &'static str) -> Result<(), SseError> {
        if self.metadata.as_ref().and_then(|m| m.id.as_deref()) != Some(required(o, key)?) {
            return Err(invalid("interaction identity changed"));
        }
        Ok(())
    }
    fn event(
        &mut self,
        kind: &str,
        o: &Map<String, Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), SseError> {
        if self.completed {
            return Err(invalid("Interactions event after completion"));
        }
        if kind == "error" {
            let error = object(field(o, "error")?, "Interactions error")?;
            let code = match error.get("code") {
                Some(Value::Number(n)) => n
                    .as_u64()
                    .and_then(|n| u16::try_from(n).ok())
                    .filter(|n| (400..600).contains(n))
                    .unwrap_or(500),
                Some(Value::String(s)) => match s.as_str() {
                    "RESOURCE_EXHAUSTED" => 429,
                    "INVALID_ARGUMENT" => 400,
                    "UNAUTHENTICATED" => 401,
                    "PERMISSION_DENIED" => 403,
                    "UNAVAILABLE" => 503,
                    _ => 500,
                },
                _ => return Err(invalid("Interactions error code")),
            };
            string(error, "message")?;
            return Err(CodecError::StreamFailure(code).into());
        }
        if kind == "interaction.created" {
            if self.metadata.is_some() {
                return Err(invalid("duplicate interaction"));
            }
            let i = object(field(o, "interaction")?, "interaction")?;
            check_lifecycle(i)?;
            super::snapshots::reconcile(i.get("steps"), &self.steps, &self.state)?;
            // Fresh foreground streams start actively. Other schema-valid opening
            // states require a separately selected admission/observation contract.
            if status(i)? != "in_progress" {
                return Err(CodecError::Unsupported("Interactions creation status".into()).into());
            }
            if string(i, "object")?.is_some_and(|v| v != "interaction") {
                return Err(invalid("interaction object"));
            }
            let metadata = ResponseMetadata {
                storage: None,
                id: Some(required(i, "id")?.into()),
                model: required(i, "model")?.into(),
                created: None,
                context: Default::default(),
                instruction_fidelity: Default::default(),
            };
            metadata.validate()?;
            self.metadata = Some(metadata);
            self.emit(StreamEvent::Started, out)?;
            if let Some(value) = i.get("usage").filter(|v| !v.is_null()) {
                self.emit(
                    StreamEvent::UsageReport(usage(
                        value,
                        UsageScope::Operation,
                        UsageStage::Checkpoint,
                    )?),
                    out,
                )?;
            }
            return Ok(());
        }
        if self.metadata.is_none() {
            return Err(invalid("Interactions event before creation"));
        }
        match kind {
            "interaction.status_update" => {
                self.check_identity(o, "interaction_id")?;
                check_continuation(o)?;
                // Updates are observations, not completion or tool authorization.
                // The final envelope alone supplies outcome/progress; no transition
                // graph or equality with an earlier checkpoint is inferred.
                status(o)?;
                if let Some(value) = o.get("usage").filter(|v| !v.is_null()) {
                    self.emit(
                        StreamEvent::UsageReport(usage(
                            value,
                            UsageScope::Operation,
                            UsageStage::Checkpoint,
                        )?),
                        out,
                    )?;
                }
                Ok(())
            }
            "step.start" => self.start(o, out),
            "step.delta" | "step.stop" => {
                let index = field(o, "index")?
                    .as_u64()
                    .ok_or_else(|| invalid("step index"))?;
                if self.open.as_ref().is_none_or(|s| s.index != index) {
                    return Err(invalid("Interactions step owner"));
                }
                if kind == "step.stop" {
                    self.stop(out)?;
                    for (field, scope, stage) in [
                        ("usage", UsageScope::Operation, UsageStage::Checkpoint),
                        ("step_usage", UsageScope::Step(index), UsageStage::Final),
                    ] {
                        if let Some(value) = o.get(field).filter(|v| !v.is_null()) {
                            self.emit(StreamEvent::UsageReport(usage(value, scope, stage)?), out)?;
                        }
                    }
                    Ok(())
                } else {
                    self.delta(object(field(o, "delta")?, "delta")?, out)
                }
            }
            "interaction.completed" => {
                self.terminal(object(field(o, "interaction")?, "interaction")?, out)
            }
            _ => Err(CodecError::Unsupported("Interactions event type".into()).into()),
        }
    }
    fn start(
        &mut self,
        o: &Map<String, Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), SseError> {
        let index = field(o, "index")?
            .as_u64()
            .ok_or_else(|| invalid("step index"))?;
        if self.open.is_some() || index != self.steps.len() as u64 {
            return Err(invalid("Interactions step order"));
        }
        if self.steps.len() >= MAX_ITEMS {
            return Err(SseError::Limit);
        }
        let s = object(field(o, "step")?, "step")?;
        if s.get("error").is_some_and(|v| !v.is_null()) {
            return Err(CodecError::Unsupported("Interactions model_output error".into()).into());
        }
        string(s, "id")?;
        string(s, "name")?;
        if s.get("arguments").is_some_and(|v| !v.is_object()) {
            return Err(invalid("call arguments"));
        }
        let signature = string(s, "signature")?.map(str::to_owned);
        if signature.as_ref().is_some_and(String::is_empty) {
            return Err(invalid("step signature"));
        }
        let step_type = required(s, "type")?;
        if signature.is_some() && step_type != "thought" {
            return Err(CodecError::Unsupported("Interactions signature owner".into()).into());
        }
        let (kind, item_kind) = match step_type {
            "thought" => (Kind::Thought, ItemKind::Reasoning),
            "model_output" if signature.is_none() => {
                (Kind::Text, ItemKind::Message { phase: None })
            }
            "function_call" => {
                let id = required(s, "id")?;
                let name = required(s, "name")?;
                if self.calls.iter().any(|c| c == id) {
                    return Err(invalid("duplicate Interactions call"));
                }
                let initial = s
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| Value::Object(Map::new()));
                if !initial.is_object() {
                    return Err(invalid("call arguments"));
                }
                self.calls.push(id.into());
                let text = |v| {
                    Text::new(v, "Interactions call", MAX_TEXT_BYTES).map_err(|_| SseError::Limit)
                };
                (
                    Kind::Call { initial },
                    ItemKind::ToolCall {
                        format: ArgumentFormat::Json,
                        call_id: text(id)?,
                        name: text(name)?,
                        context: Default::default(),
                    },
                )
            }
            _ => return Err(CodecError::Unsupported("Interactions step type".into()).into()),
        };
        let (item, part) = self.ids()?;
        // A token-limited response can open and close an empty model_output step.
        // Do not invent an empty semantic message or publish its lifecycle.
        if !matches!(kind, Kind::Text) {
            self.emit(
                StreamEvent::ItemStarted {
                    item,
                    kind: item_kind,
                    replay: None,
                },
                out,
            )?;
        }
        self.open = Some(Open {
            index,
            item,
            part,
            kind,
            started: false,
            signature,
        });
        for (field_name, part_kind) in [("summary", PartKind::Summary), ("content", PartKind::Text)]
        {
            if let Some(v) = s.get(field_name) {
                let valid = matches!(
                    (&self.open.as_ref().expect("open").kind, field_name),
                    (Kind::Thought, "summary") | (Kind::Text, "content")
                );
                if !valid {
                    return Err(invalid("Interactions step content"));
                }
                let parts = v.as_array().ok_or_else(|| invalid("step content"))?;
                if parts.len() > 1 {
                    return Err(CodecError::Unsupported(
                        "Interactions multipart step content".into(),
                    )
                    .into());
                }
                for part in parts {
                    let p = object(part, "step content part")?;
                    check_annotations(p)?;
                    if required(p, "type")? != "text" {
                        return Err(
                            CodecError::Unsupported("Interactions content type".into()).into()
                        );
                    }
                    self.fragment(
                        string(p, "text")?.ok_or_else(|| invalid("text"))?,
                        part_kind,
                        out,
                    )?;
                }
            }
        }
        Ok(())
    }
    fn fragment(
        &mut self,
        text: &str,
        kind: PartKind,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), SseError> {
        if text.is_empty() {
            return Ok(());
        }
        let open = self.open.as_mut().ok_or_else(|| invalid("step"))?;
        let (item, part, start) = (open.item, open.part, !open.started);
        let message = matches!(open.kind, Kind::Text);
        open.started = true;
        if start {
            if message {
                self.emit(
                    StreamEvent::ItemStarted {
                        item,
                        kind: ItemKind::Message { phase: None },
                        replay: None,
                    },
                    out,
                )?;
            }
            self.emit(StreamEvent::PartStarted { item, part, kind }, out)?;
        }
        self.emit(
            StreamEvent::Delta {
                item,
                part,
                fragment: text.into(),
                logprobs: vec![],
            },
            out,
        )
    }
    fn delta(
        &mut self,
        d: &Map<String, Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), SseError> {
        let open = self.open.as_mut().ok_or_else(|| invalid("step"))?;
        match (required(d, "type")?, &open.kind) {
            ("thought_signature", Kind::Thought) => {
                if open.signature.is_some() {
                    return Err(invalid("duplicate step signature"));
                }
                open.signature = Some(required(d, "signature")?.into());
                Ok(())
            }
            ("text", Kind::Text) => self.fragment(
                string(d, "text")?.ok_or_else(|| invalid("text"))?,
                PartKind::Text,
                out,
            ),
            ("thought_summary", Kind::Thought) => {
                let p = object(field(d, "content")?, "thought summary")?;
                check_annotations(p)?;
                if required(p, "type")? != "text" {
                    return Err(invalid("thought summary type"));
                }
                self.fragment(
                    string(p, "text")?.ok_or_else(|| invalid("text"))?,
                    PartKind::Summary,
                    out,
                )
            }
            ("arguments_delta", Kind::Call { initial, .. }) => {
                if initial.as_object().is_none_or(|o| !o.is_empty()) {
                    return Err(invalid("arguments delta after full arguments"));
                }
                self.fragment(
                    string(d, "arguments")?.ok_or_else(|| invalid("arguments"))?,
                    PartKind::StructuredArguments,
                    out,
                )
            }
            _ => Err(CodecError::Unsupported("Interactions delta type".into()).into()),
        }
    }
    fn reasoning(
        &mut self,
        item: ItemId,
        signed: Signed,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), SseError> {
        let value = ReplayValue::final_value(
            ReplayFormat::GoogleInteractionsV1Thought,
            Text::new(signed.encode(), "Interactions signature", MAX_TEXT_BYTES)
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
        let Item::Reasoning(r) = self
            .state
            .item(item)
            .and_then(|i| i.snapshot())
            .map_err(CodecError::from)?
        else {
            return Err(invalid("reasoning owner"));
        };
        self.fidelity
            .record_replay(item, &r, Some(self.target.origin()))?;
        Ok(())
    }
    fn stop(&mut self, out: &mut Vec<StreamEvent>) -> Result<(), SseError> {
        if let Some(Open {
            kind: Kind::Call { initial, .. },
            started: false,
            ..
        }) = &self.open
        {
            let initial = initial.to_string();
            self.fragment(&initial, PartKind::StructuredArguments, out)?;
        }
        let open = self.open.take().ok_or_else(|| invalid("step"))?;
        if matches!(open.kind, Kind::Text) && !open.started {
            self.members.pop();
            self.steps.push(None);
            return Ok(());
        }
        if open.started {
            self.emit(
                StreamEvent::ValueFinished {
                    item: open.item,
                    part: open.part,
                },
                out,
            )?;
            self.emit(
                StreamEvent::PartFinished {
                    item: open.item,
                    part: open.part,
                },
                out,
            )?;
        }
        if matches!(open.kind, Kind::Thought) {
            self.reasoning(
                open.item,
                Signed {
                    signature: open.signature,
                },
                out,
            )?;
        } else {
            self.emit(
                StreamEvent::ItemFinished {
                    item: open.item,
                    status: ItemLifecycle::Completed,
                    replay: None,
                },
                out,
            )?;
        }
        self.steps.push(Some(open.item));
        Ok(())
    }
    fn terminal(
        &mut self,
        i: &Map<String, Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), SseError> {
        if self.open.is_some() {
            return Err(invalid("unclosed Interactions step"));
        }
        check_lifecycle(i)?;
        self.check_identity(i, "id")?;
        super::snapshots::reconcile(i.get("steps"), &self.steps, &self.state)?;
        if let Some(model) = string(i, "model")?
            && self.metadata.as_ref().is_none_or(|m| m.model != model)
        {
            return Err(invalid("interaction model changed"));
        }
        let (terminal, progress, incomplete) = match status(i)? {
            "completed" if self.calls.is_empty() => (
                StreamTerminal::Completed,
                InteractionProgress::TurnFinished,
                None,
            ),
            "requires_action" if !self.calls.is_empty() => (
                StreamTerminal::Completed,
                InteractionProgress::AwaitingToolResults,
                None,
            ),
            // The status names no cause; a token limit is only one documented example.
            "incomplete" => (
                StreamTerminal::Incomplete,
                InteractionProgress::Unreported,
                Some(IncompleteReason::Unspecified),
            ),
            "failed" | "cancelled" => return Err(CodecError::StreamFailure(500).into()),
            _ => return Err(invalid("Interactions terminal status")),
        };
        let group = GroupId::new(self.scope, 1);
        let envelope = if self.members.is_empty() {
            MessageEnvelope::empty(group, MessageEnvelopeRole::Assistant, None)
        } else {
            MessageEnvelope::new(group, MessageEnvelopeRole::Assistant, self.members.clone())
                .map_err(CodecError::from)?
        };
        self.emit(StreamEvent::MessageEnvelope(envelope), out)?;
        if let Some(v) = i.get("usage").filter(|v| !v.is_null()) {
            self.emit(
                StreamEvent::UsageReport(usage(v, UsageScope::Operation, UsageStage::Final)?),
                out,
            )?;
        } else if let Some(mut latest) = self.state.usage_reports().operation().cloned() {
            latest.stage = UsageStage::Final;
            self.emit(StreamEvent::UsageReport(latest), out)?;
        }
        if progress != InteractionProgress::Unreported {
            self.emit(StreamEvent::Progress(progress), out)?;
        }
        self.emit(
            StreamEvent::Terminal {
                terminal,
                details: TerminalDetails {
                    error: None,
                    incomplete,
                    completion: None,
                },
            },
            out,
        )?;
        self.completed = true;
        Ok(())
    }
    pub fn finish(&mut self) -> Result<DecodedResponse, SseError> {
        if self.closed || self.rejected {
            return Err(SseError::Closed);
        }
        self.closed = true;
        let result = (|| {
            self.framing.finish_strict()?;
            if !self.completed || !self.done {
                return Err(invalid("unfinished Interactions stream"));
            }
            Ok(DecodedResponse {
                semantic: materialize(&self.state).map_err(CodecError::from)?,
                fidelity: self.fidelity.clone(),
                metadata: self
                    .metadata
                    .clone()
                    .ok_or_else(|| invalid("interaction metadata"))?,
            })
        })();
        if result.is_err() {
            self.rejected = true;
        }
        result
    }
}
