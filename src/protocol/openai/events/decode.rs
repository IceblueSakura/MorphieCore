//! Responses payload grammar over the shared semantic reducer.
use super::*;
use serde_json::json;

// Fixed structural failure labels only; never include IDs, text or ciphertext.
fn terminal_snapshot_difference(actual: &[Value], expected: &[Value]) -> &'static str {
    if actual.len() != expected.len() {
        return "terminal snapshot count";
    }
    for (a, e) in actual.iter().zip(expected) {
        if a == e {
            continue;
        }
        if a["id"] != e["id"] || a["type"] != e["type"] {
            return "terminal snapshot identity";
        }
        if a["status"] != e["status"] {
            return "terminal snapshot lifecycle";
        }
        if a["phase"] != e["phase"] {
            return "terminal snapshot phase";
        }
        if a["encrypted_content"] != e["encrypted_content"] {
            return match (
                a["encrypted_content"].is_null(),
                e["encrypted_content"].is_null(),
            ) {
                (false, true) => "terminal snapshot replay added",
                (true, false) => "terminal snapshot replay removed",
                _ => "terminal snapshot replay changed",
            };
        }
        if a["summary"] != e["summary"] {
            return "terminal snapshot summary";
        }
        if let (Some(ap), Some(ep)) = (a["content"].as_array(), e["content"].as_array()) {
            if ap.len() != ep.len() {
                return "terminal snapshot content";
            }
            for (p, q) in ap.iter().zip(ep) {
                if p["text"] != q["text"] {
                    return "terminal snapshot text";
                }
                if p["annotations"] != q["annotations"] {
                    return "terminal snapshot annotations";
                }
                if p["logprobs"] != q["logprobs"] {
                    let empty = |v: &Value| v.is_null() || v.as_array().is_some_and(Vec::is_empty);
                    return if empty(&p["logprobs"]) && empty(&q["logprobs"]) {
                        "terminal snapshot probability presence"
                    } else {
                        "terminal snapshot probabilities"
                    };
                }
            }
        }
        return "terminal snapshot";
    }
    "terminal snapshot"
}

pub struct EventDecoder {
    pub(super) profile: Profile,
    pub(super) adaptation: crate::protocol::adaptation::Adaptation,
    pub(super) state: Option<StreamState>,
    pub(super) fidelity: FidelityRecords,
    pub(super) metadata: Option<ResponseMetadata>,
    pub(super) origin: Option<ReplayOrigin>,
    pub(super) ids: Vec<ItemId>,
    pub(super) next_part: u64,
    pub(super) poisoned: bool,
    sequence: Option<u64>,
    events: usize,
    queued_creation_pending: bool,
    pub(super) chat_pending: Option<(StreamTerminal, TerminalDetails)>,
    pub(super) chat_finish_reason: Option<String>,
    chat_extras: super::super::adapter_shapes::Extras,
    pub(super) chat_owner: Option<ItemId>,
    pub(super) chat_calls: Vec<ItemId>,
    pub(super) chat_reasoning: Option<ItemId>,
    pub(super) chat_replay: Option<ReasoningReplay>,
    pub(super) chat_pending_part: Option<PartKind>,
    pub(super) chat_audio_bytes: super::super::chat_audio::ChunkBytes,
}
impl EventDecoder {
    pub fn new(profile: Profile) -> Self {
        Self {
            profile,
            adaptation: Default::default(),
            state: Some(StreamState::new()),
            fidelity: FidelityRecords::default(),
            metadata: None,
            origin: None,
            ids: vec![],
            next_part: 0,
            poisoned: false,
            sequence: None,
            events: 0,
            queued_creation_pending: false,
            chat_pending: None,
            chat_finish_reason: None,
            chat_extras: Default::default(),
            chat_owner: None,
            chat_calls: vec![],
            chat_reasoning: None,
            chat_replay: None,
            chat_pending_part: None,
            chat_audio_bytes: Default::default(),
        }
    }
    pub(crate) fn permits_responses_done(&self) -> bool {
        self.profile == Profile::Responses && self.adaptation.rules.responses_done_marker
    }
    pub(crate) fn validate_stream_payload(&self, value: &Value) -> Result<(), CodecError> {
        super::super::envelope::validate_stream_payload_with(value, &self.adaptation)
    }
    pub(crate) fn profile(&self) -> Profile {
        self.profile
    }
    pub(crate) fn with_adaptation(
        mut self,
        adaptation: crate::protocol::adaptation::Adaptation,
    ) -> Self {
        self.adaptation = adaptation;
        self
    }
    pub fn with_replay_origin(mut self, origin: ReplayOrigin) -> Self {
        self.origin = Some(origin);
        self
    }
    pub fn push(&mut self, payload: &Value) -> Result<Vec<StreamEvent>, CodecError> {
        if self.poisoned || self.state.as_ref().is_none_or(|s| s.terminal().is_some()) {
            return Err(CodecError::Invalid("event after terminal or rejection"));
        }
        self.events += 1;
        let result = (|| {
            if self.events > 1_000_000 {
                return Err(CodecError::Limit);
            }
            bounded(payload)?;
            let (normalized, extras) = if self.profile == Profile::Responses {
                if let Some(snapshot) = payload.get("response")
                    && (self.adaptation.rules.routing_extras
                        || self.adaptation.rules.responses_billing_view
                        || self.adaptation.rules.responses_product_accounting
                        || self.adaptation.rules.responses_context_accounting
                        || self.adaptation.rules.null_response_billing
                        || self.adaptation.rules.responses_inactive_state
                        || self.adaptation.rules.responses_reasoning_text_shorthand)
                {
                    let (snapshot, extras) = super::super::adapter_shapes::decode(
                        snapshot,
                        self.profile,
                        &self.adaptation,
                    )?;
                    let mut mapped = payload.clone();
                    mapped["response"] = snapshot.into_owned();
                    (std::borrow::Cow::Owned(mapped), extras)
                } else if (self.adaptation.rules.responses_reasoning_format
                    || self.adaptation.rules.responses_reasoning_text_shorthand)
                    && payload.get("item").is_some()
                {
                    let mut mapped = payload.clone();
                    if self.adaptation.rules.responses_reasoning_format {
                        super::super::adapter_shapes::reasoning_marker(&mut mapped["item"])?;
                    }
                    if self.adaptation.rules.responses_reasoning_text_shorthand {
                        super::super::adapter_shapes::reasoning_text_shorthand(
                            &mut mapped["item"],
                        )?;
                    }
                    (std::borrow::Cow::Owned(mapped), Default::default())
                } else {
                    (std::borrow::Cow::Borrowed(payload), Default::default())
                }
            } else {
                super::super::adapter_shapes::decode(payload, self.profile, &self.adaptation)?
            };
            let o = object(&normalized)?;
            if self.profile == Profile::Chat {
                self.adaptation.validate_response(self.profile, o)?;
            }
            if let Some(padding) = o.get("obfuscation")
                && (self.profile != Profile::Responses
                    || !string(o, "type")?.ends_with(".delta")
                    || !padding.as_str().is_some_and(|s| s.len() <= 4096))
            {
                return Err(CodecError::Invalid("obfuscation"));
            }
            if let Some(n) = o.get("sequence_number") {
                let n = n.as_u64().ok_or(CodecError::Invalid("sequence"))?;
                if self.sequence.is_some_and(|old| n <= old) {
                    return Err(CodecError::Invalid("sequence"));
                }
                self.sequence = Some(n);
            }
            let events = if self.profile == Profile::Responses {
                self.responses(o)?
            } else {
                self.chat(o)?
            };
            if self.profile == Profile::Chat && self.chat_pending.is_some() {
                self.chat_extras.extend(extras);
                super::super::adapter_shapes::check_budget(&self.chat_extras)?;
            } else if self.profile == Profile::Responses
                && !extras.is_empty()
                && self
                    .state()?
                    .terminal()
                    .is_some_and(|t| t != StreamTerminal::Error)
            {
                let semantic = materialize(self.state()?)?;
                let id = &self
                    .metadata
                    .as_ref()
                    .ok_or(CodecError::Invalid("metadata"))?
                    .id;
                self.fidelity.capture_routing_extras(
                    self.profile,
                    &self.adaptation,
                    extras,
                    &semantic,
                    id,
                )?;
            }
            Ok(events)
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    /// Available immediately after response.created; no whole-stream buffering is required.
    pub fn metadata(&self) -> Option<&ResponseMetadata> {
        self.metadata.as_ref()
    }
    pub fn fidelity(&self) -> &FidelityRecords {
        &self.fidelity
    }
    pub fn finish(&self) -> Result<(), CodecError> {
        if self.poisoned {
            return Err(CodecError::Invalid("rejected stream"));
        }
        end_of_stream(
            self.state
                .as_ref()
                .ok_or(CodecError::Invalid("stream state"))?,
        )?;
        Ok(())
    }
    /// Framing calls this only for a real Chat `[DONE]`, never for transport EOF.
    pub fn done(&mut self) -> Result<Vec<StreamEvent>, CodecError> {
        if self.profile != Profile::Chat || self.poisoned {
            return Err(CodecError::Invalid("DONE"));
        }
        let Some((terminal, details)) = self.chat_pending.take() else {
            self.poisoned = true;
            return Err(CodecError::Invalid("DONE before finish"));
        };
        let result = (|| {
            let mut out = vec![];
            self.emit(StreamEvent::Terminal { terminal, details }, &mut out)?;
            if !self.chat_extras.is_empty() {
                let semantic = materialize(self.state()?)?;
                let id = &self
                    .metadata
                    .as_ref()
                    .ok_or(CodecError::Invalid("metadata"))?
                    .id;
                self.fidelity.capture_routing_extras(
                    self.profile,
                    &self.adaptation,
                    std::mem::take(&mut self.chat_extras),
                    &semantic,
                    id,
                )?;
            }
            Ok(out)
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    pub fn materialize(&self) -> Result<super::super::DecodedResponse, CodecError> {
        self.finish()?;
        Ok(super::super::DecodedResponse {
            semantic: materialize(self.state.as_ref().ok_or(CodecError::Invalid("state"))?)?,
            fidelity: self.fidelity.clone(),
            metadata: self
                .metadata
                .clone()
                .ok_or(CodecError::Invalid("metadata"))?,
        })
    }
    pub fn into_parts(self) -> Result<(FidelityRecords, ResponseMetadata), CodecError> {
        self.finish()?;
        Ok((
            self.fidelity,
            self.metadata.ok_or(CodecError::Invalid("metadata"))?,
        ))
    }
    pub(super) fn state(&self) -> Result<&StreamState, CodecError> {
        self.state
            .as_ref()
            .ok_or(CodecError::Invalid("rejected stream"))
    }
    pub(super) fn emit(
        &mut self,
        event: StreamEvent,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        self.state = Some(reduce(
            self.state.take().ok_or(CodecError::Invalid("state"))?,
            event.clone(),
        )?);
        if let StreamEvent::ItemStarted { item, .. } | StreamEvent::ItemFinished { item, .. } =
            &event
        {
            sync_replays(
                self.state.as_ref().ok_or(CodecError::Invalid("state"))?,
                &mut self.fidelity,
                Some(*item),
            )?;
        }
        out.push(event);
        Ok(())
    }
    pub(super) fn allocate_item(&mut self) -> Result<ItemId, CodecError> {
        if self.ids.len() >= MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        let id = ItemId::new(self.ids.len() as u64 + 1);
        self.ids.push(id);
        Ok(id)
    }
    pub(super) fn allocate_part(&mut self) -> Result<PartId, CodecError> {
        if self.next_part >= MAX_ITEMS as u64 {
            return Err(CodecError::Limit);
        }
        self.next_part += 1;
        Ok(PartId::new(self.next_part))
    }
    pub(super) fn observe_metadata(&mut self, o: &Map<String, Value>) -> Result<(), CodecError> {
        let mut m = super::super::static_response::metadata(o, self.profile)?;
        if let Some(old) = &self.metadata {
            // Response identity is strict; providers re-stamp scalar facts such
            // as `created` and `system_fingerprint` per chunk, so the first
            // reported value binds and later drift is normalized away.
            if old.id != m.id || old.model != m.model {
                return Err(CodecError::Invalid("metadata changed"));
            }
            if old.created != m.created {
                if !(self.adaptation.rules.chunk_metadata_drift
                    || self.adaptation.rules.chunk_created_drift)
                {
                    return Err(CodecError::Invalid("metadata changed"));
                }
                m.created = old.created.clone();
            }
            if self.profile == Profile::Chat && !old.context.execution.service_tier.is_absent() {
                if !m.context.execution.service_tier.is_absent()
                    && old.context.execution.service_tier != m.context.execution.service_tier
                {
                    return Err(CodecError::Invalid("service tier changed"));
                }
                m.context.execution.service_tier = old.context.execution.service_tier.clone();
            }
            if !old.context.system_fingerprint.is_absent() {
                if !m.context.system_fingerprint.is_absent()
                    && old.context.system_fingerprint != m.context.system_fingerprint
                    && !self.adaptation.rules.chunk_metadata_drift
                {
                    return Err(CodecError::Invalid("fingerprint changed"));
                }
                m.context.system_fingerprint = old.context.system_fingerprint.clone();
            }
        }
        self.metadata = Some(m);
        // Vendor adaptation (ADR 0008) also covers response snapshots in streams.
        let profile = self.profile;
        super::super::envelope::record_vendor_shapes(
            profile,
            &self.adaptation,
            o,
            &mut self.fidelity,
            None,
        )?;
        Ok(())
    }
    fn responses(&mut self, o: &Map<String, Value>) -> Result<Vec<StreamEvent>, CodecError> {
        let mut out = vec![];
        let typ = string(o, "type")?;
        match typ {
            "error" => {
                event_fields(o, &["type", "code", "message", "param"])?;
                let mut e = o.clone();
                e.remove("type");
                e.remove("sequence_number");
                let details = super::super::terminal::decode_details(
                    &serde_json::from_value(json!({"error":e}))
                        .map_err(|_| CodecError::Invalid("error"))?,
                )?;
                self.emit(
                    StreamEvent::Terminal {
                        terminal: StreamTerminal::Error,
                        details,
                    },
                    &mut out,
                )?;
            }
            "response.queued" | "response.created" | "response.in_progress" => {
                event_fields(o, &["type", "response"])?;
                let r = object(o.get("response").ok_or(CodecError::Invalid("response"))?)?;
                super::super::envelope::response_fields(r)?;
                let queued_creation = self.adaptation.rules.responses_queued_creation
                    && typ == "response.created"
                    && r.get("status").and_then(Value::as_str) == Some("queued");
                let initial_status = if typ == "response.queued" || queued_creation {
                    "queued"
                } else {
                    "in_progress"
                };
                // The complete snapshot requires the output array; an omitted
                // value is never the explicit empty array.
                if string(r, "object")? != "response"
                    || string(r, "status")? != initial_status
                    || !r
                        .get("output")
                        .and_then(Value::as_array)
                        .is_some_and(Vec::is_empty)
                    || ["usage", "error", "incomplete_details"]
                        .iter()
                        .any(|k| r.get(*k).is_some_and(|v| !v.is_null()))
                {
                    return Err(CodecError::Invalid("initial response"));
                }
                match typ {
                    "response.queued" => self.emit(StreamEvent::Queued, &mut out)?,
                    "response.created" => {
                        if self.queued_creation_pending {
                            return Err(CodecError::Invalid("duplicate queued creation"));
                        }
                        if queued_creation {
                            self.emit(StreamEvent::Queued, &mut out)?;
                            self.queued_creation_pending = true;
                        } else {
                            self.emit(StreamEvent::Started, &mut out)?;
                        }
                    }
                    _ => {
                        // The queued snapshot cannot prove generation has started;
                        // this actual progress event supplies the transition.
                        if self.queued_creation_pending {
                            self.emit(StreamEvent::Started, &mut out)?;
                            self.queued_creation_pending = false;
                        }
                        if !self.state()?.started() {
                            return Err(CodecError::Invalid("response not started"));
                        }
                    }
                }
                self.observe_metadata(r)?;
            }
            "response.output_item.added" => self.item_added(o, &mut out)?,
            "response.output_text.annotation.added" => self.annotation_added(o, &mut out)?,
            "response.content_part.added" | "response.reasoning_summary_part.added" => {
                self.part_added(o, &mut out)?
            }
            "response.output_text.delta"
            | "response.refusal.delta"
            | "response.reasoning_summary_text.delta"
            | "response.reasoning_text.delta"
            | "response.function_call_arguments.delta"
            | "response.custom_tool_call_input.delta" => self.delta(o, &mut out)?,
            "response.output_text.done"
            | "response.refusal.done"
            | "response.reasoning_summary_text.done"
            | "response.reasoning_text.done"
            | "response.function_call_arguments.done"
            | "response.custom_tool_call_input.done" => self.value_done(o, &mut out)?,
            "response.content_part.done" | "response.reasoning_summary_part.done" => {
                self.part_done(o, &mut out)?
            }
            "response.output_item.done" => self.item_done(o, &mut out)?,
            "response.completed" | "response.incomplete" | "response.failed" => {
                self.terminal(o, &mut out)?
            }
            _ => return Err(CodecError::Unsupported(typ.into())),
        }
        Ok(out)
    }
    fn item_added(
        &mut self,
        o: &Map<String, Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        event_fields(o, &["type", "output_index", "item"])?;
        if index(o, "output_index")? != self.ids.len() {
            return Err(CodecError::Invalid("output index"));
        }
        let v = object(o.get("item").ok_or(CodecError::Invalid("item"))?)?;
        if v.get("status")
            .filter(|v| !v.is_null())
            .is_some_and(|s| s.as_str() != Some("in_progress"))
        {
            return Err(CodecError::Invalid("item status"));
        }
        let kind = match string(v, "type")? {
            "message" => {
                fields(v, &["type", "id", "role", "status", "content", "phase"])?;
                if string(v, "role")? != "assistant"
                    || !v
                        .get("content")
                        .and_then(Value::as_array)
                        .is_some_and(Vec::is_empty)
                {
                    return Err(CodecError::Invalid("initial message"));
                }
                ItemKind::Message {
                    phase: super::super::responses::read_phase(v)?,
                }
            }
            "reasoning" => {
                fields(
                    v,
                    &[
                        "type",
                        "id",
                        "status",
                        "summary",
                        "content",
                        "encrypted_content",
                    ],
                )?;
                if !v
                    .get("summary")
                    .and_then(Value::as_array)
                    .is_some_and(Vec::is_empty)
                    || v.get("content")
                        .is_some_and(|v| !v.is_null() && !v.as_array().is_some_and(Vec::is_empty))
                {
                    return Err(CodecError::Invalid("initial reasoning"));
                }
                ItemKind::Reasoning
            }
            "custom_tool_call" => {
                fields(
                    v,
                    &[
                        "type",
                        "id",
                        "call_id",
                        "name",
                        "input",
                        "namespace",
                        "caller",
                        "async",
                    ],
                )?;
                ItemKind::CustomCall {
                    call_id: text(string(v, "call_id")?, "call id", 256)?,
                    name: text(string(v, "name")?, "custom name", 128)?,
                    context: super::super::responses::call_context(v)?,
                }
            }
            "configuration_update" => {
                fields(v, &["type", "id", "reasoning"])?;
                ItemKind::ConfigurationUpdate {
                    effort: super::super::responses::configuration_effort(v, true)?,
                }
            }
            "program" => {
                fields(v, &["type", "id", "call_id", "code", "fingerprint"])?;
                ItemKind::Program {
                    call_id: text(string(v, "call_id")?, "call id", 256)?,
                    code: raw_string(v, "code")?,
                    fingerprint: raw_string(v, "fingerprint")?,
                }
            }
            "program_output" => {
                fields(v, &["type", "id", "call_id", "result", "status"])?;
                if v.get("status").filter(|v| !v.is_null()).is_some() {
                    // The done snapshot owns the terminal status; added is initial.
                    return Err(CodecError::Invalid("program output status"));
                }
                ItemKind::ProgramOutput {
                    call_id: text(string(v, "call_id")?, "call id", 256)?,
                    result: raw_string(v, "result")?,
                }
            }
            "function_call" => {
                fields(
                    v,
                    &[
                        "type",
                        "id",
                        "status",
                        "call_id",
                        "name",
                        "arguments",
                        "caller",
                        "namespace",
                        "async",
                    ],
                )?;
                ItemKind::ToolCall {
                    format: ArgumentFormat::Raw,
                    call_id: text(string(v, "call_id")?, "call id", 256)?,
                    name: text(string(v, "name")?, "tool name", 128)?,
                    message: None,
                    context: super::super::responses::call_context(v)?,
                }
            }
            _ => return Err(CodecError::Unsupported("item kind".into())),
        };
        let item = self.allocate_item()?;
        self.fidelity
            .record_response_item_id(item, string(v, "id")?)?;
        let replay = if matches!(kind, ItemKind::Reasoning) {
            replay(v, self.origin.as_ref(), false)?
        } else {
            None
        };
        self.emit(
            StreamEvent::ItemStarted {
                item,
                kind: kind.clone(),
                replay,
            },
            out,
        )?;
        if kind.call().is_some() {
            let part_kind = if matches!(kind, ItemKind::CustomCall { .. }) {
                PartKind::CustomInput
            } else if matches!(
                kind,
                ItemKind::ToolCall {
                    format: ArgumentFormat::Json,
                    ..
                }
            ) {
                PartKind::StructuredArguments
            } else {
                PartKind::Arguments
            };
            let part = self.allocate_part()?;
            self.emit(
                StreamEvent::PartStarted {
                    item,
                    part,
                    kind: part_kind,
                },
                out,
            )?;
            let fragment = string(
                v,
                if part_kind == PartKind::CustomInput {
                    "input"
                } else {
                    "arguments"
                },
            )?;
            if !fragment.is_empty() {
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
    fn owner(&self, o: &Map<String, Value>) -> Result<ItemId, CodecError> {
        let id = *self
            .ids
            .get(index(o, "output_index")?)
            .ok_or(CodecError::Invalid("output index"))?;
        if string(o, "item_id")? != self.fidelity.response_item_id(id).unwrap_or("") {
            return Err(CodecError::Invalid("item id"));
        }
        Ok(id)
    }
    fn part_for(&self, item: ItemId, kind: PartKind, n: usize) -> Result<PartId, CodecError> {
        // Responses summary and content indexes have independent namespaces.
        self.state()?
            .item(item)?
            .parts
            .iter()
            .filter(|p| {
                if kind == PartKind::Summary {
                    p.kind == PartKind::Summary
                } else {
                    p.kind != PartKind::Summary
                }
            })
            .nth(n)
            .filter(|p| p.kind == kind)
            .map(|p| p.id)
            .ok_or(CodecError::Invalid("part index"))
    }
    fn annotation_added(
        &mut self,
        o: &Map<String, Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        event_fields(
            o,
            &[
                "type",
                "item_id",
                "output_index",
                "content_index",
                "annotation_index",
                "annotation",
            ],
        )?;
        let item = self.owner(o)?;
        let part = self.part_for(item, PartKind::Text, index(o, "content_index")?)?;
        if index(o, "annotation_index")? != self.state()?.part(item, part)?.annotations.len() {
            return Err(CodecError::Invalid("annotation index"));
        }
        let v = o
            .get("annotation")
            .ok_or(CodecError::Invalid("annotation"))?;
        let annotation: Annotation =
            serde_json::from_value(v.clone()).map_err(|_| CodecError::Invalid("annotation"))?;
        if matches!(annotation, Annotation::FilePath { .. }) {
            return Err(CodecError::Unsupported("file_path annotation event".into()));
        }
        self.emit(
            StreamEvent::AnnotationAdded {
                item,
                part,
                annotation,
            },
            out,
        )?;
        Ok(())
    }
    fn part_added(
        &mut self,
        o: &Map<String, Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        event_fields(
            o,
            &[
                "type",
                "output_index",
                "item_id",
                "content_index",
                "summary_index",
                "part",
            ],
        )?;
        let item = self.owner(o)?;
        let p = object(o.get("part").ok_or(CodecError::Invalid("part"))?)?;
        let kind = part_kind(p)?;
        let summary = string(o, "type")? == "response.reasoning_summary_part.added";
        if o.contains_key(if summary {
            "content_index"
        } else {
            "summary_index"
        }) {
            return Err(CodecError::Invalid("part index domain"));
        }
        if summary != (kind == PartKind::Summary) {
            return Err(CodecError::Invalid("part kind"));
        }
        let key = if summary {
            "summary_index"
        } else {
            "content_index"
        };
        let n = index(o, key)?;
        let count = self
            .state()?
            .item(item)?
            .parts
            .iter()
            .filter(|p| (p.kind == PartKind::Summary) == summary)
            .count();
        if n != count || !part_text(p, kind)?.is_empty() {
            return Err(CodecError::Invalid("initial part"));
        }
        if kind == PartKind::Text {
            let text = super::super::text::read(p, false)?;
            if !text.annotations().is_empty()
                || text.logprobs().value().is_some_and(|v| !v.is_empty())
            {
                return Err(CodecError::Invalid("initial text metadata"));
            }
        }
        let part = self.allocate_part()?;
        self.emit(StreamEvent::PartStarted { item, part, kind }, out)
    }
    fn value_identity(
        &mut self,
        o: &Map<String, Value>,
        allow_implicit: bool,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(ItemId, PartId, PartKind), CodecError> {
        let item = self.owner(o)?;
        let t = string(o, "type")?;
        let summary_alias = self.adaptation.rules.responses_summary_text_alias
            && t.starts_with("response.reasoning_text.");
        let (kind, key) = if t.starts_with("response.custom_tool_call_input.") {
            (PartKind::CustomInput, None)
        } else if t.starts_with("response.function_call_arguments.") {
            (
                if matches!(
                    self.state()?.item(item)?.kind,
                    ItemKind::ToolCall {
                        format: ArgumentFormat::Json,
                        ..
                    }
                ) {
                    PartKind::StructuredArguments
                } else {
                    PartKind::Arguments
                },
                None,
            )
        } else if t.starts_with("response.reasoning_summary_text.") {
            (PartKind::Summary, Some("summary_index"))
        } else if t.starts_with("response.reasoning_text.") {
            (
                if summary_alias {
                    PartKind::Summary
                } else {
                    PartKind::ReasoningText
                },
                Some("content_index"),
            )
        } else if t.starts_with("response.refusal.") {
            (PartKind::Refusal, Some("content_index"))
        } else {
            (PartKind::Text, Some("content_index"))
        };
        if ["content_index", "summary_index"]
            .iter()
            .any(|k| Some(*k) != key && o.contains_key(*k))
        {
            return Err(CodecError::Invalid("part index domain"));
        }
        let n = key.map(|k| index(o, k)).transpose()?.unwrap_or(0);
        let part = match self.part_for(item, kind, n) {
            Ok(p) => p,
            Err(_)
                if allow_implicit
                    && (kind == PartKind::ReasoningText || summary_alias)
                    && n == self
                        .state()?
                        .item(item)?
                        .parts
                        .iter()
                        .filter(|p| (p.kind == PartKind::Summary) == (kind == PartKind::Summary))
                        .count() =>
            {
                let p = self.allocate_part()?;
                self.emit(
                    StreamEvent::PartStarted {
                        item,
                        part: p,
                        kind,
                    },
                    out,
                )?;
                p
            }
            Err(e) => return Err(e),
        };
        Ok((item, part, kind))
    }
    fn delta(
        &mut self,
        o: &Map<String, Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        event_fields(
            o,
            &[
                "type",
                "output_index",
                "item_id",
                "content_index",
                "summary_index",
                "delta",
                "logprobs",
            ],
        )?;
        let (item, part, kind) = self.value_identity(o, true, out)?;
        if o.contains_key("logprobs") && kind != PartKind::Text {
            return Err(CodecError::Invalid("logprob owner"));
        }
        // The pinned text delta/done schemas require the array, including when empty.
        // https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_text_delta_event.py
        let logprobs = if kind == PartKind::Text {
            match o.get("logprobs") {
                Some(value) => super::super::text::read_logprobs(value)?,
                None if self.adaptation.rules.responses_unreported_text_logprobs => vec![],
                None => return Err(CodecError::Invalid("logprobs")),
            }
        } else {
            vec![]
        };
        self.emit(
            StreamEvent::Delta {
                item,
                part,
                fragment: string(o, "delta")?.into(),
                logprobs,
            },
            out,
        )
    }
    fn value_done(
        &mut self,
        o: &Map<String, Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        event_fields(
            o,
            &[
                "type",
                "output_index",
                "item_id",
                "content_index",
                "summary_index",
                "text",
                "refusal",
                "arguments",
                "input",
                "logprobs",
                "name",
            ],
        )?;
        let (item, part, kind) = self.value_identity(o, true, out)?;
        if let Some(name) = o.get("name")
            && !matches!(&self.state()?.item(item)?.kind, ItemKind::ToolCall{name:expected,..} if matches!(kind,PartKind::Arguments|PartKind::StructuredArguments) && name.as_str()==Some(expected.as_str()))
        {
            return Err(CodecError::Invalid("function name echo"));
        }
        let key = match kind {
            PartKind::Arguments | PartKind::StructuredArguments => "arguments",
            PartKind::CustomInput => "input",
            PartKind::Refusal => "refusal",
            _ => "text",
        };
        if ["text", "refusal", "arguments", "input"]
            .iter()
            .any(|k| *k != key && o.contains_key(*k))
        {
            return Err(CodecError::Invalid("value domain"));
        }
        let builder = self
            .state()?
            .part(item, part)?
            .value()
            .fragments()
            .ok_or(CodecError::Invalid("value builder"))?;
        let matches = if kind == PartKind::StructuredArguments {
            StructuredValue::from_bytes(string(o, key)?.as_bytes())?
                == StructuredValue::from_bytes(builder.as_bytes())?
        } else {
            string(o, key)? == builder
        };
        if !matches {
            return Err(CodecError::Invalid("value snapshot"));
        }
        if kind == PartKind::Text
            && !o.contains_key("logprobs")
            && (!self.adaptation.rules.responses_unreported_text_logprobs
                || !self.state()?.part(item, part)?.logprobs.is_absent())
        {
            return Err(CodecError::Invalid("logprobs"));
        }
        if let Some(v) = o.get("logprobs") {
            if kind != PartKind::Text {
                return Err(CodecError::Invalid("logprob owner"));
            }
            let probabilities = super::super::text::read_logprobs(v)?;
            // The required empty event array is not an optional static report.
            // A later content-part snapshot owns explicit empty probability presence.
            if !probabilities.is_empty()
                || self.state()?.part(item, part)?.logprobs.value().is_some()
            {
                self.emit(
                    StreamEvent::LogprobsSnapshot {
                        item,
                        part,
                        logprobs: probabilities,
                    },
                    out,
                )?;
            }
        }
        self.emit(StreamEvent::ValueFinished { item, part }, out)?;
        if matches!(
            kind,
            PartKind::Arguments | PartKind::StructuredArguments | PartKind::CustomInput
        ) || (kind == PartKind::Summary
            && self.adaptation.rules.responses_summary_text_alias
            && string(o, "type")? == "response.reasoning_text.done")
        {
            // The alias has an explicit value.done but no separate part.done.
            // Never close a part from the final item snapshot alone.
            self.emit(StreamEvent::PartFinished { item, part }, out)?;
        }
        Ok(())
    }
    fn part_done(
        &mut self,
        o: &Map<String, Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        event_fields(
            o,
            &[
                "type",
                "output_index",
                "item_id",
                "content_index",
                "summary_index",
                "part",
            ],
        )?;
        let item = self.owner(o)?;
        let p = object(o.get("part").ok_or(CodecError::Invalid("part"))?)?;
        let kind = part_kind(p)?;
        let summary = string(o, "type")? == "response.reasoning_summary_part.done";
        if o.contains_key(if summary {
            "content_index"
        } else {
            "summary_index"
        }) {
            return Err(CodecError::Invalid("part index domain"));
        }
        if summary != (kind == PartKind::Summary) {
            return Err(CodecError::Invalid("part kind"));
        }
        let part = self.part_for(
            item,
            kind,
            index(
                o,
                if summary {
                    "summary_index"
                } else {
                    "content_index"
                },
            )?,
        )?;
        if part_text(p, kind)?
            != self
                .state()?
                .part(item, part)?
                .value()
                .fragments()
                .ok_or(CodecError::Invalid("value builder"))?
        {
            return Err(CodecError::Invalid("part snapshot"));
        }
        if kind == PartKind::Text {
            let t = super::super::text::read(p, false)?;
            self.emit(
                StreamEvent::TextMetadata {
                    item,
                    part,
                    annotations: t.annotations().to_vec(),
                    logprobs: t.logprobs().clone(),
                },
                out,
            )?;
        }
        self.emit(StreamEvent::PartFinished { item, part }, out)
    }
    fn item_done(
        &mut self,
        o: &Map<String, Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        event_fields(o, &["type", "output_index", "item"])?;
        let item = *self
            .ids
            .get(index(o, "output_index")?)
            .ok_or(CodecError::Invalid("output index"))?;
        let v = o.get("item").ok_or(CodecError::Invalid("item"))?;
        let p = object(v)?;
        let status = if matches!(self.state()?.item(item)?.kind, ItemKind::CustomCall { .. }) {
            ItemLifecycle::Completed
        } else {
            match p.get("status").filter(|v| !v.is_null()) {
                Some(v) => lifecycle(v.as_str().ok_or(CodecError::Invalid("item status"))?)?,
                None => ItemLifecycle::Completed,
            }
        };
        // Reasoning-text may omit generic content-part.done; its value.done still closes the value.
        let parts = self.state()?.item(item)?.parts.clone();
        for part in parts {
            if part.kind == PartKind::ReasoningText && part.value_finished && !part.finished {
                self.emit(
                    StreamEvent::PartFinished {
                        item,
                        part: part.id,
                    },
                    out,
                )?;
            }
        }
        let token = if matches!(self.state()?.item(item)?.kind, ItemKind::Reasoning) {
            replay(p, self.origin.as_ref(), true)?
        } else {
            None
        };
        self.emit(
            StreamEvent::ItemFinished {
                item,
                status,
                replay: token,
            },
            out,
        )?;
        let expected = item_wire(self.state()?, item, &self.fidelity)?;
        let wrapper = json!({"id":"snapshot","object":"response","created_at":0,"model":"snapshot","status":"incomplete","output":[v]});
        let decoded =
            super::super::static_response::decode_responses_with(&wrapper, &self.adaptation)?;
        let normalized = super::super::responses::encode_items(
            decoded.semantic.items(),
            &decoded.fidelity,
            true,
        );
        if normalized.first() != Some(&expected) {
            return Err(CodecError::Invalid("item snapshot"));
        }
        Ok(())
    }
    fn close_terminal_reasoning(
        &mut self,
        response: &Map<String, Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        let output = response
            .get("output")
            .and_then(Value::as_array)
            .ok_or(CodecError::Invalid("terminal output"))?;
        let pending: Vec<_> = self
            .state()?
            .items()
            .iter()
            .enumerate()
            .filter(|(_, item)| item.kind == ItemKind::Reasoning && item.status.is_none())
            .map(|(index, item)| (index, item.clone()))
            .collect();
        for (index, item) in pending {
            let snapshot = output
                .get(index)
                .and_then(Value::as_object)
                .ok_or(CodecError::Invalid("terminal reasoning identity"))?;
            if snapshot.get("type").and_then(Value::as_str) != Some("reasoning")
                || snapshot.get("id").and_then(Value::as_str)
                    != self.fidelity.response_item_id(item.id)
                || snapshot.get("status").and_then(Value::as_str) != Some("completed")
                || item.replay.is_some()
                || snapshot
                    .get("encrypted_content")
                    .is_some_and(|v| !v.is_null())
                || item.parts.iter().any(|p| p.kind != PartKind::ReasoningText)
            {
                return Err(CodecError::Invalid("terminal reasoning closure"));
            }
            // These events are not published unless the whole terminal snapshot
            // below exactly matches accumulated values. Never add snapshot text.
            for part in item.parts {
                if !part.value_finished {
                    self.emit(
                        StreamEvent::ValueFinished {
                            item: item.id,
                            part: part.id,
                        },
                        out,
                    )?;
                }
                if !part.finished {
                    self.emit(
                        StreamEvent::PartFinished {
                            item: item.id,
                            part: part.id,
                        },
                        out,
                    )?;
                }
            }
            self.emit(
                StreamEvent::ItemFinished {
                    item: item.id,
                    status: ItemLifecycle::Completed,
                    replay: None,
                },
                out,
            )?;
        }
        Ok(())
    }
    fn terminal(
        &mut self,
        o: &Map<String, Value>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        event_fields(o, &["type", "response"])?;
        let r = o.get("response").ok_or(CodecError::Invalid("response"))?;
        let mut summary;
        let r = if self.adaptation.rules.responses_event_owned_output
            && r.get("status").and_then(Value::as_str) == Some("completed")
            && r.get("output")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
            && !self.state()?.items().is_empty()
        {
            if self.state()?.items().iter().any(|item| {
                item.status != Some(ItemLifecycle::Completed)
                    || item
                        .parts
                        .iter()
                        .any(|part| !part.finished || !part.value_finished)
            }) {
                return Err(CodecError::Invalid("unfinished event-owned output"));
            }
            // Real item.done snapshots already checked every value and owner.
            // Missing/null/nonempty conflicting terminal output never uses this rule.
            summary = r.clone();
            summary["output"] = json!(super::super::responses::encode_items(
                &snapshot_items(self.state()?)?,
                &self.fidelity,
                true
            ));
            &summary
        } else {
            r
        };
        let p = object(r)?;
        let status = string(p, "status")?;
        if string(o, "type")? != format!("response.{status}") {
            return Err(CodecError::Invalid("terminal status"));
        }
        let terminal = match status {
            "completed" => StreamTerminal::Completed,
            "incomplete" => StreamTerminal::Incomplete,
            "failed" => StreamTerminal::Failed,
            _ => return Err(CodecError::Invalid("terminal")),
        };
        self.observe_metadata(p)?;
        let decoded = super::super::static_response::decode_responses_with(r, &self.adaptation)?;
        self.fidelity.copy_response_records(&decoded.fidelity);
        if self.adaptation.rules.responses_terminal_reasoning
            && terminal == StreamTerminal::Completed
        {
            self.close_terminal_reasoning(p, out)?;
        }
        sync_replays(
            self.state.as_ref().ok_or(CodecError::Invalid("state"))?,
            &mut self.fidelity,
            None,
        )?;
        let expected = super::super::responses::encode_items(
            &snapshot_items(self.state()?)?,
            &self.fidelity,
            true,
        );
        let actual = super::super::responses::encode_items(
            decoded.semantic.items(),
            &decoded.fidelity,
            true,
        );
        if actual != expected {
            return Err(CodecError::Invalid(terminal_snapshot_difference(
                &actual, &expected,
            )));
        }
        for usage in decoded.semantic.usage_reports() {
            self.emit(StreamEvent::Usage(*usage), out)?;
        }
        self.emit(
            StreamEvent::Terminal {
                terminal,
                details: decoded.semantic.details().clone(),
            },
            out,
        )
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    #[test]
    fn replay_difference_reports_direction_without_values() {
        let absent = vec![json!({"type":"reasoning","id":"synthetic-private-id","summary":[]})];
        let first = vec![
            json!({"type":"reasoning","id":"synthetic-private-id","summary":[],"encrypted_content":"synthetic-private-first"}),
        ];
        let second = vec![
            json!({"type":"reasoning","id":"synthetic-private-id","summary":[],"encrypted_content":"synthetic-private-second"}),
        ];
        for (actual, expected, label) in [
            (&first, &absent, "terminal snapshot replay added"),
            (&absent, &first, "terminal snapshot replay removed"),
            (&second, &first, "terminal snapshot replay changed"),
        ] {
            assert_eq!(terminal_snapshot_difference(actual, expected), label);
        }
    }
}
