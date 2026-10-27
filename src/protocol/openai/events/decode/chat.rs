//! Incoming Chat chunks and their explicit framing-level DONE boundary.
use crate::protocol::{
    failure::{CarrierReason, ShapeReason},
    openai::events::*,
};
use crate::semantic::value::Text;
use std::collections::BTreeMap;

#[derive(Default)]
struct PendingCall {
    id: Option<Text>,
    name: Option<Text>,
    function: bool,
    arguments: String,
}

fn check_identity<'a>(
    value: Option<&'a Value>,
    expected: Option<&str>,
) -> Result<Option<&'a str>, CodecError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if expected.is_none_or(|old| old == s) => Ok(Some(s)),
        _ => Err(CodecError::Invalid(ShapeReason::CallIdentity)),
    }
}

/// Chat-only chunk bookkeeping. The shared stream state stays in the decoder.
#[derive(Default)]
pub(super) struct State {
    pub(super) pending: Option<(StreamTerminal, TerminalDetails)>,
    pub(super) finish_reason: Option<String>,
    pub(super) extras: crate::protocol::openai::dialect::adapter_shapes::Extras,
    pub(super) owner: Option<ItemId>,
    pub(super) calls: Vec<ItemId>,
    pending_calls: BTreeMap<usize, PendingCall>,
    pending_bytes: usize,
    pub(super) reasoning: Option<ItemId>,
    /// Message position of the open reasoning item's first detail.
    pub(super) detail_base: u64,
    pub(super) pending_part: Option<PartKind>,
    pub(super) audio_bytes: crate::protocol::openai::chat::audio::ChunkBytes,
}

impl EventDecoder {
    pub(super) fn chat(&mut self, o: &Map<String, Value>) -> Result<Vec<StreamEvent>, CodecError> {
        crate::protocol::openai::chat::items::reject_moderation(o)?;
        DecodeDirection::Upstream.fields(
            o,
            &[
                "id",
                "object",
                "created",
                "model",
                "choices",
                "usage",
                "system_fingerprint",
                "service_tier",
            ],
        )?;
        if string(o, "object")? != "chat.completion.chunk" {
            return Err(CodecError::Invalid(ShapeReason::Object));
        }
        let mut out = vec![];
        if self.metadata.is_none() {
            self.emit(StreamEvent::Started, &mut out)?;
        }
        self.observe_metadata(o)?;
        let choices = o
            .get("choices")
            .and_then(Value::as_array)
            .ok_or(CodecError::Invalid(ShapeReason::Choices))?;
        if self.chat.pending.is_some() {
            self.chat_usage_tail(choices, o, &mut out)?;
            return Ok(out);
        }
        if choices.len() != 1 {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        let choice = object(&choices[0])?;
        DecodeDirection::Upstream
            .fields(choice, &["index", "delta", "finish_reason", "logprobs"])?;
        // Usage belongs to the terminal: either this finish chunk or a trailing
        // empty-choices chunk. Anything earlier is rejected, never guessed.
        let terminal_usage = o.get("usage").filter(|v| !v.is_null());
        if terminal_usage.is_some()
            && choice
                .get("finish_reason")
                .filter(|v| !v.is_null())
                .is_none()
        {
            return Err(CodecError::Invalid(ShapeReason::EarlyUsage));
        }
        if choice.get("index").and_then(Value::as_u64) != Some(0) {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        let probabilities = crate::protocol::openai::chat::logprobs::read(choice.get("logprobs"))?;
        let delta = object(
            choice
                .get("delta")
                .ok_or(CodecError::Invalid(ShapeReason::Delta))?,
        )?;
        reject_chat_orchestration(delta)?;
        if delta.get("function_call").is_some_and(|v| !v.is_null()) {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        DecodeDirection::Upstream.fields(
            delta,
            &[
                "role",
                "content",
                "refusal",
                "tool_calls",
                "reasoning_content",
                "reasoning",
                "reasoning_details",
                "audio",
            ],
        )?;
        if let Some(role) = delta.get("role").filter(|v| !v.is_null())
            && role.as_str() != Some("assistant")
        {
            return Err(CodecError::Invalid(ShapeReason::Delta));
        }
        self.chat_reasoning_delta(delta, &mut out)?;
        self.chat_message_parts(delta, &mut out)?;
        self.chat_audio_delta(delta, &mut out)?;
        self.chat_probabilities(delta, probabilities, &mut out)?;
        self.chat_tool_calls(delta, &mut out)?;
        if let Some(reason) = choice.get("finish_reason").filter(|v| !v.is_null()) {
            self.chat_finish(reason, terminal_usage, &mut out)?;
        }
        Ok(out)
    }

    fn chat_usage_tail(
        &mut self,
        choices: &[Value],
        chunk: &Map<String, Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        if !choices.is_empty() {
            if !self.adaptation.rules.repeated_finish_usage || choices.len() != 1 {
                return Err(CodecError::Invalid(ShapeReason::AfterFinish));
            }
            let choice = object(&choices[0])?;
            DecodeDirection::Upstream
                .fields(choice, &["index", "delta", "finish_reason", "logprobs"])?;
            if choice.get("index").and_then(Value::as_u64) != Some(0)
                || choice.get("finish_reason").and_then(Value::as_str)
                    != self.chat.finish_reason.as_deref()
                || choice.get("logprobs").is_some_and(|v| !v.is_null())
            {
                return Err(CodecError::Invalid(ShapeReason::AfterFinish));
            }
            let delta = object(
                choice
                    .get("delta")
                    .ok_or(CodecError::Invalid(ShapeReason::Delta))?,
            )?;
            reject_chat_orchestration(delta)?;
            DecodeDirection::Upstream.fields(delta, &["role", "content"])?;
            // Explicit, not via `fields`: upstream decoding ignores unknown keys, but
            // late content after the finish must still fail rather than be dropped.
            if ["refusal", "tool_calls", "function_call", "audio"]
                .iter()
                .any(|key| delta.get(*key).is_some_and(|v| !v.is_null()))
            {
                return Err(CodecError::Invalid(ShapeReason::AfterFinish));
            }
            if [
                (
                    "reasoning_content",
                    self.adaptation.rules.readable_reasoning,
                ),
                ("reasoning", self.adaptation.rules.accept_reasoning_alias),
                (
                    "reasoning_details",
                    self.adaptation.rules.structured_chat_reasoning
                        || self.adaptation.rules.unversioned_chat_reasoning_view,
                ),
            ]
            .into_iter()
            .any(|(key, admitted)| admitted && delta.get(key).is_some_and(|v| !v.is_null()))
            {
                return Err(CodecError::Invalid(ShapeReason::AfterFinish));
            }
            if delta
                .get("role")
                .is_some_and(|v| !v.is_null() && v.as_str() != Some("assistant"))
                || delta
                    .get("content")
                    .is_some_and(|v| !v.is_null() && v.as_str() != Some(""))
            {
                return Err(CodecError::Invalid(ShapeReason::AfterFinish));
            }
        }
        let usage = crate::protocol::openai::accounting::usage(
            chunk.get("usage"),
            GenerationApi::Chat,
            &self.adaptation,
            &mut self.evidence,
            crate::protocol::conversion::generation::ConversionBoundary::EventDecode,
        )?
        .ok_or(CodecError::Invalid(ShapeReason::UsageTail))?;
        self.emit(StreamEvent::Usage(usage), out)?;
        Ok(())
    }

    fn chat_reasoning_delta(
        &mut self,
        delta: &Map<String, Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        let prior = self.chat.reasoning.and_then(|id| {
            self.state
                .as_ref()?
                .item(id)
                .ok()?
                .parts
                .first()
                .map(|p| p.kind)
        });
        let details = if self.adaptation.rules.structured_chat_reasoning
            || self.adaptation.rules.unversioned_chat_reasoning_view
        {
            crate::protocol::openai::chat::reasoning::parse(
                delta,
                &self.adaptation,
                prior,
                self.chat.detail_base,
                DecodeDirection::Upstream,
            )?
        } else {
            None
        };
        let kind = details
            .as_ref()
            .and_then(|d| d.part.as_ref().map(|(k, _)| *k))
            .unwrap_or(PartKind::ReasoningText);
        let fragment = if let Some((_, s)) = details.as_ref().and_then(|d| d.part.as_ref()) {
            Some(s.as_str())
        } else if self.adaptation.rules.readable_reasoning {
            delta
                .get("reasoning_content")
                .filter(|v| !v.is_null())
                .map(|v| v.as_str().ok_or(CodecError::invalid()))
                .transpose()?
        } else {
            None
        };
        if let Some(fragment) = fragment
            && (!fragment.is_empty() || details.as_ref().is_some_and(|d| d.part.is_some()))
        {
            let item = self.reasoning_owner(out)?;
            let part = match self
                .state()?
                .item(item)?
                .parts
                .first()
                .map(|p| (p.id, p.kind))
            {
                Some((part, old)) if old == kind => part,
                Some(_) => {
                    return Err(CodecError::Unsupported(CarrierReason::Reasoning));
                }
                None => {
                    let part = self.allocate_part()?;
                    self.emit(StreamEvent::PartStarted { item, part, kind }, out)?;
                    part
                }
            };
            self.emit(
                StreamEvent::Delta {
                    item,
                    part,
                    fragment: fragment.into(),
                    logprobs: vec![],
                },
                out,
            )?;
        }
        if let Some((wire_id, data)) = details.and_then(|d| d.responses_encrypted) {
            let item = self.reasoning_owner(out)?;
            self.fidelity.record_response_item_id(item, &wire_id)?;
            let replay = ItemReplay {
                value: ReplayValue::final_value(
                    ReplayFormat::ResponsesEncrypted,
                    text(&data, "encrypted reasoning", MAX_TEXT_BYTES)?,
                ),
                origin: self.origin.clone(),
            };
            // The complete encrypted detail closes its reasoning owner before
            // the carrier content; never wait for response finish to invent it.
            for part in self.state()?.item(item)?.parts.clone() {
                self.emit(
                    StreamEvent::ValueFinished {
                        item,
                        part: part.id,
                    },
                    out,
                )?;
                self.emit(
                    StreamEvent::PartFinished {
                        item,
                        part: part.id,
                    },
                    out,
                )?;
            }
            self.emit(
                StreamEvent::ItemFinished {
                    item,
                    status: ItemLifecycle::Completed,
                    replay: Some(replay),
                },
                out,
            )?;
            // The value closes its item; a later detail opens the next one.
            let details = self.state()?.item(item)?.parts.len() as u64 + 1;
            self.chat.detail_base += details;
            self.chat.reasoning = None;
        }
        Ok(())
    }

    fn chat_message_parts(
        &mut self,
        delta: &Map<String, Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        // One Chat candidate owns an assistant message even when it only contains
        // calls. The owner is allocated at the first message-owned signal so
        // readable reasoning keeps its static order ahead of the message.
        let message_owned = delta
            .get("content")
            .and_then(Value::as_str)
            .is_some_and(|fragment| !fragment.is_empty())
            || delta
                .get("refusal")
                .and_then(Value::as_str)
                .is_some_and(|fragment| !fragment.is_empty())
            || delta.get("tool_calls").is_some_and(|v| !v.is_null())
            || delta.get("audio").is_some_and(|v| !v.is_null());
        if message_owned {
            self.chat_message_owner(out)?;
        }
        let owner = self.chat.owner;
        for (key, kind) in [("content", PartKind::Text), ("refusal", PartKind::Refusal)] {
            if let Some(v) = delta.get(key).filter(|v| !v.is_null()) {
                let fragment = v.as_str().ok_or(CodecError::invalid())?;
                if fragment.is_empty() && self.chat.owner.is_none() {
                    // An opening empty fragment defers the carrier message so
                    // readable reasoning keeps its static order ahead of it.
                    if self.chat.pending_part.is_none() {
                        self.chat.pending_part = Some(kind);
                    }
                    continue;
                }
                let item = owner.ok_or(CodecError::invalid())?;
                let existing = self
                    .state()?
                    .item(item)?
                    .parts
                    .iter()
                    .find(|p| p.kind != PartKind::Audio)
                    .map(|p| (p.id, p.kind));
                if existing.is_none()
                    && self
                        .state()?
                        .item(item)?
                        .parts
                        .iter()
                        .any(|p| p.kind == PartKind::Audio)
                {
                    return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                }
                let part = if let Some((part, old_kind)) = existing {
                    if kind != old_kind {
                        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                    }
                    part
                } else {
                    let id = self.allocate_part()?;
                    self.emit(
                        StreamEvent::PartStarted {
                            item,
                            part: id,
                            kind,
                        },
                        out,
                    )?;
                    id
                };
                self.emit(
                    StreamEvent::Delta {
                        item,
                        part,
                        fragment: fragment.into(),
                        logprobs: vec![],
                    },
                    out,
                )?;
            }
        }
        Ok(())
    }

    fn chat_audio_delta(
        &mut self,
        delta: &Map<String, Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        if let Some(audio) = delta.get("audio").filter(|v| !v.is_null()) {
            let item = self.chat.owner.ok_or(CodecError::invalid())?;
            if self
                .state()?
                .item(item)?
                .parts
                .iter()
                .any(|p| p.kind == PartKind::Refusal)
            {
                return Err(CodecError::invalid());
            }
            let existing = self
                .state()?
                .item(item)?
                .parts
                .iter()
                .find(|p| p.kind == PartKind::Audio)
                .map(|p| p.id);
            let part = if let Some(part) = existing {
                part
            } else {
                let part = self.allocate_part()?;
                self.emit(
                    StreamEvent::PartStarted {
                        item,
                        part,
                        kind: PartKind::Audio,
                    },
                    out,
                )?;
                part
            };
            let update = crate::protocol::openai::chat::audio::read_update(
                audio,
                &mut self.chat.audio_bytes,
            )?;
            self.emit(StreamEvent::AudioDelta { item, part, update }, out)?;
        }
        Ok(())
    }

    fn chat_probabilities(
        &mut self,
        delta: &Map<String, Value>,
        probabilities: Vec<(PartKind, Vec<Logprob>)>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        for (kind, probs) in probabilities {
            let key = if kind == PartKind::Text {
                "content"
            } else {
                "refusal"
            };
            let existing = self.chat.owner.and_then(|item| {
                self.state()
                    .ok()?
                    .item(item)
                    .ok()?
                    .parts
                    .first()
                    .map(|p| (item, p.id, p.kind))
            });
            let (item, part) = if let Some((item, part, old_kind)) = existing {
                if kind != old_kind {
                    return Err(CodecError::invalid());
                }
                (item, part)
            } else {
                if delta.get(key).and_then(Value::as_str).is_none()
                    && self.chat.pending_part != Some(kind)
                {
                    return Err(CodecError::invalid());
                }
                let item = self.chat_message_owner(out)?;
                let part = self.allocate_part()?;
                self.emit(StreamEvent::PartStarted { item, part, kind }, out)?;
                self.chat.pending_part = None;
                (item, part)
            };
            if probs.is_empty() {
                if self.state()?.part(item, part)?.logprobs.is_absent() {
                    self.emit(
                        StreamEvent::LogprobsSnapshot {
                            item,
                            part,
                            logprobs: probs,
                        },
                        out,
                    )?;
                }
            } else {
                self.emit(
                    StreamEvent::Delta {
                        item,
                        part,
                        fragment: String::new(),
                        logprobs: probs,
                    },
                    out,
                )?;
            }
        }
        Ok(())
    }

    fn chat_tool_calls(
        &mut self,
        delta: &Map<String, Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        if let Some(calls) = delta.get("tool_calls").filter(|v| !v.is_null()) {
            for call in calls.as_array().ok_or(CodecError::invalid())? {
                let call = object(call)?;
                reject_chat_orchestration(call)?;
                if call.contains_key("custom") {
                    return Err(CodecError::Unsupported(CarrierReason::Tools));
                }
                DecodeDirection::Upstream.fields(call, &["index", "id", "type", "function"])?;
                let n = index(call, "index")?;
                let function = match call.get("type") {
                    None | Some(Value::Null) => false,
                    Some(Value::String(kind)) if kind == "function" => true,
                    _ => return Err(CodecError::Unsupported(CarrierReason::Tools)),
                };
                let empty = Map::new();
                let f = match call.get("function") {
                    None | Some(Value::Null) => &empty,
                    Some(value) => object(value)?,
                };
                reject_chat_orchestration(f)?;
                DecodeDirection::Upstream.fields(f, &["name", "arguments"])?;
                let fragment = f
                    .get("arguments")
                    .filter(|v| !v.is_null())
                    .map(|v| v.as_str().ok_or(CodecError::invalid()))
                    .transpose()?;
                if let Some(&item) = self.chat.calls.get(n) {
                    let ItemKind::ToolCall { call_id, name, .. } = &self.state()?.item(item)?.kind
                    else {
                        return Err(CodecError::invalid());
                    };
                    // Only the selected Provider sentinel permits an empty continuation ID.
                    if !(self.adaptation.rules.empty_continuation_call_id
                        && call.get("id").and_then(Value::as_str) == Some(""))
                    {
                        check_identity(call.get("id"), Some(call_id.as_str()))?;
                    }
                    check_identity(f.get("name"), Some(name.as_str()))?;
                    if let Some(fragment) = fragment {
                        let part = self.state()?.item(item)?.parts[0].id;
                        self.emit(
                            StreamEvent::Delta {
                                item,
                                part,
                                fragment: fragment.into(),
                                logprobs: vec![],
                            },
                            out,
                        )?;
                    }
                } else {
                    let pending = self.chat.pending_calls.entry(n).or_default();
                    let mut added = 0;
                    for (slot, value, label, limit) in [
                        (&mut pending.id, call.get("id"), "call id", 256),
                        (&mut pending.name, f.get("name"), "name", 128),
                    ] {
                        if let Some(value) =
                            check_identity(value, slot.as_ref().map(|v| v.as_str()))?
                            && slot.is_none()
                        {
                            let value = text(value, label, limit)?;
                            added += value.as_str().len();
                            *slot = Some(value);
                        }
                    }
                    pending.function |= function;
                    if let Some(fragment) = fragment {
                        if pending
                            .arguments
                            .len()
                            .checked_add(fragment.len())
                            .is_none_or(|n| n > MAX_TEXT_BYTES)
                        {
                            return Err(CodecError::Limit);
                        }
                        added += fragment.len();
                    }
                    self.chat.pending_bytes = self
                        .chat
                        .pending_bytes
                        .checked_add(added)
                        .ok_or(CodecError::Limit)?;
                    // The reducer charges published owners independently. Pending payload
                    // has its own aggregate ceiling; indexed slot overhead is also charged.
                    if self.chat.pending_bytes.saturating_add(
                        self.chat.pending_calls.len() * std::mem::size_of::<PendingCall>(),
                    ) > MAX_TOTAL_BYTES
                    {
                        return Err(CodecError::Limit);
                    }
                    if let Some(fragment) = fragment {
                        self.chat
                            .pending_calls
                            .get_mut(&n)
                            .unwrap()
                            .arguments
                            .push_str(fragment);
                    }
                    self.flush_chat_calls(false, out)?;
                }
            }
        }
        Ok(())
    }

    fn flush_chat_calls(
        &mut self,
        finalizing: bool,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        while let Some((&index, call)) = self.chat.pending_calls.first_key_value() {
            let ready = call.id.is_some() && call.name.is_some() && call.function;
            if !ready {
                return if finalizing {
                    Err(CodecError::Invalid(ShapeReason::CallIdentity))
                } else {
                    Ok(())
                };
            }
            // Before finish, a missing lower index may still arrive. Once the
            // source closes, flush sparse observed indices without inventing calls.
            if !finalizing && index != self.chat.calls.len() {
                break;
            }
            let call = self.chat.pending_calls.remove(&index).unwrap();
            let call_id = call.id.unwrap();
            let name = call.name.unwrap();
            self.chat.pending_bytes -=
                call_id.as_str().len() + name.as_str().len() + call.arguments.len();
            let item = self.allocate_item()?;
            let part = self.allocate_part()?;
            self.emit(
                StreamEvent::ItemStarted {
                    item,
                    kind: ItemKind::ToolCall {
                        format: ArgumentFormat::Raw,
                        call_id,
                        name,
                        context: CallContext::default(),
                    },
                    replay: None,
                },
                out,
            )?;
            self.emit(
                StreamEvent::PartStarted {
                    item,
                    part,
                    kind: PartKind::Arguments,
                },
                out,
            )?;
            if !call.arguments.is_empty() {
                self.emit(
                    StreamEvent::Delta {
                        item,
                        part,
                        fragment: call.arguments,
                        logprobs: vec![],
                    },
                    out,
                )?;
            }
            self.chat.calls.push(item);
        }
        Ok(())
    }

    fn chat_finish(
        &mut self,
        reason: &Value,
        terminal_usage: Option<&Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        self.flush_chat_calls(true, out)?;
        let owner = self.chat_message_owner(out)?;
        // A deferred opening fragment materializes its empty part only when
        // no other content won the part, matching static `content: ""`.
        if let Some(kind) = self.chat.pending_part.take()
            && self.state()?.item(owner)?.parts.is_empty()
        {
            let part = self.allocate_part()?;
            self.emit(
                StreamEvent::PartStarted {
                    item: owner,
                    part,
                    kind,
                },
                out,
            )?;
        }
        let terminal = match reason.as_str() {
            // Forced tool choice can report `stop` beside its calls.
            Some("stop") => StreamTerminal::Completed,
            Some("tool_calls") if !self.chat.calls.is_empty() => StreamTerminal::Completed,
            Some("length" | "content_filter") => StreamTerminal::Incomplete,
            _ => {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
        };
        let items = self.state()?.items().to_vec();
        for item in items {
            if item.status.is_some() {
                continue;
            }
            for part in item.parts {
                self.emit(
                    StreamEvent::ValueFinished {
                        item: item.id,
                        part: part.id,
                    },
                    out,
                )?;
                if part.kind == PartKind::Audio {
                    let StreamPartValue::Audio(audio) =
                        self.state()?.part(item.id, part.id)?.value()
                    else {
                        return Err(CodecError::invalid());
                    };
                    let content = ContentPart::Audio(audio.clone());
                    self.fidelity
                        .record_audio(part.id, &content, self.origin.clone())?;
                }
                self.emit(
                    StreamEvent::PartFinished {
                        item: item.id,
                        part: part.id,
                    },
                    out,
                )?;
            }
            self.emit(
                StreamEvent::ItemFinished {
                    item: item.id,
                    status: if terminal == StreamTerminal::Completed {
                        ItemLifecycle::Completed
                    } else {
                        ItemLifecycle::Incomplete
                    },
                    replay: None,
                },
                out,
            )?;
        }
        // The calls belong to the message that opened before them.
        if !self.chat.calls.is_empty() {
            if self
                .state()?
                .item(owner)?
                .parts
                .iter()
                .any(|p| p.kind == PartKind::Refusal)
            {
                return Err(CodecError::invalid());
            }
            let mut members = vec![owner];
            members.extend(&self.chat.calls);
            let group = MessageEnvelope::new(
                GroupId::new(owner.scope(), owner.get()),
                MessageEnvelopeRole::Assistant,
                members,
            )?;
            self.emit(StreamEvent::MessageEnvelope(group), out)?;
        }
        self.chat.finish_reason = reason.as_str().map(str::to_owned);
        self.chat.pending = Some((
            terminal,
            if terminal == StreamTerminal::Incomplete {
                TerminalDetails {
                    error: None,
                    completion: None,
                    incomplete: Some(if reason.as_str() == Some("content_filter") {
                        IncompleteReason::ContentFilter
                    } else {
                        IncompleteReason::MaxOutputTokens
                    }),
                    ..Default::default()
                }
            } else {
                TerminalDetails {
                    completion: (reason.as_str() == Some("stop"))
                        .then_some(CompletionCause::NaturalOrStopSequence),
                    ..Default::default()
                }
            },
        ));
        if let Some(value) = terminal_usage {
            let usage = crate::protocol::openai::accounting::usage(
                Some(value),
                GenerationApi::Chat,
                &self.adaptation,
                &mut self.evidence,
                crate::protocol::conversion::generation::ConversionBoundary::EventDecode,
            )?
            .ok_or(CodecError::Invalid(ShapeReason::UsageTail))?;
            self.emit(StreamEvent::Usage(usage), out)?;
        }
        Ok(())
    }

    fn chat_message_owner(&mut self, out: &mut Vec<StreamEvent>) -> Result<ItemId, CodecError> {
        if let Some(item) = self.chat.owner {
            return Ok(item);
        }
        let item = self.allocate_item()?;
        self.emit(
            StreamEvent::ItemStarted {
                item,
                kind: ItemKind::Message { phase: None },
                replay: None,
            },
            out,
        )?;
        self.chat.owner = Some(item);
        Ok(item)
    }

    fn reasoning_owner(&mut self, out: &mut Vec<StreamEvent>) -> Result<ItemId, CodecError> {
        if self.chat.owner.is_some() {
            return Err(CodecError::Unsupported(CarrierReason::Reasoning));
        }
        if let Some(item) = self.chat.reasoning {
            return Ok(item);
        }
        let item = self.allocate_item()?;
        self.emit(
            StreamEvent::ItemStarted {
                item,
                kind: ItemKind::Reasoning,
                replay: None,
            },
            out,
        )?;
        self.chat.reasoning = Some(item);
        Ok(item)
    }
}
