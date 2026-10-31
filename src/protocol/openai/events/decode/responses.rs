//! Responses event grammar and exact terminal snapshot verification.
use super::*;
use crate::protocol::{
    failure::{CarrierReason, ShapeReason},
    openai::events::responses_wire::*,
};

/// Snapshot-only bookkeeping. The shared stream state stays in the decoder.
#[derive(Default)]
pub(super) struct State {
    pub(super) native_reports: Option<crate::semantic::context::ResponseReports>,
    pub(super) native_instructions: Option<Vec<(ItemId, Item)>>,
    pub(super) program_output_status: std::collections::BTreeMap<ItemId, ItemLifecycle>,
    pub(super) queued_creation_pending: bool,
}

// Fixed structural failure labels only; never include IDs, text or ciphertext.
fn terminal_snapshot_difference(actual: &[Value], expected: &[Value]) -> ShapeReason {
    if actual.len() != expected.len() {
        return ShapeReason::SnapshotShape;
    }
    for (a, e) in actual.iter().zip(expected) {
        if a == e {
            continue;
        }
        if a["id"] != e["id"] || a["type"] != e["type"] {
            return ShapeReason::SnapshotIdentity;
        }
        if a["status"] != e["status"] {
            return ShapeReason::SnapshotLifecycle;
        }
        if a["phase"] != e["phase"] {
            return ShapeReason::SnapshotPhase;
        }
        if a["encrypted_content"] != e["encrypted_content"] {
            return match (
                a["encrypted_content"].is_null(),
                e["encrypted_content"].is_null(),
            ) {
                (false, true) => ShapeReason::SnapshotReplayAdded,
                (true, false) => ShapeReason::SnapshotReplayRemoved,
                _ => ShapeReason::SnapshotReplayChanged,
            };
        }
        if a["summary"] != e["summary"] {
            return ShapeReason::SnapshotSummary;
        }
        if let (Some(ap), Some(ep)) = (a["content"].as_array(), e["content"].as_array()) {
            if ap.len() != ep.len() {
                return ShapeReason::SnapshotShape;
            }
            for (p, q) in ap.iter().zip(ep) {
                if p["text"] != q["text"] {
                    return ShapeReason::SnapshotText;
                }
                if p["annotations"] != q["annotations"] {
                    return ShapeReason::SnapshotAnnotations;
                }
                if p["logprobs"] != q["logprobs"] {
                    let empty = |v: &Value| v.is_null() || v.as_array().is_some_and(Vec::is_empty);
                    return if empty(&p["logprobs"]) && empty(&q["logprobs"]) {
                        ShapeReason::SnapshotProbabilityPresence
                    } else {
                        ShapeReason::SnapshotProbabilities
                    };
                }
            }
        }
        return ShapeReason::TerminalSnapshot;
    }
    ShapeReason::TerminalSnapshot
}

impl EventDecoder {
    pub(super) fn responses(
        &mut self,
        o: &Map<String, Value>,
    ) -> Result<Vec<StreamEvent>, CodecError> {
        let mut out = vec![];
        let typ = string(o, "type")?;
        match typ {
            "error" => {
                event_fields(o, &["type", "code", "message", "param"])?;
                let mut e = o.clone();
                e.remove("type");
                e.remove("sequence_number");
                let details = crate::protocol::openai::terminal::decode_details(
                    &serde_json::from_value(json!({"error":e}))
                        .map_err(|_| CodecError::invalid())?,
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
                let r = object(o.get("response").ok_or(CodecError::invalid())?)?;
                crate::protocol::openai::responses::envelope::response_fields(r)?;
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
                    return Err(CodecError::invalid());
                }
                match typ {
                    "response.queued" => self.emit(StreamEvent::Queued, &mut out)?,
                    "response.created" => {
                        if self.responses.queued_creation_pending {
                            return Err(CodecError::invalid());
                        }
                        if queued_creation {
                            self.emit(StreamEvent::Queued, &mut out)?;
                            self.responses.queued_creation_pending = true;
                        } else {
                            self.emit(StreamEvent::Started, &mut out)?;
                        }
                    }
                    _ => {
                        // The queued snapshot cannot prove generation has started;
                        // this actual progress event supplies the transition.
                        if self.responses.queued_creation_pending {
                            self.emit(StreamEvent::Started, &mut out)?;
                            self.responses.queued_creation_pending = false;
                        }
                        if !self.state()?.started() {
                            return Err(CodecError::invalid());
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
            "response.compaction.compacting" => {
                let item = self.owner(o)?;
                self.emit(StreamEvent::CompactionProgress { item }, &mut out)?;
            }
            "response.completed" | "response.incomplete" | "response.failed" => {
                self.terminal(o, &mut out)?
            }
            // Unregistered types are skipped, as Codex does; registered content and
            // terminal events above stay strict.
            _ => self.ignored += 1,
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
            return Err(CodecError::Invalid(ShapeReason::Identity));
        }
        let v = object(o.get("item").ok_or(CodecError::invalid())?)?;
        if v.get("type").and_then(Value::as_str) != Some("program_output")
            && v.get("status")
                .filter(|v| !v.is_null())
                .is_some_and(|s| s.as_str() != Some("in_progress"))
        {
            return Err(CodecError::Invalid(ShapeReason::Item));
        }
        let kind = match string(v, "type")? {
            "message" => {
                DecodeDirection::Upstream
                    .fields(v, &["type", "id", "role", "status", "content", "phase"])?;
                if string(v, "role")? != "assistant"
                    || !v
                        .get("content")
                        .and_then(Value::as_array)
                        .is_some_and(Vec::is_empty)
                {
                    return Err(CodecError::Invalid(ShapeReason::Item));
                }
                ItemKind::Message {
                    phase: crate::protocol::openai::responses::items::read_phase(v)?,
                }
            }
            "reasoning" => {
                DecodeDirection::Upstream.fields(
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
                    return Err(CodecError::Invalid(ShapeReason::Reasoning));
                }
                ItemKind::Reasoning
            }
            "custom_tool_call" => {
                DecodeDirection::Upstream.fields(
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
                    context: crate::protocol::openai::responses::items::call_context(
                        v,
                        DecodeDirection::Upstream,
                    )?,
                }
            }
            "function_call_output" => {
                ItemKind::ToolResult(self.read_tool_result(v, false, None, out)?)
            }
            "custom_tool_call_output" => {
                ItemKind::CustomResult(self.read_tool_result(v, true, None, out)?)
            }
            "compaction" => {
                DecodeDirection::Upstream.fields(v, &["type", "id", "encrypted_content"])?;
                ItemKind::Compaction
            }
            "program" => {
                DecodeDirection::Upstream
                    .fields(v, &["type", "id", "call_id", "code", "fingerprint"])?;
                ItemKind::Program {
                    call_id: text(string(v, "call_id")?, "call id", 256)?,
                    code: raw_string(v, "code")?,
                    fingerprint: raw_string(v, "fingerprint")?,
                }
            }
            "program_output" => {
                DecodeDirection::Upstream
                    .fields(v, &["type", "id", "call_id", "result", "status"])?;
                if !matches!(
                    v.get("status").and_then(Value::as_str),
                    Some("completed" | "incomplete")
                ) {
                    return Err(CodecError::invalid());
                }
                ItemKind::ProgramOutput {
                    call_id: text(string(v, "call_id")?, "call id", 256)?,
                    result: raw_string(v, "result")?,
                }
            }
            "function_call" => {
                DecodeDirection::Upstream.fields(
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
                    context: crate::protocol::openai::responses::items::call_context(
                        v,
                        DecodeDirection::Upstream,
                    )?,
                }
            }
            _ => {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
        };
        let item = self.allocate_item()?;
        if matches!(kind, ItemKind::ProgramOutput { .. }) {
            self.responses
                .program_output_status
                .insert(item, lifecycle(string(v, "status")?)?);
        }
        self.fidelity
            .record_response_item_id(item, string(v, "id")?)?;
        let replay = if matches!(kind, ItemKind::Reasoning | ItemKind::Compaction) {
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
        if matches!(
            kind,
            ItemKind::ToolResult(_) | ItemKind::CustomResult(_) | ItemKind::Compaction
        ) {
            let snapshot = self.state()?.item(item)?.snapshot()?;
            self.fidelity.observe_created_by(
                item,
                &snapshot,
                self.state.as_ref().expect("state").resources(),
                v.get("created_by"),
            )?;
        }
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
    fn read_tool_result(
        &mut self,
        value: &Map<String, Value>,
        custom: bool,
        previous: Option<&ToolResult>,
        out: &mut Vec<StreamEvent>,
    ) -> Result<ToolResult, CodecError> {
        let mut sources =
            Items::from_resources(self.state()?.resources().clone(), DecodeDirection::Upstream);
        let mut result = crate::protocol::openai::responses::items::tool_result(
            &mut sources,
            value,
            true,
            custom,
        )?;
        if let Some(previous) = previous
            && crate::protocol::openai::responses::items::output_wire(
                &previous.output,
                self.state()?.resources(),
            ) == crate::protocol::openai::responses::items::output_wire(
                &result.output,
                &sources.resources,
            )
        {
            // This item's repeated snapshot preserves its part/resource identities.
            // Equal URLs in different items still declare distinct resources.
            result.output = previous.output.clone();
            return Ok(result);
        }
        if let ToolOutput::Parts(parts) = &mut result.output {
            for part in parts {
                let next = self.allocate_part()?;
                part.id = next;
            }
        }
        self.declare_sources(&sources.resources, out)?;
        Ok(result)
    }
    fn owner(&self, o: &Map<String, Value>) -> Result<ItemId, CodecError> {
        let id = *self
            .ids
            .get(index(o, "output_index")?)
            .ok_or(CodecError::Invalid(ShapeReason::Identity))?;
        if string(o, "item_id")? != self.fidelity.response_item_id(id).unwrap_or("") {
            return Err(CodecError::Invalid(ShapeReason::Identity));
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
            .ok_or(CodecError::Invalid(ShapeReason::Identity))
    }
    fn declare_sources(
        &mut self,
        resources: &ResourceTable,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        for (id, resource) in resources.iter() {
            if self.state()?.resources().get(id).is_none() {
                self.emit(
                    StreamEvent::ResourceDeclared {
                        id,
                        resource: resource.clone(),
                    },
                    out,
                )?;
            }
        }
        Ok(())
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
            return Err(CodecError::invalid());
        }
        let v = o.get("annotation").ok_or(CodecError::invalid())?;
        let mut sources =
            Items::from_resources(self.state()?.resources().clone(), DecodeDirection::Upstream);
        let annotation = crate::protocol::openai::content::citations::read(v, &mut sources)?;
        self.declare_sources(&sources.resources, out)?;
        if annotation.value().kind == CitationKind::ArtifactReference {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
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
        let p = object(o.get("part").ok_or(CodecError::invalid())?)?;
        let kind = part_kind(p)?;
        let summary = string(o, "type")? == "response.reasoning_summary_part.added";
        if o.contains_key(if summary {
            "content_index"
        } else {
            "summary_index"
        }) {
            return Err(CodecError::Invalid(ShapeReason::Identity));
        }
        if summary != (kind == PartKind::Summary) {
            return Err(CodecError::invalid());
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
            return Err(CodecError::invalid());
        }
        if kind == PartKind::Text {
            let text = crate::protocol::openai::content::text::read(
                p,
                &mut Items::new(DecodeDirection::Upstream),
            )?;
            if !text.annotations().is_empty()
                || text.logprobs().value().is_some_and(|v| !v.is_empty())
            {
                return Err(CodecError::Invalid(ShapeReason::Probabilities));
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
            return Err(CodecError::Invalid(ShapeReason::Identity));
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
            return Err(CodecError::Invalid(ShapeReason::Probabilities));
        }
        // The pinned text delta/done schemas require the array, including when empty.
        // https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_text_delta_event.py
        let logprobs = if kind == PartKind::Text {
            match o.get("logprobs") {
                Some(value) => crate::protocol::openai::content::text::read_logprobs(value)?,
                None if self.adaptation.rules.responses_unreported_text_logprobs => vec![],
                None => {
                    return Err(CodecError::Invalid(ShapeReason::Probabilities));
                }
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
            && !matches!(&self.state()?.item(item)?.kind, ItemKind::ToolCall { name: expected, .. }
                if matches!(kind, PartKind::Arguments | PartKind::StructuredArguments)
                    && name.as_str() == Some(expected.as_str()))
        {
            return Err(CodecError::invalid());
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
            return Err(CodecError::invalid());
        }
        let mut builder = self
            .state()?
            .part(item, part)?
            .value()
            .fragments()
            .ok_or(CodecError::invalid())?
            .to_owned();
        // Some Responses servers stream a call's arguments partly or not at all
        // and send them whole here; the missing suffix becomes the last delta.
        // A done value that does not extend the deltas still conflicts.
        if matches!(
            kind,
            PartKind::Arguments | PartKind::StructuredArguments | PartKind::CustomInput
        ) && let Some(suffix) = string(o, key)?.strip_prefix(builder.as_str())
            && !suffix.is_empty()
        {
            self.emit(
                StreamEvent::Delta {
                    item,
                    part,
                    fragment: suffix.into(),
                    logprobs: vec![],
                },
                out,
            )?;
            builder = string(o, key)?.to_owned();
        }
        let matches = if kind == PartKind::StructuredArguments {
            StructuredValue::from_bytes(string(o, key)?.as_bytes())?
                == StructuredValue::from_bytes(builder.as_bytes())?
        } else {
            string(o, key)? == builder
        };
        if !matches {
            return Err(CodecError::Invalid(ShapeReason::ValueSnapshot));
        }
        if kind == PartKind::Text
            && !o.contains_key("logprobs")
            && (!self.adaptation.rules.responses_unreported_text_logprobs
                || !self.state()?.part(item, part)?.logprobs.is_absent())
        {
            return Err(CodecError::Invalid(ShapeReason::Probabilities));
        }
        if let Some(v) = o.get("logprobs") {
            if kind != PartKind::Text {
                return Err(CodecError::Invalid(ShapeReason::Probabilities));
            }
            let probabilities = crate::protocol::openai::content::text::read_logprobs(v)?;
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
        let p = object(o.get("part").ok_or(CodecError::invalid())?)?;
        let kind = part_kind(p)?;
        let summary = string(o, "type")? == "response.reasoning_summary_part.done";
        if o.contains_key(if summary {
            "content_index"
        } else {
            "summary_index"
        }) {
            return Err(CodecError::Invalid(ShapeReason::Identity));
        }
        if summary != (kind == PartKind::Summary) {
            return Err(CodecError::invalid());
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
                .ok_or(CodecError::invalid())?
        {
            return Err(CodecError::Invalid(ShapeReason::ValueSnapshot));
        }
        if kind == PartKind::Text {
            let mut sources =
                Items::from_resources(self.state()?.resources().clone(), DecodeDirection::Upstream);
            let t = crate::protocol::openai::content::text::read_existing(
                p,
                &mut sources,
                &self.state()?.part(item, part)?.annotations,
            )?;
            self.declare_sources(&sources.resources, out)?;
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
        let interrupted = if summary {
            match o.get("status") {
                None | Some(Value::Null) => false,
                Some(value) if value == "incomplete" => true,
                Some(_) => {
                    return Err(CodecError::invalid());
                }
            }
        } else {
            false
        };
        self.emit(
            if interrupted {
                StreamEvent::PartInterrupted { item, part }
            } else {
                StreamEvent::PartFinished { item, part }
            },
            out,
        )
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
            .ok_or(CodecError::Invalid(ShapeReason::Identity))?;
        let v = o.get("item").ok_or(CodecError::invalid())?;
        let p = object(v)?;
        if let ItemKind::ToolResult(old) | ItemKind::CustomResult(old) =
            &self.state()?.item(item)?.kind
        {
            let old = old.clone();
            let custom = matches!(self.state()?.item(item)?.kind, ItemKind::CustomResult(_));
            if string(p, "type")?
                != if custom {
                    "custom_tool_call_output"
                } else {
                    "function_call_output"
                }
                || self.fidelity.response_item_id(item) != Some(string(p, "id")?)
            {
                return Err(CodecError::invalid());
            }
            let next = self.read_tool_result(p, custom, Some(&old), out)?;
            if old.call_id != next.call_id || old.name != next.name || old.context != next.context {
                return Err(CodecError::invalid());
            }
            self.emit(
                StreamEvent::ToolResultOutput {
                    item,
                    output: next.output,
                },
                out,
            )?;
        }
        let status = if matches!(self.state()?.item(item)?.kind, ItemKind::CustomCall { .. }) {
            ItemLifecycle::Completed
        } else {
            match p.get("status").filter(|v| !v.is_null()) {
                Some(v) => lifecycle(v.as_str().ok_or(CodecError::Invalid(ShapeReason::Item))?)?,
                None => ItemLifecycle::Completed,
            }
        };
        if self
            .responses
            .program_output_status
            .get(&item)
            .is_some_and(|old| *old != status)
        {
            return Err(CodecError::Invalid(ShapeReason::ItemSnapshot));
        }
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
        let token = if matches!(
            self.state()?.item(item)?.kind,
            ItemKind::Reasoning | ItemKind::Compaction
        ) {
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
        let snapshot = self.state()?.item(item)?.snapshot()?;
        self.fidelity.observe_created_by(
            item,
            &snapshot,
            self.state.as_ref().expect("state").resources(),
            p.get("created_by"),
        )?;
        let expected = item_wire(self.state()?, item, &self.fidelity)?;
        let wrapper = json!({
            "id": "snapshot",
            "object": "response",
            "created_at": 0,
            "model": "snapshot",
            "status": "incomplete",
            "output": [v],
        });
        let decoded =
            crate::protocol::openai::responses::response::decode_with(&wrapper, &self.adaptation)?;
        let mut normalized = crate::protocol::openai::responses::items::encode_items(
            decoded.semantic.items(),
            &decoded.fidelity,
            true,
            decoded.semantic.resources(),
        )?;
        if v.get("created_by").is_none_or(Value::is_null)
            && let Some(actor) = expected.get("created_by")
        {
            normalized[0]["created_by"] = actor.clone();
        }
        if normalized.first() != Some(&expected) {
            return Err(CodecError::Invalid(ShapeReason::ItemSnapshot));
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
            .ok_or(CodecError::invalid())?;
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
                .ok_or(CodecError::invalid())?;
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
                return Err(CodecError::Invalid(ShapeReason::Reasoning));
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
        let r = o.get("response").ok_or(CodecError::invalid())?;
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
                return Err(CodecError::invalid());
            }
            // Real item.done snapshots already checked every value and owner.
            // Missing/null/nonempty conflicting terminal output never uses this rule.
            summary = r.clone();
            summary["output"] = json!(crate::protocol::openai::responses::items::encode_items(
                &snapshot_items(self.state()?)?,
                &self.fidelity,
                true,
                self.state()?.resources()
            )?);
            &summary
        } else {
            r
        };
        let p = object(r)?;
        let status = string(p, "status")?;
        if string(o, "type")? != format!("response.{status}") {
            return Err(CodecError::invalid());
        }
        let terminal = match status {
            "completed" => StreamTerminal::Completed,
            "incomplete" => StreamTerminal::Incomplete,
            "failed" => StreamTerminal::Failed,
            _ => {
                return Err(CodecError::invalid());
            }
        };
        self.observe_metadata(p)?;
        let decoded =
            crate::protocol::openai::responses::response::decode_with(r, &self.adaptation)?;
        if self.adaptation.rules.responses_terminal_reasoning
            && terminal == StreamTerminal::Completed
        {
            self.close_terminal_reasoning(p, out)?;
        }
        sync_replays(
            self.state.as_ref().ok_or(CodecError::invalid())?,
            &mut self.fidelity,
            None,
        )?;
        let snapshots = snapshot_items(self.state()?)?;
        for (index, (owner, item)) in snapshots.iter().enumerate() {
            self.fidelity.observe_created_by(
                *owner,
                item,
                self.state.as_ref().expect("state").resources(),
                p.get("output")
                    .and_then(Value::as_array)
                    .and_then(|items| items.get(index))
                    .and_then(|item| item.get("created_by")),
            )?;
        }
        let expected = crate::protocol::openai::responses::items::encode_items(
            &snapshot_items(self.state()?)?,
            &self.fidelity,
            true,
            self.state()?.resources(),
        )?;
        let mut actual = crate::protocol::openai::responses::items::encode_items(
            decoded.semantic.items(),
            &decoded.fidelity,
            true,
            decoded.semantic.resources(),
        )?;
        for (value, expected) in actual.iter_mut().zip(&expected) {
            if value.get("created_by").is_none()
                && let Some(actor) = expected.get("created_by")
            {
                value["created_by"] = actor.clone();
            }
        }
        if actual != expected {
            return Err(CodecError::Invalid(terminal_snapshot_difference(
                &actual, &expected,
            )));
        }
        if let Some(usage) = decoded.semantic.usage() {
            self.emit(StreamEvent::Usage(usage), out)?;
        }
        self.emit(
            StreamEvent::Terminal {
                terminal,
                details: decoded.semantic.details().clone(),
            },
            out,
        )?;
        let semantic = materialize(self.state()?)?;
        self.fidelity
            .bind_verified_snapshot_records(&decoded.fidelity, &semantic);
        self.evidence.extend(decoded.evidence);
        Ok(())
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    #[test]
    fn replay_difference_reports_direction_without_values() {
        let absent = vec![json!({"type":"reasoning","id":"synthetic-private-id","summary":[]})];
        let first = vec![json!({
            "type": "reasoning",
            "id": "synthetic-private-id",
            "summary": [],
            "encrypted_content": "synthetic-private-first",
        })];
        let second = vec![json!({
            "type": "reasoning",
            "id": "synthetic-private-id",
            "summary": [],
            "encrypted_content": "synthetic-private-second",
        })];
        for (actual, expected, label) in [
            (&first, &absent, ShapeReason::SnapshotReplayAdded),
            (&absent, &first, ShapeReason::SnapshotReplayRemoved),
            (&second, &first, ShapeReason::SnapshotReplayChanged),
        ] {
            assert_eq!(terminal_snapshot_difference(actual, expected), label);
        }
    }
}

#[cfg(test)]
mod mapping_evidence_tests {
    use super::*;
    use crate::protocol::{
        openai::dialect::adaptation::Adaptation, reasoning::ReasoningEffortMapping,
    };
    #[test]
    fn response_alias_keeps_event_boundary_and_input_only_output_rejects() {
        let adaptation = Adaptation {
            effort_mapping: Some(&ReasoningEffortMapping {
                canonical_model: "synthetic",
                rule: "synthetic.effort-alias.v1",
                scope: EffortScope::Reasoning,
                native_levels: &[EffortLevel::High, EffortLevel::XHigh],
                encode: &[(EffortLevel::High, EffortLevel::High)],
                decode: &[
                    (EffortLevel::XHigh, EffortLevel::High),
                    (EffortLevel::High, EffortLevel::High),
                ],
            }),
            ..Default::default()
        };
        let mut decoder = EventDecoder::new(GenerationApi::Responses).with_adaptation(adaptation);
        decoder.push(&serde_json::json!({"type":"response.created","sequence_number":0,"response":{"id":"r","object":"response","created_at":1,"model":"synthetic","status":"in_progress","output":[],"reasoning":{"effort":"xhigh"}}})).unwrap();
        let root = decoder.take_evidence();
        assert!(root.is_empty());
        assert_eq!(
            decoder
                .metadata()
                .unwrap()
                .context
                .reports
                .iter()
                .last()
                .unwrap()
                .settings
                .as_ref()
                .unwrap()
                .effort
                .unwrap()
                .level,
            EffortLevel::High
        );
        assert!(decoder.push(&json!({"type":"response.output_item.added","sequence_number":1,"output_index":0,"item":{"type":"configuration_update","id":"config","reasoning":{"effort":"xhigh"}}})).is_err());
        assert!(decoder.take_evidence().is_empty());
    }
}
