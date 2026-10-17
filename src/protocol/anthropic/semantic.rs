//! Static native request/response mapping over the shared IR.
use super::{
    Block, MessageContent, Profile, Role, Selection, TextContent as WireText,
    semantic_intake::{Intake, bind, text},
    semantic_request::{RequestRule, item_blocks, project, unsupported},
};
use crate::{
    protocol::{
        CodecError, DecodedRequest, DecodedResponse, fidelity::FidelityRecords,
        native::ReplayTarget,
    },
    semantic::{task::generation::*, value::Presence},
};
use serde_json::Value;
/// Body-free receipts identify actual selected normalization/loss, not permission.
pub struct NativeProjection {
    pub value: Value,
    pub normalized_arguments: Vec<ItemId>,
    pub omitted_execution: Vec<ItemId>,
    pub rules: Vec<RequestRule>,
    pub control_losses: Vec<crate::lowering::generation::RequestControlLoss>,
}
pub(super) fn display(value: &Presence<super::Display>) -> Presence<ReasoningDisplay> {
    match value {
        Presence::Absent => Presence::Absent,
        Presence::Null => Presence::Null,
        Presence::Value(super::Display::Summarized) => Presence::Value(ReasoningDisplay::Summary),
        Presence::Value(super::Display::Omitted) => Presence::Value(ReasoningDisplay::Omitted),
    }
}
impl std::fmt::Debug for NativeProjection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeProjection")
            .field("body", &"[redacted]")
            .field("normalized_arguments", &self.normalized_arguments)
            .field("omitted_execution", &self.omitted_execution)
            .field("rules", &self.rules)
            .field("control_losses", &self.control_losses)
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
        if !wire.service_policy.is_absent() {
            return Err(CodecError::Unsupported(
                "execution policy requires request envelope".into(),
            ));
        }
        if wire.model != target.model() {
            return Err(CodecError::Invalid("native model binding"));
        }
        if wire.stream == Presence::Value(true) {
            return Err(CodecError::Unsupported("native streaming mapping".into()));
        }
        let mut intake = Intake::new(scope);
        let mut settings = GenerationSettings {
            automatic_prefix_cache: wire.cache_control.value().map(|c| c.ttl()),
            ..Default::default()
        };
        settings.controls.max_output_tokens = Some(wire.max_tokens);
        settings.controls.stop_sequences = wire
            .stop_sequences
            .iter()
            .map(|s| text(s))
            .collect::<Result<_, _>>()?;
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
                            .map(|(i, s)| {
                                Ok(Part {
                                    id: PartId::scoped(scope, i as u64 + 1),
                                    content: text(&s.content)?,
                                    cache: s.cache,
                                })
                            })
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
                            cache: t.cache_control.value().copied(),
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
            settings.reasoning = ReasoningRequest::present(None);
            match thinking {
                super::Thinking::Disabled => {
                    settings.reasoning.activation = Presence::Value(ReasoningActivation::Disabled);
                }
                super::Thinking::Adaptive { display: view } => {
                    settings.reasoning.activation = Presence::Value(ReasoningActivation::Enabled);
                    settings.reasoning.mode = Presence::Value(ReasoningMode::Adaptive);
                    settings.reasoning.display = display(view);
                }
            }
        }
        if let Presence::Value(config) = &wire.output_config {
            output_settings(config, &mut settings);
        }
        for message in &wire.messages {
            match (&message.content, message.role) {
                (MessageContent::Text(s), super::Role::System) => {
                    intake.instruction(&[super::CacheMarked {
                        content: s.clone(),
                        cache: None,
                    }])?
                }
                (MessageContent::Blocks(blocks), super::Role::System) => intake.instruction(
                    &blocks
                        .iter()
                        .map(|block| match &block.content {
                            Block::Text(s) => Ok(super::CacheMarked {
                                content: s.clone(),
                                cache: block.cache,
                            }),
                            _ => Err(CodecError::Invalid("Anthropic block role")),
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                )?,
                (MessageContent::Text(s), role) => {
                    intake.message(role, &[Block::Text(s.clone())])?
                }
                (MessageContent::Blocks(blocks), role) => intake.request_message(role, blocks)?,
            }
        }
        let semantic = GenerationRequest::from_resources(intake.items, settings, intake.resources)?
            .with_message_envelopes(intake.groups)?;
        let mut fidelity = FidelityRecords::default();
        bind(&mut fidelity, semantic.items(), target)?;
        for part in intake.strict_images {
            fidelity.record_strict_image_size(part)?;
        }
        Ok(DecodedRequest { semantic, fidelity })
    }
    /// Decode complete native output using the selected Provider/model label.
    pub fn decode_generation_response(
        self,
        bytes: &[u8],
        scope: LocalScope,
        target: &ReplayTarget,
    ) -> Result<DecodedResponse, CodecError> {
        let wire = self.decode_response(bytes)?;
        self.decode_generation_message(wire, scope, target)
    }
    pub(crate) fn decode_generation_message(
        self,
        wire: super::Message,
        scope: LocalScope,
        target: &ReplayTarget,
    ) -> Result<DecodedResponse, CodecError> {
        wire.validate(false)?;
        if wire.model != target.model() {
            return Err(CodecError::Invalid("native model binding"));
        }
        let mut intake = Intake::new(scope);
        intake.message(Role::Assistant, &wire.content)?;
        let (outcome, progress, details) = outcome(&wire)?;
        let mut reports = UsageReports::default();
        reports.replace(super::accounting::report(&wire.usage, UsageStage::Final))?;
        let semantic = GenerationResponse::new(intake.items, outcome)?
            .with_message_envelopes(intake.groups)?
            .with_progress(progress)?
            .with_details(details)?
            .with_usage_reports(reports)?;
        let mut fidelity = FidelityRecords::default();
        bind(&mut fidelity, semantic.items(), target)?;
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
        self.encode_request_envelope(request, fidelity, target, &Presence::Absent)
    }
    pub(super) fn encode_request_envelope(
        self,
        request: &GenerationRequest,
        fidelity: &FidelityRecords,
        target: &ReplayTarget,
        policy: &Presence<crate::semantic::context::ServicePolicy>,
    ) -> Result<NativeProjection, CodecError> {
        let (request, fidelity, transferred) = crate::protocol::native::native_reasoning_history(
            request,
            fidelity,
            target,
            &[
                ReplayFormat::AnthropicMessagesThinking,
                ReplayFormat::AnthropicMessagesRedactedThinking,
            ],
        )?;
        let mut normalized = vec![];
        let mut omitted = vec![];
        let mut rules = std::collections::BTreeSet::new();
        if transferred {
            rules.insert(RequestRule::TransferVisibleHistory);
        }
        let mut wire = project(
            &request,
            &fidelity,
            target,
            &mut normalized,
            &mut omitted,
            &mut rules,
        )?;
        wire.service_policy = policy.clone();
        let value = self.encode_request(&wire)?;
        Ok(NativeProjection {
            value,
            normalized_arguments: normalized,
            omitted_execution: omitted,
            rules: rules.into_iter().collect(),
            control_losses: vec![],
        })
    }
    pub fn encode_generation_response(
        self,
        response: &DecodedResponse,
        source: &DecodedRequest,
        target: &ReplayTarget,
    ) -> Result<Value, CodecError> {
        response.metadata.validate()?;
        let report = response
            .semantic
            .usage_reports()
            .operation()
            .ok_or_else(unsupported)?;
        if response.semantic.usage_reports().reports().len() != 1
            || report.stage != UsageStage::Final
        {
            return Err(unsupported());
        }
        let mut context = response.metadata.context.clone();
        let service_class = std::mem::take(&mut context.service_class);
        if response.metadata.model != target.model()
            || response.metadata.created.is_some()
            || context != Default::default()
        {
            return Err(unsupported());
        }
        let joined = ClientManaged::new(source.semantic.settings().clone())?
            .select_request(&source.semantic)?
            .select_response(&response.semantic)?
            .build()?;
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
                    &mut Default::default(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        let (stop_reason, stop_sequence) = super::client::terminal(
            response.semantic.outcome(),
            response.semantic.progress(),
            response
                .semantic
                .items()
                .iter()
                .any(|(_, item)| item.is_call()),
            response.semantic.details(),
        )?;
        let stop_reason = super::StopReason::read(&Value::String(stop_reason.into()))?;
        let stop_sequence = stop_sequence
            .map(|value| Presence::Value(value.to_owned()))
            .unwrap_or(Presence::Null);
        self.encode_response(&super::Message {
            id: response.metadata.required_id()?.into(),
            model: target.model().into(),
            content,
            stop_reason: Presence::Value(stop_reason),
            stop_sequence,
            usage: super::accounting::wire(report, service_class)?,
        })
    }
}
/// Effort and format of a Messages `output_config`; a null member is unset.
pub(super) fn output_settings(config: &super::OutputConfig, settings: &mut GenerationSettings) {
    if let Presence::Value(effort) = config.effort {
        settings.effort = Presence::Value(EffortControl {
            scope: EffortScope::Response,
            level: match effort {
                super::Effort::Low => EffortLevel::Low,
                super::Effort::Medium => EffortLevel::Medium,
                super::Effort::High => EffortLevel::High,
                super::Effort::XHigh => EffortLevel::XHigh,
                super::Effort::Max => EffortLevel::Max,
            },
        });
    }
    if let Presence::Value(schema) = &config.format {
        // Messages output always conforms to the schema, as OpenAI strict output does.
        settings.text.presence = true;
        settings.text.format = Presence::Value(OutputConstraint::JsonSchema {
            name: None,
            description: None,
            schema: schema.clone(),
            strict: Some(true),
        });
    }
}
pub(super) fn outcome(
    message: &super::Message,
) -> Result<(Outcome, InteractionProgress, TerminalDetails), CodecError> {
    let stop = *message
        .stop_reason
        .value()
        .ok_or(CodecError::Invalid("native stop"))?;
    let (outcome, progress, mut details) = match stop {
        super::StopReason::EndTurn | super::StopReason::StopSequence => (
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
                completion: None,
            },
        ),
        super::StopReason::ModelContextWindowExceeded => (
            Outcome::Incomplete,
            InteractionProgress::Unreported,
            TerminalDetails {
                incomplete: Some(IncompleteReason::ContextWindowExceeded),
                error: None,
                completion: None,
            },
        ),
        super::StopReason::Refusal => (
            Outcome::Incomplete,
            InteractionProgress::Unreported,
            TerminalDetails {
                incomplete: Some(IncompleteReason::ContentFilter),
                error: None,
                completion: None,
            },
        ),
    };
    details.completion = match stop {
        super::StopReason::EndTurn => Some(CompletionCause::Natural),
        super::StopReason::StopSequence => Some(CompletionCause::StopSequence(text(
            message
                .stop_sequence
                .value()
                .ok_or(CodecError::Invalid("native stop sequence"))?,
        )?)),
        _ => None,
    };
    Ok((outcome, progress, details))
}
