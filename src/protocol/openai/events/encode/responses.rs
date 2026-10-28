//! Responses event coordinates, item snapshots and wire rendering.
use super::*;
use crate::protocol::{
    failure::{CarrierReason, ShapeReason},
    openai::events::responses_wire::*,
};

impl EventEncoder {
    fn index(&self, id: ItemId) -> Result<usize, CodecError> {
        self.state()?
            .items()
            .iter()
            .position(|i| i.id == id)
            .ok_or(CodecError::invalid())
    }
    fn part_index(&self, item: ItemId, part: PartId) -> Result<usize, CodecError> {
        let kind = self.state()?.part(item, part)?.kind;
        self.state()?
            .item(item)?
            .parts
            .iter()
            .filter(|p| (p.kind == PartKind::Summary) == (kind == PartKind::Summary))
            .position(|p| p.id == part)
            .ok_or(CodecError::invalid())
    }
    fn coordinates(&self, item: ItemId, part: PartId) -> Result<Value, CodecError> {
        let kind = self.state()?.part(item, part)?.kind;
        let id = self
            .fidelity
            .response_item_id(item)
            .ok_or(CodecError::invalid())?;
        let mut v = json!({"output_index": self.index(item)?, "item_id": id});
        if !matches!(
            kind,
            PartKind::Arguments | PartKind::StructuredArguments | PartKind::CustomInput
        ) {
            v[if kind == PartKind::Summary {
                "summary_index"
            } else {
                "content_index"
            }] = json!(self.part_index(item, part)?);
        }
        Ok(v)
    }
    pub(super) fn envelope(
        &mut self,
        status: &str,
        output: Vec<Value>,
    ) -> Result<Value, CodecError> {
        let created = self
            .metadata
            .created
            .as_ref()
            .ok_or(CodecError::Invalid(ShapeReason::Metadata))?;
        let mut v = json!({
            "id": self.metadata.id,
            "object": "response",
            "created_at": created,
            "model": self.metadata.model,
            "status": status,
            "output": output,
        });
        let mut delivered = self.metadata.clone();
        crate::protocol::reports::opening_view(&mut delivered.context.reports)?;
        let service = projection::reported_service_class(&mut delivered, self.profile);
        let loss = projection::reported_settings(&mut delivered, self.profile)
            .map_err(|_| CodecError::Unsupported(CarrierReason::Unavailable))?;
        if loss.is_some()
            && self.contract.reported_facts
                == crate::protocol::conversion::generation::ReportedFactPolicy::StrictComplete
        {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        let fingerprint = if !delivered.context.system_fingerprint.is_absent() {
            delivered.context.system_fingerprint = crate::semantic::value::Presence::Absent;
            Some(projection::ProjectionRule::OmitResponsesSystemFingerprint)
        } else {
            None
        };
        crate::protocol::openai::responses::envelope::write_metadata(
            &delivered,
            v.as_object_mut().expect("object"),
            status == "completed",
        )?;
        for loss in loss.into_iter().chain(fingerprint).chain(service) {
            self.record_loss(loss);
        }
        Ok(v)
    }
    pub(super) fn responses(&mut self, event: &StreamEvent) -> Result<Vec<Value>, CodecError> {
        let result = match event {
            StreamEvent::AudioDelta { .. } => {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
            StreamEvent::Queued => {
                vec![json!({"type":"response.queued","response":self.envelope("queued",vec![])?})]
            }
            StreamEvent::Started => vec![
                json!({
                    "type": "response.created",
                    "response": self.envelope("in_progress", vec![])?,
                }),
                json!({
                    "type": "response.in_progress",
                    "response": self.envelope("in_progress", vec![])?,
                }),
            ],
            StreamEvent::ItemStarted { item, kind, replay } => {
                let id = self
                    .fidelity
                    .response_item_id(*item)
                    .ok_or(CodecError::invalid())?;
                let mut v = match kind {
                    ItemKind::Message { phase } => {
                        let mut v = json!({
                            "id": id,
                            "type": "message",
                            "role": "assistant",
                            "content": [],
                            "status": "in_progress",
                        });
                        if let Some(p) = phase {
                            v["phase"] = json!(p.label());
                        }
                        v
                    }
                    ItemKind::CustomCall {
                        call_id,
                        name,
                        context,
                    } => {
                        let mut v = json!({
                            "id": id,
                            "type": "custom_tool_call",
                            "call_id": call_id.as_str(),
                            "name": name.as_str(),
                            "input": "",
                        });
                        crate::protocol::openai::responses::items::write_call_context(
                            context,
                            v.as_object_mut().expect("object"),
                        );
                        v
                    }
                    ItemKind::Reasoning => {
                        json!({"id":id,"type":"reasoning","summary":[],"status":"in_progress"})
                    }
                    ItemKind::Compaction => {
                        json!({"id":id,"type":"compaction"})
                    }
                    ItemKind::ConfigurationUpdate(_) => {
                        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                    }
                    ItemKind::Program {
                        call_id,
                        code,
                        fingerprint,
                    } => {
                        json!({
                            "id": id,
                            "type": "program",
                            "call_id": call_id.as_str(),
                            "code": code,
                            "fingerprint": fingerprint,
                        })
                    }
                    ItemKind::ProgramOutput { call_id, result } => {
                        self.pending_program_outputs
                            .insert(*item, self.held_frames.len());
                        json!({"id":id,"type":"program_output","call_id":call_id.as_str(),"result":result})
                    }
                    ItemKind::ToolResult(_) | ItemKind::CustomResult(_) => {
                        item_wire(self.state()?, *item, &self.fidelity)?
                    }
                    ItemKind::ProviderTool(_) | ItemKind::ProviderResult { .. } => {
                        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                    }
                    ItemKind::ToolCall {
                        call_id,
                        name,
                        context,
                        ..
                    } => {
                        let mut v = json!({
                            "id": id,
                            "type": "function_call",
                            "call_id": call_id.as_str(),
                            "name": name.as_str(),
                            "arguments": "",
                            "status": "in_progress",
                        });
                        crate::protocol::openai::responses::items::write_call_context(
                            context,
                            v.as_object_mut().expect("object"),
                        );
                        v
                    }
                };
                if let Some(r) = replay
                    && matches!(
                        r.value.format(),
                        ReplayFormat::ResponsesEncrypted | ReplayFormat::ResponsesCompaction
                    )
                {
                    v["encrypted_content"] = json!(r.value.as_str());
                }
                if matches!(
                    kind,
                    ItemKind::ToolResult(_) | ItemKind::CustomResult(_) | ItemKind::Compaction
                ) {
                    let snapshot = self.state()?.item(*item)?.snapshot()?;
                    if let Some(actor) =
                        self.fidelity
                            .created_by(*item, &snapshot, self.state()?.resources())
                    {
                        v["created_by"] = json!(actor);
                    }
                }
                vec![json!({
                    "type": "response.output_item.added",
                    "output_index": self.index(*item)?,
                    "item": v,
                })]
            }
            StreamEvent::PartStarted { item, part, kind } => {
                if matches!(
                    kind,
                    PartKind::Arguments
                        | PartKind::StructuredArguments
                        | PartKind::CustomInput
                        | PartKind::ReasoningText
                ) {
                    vec![]
                } else {
                    let mut v = self.coordinates(*item, *part)?;
                    v["type"] = json!(if *kind == PartKind::Summary {
                        "response.reasoning_summary_part.added"
                    } else {
                        "response.content_part.added"
                    });
                    v["part"] = part_wire(*kind, "");
                    vec![v]
                }
            }
            StreamEvent::Delta {
                item,
                part,
                fragment,
                logprobs,
            } => {
                let kind = self.state()?.part(*item, *part)?.kind;
                let mut v = self.coordinates(*item, *part)?;
                v["type"] = json!(format!("response.{}.delta", event_stem(kind)));
                v["delta"] = json!(fragment);
                if kind == PartKind::Text {
                    v["logprobs"] =
                        crate::protocol::openai::content::text::event_logprobs(logprobs);
                }
                vec![v]
            }
            StreamEvent::ValueFinished { item, part } => {
                let p = self.state()?.part(*item, *part)?;
                let mut v = self.coordinates(*item, *part)?;
                v["type"] = json!(format!("response.{}.done", event_stem(p.kind)));
                v[match p.kind {
                    PartKind::Arguments | PartKind::StructuredArguments => "arguments",
                    PartKind::CustomInput => "input",
                    PartKind::Refusal => "refusal",
                    _ => "text",
                }] = json!(p.value().wire_text());
                if p.kind == PartKind::Text {
                    v["logprobs"] = crate::protocol::openai::content::text::event_logprobs(
                        p.logprobs.value().map(Vec::as_slice).unwrap_or_default(),
                    );
                }
                vec![v]
            }
            StreamEvent::PartFinished { item, part }
            | StreamEvent::PartInterrupted { item, part } => {
                let p = self.state()?.part(*item, *part)?;
                if matches!(
                    p.kind,
                    PartKind::Arguments
                        | PartKind::StructuredArguments
                        | PartKind::CustomInput
                        | PartKind::ReasoningText
                ) {
                    vec![]
                } else {
                    let mut v = self.coordinates(*item, *part)?;
                    v["type"] = json!(if p.kind == PartKind::Summary {
                        "response.reasoning_summary_part.done"
                    } else {
                        "response.content_part.done"
                    });
                    v["part"] = if p.kind == PartKind::Text {
                        let text = crate::semantic::value::Text::allowing_empty(
                            p.text()
                                .ok_or(CodecError::Unsupported(CarrierReason::Unavailable))?,
                            "text",
                            MAX_TEXT_BYTES,
                        )
                        .map_err(|_| CodecError::Limit)?;
                        crate::protocol::openai::content::text::write(
                            &TextContent::new(text, p.annotations.clone(), p.logprobs.clone())?,
                            "output_text",
                            self.state()?.resources(),
                        )
                    } else {
                        part_wire(
                            p.kind,
                            p.text()
                                .ok_or(CodecError::Unsupported(CarrierReason::Unavailable))?,
                        )
                    };
                    if matches!(event, StreamEvent::PartInterrupted { .. }) {
                        v["status"] = json!("incomplete");
                    }
                    vec![v]
                }
            }
            StreamEvent::CompactionProgress { item } => vec![json!({
                "type":"response.compaction.compacting",
                "output_index":self.index(*item)?,
                "item_id":self.fidelity.response_item_id(*item).ok_or(CodecError::invalid())?,
            })],
            StreamEvent::ItemFinished { item, .. } => {
                let value = item_wire(self.state()?, *item, &self.fidelity)?;
                if let Some(slot) = self.pending_program_outputs.remove(item) {
                    self.held_frames[slot]["item"]["status"] = value["status"].clone();
                }
                vec![json!({
                    "type": "response.output_item.done",
                    "output_index": self.index(*item)?,
                    "item": value,
                })]
            }
            StreamEvent::AnnotationAdded {
                item,
                part,
                annotation,
            } => {
                if annotation.value().kind == CitationKind::ArtifactReference {
                    return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                }
                let mut v = self.coordinates(*item, *part)?;
                v["type"] = json!("response.output_text.annotation.added");
                v["annotation_index"] =
                    json!(self.state()?.part(*item, *part)?.annotations.len() - 1);
                v["annotation"] = crate::protocol::openai::content::citations::write(
                    annotation,
                    self.state()?.resources(),
                )?;
                vec![v]
            }
            StreamEvent::ResourceDeclared { .. }
            | StreamEvent::ToolResultOutput { .. }
            | StreamEvent::MessageEnvelope(_) => vec![],
            StreamEvent::LogprobsSnapshot { .. }
            | StreamEvent::TextMetadata { .. }
            | StreamEvent::Usage(_)
            | StreamEvent::UsageReport(_) => vec![],
            StreamEvent::PartReplay { .. } => {
                return Err(CodecError::Unsupported(CarrierReason::Replay));
            }
            StreamEvent::SafetyReport(_) => {
                return Err(CodecError::Unsupported(CarrierReason::Safety));
            }
            StreamEvent::Progress(_) => {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
            StreamEvent::Terminal {
                terminal: StreamTerminal::Error,
                details,
            } => {
                let mut v = crate::protocol::openai::terminal::encode_error(details.error.as_ref());
                if details
                    .error
                    .as_ref()
                    .is_some_and(|e| e.original_type.is_some())
                {
                    return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                }
                if !v.is_object() {
                    return Err(CodecError::invalid());
                }
                v["type"] = json!("error");
                if v.get("param").is_none() {
                    v["param"] = Value::Null;
                }
                vec![v]
            }
            StreamEvent::Terminal { terminal, .. } => {
                let response = materialize(self.state()?)?;
                let mut metadata = self.metadata.clone();
                // The stream already carried its opening observation. Only the
                // terminal frame uses this view; static snapshots record the loss.
                if *terminal == StreamTerminal::Failed
                    && matches!(
                        metadata.context.reports,
                        crate::semantic::context::ResponseReports::Lifecycle { terminal: None, .. }
                    )
                {
                    // A gateway-classified failure has no upstream terminal
                    // report. Emit the failure without inventing reported values.
                    metadata.context.reports = Default::default();
                } else {
                    crate::protocol::reports::terminal_view(&mut metadata.context.reports)?;
                }
                let target = crate::protocol::openai::lowering::lower_response(
                    &response,
                    &self.fidelity,
                    &metadata,
                    GenerationApi::Responses,
                    self.contract.clone(),
                )
                .map_err(|_| CodecError::Unsupported(CarrierReason::Terminal))?;
                let v = crate::protocol::openai::responses::encode_response(&target)?;
                let kind = match terminal {
                    StreamTerminal::Completed => "response.completed",
                    StreamTerminal::Incomplete => "response.incomplete",
                    StreamTerminal::Failed => "response.failed",
                    StreamTerminal::Cancelled => {
                        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                    }
                    StreamTerminal::Error => unreachable!(),
                };
                super::record_stages(&mut self.projection, target.projection().iter().cloned());
                vec![json!({"type":kind,"response":v})]
            }
        };
        Ok(result)
    }
}
fn event_stem(kind: PartKind) -> &'static str {
    match kind {
        PartKind::ResultText | PartKind::ResultJson => {
            unreachable!("Provider result carrier rejected")
        }
        PartKind::Audio => unreachable!("Responses audio carrier rejected"),
        PartKind::Text => "output_text",
        PartKind::Refusal => "refusal",
        PartKind::Summary => "reasoning_summary_text",
        PartKind::ReasoningText => "reasoning_text",
        PartKind::Arguments => "function_call_arguments",
        PartKind::StructuredArguments => "function_call_arguments",
        PartKind::CustomInput => "custom_tool_call_input",
    }
}
