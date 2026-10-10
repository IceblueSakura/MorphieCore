//! Static native request/response mapping over the shared IR.
use super::{
    Block, MessageContent, Profile, Role, Selection, TextContent as WireText,
    semantic_intake::{Intake, bind, text},
    semantic_request::{item_blocks, project, unsupported},
    target::ReplayTarget,
};
use crate::{
    protocol::{CodecError, DecodedRequest, DecodedResponse, fidelity::FidelityRecords},
    semantic::{task::generation::*, value::Presence},
};
use serde_json::Value;
/// Body-free receipts identify actual selected normalization/loss, not permission.
pub struct NativeProjection {
    pub value: Value,
    pub normalized_arguments: Vec<ItemId>,
    pub omitted_execution: Vec<ItemId>,
}
impl std::fmt::Debug for NativeProjection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeProjection")
            .field("body", &"[redacted]")
            .field("normalized_arguments", &self.normalized_arguments)
            .field("omitted_execution", &self.omitted_execution)
            .finish()
    }
}
impl Profile {
    pub fn decode_generation_request(
        self,
        bytes: &[u8],
        scope: LocalScope,
        target: &ReplayTarget,
    ) -> Result<DecodedRequest, CodecError> {
        let wire = self.decode_request(bytes)?;
        if wire.model != target.model() {
            return Err(CodecError::Invalid("native model binding"));
        }
        if wire.stream == Presence::Value(true) {
            return Err(CodecError::Unsupported("native streaming mapping".into()));
        }
        let mut intake = Intake::new(scope);
        let mut settings = GenerationSettings::default();
        settings.controls.max_output_tokens = Some(wire.max_tokens);
        match &wire.system {
            Presence::Value(WireText::Text(s)) => settings.instructions = Presence::Value(text(s)?),
            Presence::Value(WireText::Blocks(parts)) => {
                if parts.is_empty() {
                    return Err(CodecError::Unsupported("empty system block carrier".into()));
                }
                intake.items.push((
                    ItemId::scoped(scope, 1),
                    Item::Instruction(Instruction {
                        authority: InstructionAuthority::System,
                        status: None,
                        parts: parts
                            .iter()
                            .enumerate()
                            .map(|(i, s)| Ok((PartId::scoped(scope, i as u64 + 1), text(s)?)))
                            .collect::<Result<_, CodecError>>()?,
                    }),
                ));
            }
            _ => {}
        }
        if let Presence::Value(tools) = &wire.tools {
            settings.tools = Some(
                tools
                    .iter()
                    .map(|t| {
                        Ok(ToolDefinition::Function(FunctionTool {
                            name: text(&t.name)?,
                            description: t.description.value().cloned(),
                            parameters: Some(t.input_schema.clone()),
                            strict: t.strict.value().map_or(
                                FunctionStrictness::Omitted(StrictDefault::NonStrict),
                                |b| FunctionStrictness::Explicit(*b),
                            ),
                            output_schema: None,
                            dispatch: Default::default(),
                        }))
                    })
                    .collect::<Result<_, CodecError>>()?,
            );
        }
        if let Presence::Value(choice) = &wire.tool_choice {
            settings.tool_choice = Some(match &choice.selection {
                Selection::Auto => ToolChoice::Auto,
                Selection::None => ToolChoice::None,
                Selection::Any => ToolChoice::Required,
                Selection::Tool(s) => ToolChoice::Specific(text(s)?),
            });
            settings.parallel_tool_calls = choice.disable_parallel_tool_use.value().map(|b| !b);
        }
        if let Presence::Value(thinking) = &wire.thinking {
            settings.reasoning = ReasoningRequest::present(None, None);
            settings.reasoning.mode = Presence::Value(ReasoningMode::Adaptive);
            settings.reasoning.display = match thinking.display {
                Presence::Absent => Presence::Absent,
                Presence::Null => Presence::Null,
                Presence::Value(super::Display::Summarized) => {
                    Presence::Value(ReasoningDisplay::Summary)
                }
                Presence::Value(super::Display::Omitted) => {
                    Presence::Value(ReasoningDisplay::Omitted)
                }
            };
        }
        if let Presence::Value(effort) = &wire.output_config {
            settings.reasoning.presence = ReasoningPresence::Present;
            settings.reasoning.effort = Presence::Value(match effort {
                super::Effort::Low => ReasoningEffort::Low,
                super::Effort::Medium => ReasoningEffort::Medium,
                super::Effort::High => ReasoningEffort::High,
            });
        }
        for message in &wire.messages {
            match &message.content {
                MessageContent::Text(s) => {
                    intake.message(message.role, &[Block::Text(s.clone())])?
                }
                MessageContent::Blocks(blocks) => intake.message(message.role, blocks)?,
            }
        }
        let semantic = GenerationRequest::from_settings(intake.items, settings)?
            .with_message_envelopes(intake.groups)?;
        let mut fidelity = FidelityRecords::default();
        bind(&mut fidelity, &semantic, semantic.items(), target)?;
        Ok(DecodedRequest { semantic, fidelity })
    }
    /// Capture dependencies while the actual request and complete native output are available.
    pub fn decode_generation_response(
        self,
        bytes: &[u8],
        scope: LocalScope,
        source: &DecodedRequest,
        target: &ReplayTarget,
    ) -> Result<DecodedResponse, CodecError> {
        source.semantic.validate()?;
        let wire = self.decode_response(bytes)?;
        if wire.model != target.model() {
            return Err(CodecError::Invalid("native model binding"));
        }
        let mut intake = Intake::new(scope);
        intake.message(Role::Assistant, &wire.content)?;
        let (outcome, progress, details) = match wire.stop_reason.value().expect("validated stop") {
            super::StopReason::EndTurn => (
                Outcome::Completed,
                InteractionProgress::TurnFinished,
                TerminalDetails::default(),
            ),
            super::StopReason::ToolUse => (
                Outcome::Completed,
                InteractionProgress::AwaitingToolResults,
                TerminalDetails::default(),
            ),
            super::StopReason::MaxTokens => (
                Outcome::Incomplete,
                InteractionProgress::Unreported,
                TerminalDetails {
                    incomplete: Some(IncompleteReason::MaxOutputTokens),
                    error: None,
                },
            ),
        };
        let mut usage = Usage::operation(0, 0, 0);
        usage.input_tokens = wire.usage.input_tokens.value().copied();
        usage.output_tokens = Some(wire.usage.output_tokens);
        usage.total_tokens = None;
        usage.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
        usage.total_relation = TotalTokenRelation::Unreported;
        usage.cached_input_tokens = wire.usage.cache_read_input_tokens.value().copied();
        usage.input_cache_write_tokens = wire.usage.cache_creation_input_tokens.value().copied();
        let semantic = GenerationResponse::new(intake.items, outcome)?
            .with_message_envelopes(intake.groups)?
            .with_progress(progress)?
            .with_details(details)?
            .with_usage(usage)?;
        let joined = ClientManaged::new(source.semantic.settings().clone())?
            .select_request(&source.semantic)?
            .select_response(&semantic)?
            .build(&[])?;
        let mut fidelity = source.fidelity.clone();
        bind(&mut fidelity, &joined, semantic.items(), target)?;
        Ok(DecodedResponse {
            semantic,
            fidelity,
            metadata: wire.reported_metadata()?,
        })
    }
    pub fn encode_generation_request(
        self,
        request: &GenerationRequest,
        fidelity: &FidelityRecords,
        target: &ReplayTarget,
    ) -> Result<NativeProjection, CodecError> {
        request.validate()?;
        fidelity.require_reasoning_replay(request, &target.origin())?;
        let mut normalized = vec![];
        let mut omitted = vec![];
        let wire = project(
            request,
            fidelity,
            target,
            &mut normalized,
            &mut omitted,
            None,
        )?;
        let value = self.encode_request(&wire)?;
        Ok(NativeProjection {
            value,
            normalized_arguments: normalized,
            omitted_execution: omitted,
        })
    }
    pub fn encode_generation_response(
        self,
        response: &DecodedResponse,
        source: &DecodedRequest,
        target: &ReplayTarget,
    ) -> Result<Value, CodecError> {
        response.metadata.validate()?;
        if response.metadata.model != target.model()
            || response.metadata.created.is_some()
            || response.metadata.context != Default::default()
        {
            return Err(unsupported());
        }
        let joined = ClientManaged::new(source.semantic.settings().clone())?
            .select_request(&source.semantic)?
            .select_response(&response.semantic)?
            .build(&[])?;
        response
            .fidelity
            .require_reasoning_replay(&joined, &target.origin())?;
        let actual = response.semantic.message_envelopes();
        if actual.len() != 1
            || actual[0].role() != MessageEnvelopeRole::Assistant
            || actual[0].members()
                != response
                    .semantic
                    .items()
                    .iter()
                    .map(|(id, _)| *id)
                    .collect::<Vec<_>>()
        {
            return Err(unsupported());
        }
        let content = response
            .semantic
            .items()
            .iter()
            .map(|(id, item)| {
                item_blocks(
                    *id,
                    item,
                    &joined,
                    &response.fidelity,
                    target,
                    &mut vec![],
                    &mut vec![],
                )
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        let stop_reason = match (response.semantic.outcome(), response.semantic.progress()) {
            (Outcome::Completed, InteractionProgress::TurnFinished) => super::StopReason::EndTurn,
            (Outcome::Completed, InteractionProgress::AwaitingToolResults) => {
                super::StopReason::ToolUse
            }
            (Outcome::Incomplete, InteractionProgress::Unreported)
                if response.semantic.details().incomplete
                    == Some(IncompleteReason::MaxOutputTokens) =>
            {
                super::StopReason::MaxTokens
            }
            _ => return Err(unsupported()),
        };
        let usage = response.semantic.usage().ok_or_else(unsupported)?;
        let mut selected = Usage::operation(0, 0, 0);
        selected.input_tokens = usage.input_tokens;
        selected.output_tokens = usage.output_tokens;
        selected.total_tokens = None;
        selected.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
        selected.total_relation = TotalTokenRelation::Unreported;
        selected.cached_input_tokens = usage.cached_input_tokens;
        selected.input_cache_write_tokens = usage.input_cache_write_tokens;
        if selected != usage || response.semantic.usage_reports().len() != 1 {
            return Err(unsupported());
        }
        self.encode_response(&super::Message {
            id: response.metadata.id.clone(),
            model: target.model().into(),
            content,
            stop_reason: Presence::Value(stop_reason),
            stop_sequence: Presence::Null,
            usage: super::Usage {
                input_tokens: usage.input_tokens.map_or(Presence::Absent, Presence::Value),
                output_tokens: usage.output_tokens.ok_or_else(unsupported)?,
                cache_creation_input_tokens: usage
                    .input_cache_write_tokens
                    .map_or(Presence::Absent, Presence::Value),
                cache_read_input_tokens: usage
                    .cached_input_tokens
                    .map_or(Presence::Absent, Presence::Value),
            },
        })
    }
}
