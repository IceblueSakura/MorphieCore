//! Event rendering uses only validated semantic state and owner-bound wire identities.
use super::*;
use serde_json::json;

pub struct EventEncoder {
    pub(super) profile: Profile,
    pub(super) metadata: ResponseMetadata,
    pub(super) state: Option<StreamState>,
    pub(super) fidelity: FidelityRecords,
    pub(super) contract: GenerationRepresentationContract,
    poisoned: bool,
    sequence: u64,
    pub(super) chat_audio_pending: Vec<u8>,
}
impl EventEncoder {
    pub fn new(profile: Profile, metadata: ResponseMetadata) -> Result<Self, CodecError> {
        if metadata.id.is_empty()
            || metadata.model.is_empty()
            || metadata.id.len() > 256
            || metadata.model.len() > 256
        {
            return Err(CodecError::Invalid("metadata"));
        }
        metadata.context.validate()?;
        validate_metadata(profile, &metadata)?;
        super::super::envelope::timestamp(&Value::Number(metadata.created.clone()))?;
        Ok(Self {
            profile,
            metadata,
            state: Some(StreamState::new()),
            fidelity: FidelityRecords::default(),
            contract: GenerationRepresentationContract::full(),
            poisoned: false,
            sequence: 0,
            chat_audio_pending: Vec::new(),
        })
    }
    /// Refresh reported settings/completion metadata without changing response identity.
    pub fn update_metadata(&mut self, metadata: ResponseMetadata) -> Result<(), CodecError> {
        if self.poisoned {
            return Err(CodecError::Invalid("rejected stream"));
        }
        let result = (|| {
            if self.state()?.terminal().is_some()
                || self.metadata.id != metadata.id
                || self.metadata.model != metadata.model
                || self.metadata.created != metadata.created
            {
                return Err(CodecError::Invalid("metadata changed"));
            }
            metadata.context.validate()?;
            validate_metadata(self.profile, &metadata)?;
            if self.profile == Profile::Chat && self.state()?.started() {
                let old = &self.metadata.context;
                if (!old.system_fingerprint.is_absent()
                    && old.system_fingerprint != metadata.context.system_fingerprint)
                    || (!old.execution.service_tier.is_absent()
                        && old.execution.service_tier != metadata.context.execution.service_tier)
                {
                    // Intake normalization is not permission to rewrite already emitted facts.
                    return Err(CodecError::Invalid("bound Chat metadata changed"));
                }
            }
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        } else {
            self.metadata = metadata;
        }
        result
    }
    pub fn with_contract(mut self, contract: GenerationRepresentationContract) -> Self {
        self.contract = contract;
        self
    }
    pub fn encode(
        &mut self,
        event: &StreamEvent,
        source: &FidelityRecords,
    ) -> Result<Vec<Value>, CodecError> {
        if self.poisoned {
            return Err(CodecError::Invalid("rejected stream"));
        }
        let result = (|| {
            crate::lowering::events::check_event(
                self.state()?,
                event,
                self.profile,
                &self.contract,
            )
            .map_err(|_| CodecError::Unsupported("event target representation".into()))?;
            if let StreamEvent::ItemStarted { item, .. } = event {
                let id = source
                    .response_item_id(*item)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("item_{}", item.get()));
                self.fidelity.record_response_item_id(*item, &id)?;
            }
            // Chat summary chunks have not emitted a wire item identity yet.
            // Bind the issuer's late identity before rendering encrypted replay.
            if self.profile == Profile::Chat
                && self.contract.adaptation.rules.structured_chat_reasoning
                && let StreamEvent::ItemFinished {
                    item,
                    replay: Some(_),
                    ..
                } = event
                && let Some(id) = source.response_item_id(*item)
            {
                self.fidelity.record_response_item_id(*item, id)?;
            }
            let probabilities = if self.profile == Profile::Chat {
                self.chat_probabilities(event)?
            } else {
                None
            };
            self.state = Some(reduce(
                self.state.take().ok_or(CodecError::Invalid("state"))?,
                event.clone(),
            )?);
            // Replay is event authority, never restored from the source sidecar.
            match event {
                StreamEvent::ItemStarted { item, .. } | StreamEvent::ItemFinished { item, .. } => {
                    sync_replays(
                        self.state.as_ref().ok_or(CodecError::Invalid("state"))?,
                        &mut self.fidelity,
                        Some(*item),
                    )?
                }
                StreamEvent::Terminal { .. } => sync_replays(
                    self.state.as_ref().ok_or(CodecError::Invalid("state"))?,
                    &mut self.fidelity,
                    None,
                )?,
                _ => {}
            }
            if matches!(event, StreamEvent::Terminal { .. }) {
                self.fidelity.copy_response_records(source);
            }
            let mut values = if self.profile == Profile::Responses {
                self.responses(event)?
            } else {
                self.chat(event, probabilities)?
            };
            for v in &mut values {
                if self.profile == Profile::Responses {
                    v["sequence_number"] = json!(self.sequence);
                    self.sequence += 1;
                }
                bounded(v)?;
            }
            Ok(values)
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    pub fn finish(&self) -> Result<(), CodecError> {
        if self.poisoned {
            return Err(CodecError::Invalid("rejected stream"));
        }
        end_of_stream(self.state()?)?;
        Ok(())
    }
    pub(super) fn state(&self) -> Result<&StreamState, CodecError> {
        self.state.as_ref().ok_or(CodecError::Invalid("state"))
    }
    fn index(&self, id: ItemId) -> Result<usize, CodecError> {
        self.state()?
            .items()
            .iter()
            .position(|i| i.id == id)
            .ok_or(CodecError::Invalid("item identity"))
    }
    fn part_index(&self, item: ItemId, part: PartId) -> Result<usize, CodecError> {
        let kind = self.state()?.part(item, part)?.kind;
        self.state()?
            .item(item)?
            .parts
            .iter()
            .filter(|p| (p.kind == PartKind::Summary) == (kind == PartKind::Summary))
            .position(|p| p.id == part)
            .ok_or(CodecError::Invalid("part identity"))
    }
    fn coordinates(&self, item: ItemId, part: PartId) -> Result<Value, CodecError> {
        let kind = self.state()?.part(item, part)?.kind;
        let mut v = json!({"output_index":self.index(item)?,"item_id":self.fidelity.response_item_id(item).ok_or(CodecError::Invalid("wire identity"))?});
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
    pub(super) fn envelope(&self, status: &str, output: Vec<Value>) -> Result<Value, CodecError> {
        let mut v = json!({"id":self.metadata.id,"object":"response","created_at":self.metadata.created,"model":self.metadata.model,"status":status,"output":output});
        super::super::envelope::write_metadata(
            &self.metadata,
            v.as_object_mut().expect("object"),
            status == "completed",
        )?;
        Ok(v)
    }
    fn responses(&self, event: &StreamEvent) -> Result<Vec<Value>, CodecError> {
        let result = match event {
            StreamEvent::AudioDelta { .. } => {
                return Err(CodecError::Unsupported("Responses audio carrier".into()));
            }
            StreamEvent::Queued => {
                vec![json!({"type":"response.queued","response":self.envelope("queued",vec![])?})]
            }
            StreamEvent::Started => vec![
                json!({"type":"response.created","response":self.envelope("in_progress",vec![])?}),
                json!({"type":"response.in_progress","response":self.envelope("in_progress",vec![])?}),
            ],
            StreamEvent::ItemStarted { item, kind, replay } => {
                let id = self
                    .fidelity
                    .response_item_id(*item)
                    .ok_or(CodecError::Invalid("wire identity"))?;
                let mut v = match kind {
                    ItemKind::Message { phase } => {
                        let mut v = json!({"id":id,"type":"message","role":"assistant","content":[],"status":"in_progress"});
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
                        let mut v = json!({"id":id,"type":"custom_tool_call","call_id":call_id.as_str(),"name":name.as_str(),"input":""});
                        super::super::responses::write_call_context(
                            context,
                            v.as_object_mut().expect("object"),
                        );
                        v
                    }
                    ItemKind::Reasoning => {
                        json!({"id":id,"type":"reasoning","summary":[],"status":"in_progress"})
                    }
                    ItemKind::ConfigurationUpdate { effort } => {
                        let mut v = json!({"id":id,"type":"configuration_update"});
                        if let Some(effort) = effort {
                            v["reasoning"] =
                                json!({"effort": super::super::reasoning::effort_label(*effort)});
                        }
                        v
                    }
                    ItemKind::Program {
                        call_id,
                        code,
                        fingerprint,
                    } => {
                        json!({"id":id,"type":"program","call_id":call_id.as_str(),"code":code,"fingerprint":fingerprint})
                    }
                    ItemKind::ProgramOutput { call_id, result } => {
                        json!({"id":id,"type":"program_output","call_id":call_id.as_str(),"result":result})
                    }
                    ItemKind::ToolCall {
                        call_id,
                        name,
                        context,
                        ..
                    } => {
                        let mut v = json!({"id":id,"type":"function_call","call_id":call_id.as_str(),"name":name.as_str(),"arguments":"","status":"in_progress"});
                        super::super::responses::write_call_context(
                            context,
                            v.as_object_mut().expect("object"),
                        );
                        v
                    }
                };
                if let Some(r) = replay
                    && r.value.format() == ReplayFormat::ResponsesEncrypted
                {
                    v["encrypted_content"] = json!(r.value.as_str());
                }
                vec![
                    json!({"type":"response.output_item.added","output_index":self.index(*item)?,"item":v}),
                ]
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
                    v["logprobs"] = super::super::text::event_logprobs(logprobs);
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
                    v["logprobs"] = super::super::text::event_logprobs(
                        p.logprobs.value().map(Vec::as_slice).unwrap_or_default(),
                    );
                }
                vec![v]
            }
            StreamEvent::PartFinished { item, part } => {
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
                                .ok_or(CodecError::Unsupported("structured arguments".into()))?,
                            "text",
                            MAX_TEXT_BYTES,
                        )
                        .map_err(|_| CodecError::Limit)?;
                        super::super::text::write(
                            &TextContent::new(text, p.annotations.clone(), p.logprobs.clone())?,
                            "output_text",
                        )
                    } else {
                        part_wire(
                            p.kind,
                            p.text()
                                .ok_or(CodecError::Unsupported("structured arguments".into()))?,
                        )
                    };
                    vec![v]
                }
            }
            StreamEvent::ItemFinished { item, .. } => vec![
                json!({"type":"response.output_item.done","output_index":self.index(*item)?,"item":item_wire(self.state()?,*item,&self.fidelity)?}),
            ],
            StreamEvent::AnnotationAdded {
                item,
                part,
                annotation,
            } => {
                if matches!(annotation, Annotation::FilePath { .. }) {
                    return Err(CodecError::Unsupported("file_path annotation event".into()));
                }
                let mut v = self.coordinates(*item, *part)?;
                v["type"] = json!("response.output_text.annotation.added");
                v["annotation_index"] =
                    json!(self.state()?.part(*item, *part)?.annotations.len() - 1);
                v["annotation"] = json!(annotation);
                vec![v]
            }
            StreamEvent::LogprobsSnapshot { .. }
            | StreamEvent::TextMetadata { .. }
            | StreamEvent::Usage(_) => vec![],
            StreamEvent::Progress(_) => {
                return Err(CodecError::Unsupported("interaction progress".into()));
            }
            StreamEvent::Terminal {
                terminal: StreamTerminal::Error,
                details,
            } => {
                let mut v = super::super::terminal::encode_error(details.error.as_ref());
                if !v.is_object() {
                    return Err(CodecError::Invalid("error details"));
                }
                v["type"] = json!("error");
                vec![v]
            }
            StreamEvent::Terminal { terminal, .. } => {
                let response = materialize(self.state()?)?;
                let target = crate::lowering::generation::lower_response(
                    &response,
                    &self.fidelity,
                    &self.metadata,
                    Profile::Responses,
                    self.contract.clone(),
                )
                .map_err(|_| CodecError::Unsupported("terminal representation".into()))?;
                let v = super::super::responses::encode_response(&target)?;
                let kind = match terminal {
                    StreamTerminal::Completed => "response.completed",
                    StreamTerminal::Incomplete => "response.incomplete",
                    StreamTerminal::Failed => "response.failed",
                    StreamTerminal::Cancelled => {
                        return Err(CodecError::Unsupported("cancelled event".into()));
                    }
                    StreamTerminal::Error => unreachable!(),
                };
                vec![json!({"type":kind,"response":v})]
            }
        };
        Ok(result)
    }
}
fn validate_metadata(profile: Profile, metadata: &ResponseMetadata) -> Result<(), CodecError> {
    if profile == Profile::Chat {
        super::super::chat_envelope::validate_context(&metadata.context.execution)?;
        if metadata.created.as_u64().is_none() || !metadata.context.execution.metadata.is_absent() {
            // Chat chunks have no metadata slot; never silently discard a static fact.
            return Err(CodecError::Unsupported("Chat stream metadata".into()));
        }
    }
    Ok(())
}
fn event_stem(kind: PartKind) -> &'static str {
    match kind {
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
