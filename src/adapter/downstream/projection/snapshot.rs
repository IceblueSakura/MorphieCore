//! Feed a decoded complete body through the same client event projection as SSE.
use super::*;
use base64::Engine;

pub(crate) fn emit(
    response: &DecodedResponse,
    mut push: impl FnMut(StreamEvent) -> Result<(), CodecError>,
) -> Result<(), CodecError> {
    let source = &response.semantic;
    push(StreamEvent::Started)?;
    for (id, resource) in source.resources().iter() {
        push(StreamEvent::ResourceDeclared {
            id,
            resource: resource.clone(),
        })?;
    }
    for (id, item) in source.items() {
        let replay = |value: &ReplayValue| ItemReplay {
            value: value.clone(),
            origin: response.fidelity.replay_origin(*id).cloned(),
        };
        let kind = match item {
            Item::Message(message) => {
                if message.role != MessageRole::Assistant || message.participant.is_some() {
                    return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                }
                ItemKind::Message {
                    phase: message.phase,
                }
            }
            Item::ToolCall(call) => ItemKind::ToolCall {
                format: if matches!(call.arguments, ToolArguments::Raw(_)) {
                    ArgumentFormat::Raw
                } else {
                    ArgumentFormat::Json
                },
                call_id: call.call_id.clone(),
                name: call.name.clone(),
                context: call.context.clone(),
            },
            Item::CustomCall(call) => ItemKind::CustomCall {
                call_id: call.call_id.clone(),
                name: call.name.clone(),
                context: call.context.clone(),
            },
            Item::Reasoning(_) => ItemKind::Reasoning,
            Item::Compaction(_) => ItemKind::Compaction,
            Item::ToolResult(result) | Item::CustomResult(result) => {
                let mut opening = result.clone();
                opening.status = Some(ItemLifecycle::InProgress);
                if matches!(item, Item::ToolResult(_)) {
                    ItemKind::ToolResult(opening)
                } else {
                    ItemKind::CustomResult(opening)
                }
            }
            Item::ConfigurationUpdate(update) => ItemKind::ConfigurationUpdate(*update),
            Item::ToolBatch(batch) => ItemKind::ToolBatch(batch.clone()),
            Item::Program(program) => ItemKind::Program {
                call_id: program.call_id.clone(),
                code: program.code.clone(),
                fingerprint: program.fingerprint.clone(),
            },
            Item::ProgramOutput(output) => ItemKind::ProgramOutput {
                call_id: output.call_id.clone(),
                result: output.result.clone(),
            },
            Item::ProviderTool(observation) => ItemKind::ProviderTool(observation.clone()),
            _ => return Err(CodecError::Unsupported(CarrierReason::Unavailable)),
        };
        let opaque = match item {
            Item::Reasoning(reasoning) => reasoning.replay.as_ref().map(replay),
            Item::Compaction(value) => Some(replay(value)),
            Item::ToolCall(call) => call.replay.as_ref().map(replay),
            _ => None,
        };
        push(StreamEvent::ItemStarted {
            item: *id,
            kind,
            replay: opaque.clone(),
        })?;
        match item {
            Item::Message(message) => {
                for part in &message.parts {
                    let (kind, text, annotations, logprobs) = match &part.content {
                        ContentPart::Text(text) => (
                            PartKind::Text,
                            text.as_str(),
                            text.annotations().to_vec(),
                            text.logprobs().clone(),
                        ),
                        ContentPart::Refusal(text) => (
                            PartKind::Refusal,
                            text.as_str(),
                            vec![],
                            text.logprobs().clone(),
                        ),
                        ContentPart::Audio(audio) => {
                            push(StreamEvent::PartStarted {
                                item: *id,
                                part: part.id,
                                kind: PartKind::Audio,
                            })?;
                            push(StreamEvent::AudioDelta {
                                item: *id,
                                part: part.id,
                                update: AudioUpdate {
                                    id: Some(
                                        crate::semantic::value::Text::new(
                                            audio.reference().id(),
                                            "audio id",
                                            256,
                                        )
                                        .map_err(|_| CodecError::invalid())?,
                                    ),
                                    expires_at: audio.reference().expires_at(),
                                    data: Some(
                                        base64::engine::general_purpose::STANDARD
                                            .decode(audio.data_base64())
                                            .map_err(|_| CodecError::invalid())?,
                                    ),
                                    transcript: Some(
                                        crate::semantic::value::Text::allowing_empty(
                                            audio.transcript(),
                                            "transcript",
                                            MAX_TEXT_BYTES,
                                        )
                                        .map_err(|_| CodecError::invalid())?,
                                    ),
                                },
                            })?;
                            push(StreamEvent::ValueFinished {
                                item: *id,
                                part: part.id,
                            })?;
                            push(StreamEvent::PartFinished {
                                item: *id,
                                part: part.id,
                            })?;
                            continue;
                        }
                        _ => return Err(CodecError::Unsupported(CarrierReason::Unavailable)),
                    };
                    push(StreamEvent::PartStarted {
                        item: *id,
                        part: part.id,
                        kind,
                    })?;
                    push(StreamEvent::Delta {
                        item: *id,
                        part: part.id,
                        fragment: text.into(),
                        logprobs: vec![],
                    })?;
                    push(StreamEvent::ValueFinished {
                        item: *id,
                        part: part.id,
                    })?;
                    push(StreamEvent::TextMetadata {
                        item: *id,
                        part: part.id,
                        annotations,
                        logprobs,
                    })?;
                    push(StreamEvent::PartFinished {
                        item: *id,
                        part: part.id,
                    })?;
                }
            }
            Item::Reasoning(reasoning) => {
                for (part, content) in &reasoning.parts {
                    let (kind, text) = match content {
                        ReasoningContent::Summary(text) => (PartKind::Summary, text.as_str()),
                        ReasoningContent::Text(text) => (PartKind::ReasoningText, text.as_str()),
                    };
                    part_events(*id, *part, kind, text, reasoning.status, &mut push)?;
                }
            }
            Item::ToolCall(call) => {
                let part = PartId::scoped(
                    id.scope(),
                    id.get()
                        .checked_mul(MAX_ITEMS as u64 + 1)
                        .ok_or(CodecError::Limit)?,
                );
                let (kind, text) = match &call.arguments {
                    ToolArguments::Raw(text) => (PartKind::Arguments, text.clone()),
                    ToolArguments::Structured(value) => {
                        (PartKind::StructuredArguments, value.value().to_string())
                    }
                    ToolArguments::StructuredPartial(text) => {
                        (PartKind::StructuredArguments, text.clone())
                    }
                };
                part_events(
                    *id,
                    part,
                    kind,
                    &text,
                    if matches!(call.arguments, ToolArguments::StructuredPartial(_)) {
                        ItemLifecycle::Incomplete
                    } else {
                        ItemLifecycle::Completed
                    },
                    &mut push,
                )?;
            }
            Item::CustomCall(call) => {
                let part = PartId::scoped(
                    id.scope(),
                    id.get()
                        .checked_mul(MAX_ITEMS as u64 + 1)
                        .ok_or(CodecError::Limit)?,
                );
                part_events(
                    *id,
                    part,
                    PartKind::CustomInput,
                    &call.input,
                    ItemLifecycle::Completed,
                    &mut push,
                )?;
            }
            _ => {}
        }
        let status = item.lifecycle().unwrap_or(ItemLifecycle::Completed);
        if status != ItemLifecycle::InProgress {
            push(StreamEvent::ItemFinished {
                item: *id,
                status,
                replay: opaque,
            })?;
        }
    }
    for envelope in source.message_envelopes() {
        push(StreamEvent::MessageEnvelope(envelope.clone()))?;
    }
    if source.progress() != InteractionProgress::Unreported {
        push(StreamEvent::Progress(source.progress()))?;
    }
    for report in source.usage_reports().reports() {
        push(StreamEvent::UsageReport(report.clone()))?;
    }
    for report in source.safety_reports() {
        push(StreamEvent::SafetyReport(report.clone()))?;
    }
    push(StreamEvent::Terminal {
        terminal: match source.outcome() {
            Outcome::Completed => StreamTerminal::Completed,
            Outcome::Incomplete => StreamTerminal::Incomplete,
            Outcome::Failed => StreamTerminal::Failed,
            Outcome::Cancelled => StreamTerminal::Cancelled,
        },
        details: source.details().clone(),
    })
}
fn part_events(
    item: ItemId,
    part: PartId,
    kind: PartKind,
    text: &str,
    status: ItemLifecycle,
    push: &mut impl FnMut(StreamEvent) -> Result<(), CodecError>,
) -> Result<(), CodecError> {
    push(StreamEvent::PartStarted { item, part, kind })?;
    push(StreamEvent::Delta {
        item,
        part,
        fragment: text.into(),
        logprobs: vec![],
    })?;
    if status == ItemLifecycle::Completed {
        push(StreamEvent::ValueFinished { item, part })?;
        push(StreamEvent::PartFinished { item, part })?;
    }
    Ok(())
}
