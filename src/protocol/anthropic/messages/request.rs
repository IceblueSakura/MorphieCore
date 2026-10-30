//! Native Messages request mapping: wire → IR intake and IR → wire projection.
use super::{
    Block, InputMessage, MessageContent, Profile, Role, Selection, TextContent as WireText,
    items::{Intake, bind, item_blocks, text},
    settings::{display, effort_level, output_settings, replay_mismatch},
};
use crate::{
    protocol::{
        CodecError, DecodedRequest, failure::CarrierReason, fidelity::FidelityRecords,
        native::ReplayTarget,
    },
    semantic::{task::generation::*, value::Presence},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

impl Profile {
    pub fn encode_generation_request(
        self,
        request: &GenerationRequest,
        fidelity: &FidelityRecords,
        target: &ReplayTarget,
    ) -> Result<crate::protocol::conversion::generation::Conversion<Value>, CodecError> {
        self.encode_request_envelope(
            request,
            fidelity,
            target,
            &Presence::Absent,
            &Default::default(),
        )
    }
    pub(super) fn encode_request_envelope(
        self,
        request: &GenerationRequest,
        fidelity: &FidelityRecords,
        target: &ReplayTarget,
        policy: &Presence<crate::semantic::context::ServicePolicy>,
        limits: &crate::protocol::model_limits::ModelLimits,
    ) -> Result<crate::protocol::conversion::generation::Conversion<Value>, CodecError> {
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
        let phase = crate::protocol::phase::omit_request(&request)?;
        if phase.is_some() {
            rules.insert(RequestRule::OmitMessagePhase);
        }
        let conditional = crate::protocol::openai::lowering::strictness::project(
            phase.as_ref().unwrap_or(&request),
            |schema| super::strict_schema::admits(schema.root()),
        )
        .map_err(|cause| CodecError::Representation {
            boundary: crate::protocol::conversion::generation::ConversionBoundary::RequestProject,
            cause,
        })?;
        let mut wire = project(
            conditional
                .as_ref()
                .map(|(semantic, _)| semantic)
                .or(phase.as_ref())
                .unwrap_or(&request),
            &fidelity,
            target,
            &mut normalized,
            &mut omitted,
            &mut rules,
        )?;
        wire.service_policy = policy.clone();
        limits.check_values(
            Some(wire.max_tokens),
            wire.output_config
                .value()
                .and_then(|config| config.effort.value())
                .copied()
                .into_iter()
                .chain(wire.messages.iter().filter_map(|message| message.effort))
                .map(effort_level),
        )?;
        let value = self.encode_request(&wire)?;
        let mut evidence: crate::protocol::conversion::generation::EvidenceDelta =
            rules.into_iter().map(|r| r.effect(&request)).collect();
        if let Some((_, effects)) = conditional {
            for effect in effects {
                evidence.push(effect);
            }
        }
        for owner in normalized {
            evidence.push(
                crate::protocol::conversion::generation::Effect::new(
                    "messages.normalize-tool-arguments.v1",
                    crate::protocol::conversion::generation::Field::ToolArguments,
                    crate::protocol::conversion::generation::ConversionBoundary::RequestEncode,
                    crate::protocol::conversion::generation::Change::Normalize,
                )
                .at(crate::protocol::conversion::generation::Owner::item(owner)),
            );
        }
        for owner in omitted {
            evidence.push(
                crate::protocol::conversion::generation::Effect::new(
                    "messages.omit-tool-execution.v1",
                    crate::protocol::conversion::generation::Field::ToolExecution,
                    crate::protocol::conversion::generation::ConversionBoundary::RequestProject,
                    crate::protocol::conversion::generation::Change::Omit,
                )
                .at(crate::protocol::conversion::generation::Owner::item(owner)),
            );
        }
        Ok(crate::protocol::conversion::generation::Conversion { value, evidence })
    }
    pub fn decode_generation_request(
        self,
        bytes: &[u8],
        scope: LocalScope,
        target: &ReplayTarget,
    ) -> Result<DecodedRequest, CodecError> {
        let wire = self.decode_request(bytes)?;
        if !wire.service_policy.is_absent() || !wire.user_id.is_absent() {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        if wire.model != target.model() {
            return Err(CodecError::invalid());
        }
        if wire.stream == Presence::Value(true) {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
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
        settings.instructions = match wire.system {
            Presence::Value(system) => Presence::Value(super::items::root_instructions(system)?),
            Presence::Absent => Presence::Absent,
            Presence::Null => return Err(unsupported()),
        };
        if let Presence::Value(tools) = &wire.tools {
            settings.tools = Some(
                tools
                    .iter()
                    .map(|t| {
                        crate::protocol::anthropic::messages::items::function_tool(t)
                            .map(ToolDefinition::Function)
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
                super::Thinking::Enabled {
                    budget_tokens,
                    display: view,
                } => {
                    settings.reasoning.activation = Presence::Value(ReasoningActivation::Enabled);
                    settings.reasoning.mode = Presence::Value(ReasoningMode::Budgeted);
                    settings.reasoning.budget_tokens = Some(*budget_tokens);
                    settings.reasoning.presentation.visibility = display(view);
                }
                super::Thinking::Adaptive {
                    display: view,
                    block_binding,
                } => {
                    settings.reasoning.activation = Presence::Value(ReasoningActivation::Enabled);
                    settings.reasoning.mode = Presence::Value(ReasoningMode::Adaptive);
                    settings.reasoning.presentation.visibility = display(view);
                    settings.reasoning.replay_mismatch = block_binding.map(replay_mismatch);
                }
            }
        }
        if let Presence::Value(config) = &wire.output_config {
            output_settings(config, &mut settings);
        }
        for message in &wire.messages {
            match (&message.content, message.role) {
                (MessageContent::Text(s), super::Role::System) => intake.system_message(
                    &[super::CacheMarked {
                        content: Block::Text(s.clone()),
                        cache: None,
                    }],
                    message.effort,
                )?,
                (MessageContent::Blocks(blocks), super::Role::System) => {
                    intake.system_message(blocks, message.effort)?
                }
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
        Ok(DecodedRequest {
            evidence: Default::default(),
            semantic,
            fidelity,
        })
    }
}

use super::items::{cache_ttl, unsupported};
/// Named degradable rules applied to one candidate's request copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RequestRule {
    OmitPrediction,
    ReasoningPolicy(&'static str),
    Hint(crate::protocol::cache::HintRule),
    OmitVerbosity,
    OmitMessagePhase,
    /// Messages has one summarized display, without concise or detailed variants.
    SummarizeReasoning,
    /// Claude 5.x models reject non-default `temperature` and `top_p`.
    OmitSampling,
    ProjectCallIds,
    /// Incompatible complete reasoning is omitted, retaining visible history.
    TransferVisibleHistory,
    /// Explicit opt-in for unattributed readable reasoning, not foreign-state transfer.
    ReasoningAsText,
    /// A Messages output format has no name; OpenAI requires one as a label.
    OmitSchemaName,
    /// A trusted target inserts its configured ceiling on the candidate copy.
    DefaultMaxTokens,
    /// A trusted target explicitly selects automatic caching for marker-free input.
    DefaultAutomaticCache,
    /// Tolerant `drop_block` replay handling without a target carrier.
    OmitBlockBinding,
    /// Per-message effort becomes the level in effect for the requested reply.
    FlattenMessageEffort,
    /// Ordered tool changes become the final available tool set.
    CollapseToolChanges,
}
impl RequestRule {
    pub(crate) fn effect(
        self,
        request: &GenerationRequest,
    ) -> crate::protocol::conversion::generation::Effect {
        use crate::protocol::conversion::generation::{
            Change, ConversionBoundary as B, Effect, Field, Owner,
        };
        let (field, change) = match self {
            Self::CollapseToolChanges => (Field::Tools, Change::Omit),
            Self::DefaultAutomaticCache => (
                Field::Cache,
                Change::Default(
                    crate::protocol::conversion::generation::DefaultBasis::SelectedPolicy,
                ),
            ),
            Self::DefaultMaxTokens => (
                Field::MaximumOutput,
                Change::Default(
                    crate::protocol::conversion::generation::DefaultBasis::SelectedPolicy,
                ),
            ),
            Self::FlattenMessageEffort => (Field::Effort, Change::Omit),
            Self::Hint(hint) => return hint.effect(),
            Self::OmitBlockBinding => (Field::Replay, Change::Omit),
            Self::OmitMessagePhase => (Field::MessagePhase, Change::Omit),
            Self::OmitPrediction => (Field::Prediction, Change::Omit),
            Self::OmitSampling => (Field::Sampling, Change::Omit),
            Self::OmitSchemaName => (Field::SchemaName, Change::Omit),
            Self::OmitVerbosity => (Field::Verbosity, Change::Omit),
            Self::ProjectCallIds => (Field::ToolCallIdentity, Change::Normalize),
            Self::ReasoningAsText => (Field::ReportedReasoning, Change::Approximate),
            Self::ReasoningPolicy(rule) => {
                return Effect::new(
                    rule,
                    Field::ReasoningAllocation,
                    B::RequestProject,
                    Change::Approximate,
                );
            }
            Self::SummarizeReasoning => (Field::ReasoningSummary, Change::Approximate),
            Self::TransferVisibleHistory => (Field::Replay, Change::Omit),
        };
        match self {
            Self::SummarizeReasoning => Effect::Summary {
                rule: self.name().into(),
                owner: Owner::Root,
                input: request.reasoning().presentation.summary.clone(),
                output: ReasoningDisplay::Summary,
            },
            _ => Effect::new(self.name(), field, B::RequestProject, change),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::OmitPrediction => "generation.omit-prediction.v1",
            Self::ReasoningPolicy(rule) => rule,
            Self::Hint(rule) => rule.name(),
            Self::OmitVerbosity => "messages.omit-verbosity.v1",
            Self::OmitMessagePhase => "messages.omit-message-phase.v1",
            Self::SummarizeReasoning => "messages.summarize-reasoning.v1",
            Self::OmitSampling => "messages.omit-sampling.v1",
            Self::ProjectCallIds => "messages.project-call-ids.v1",
            Self::TransferVisibleHistory => "messages.transfer-visible-history.v1",
            Self::ReasoningAsText => "messages.reasoning-as-text.v1",
            Self::OmitSchemaName => "messages.omit-schema-name.v1",
            Self::DefaultMaxTokens => "messages.default-max-tokens.v1",
            Self::DefaultAutomaticCache => "messages.default-automatic-cache.v1",
            Self::OmitBlockBinding => "messages.omit-block-binding.v1",
            Self::FlattenMessageEffort => "messages.flatten-message-effort.v1",
            Self::CollapseToolChanges => "messages.collapse-tool-changes.v1",
        }
    }
}
/// Messages call IDs must match `^[a-zA-Z0-9_-]{1,64}$`. Other IDs map to a stable digest,
/// so earlier turns project identically; a digest equal to a client ID fails validation as
/// a duplicate call instead of merging links.
fn call_id(id: &str, rules: &mut BTreeSet<RequestRule>) -> String {
    if super::shape::name(id, 64).is_ok() {
        return id.into();
    }
    rules.insert(RequestRule::ProjectCallIds);
    let digest = Sha256::digest(id.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("mc_{}", &hex[..40])
}
fn native_tool(t: &FunctionTool) -> Result<super::Tool, CodecError> {
    let ToolDispatch {
        async_call: false,
        defer_loading,
        allowed_callers: None,
    } = t.dispatch
    else {
        return Err(unsupported());
    };
    if t.output_schema.is_some() {
        return Err(unsupported());
    }
    Ok(super::Tool {
        name: t.name.as_str().into(),
        description: t
            .description
            .clone()
            .map_or(Presence::Absent, Presence::Value),
        input_schema: t.parameters.clone().ok_or_else(unsupported)?,
        input_examples: t
            .input_examples
            .clone()
            .map_or(Presence::Absent, Presence::Value),
        strict: match t.strict {
            FunctionStrictness::Omitted(StrictDefault::NonStrict) => Presence::Absent,
            FunctionStrictness::Explicit(false) => Presence::Value(false),
            FunctionStrictness::Explicit(true) => Presence::Value(true),
            FunctionStrictness::Omitted(StrictDefault::Conditional) => {
                return Err(unsupported());
            }
        },
        cache_control: t.cache.map_or(Presence::Absent, Presence::Value),
        defer_loading,
        eager_input_streaming: t.eager_input.map_or(Presence::Absent, Presence::Value),
    })
}
pub(super) fn native_effort(level: EffortLevel) -> Result<super::Effort, CodecError> {
    Ok(match level {
        EffortLevel::Low => super::Effort::Low,
        EffortLevel::Medium => super::Effort::Medium,
        EffortLevel::High => super::Effort::High,
        EffortLevel::XHigh => super::Effort::XHigh,
        EffortLevel::Max => super::Effort::Max,
        EffortLevel::Minimal => return Err(unsupported()),
    })
}
/// Items that render as mid-conversation system content, outside message envelopes.
fn system_side(item: &Item) -> bool {
    matches!(
        item,
        Item::Instruction(_) | Item::ToolChange(_) | Item::ConfigurationUpdate(_)
    )
}
/// One run of system-side items. Consecutive system messages form one section, so the
/// run shares a message; an effort closes it, keeping each message to one level.
fn system_messages(run: &[&Item]) -> Result<Vec<InputMessage>, CodecError> {
    let mut messages = vec![];
    let mut blocks = vec![];
    for item in run {
        match item {
            Item::Instruction(i) => {
                if i.status.is_some() {
                    return Err(unsupported());
                }
                blocks.extend(
                    i.parts
                        .iter()
                        .map(|p| {
                            Ok(super::CacheMarked {
                                content: Block::Text(p.content.as_str().into()),
                                cache: cache_ttl(p.cache)?,
                            })
                        })
                        .collect::<Result<Vec<_>, CodecError>>()?,
                );
            }
            Item::ToolChange(change) => blocks.push(super::CacheMarked {
                content: match &change.action {
                    ToolChangeAction::AddReference(name) => {
                        Block::ToolAddition(super::ToolTarget::Reference(name.as_str().into()))
                    }
                    ToolChangeAction::AddDefinition(tool) => {
                        Block::ToolAddition(super::ToolTarget::Definition(native_tool(tool)?))
                    }
                    ToolChangeAction::Remove(name) => Block::ToolRemoval(name.as_str().into()),
                },
                cache: change.cache,
            }),
            Item::ConfigurationUpdate(ConfigurationUpdate {
                activation: None,
                effort: Some(effort),
            }) if effort.scope == EffortScope::Response => messages.push(InputMessage {
                role: Role::System,
                content: MessageContent::Blocks(std::mem::take(&mut blocks)),
                effort: Some(native_effort(effort.level)?),
            }),
            _ => return Err(unsupported()),
        }
    }
    if !blocks.is_empty() {
        messages.push(InputMessage {
            role: Role::System,
            content: MessageContent::Blocks(blocks),
            effort: None,
        });
    }
    Ok(messages)
}
pub(in crate::protocol::anthropic::messages) fn project(
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
    normalized: &mut Vec<ItemId>,
    omitted: &mut Vec<ItemId>,
    rules: &mut BTreeSet<RequestRule>,
) -> Result<super::Request, CodecError> {
    if request
        .items()
        .iter()
        .any(|(_, i)| i.participant().is_some())
    {
        return Err(unsupported());
    }
    let s = request.settings();
    if s.prediction.is_some() {
        rules.insert(RequestRule::OmitPrediction);
    }
    if request.cached_context().is_some() || !s.safety.is_empty() {
        return Err(unsupported());
    }
    if s.controls.temperature().is_some()
        || s.controls.top_p().is_some()
        || s.controls.top_k.is_some()
        || s.controls.presence_penalty().is_some()
        || s.controls.frequency_penalty().is_some()
        || s.controls.seed.is_some()
    {
        return Err(unsupported());
    }
    if s.controls.top_logprobs.is_some()
        || !matches!(
            s.controls.logprobs,
            Presence::Absent | Presence::Value(false)
        )
        || s.controls.truncation.is_some()
        || !s.text.verbosity.is_absent()
        || !s.output_modalities.is_absent()
        || !s.audio.is_absent()
    {
        return Err(unsupported());
    }
    let mut system = match &s.instructions {
        Presence::Absent => Presence::Absent,
        Presence::Value(root) if root.plain_text().is_some() => Presence::Value(WireText::Text(
            root.plain_text()
                .expect("checked plain root")
                .as_str()
                .into(),
        )),
        Presence::Value(root) => Presence::Value(WireText::Blocks(
            root.parts
                .iter()
                .map(|part| {
                    Ok(super::CacheMarked {
                        content: part.text.as_str().into(),
                        cache: cache_ttl(part.cache)?,
                    })
                })
                .collect::<Result<_, CodecError>>()?,
        )),
        Presence::Null => return Err(unsupported()),
    };
    let leading_instructions = request
        .items()
        .iter()
        .take_while(|(_, item)| matches!(item, Item::Instruction(_)))
        .count();
    if leading_instructions > 0 {
        let mut parts = match system {
            Presence::Value(WireText::Text(content)) => vec![super::CacheMarked {
                content,
                cache: None,
            }],
            Presence::Value(WireText::Blocks(parts)) => parts,
            Presence::Absent => vec![],
            Presence::Null => return Err(unsupported()),
        };
        for (_, item) in &request.items()[..leading_instructions] {
            let Item::Instruction(i) = item else {
                unreachable!()
            };
            if i.status.is_some() {
                return Err(unsupported());
            }
            for p in &i.parts {
                parts.push(super::CacheMarked {
                    content: p.content.as_str().into(),
                    cache: cache_ttl(p.cache)?,
                });
            }
        }
        system = Presence::Value(WireText::Blocks(parts));
    }
    let tools = if s.tools.is_some() {
        Presence::Value(
            request
                .tools()
                .iter()
                .map(|tool| match tool {
                    ToolDefinition::Function(t) => native_tool(t),
                    _ => Err(unsupported()),
                })
                .collect::<Result<_, _>>()?,
        )
    } else {
        Presence::Absent
    };
    let tool_choice = if let Some(choice) = &s.tool_choice {
        Presence::Value(super::ToolChoice {
            selection: match choice {
                ToolChoice::Auto => Selection::Auto,
                ToolChoice::None => Selection::None,
                ToolChoice::Required => Selection::Any,
                ToolChoice::Specific(t) => Selection::Tool(t.as_str().into()),
                _ => return Err(unsupported()),
            },
            disable_parallel_tool_use: s
                .parallel_tool_calls
                .map_or(Presence::Absent, |b| Presence::Value(!b)),
        })
    } else {
        // Messages carries the switch only inside `tool_choice`, whose default is `auto`.
        match s.parallel_tool_calls {
            Some(false) if tools.value().is_some() => Presence::Value(super::ToolChoice {
                selection: Selection::Auto,
                disable_parallel_tool_use: Presence::Value(true),
            }),
            _ => Presence::Absent,
        }
    };
    let reasoning = &s.reasoning;
    if reasoning.encrypted_output() || reasoning.presence == ReasoningPresence::Null {
        return Err(unsupported());
    }
    if !reasoning.context.is_absent() || !reasoning.read_history.is_absent() {
        return Err(unsupported());
    }
    // Visibility is an explicit guarantee, not an inference from nullable summary
    // preference. Absent display preserves the target's model-dependent default.
    let display = match (
        &reasoning.presentation.visibility,
        &reasoning.presentation.summary,
    ) {
        (Presence::Value(ReasoningDisplay::Summary), granularity) => {
            if matches!(
                granularity,
                Presence::Value(ReasoningSummary::Concise | ReasoningSummary::Detailed)
            ) {
                rules.insert(RequestRule::SummarizeReasoning);
            }
            Presence::Value(super::Display::Summarized)
        }
        (Presence::Value(ReasoningDisplay::Omitted), _) => Presence::Value(super::Display::Omitted),
        (Presence::Null, _) => Presence::Null,
        (Presence::Absent, Presence::Absent) => Presence::Absent,
        _ => return Err(unsupported()),
    };
    let thinking = if let Some(budget_tokens) = reasoning.budget_tokens {
        if reasoning.activation == Presence::Value(ReasoningActivation::Disabled)
            || !matches!(
                reasoning.mode,
                Presence::Absent | Presence::Value(ReasoningMode::Budgeted)
            )
        {
            return Err(unsupported());
        }
        Presence::Value(super::Thinking::Enabled {
            budget_tokens,
            display,
        })
    } else {
        match (&reasoning.activation, &reasoning.mode) {
            (Presence::Value(ReasoningActivation::Disabled), Presence::Absent)
                if matches!(
                    display,
                    Presence::Absent | Presence::Value(super::Display::Omitted)
                ) =>
            {
                Presence::Value(super::Thinking::Disabled)
            }
            (
                Presence::Absent | Presence::Value(ReasoningActivation::Enabled),
                Presence::Value(ReasoningMode::Adaptive),
            )
            | (Presence::Value(ReasoningActivation::Enabled), Presence::Absent) => {
                Presence::Value(super::Thinking::Adaptive {
                    display,
                    block_binding: reasoning.replay_mismatch.map(|m| match m {
                        ReplayMismatch::Reject => super::PrefixMismatch::Error,
                        ReplayMismatch::Drop => super::PrefixMismatch::DropBlock,
                    }),
                })
            }
            (Presence::Absent, Presence::Absent) if display.is_absent() => Presence::Absent,
            _ => return Err(unsupported()),
        }
    };
    // The binding check exists only for adaptive thinking.
    if reasoning.replay_mismatch.is_some()
        && !matches!(thinking, Presence::Value(super::Thinking::Adaptive { .. }))
    {
        return Err(unsupported());
    }
    let effort = match &s.effort {
        Presence::Absent => Presence::Absent,
        Presence::Value(control) if control.scope == EffortScope::Response => {
            Presence::Value(native_effort(control.level)?)
        }
        Presence::Value(_) => return Err(unsupported()),
        Presence::Null => return Err(unsupported()),
    };
    if reasoning.presence == ReasoningPresence::Present
        && thinking.is_absent()
        && effort.is_absent()
    {
        return Err(unsupported());
    }
    let format = match &s.text.format {
        Presence::Absent | Presence::Null | Presence::Value(OutputConstraint::Text) => {
            Presence::Absent
        }
        // Messages has no JSON-object mode, and a schema description is model-visible.
        Presence::Value(OutputConstraint::JsonObject)
        | Presence::Value(OutputConstraint::JsonSchema {
            description: Some(_),
            ..
        }) => return Err(unsupported()),
        // Messages output is always constrained, which satisfies any OpenAI `strict`.
        Presence::Value(OutputConstraint::JsonSchema { name, schema, .. }) => {
            if name.is_some() {
                rules.insert(RequestRule::OmitSchemaName);
            }
            Presence::Value(schema.clone())
        }
    };
    let output_config = if effort.is_absent() && format.is_absent() {
        Presence::Absent
    } else {
        Presence::Value(super::OutputConfig { effort, format })
    };
    let mut included = BTreeSet::new();
    let mut messages = vec![];
    // Later system-side items become mid-conversation system messages at their
    // envelope boundary; placement is validated with the complete wire request.
    let position = |owner: &ItemId| request.items().iter().position(|(id, _)| id == owner);
    let mut system_items = request
        .items()
        .iter()
        .enumerate()
        .skip(leading_instructions)
        .filter(|(_, (_, item))| system_side(item))
        .map(|(index, (_, item))| (index, item))
        .peekable();
    let mut previous_end = None;
    for group in request.message_envelopes() {
        if group.members().is_empty() {
            return Err(unsupported());
        }
        let first = group
            .members()
            .first()
            .and_then(position)
            .ok_or_else(unsupported)?;
        let last = group
            .members()
            .last()
            .and_then(position)
            .ok_or_else(unsupported)?;
        let mut run = vec![];
        while let Some((index, item)) = system_items.next_if(|(index, _)| *index < first) {
            // A system item inside another envelope would split a native message.
            if previous_end.is_some_and(|end| index < end) {
                return Err(unsupported());
            }
            run.push(item);
        }
        messages.extend(system_messages(&run)?);
        previous_end = Some(last);
        let role = match group.role() {
            MessageEnvelopeRole::User => Role::User,
            MessageEnvelopeRole::Assistant => Role::Assistant,
            _ => return Err(unsupported()),
        };
        let mut blocks = vec![];
        for owner in group.members() {
            if !included.insert(*owner) {
                return Err(unsupported());
            }
            let item = &request
                .items()
                .iter()
                .find(|(id, _)| id == owner)
                .ok_or_else(unsupported)?
                .1;
            // Every turn converts alike, keeping the projection append-only.
            if let Item::Reasoning(r) = item
                && r.replay.is_none()
            {
                if r.status != ItemLifecycle::Completed {
                    return Err(unsupported());
                }
                for (_, part) in &r.parts {
                    let (ReasoningContent::Summary(t) | ReasoningContent::Text(t)) = part;
                    if !t.as_str().trim().is_empty() {
                        rules.insert(RequestRule::ReasoningAsText);
                        blocks.push(super::CacheMarked {
                            content: Block::Text(t.as_str().into()),
                            cache: None,
                        });
                    }
                }
                continue;
            }
            blocks.extend(
                item_blocks(*owner, item, request, fidelity, target, normalized, omitted)?
                    .into_iter()
                    .enumerate()
                    .map(|(index, block)| {
                        Ok(super::CacheMarked {
                            cache: match item {
                                Item::Message(m) => cache_ttl(m.parts[index].cache)?,
                                Item::ToolCall(c) => c.cache,
                                Item::ToolResult(r) => r.cache,
                                _ => None,
                            },
                            content: match block {
                                Block::ToolUse {
                                    id,
                                    name,
                                    input,
                                    caller,
                                } => Block::ToolUse {
                                    id: call_id(&id, rules),
                                    name,
                                    input,
                                    caller,
                                },
                                Block::ToolResult {
                                    tool_use_id,
                                    content,
                                    is_error,
                                } => Block::ToolResult {
                                    tool_use_id: call_id(&tool_use_id, rules),
                                    content,
                                    is_error,
                                },
                                other => other,
                            },
                        })
                    })
                    .collect::<Result<Vec<_>, CodecError>>()?,
            );
        }
        messages.push(InputMessage {
            role,
            content: MessageContent::Blocks(blocks),
            effort: None,
        });
    }
    let mut run = vec![];
    for (index, item) in system_items {
        if previous_end.is_some_and(|end| index < end) {
            return Err(unsupported());
        }
        run.push(item);
    }
    messages.extend(system_messages(&run)?);
    let expected = request
        .items()
        .iter()
        .filter(|(_, item)| !system_side(item))
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    let actual = request
        .message_envelopes()
        .iter()
        .flat_map(|g| g.members())
        .copied()
        .collect::<Vec<_>>();
    if actual != expected {
        return Err(unsupported());
    }
    Ok(super::Request {
        model: target.model().into(),
        user_id: Presence::Absent,
        service_policy: Presence::Absent,
        max_tokens: s
            .controls
            .max_output_tokens
            .ok_or(CodecError::Unsupported(CarrierReason::Unavailable))?,
        stop_sequences: s
            .controls
            .stop_sequences
            .iter()
            .map(|s| s.as_str().into())
            .collect(),
        system,
        tools,
        tool_choice,
        thinking,
        output_config,
        // Absence is preserved by the codec; defaults belong to trusted targets.
        cache_control: match s.automatic_prefix_cache {
            Some(ttl) => Presence::Value(super::CacheControl::from_ttl(ttl)),
            None => Presence::Absent,
        },
        messages,
        stream: Presence::Absent,
    })
}
