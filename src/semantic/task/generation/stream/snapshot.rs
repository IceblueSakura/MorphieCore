//! Partial item snapshots and terminal response materialization.
use crate::semantic::task::generation::*;
use crate::semantic::value::Text;
pub(super) fn outcome(terminal: StreamTerminal) -> Outcome {
    match terminal {
        StreamTerminal::Completed => Outcome::Completed,
        StreamTerminal::Incomplete => Outcome::Incomplete,
        StreamTerminal::Failed | StreamTerminal::Error => Outcome::Failed,
        StreamTerminal::Cancelled => Outcome::Cancelled,
    }
}
pub fn end_of_stream(state: &StreamState) -> Result<(), EventError> {
    if state.terminal.is_some() {
        Ok(())
    } else {
        Err(EventError::EofBeforeTerminal)
    }
}
/// Partial snapshots preserve item order, unfinished status and typed opaque values.
pub fn snapshot_items(state: &StreamState) -> Result<Vec<(ItemId, Item)>, EventError> {
    state
        .items
        .iter()
        .map(|i| Ok((i.id, i.snapshot()?)))
        .collect()
}
impl StreamItem {
    pub(crate) fn snapshot(&self) -> Result<Item, EventError> {
        let i = self;
        let status = i.status.unwrap_or(ItemLifecycle::InProgress);
        let bounded = |p: &StreamPart| {
            Text::allowing_empty(
                p.text().ok_or(EventError::Lifecycle)?,
                "event part",
                MAX_TEXT_BYTES,
            )
            .map_err(|_| EventError::Limit)
        };
        let item = match &i.kind {
            ItemKind::Message { phase } => Item::Message(Message {
                role: MessageRole::Assistant,
                status,
                phase: *phase,
                parts: i
                    .parts
                    .iter()
                    .map(|p| {
                        Ok(Part {
                            id: p.id,
                            content: match p.kind {
                                PartKind::Audio => match &p.data {
                                    StreamPartValue::Audio(value) => {
                                        ContentPart::Audio(value.clone())
                                    }
                                    _ => return Err(EventError::Lifecycle),
                                },
                                PartKind::Text => ContentPart::Text(TextContent::new(
                                    bounded(p)?,
                                    p.annotations.clone(),
                                    p.logprobs.clone(),
                                )?),
                                PartKind::Refusal => ContentPart::Refusal(RefusalContent::new(
                                    bounded(p)?,
                                    p.logprobs.clone(),
                                )?),
                                _ => return Err(EventError::Lifecycle),
                            },
                        })
                    })
                    .collect::<Result<_, EventError>>()?,
            }),
            ItemKind::ToolCall {
                format,
                call_id,
                name,
                context,
                ..
            } => Item::ToolCall(ToolCall {
                call_id: call_id.clone(),
                name: name.clone(),
                status,
                arguments: i
                    .parts
                    .first()
                    .map(|p| p.data.arguments())
                    .unwrap_or_else(|| match format {
                        ArgumentFormat::Raw => "".into(),
                        ArgumentFormat::Json => ToolArguments::StructuredPartial(String::new()),
                    }),
                context: context.clone(),
            }),
            ItemKind::CustomCall {
                call_id,
                name,
                context,
            } => Item::CustomCall(CustomCall {
                call_id: call_id.clone(),
                name: name.clone(),
                input: i
                    .parts
                    .first()
                    .map(|p| p.text().map(str::to_owned).ok_or(EventError::Lifecycle))
                    .transpose()?
                    .unwrap_or_default(),
                context: context.clone(),
            }),
            ItemKind::Reasoning => Item::Reasoning(ReasoningItem {
                status,
                replay: i.replay.as_ref().map(|r| r.value.clone()),
                parts: i
                    .parts
                    .iter()
                    .map(|p| {
                        Ok((
                            p.id,
                            match p.kind {
                                PartKind::Summary => ReasoningContent::Summary(bounded(p)?),
                                PartKind::ReasoningText => ReasoningContent::Text(bounded(p)?),
                                _ => return Err(EventError::Lifecycle),
                            },
                        ))
                    })
                    .collect::<Result<_, EventError>>()?,
            }),
            ItemKind::ConfigurationUpdate { effort } => {
                Item::ConfigurationUpdate(ConfigurationUpdate { effort: *effort })
            }
            ItemKind::Program {
                call_id,
                code,
                fingerprint,
            } => Item::Program(Program {
                call_id: call_id.clone(),
                code: code.clone(),
                fingerprint: fingerprint.clone(),
            }),
            ItemKind::ProgramOutput { call_id, result } => Item::ProgramOutput(ProgramOutput {
                call_id: call_id.clone(),
                result: result.clone(),
                status,
            }),
            ItemKind::ProviderTool(observed) => Item::ProviderTool(observed.clone()),
            ItemKind::ProviderResult { observation, .. } => {
                let part = i
                    .parts
                    .first()
                    .filter(|part| part.finished)
                    .ok_or(EventError::Lifecycle)?;
                let mut observed = observation.clone();
                observed.output = Some(match &part.data {
                    StreamPartValue::Text(value) => ToolOutput::Text(value.clone()),
                    StreamPartValue::Json(value) => ToolOutput::Structured(value.clone()),
                    _ => return Err(EventError::Lifecycle),
                });
                Item::ProviderTool(observed)
            }
        };
        Ok(item)
    }
}
pub fn materialize(state: &StreamState) -> Result<GenerationResponse, EventError> {
    let terminal = state.terminal.ok_or(EventError::EofBeforeTerminal)?;
    if terminal == StreamTerminal::Error {
        return Err(EventError::TerminalFailure(terminal));
    }
    let mut response = GenerationResponse::from_resources(
        snapshot_items(state)?,
        outcome(terminal),
        state.resources.clone(),
    )?
    .with_message_envelopes(state.message_envelopes.clone())?
    .with_progress(state.progress)?
    .with_details(state.details.clone())?;
    response = response.with_usage(state.usage)?;
    Ok(response)
}
