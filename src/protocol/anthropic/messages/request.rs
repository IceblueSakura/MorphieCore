//! Native Messages request mapping: wire → IR intake and IR → wire projection.
use super::{
    Block, InputMessage, MessageContent, Profile, Role, Selection, TextContent as WireText,
    items::{Intake, bind, item_blocks, text},
    settings::{display, output_settings, replay_mismatch},
};
use crate::{
    protocol::{CodecError, DecodedRequest, fidelity::FidelityRecords, native::ReplayTarget},
    semantic::{task::generation::*, value::Presence},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

/// Body-free receipts identify actual selected normalization/loss, not permission.
pub struct NativeProjection {
    pub value: Value,
    pub normalized_arguments: Vec<ItemId>,
    pub omitted_execution: Vec<ItemId>,
    pub rules: Vec<RequestRule>,
    pub control_losses: Vec<crate::lowering::generation::RequestControlLoss>,
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
        if !wire.service_policy.is_absent() || !wire.user_id.is_absent() {
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
        if let Some(value) = wire.temperature {
            settings.controls = settings.controls.with_temperature(value)?;
        }
        if let Some(value) = wire.top_p {
            settings.controls = settings.controls.with_top_p(value)?;
        }
        settings.controls.top_k = wire.top_k;
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
                                    replay: None,
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
                    settings.reasoning.display = display(view);
                }
                super::Thinking::Adaptive {
                    display: view,
                    block_binding,
                } => {
                    settings.reasoning.activation = Presence::Value(ReasoningActivation::Enabled);
                    settings.reasoning.mode = Presence::Value(ReasoningMode::Adaptive);
                    settings.reasoning.display = display(view);
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
        for part in intake.strict_images {
            fidelity.record_strict_image_size(part)?;
        }
        Ok(DecodedRequest { semantic, fidelity })
    }
}

use super::items::unsupported;
/// Named degradable rules applied to one candidate's request copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RequestRule {
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
    /// A leading developer instruction (pi, Codex) is the system prompt.
    DeveloperAsSystem,
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
    pub const fn name(self) -> &'static str {
        match self {
            Self::ReasoningPolicy(rule) => rule,
            Self::Hint(rule) => rule.name(),
            Self::OmitVerbosity => "messages.omit-verbosity.v1",
            Self::OmitMessagePhase => "messages.omit-message-phase.v1",
            Self::SummarizeReasoning => "messages.summarize-reasoning.v1",
            Self::OmitSampling => "messages.omit-sampling.v1",
            Self::ProjectCallIds => "messages.project-call-ids.v1",
            Self::TransferVisibleHistory => "messages.transfer-visible-history.v1",
            Self::ReasoningAsText => "messages.reasoning-as-text.v1",
            Self::DeveloperAsSystem => "messages.developer-as-system.v1",
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
        strict: match t.strict {
            FunctionStrictness::Omitted(StrictDefault::NonStrict) => Presence::Absent,
            FunctionStrictness::Explicit(false) => Presence::Value(false),
            FunctionStrictness::Explicit(true) => Presence::Value(true),
            FunctionStrictness::Omitted(StrictDefault::NormalizeSchema) => {
                return Err(unsupported());
            }
        },
        cache_control: t.cache.map_or(Presence::Absent, Presence::Value),
        defer_loading,
        eager_input_streaming: t.eager_input.map_or(Presence::Absent, Presence::Value),
    })
}
fn native_effort(level: EffortLevel) -> Result<super::Effort, CodecError> {
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
fn system_messages(
    run: &[&Item],
    rules: &mut BTreeSet<RequestRule>,
) -> Result<Vec<InputMessage>, CodecError> {
    let mut messages = vec![];
    let mut blocks = vec![];
    for item in run {
        match item {
            Item::Instruction(i) => {
                if i.status.is_some() {
                    return Err(unsupported());
                }
                if i.authority == InstructionAuthority::Developer {
                    rules.insert(RequestRule::DeveloperAsSystem);
                }
                blocks.extend(i.parts.iter().map(|p| super::CacheMarked {
                    content: Block::Text(p.content.as_str().into()),
                    cache: p.cache,
                }));
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
    let s = request.settings();
    if request.cached_context().is_some() || !s.safety.is_empty() {
        return Err(unsupported());
    }
    if s.controls.presence_penalty().is_some()
        || s.controls.frequency_penalty().is_some()
        || s.controls.seed.is_some()
    {
        return Err(unsupported());
    }
    if s.controls.top_logprobs.is_some()
        || !s.controls.logprobs.is_absent()
        || s.controls.truncation.is_some()
        || !s.text.verbosity.is_absent()
        || !s.output_modalities.is_absent()
        || !s.audio.is_absent()
    {
        return Err(unsupported());
    }
    let mut system = match &s.instructions {
        Presence::Absent => Presence::Absent,
        Presence::Value(t) => Presence::Value(WireText::Text(t.as_str().into())),
        Presence::Null => return Err(unsupported()),
    };
    if let Some((_, Item::Instruction(i))) = request.items().first() {
        if i.status.is_some() || !system.is_absent() {
            return Err(unsupported());
        }
        match i.authority {
            InstructionAuthority::System => {}
            InstructionAuthority::Developer => {
                rules.insert(RequestRule::DeveloperAsSystem);
            }
        }
        system = Presence::Value(WireText::Blocks(
            i.parts
                .iter()
                .map(|p| super::CacheMarked {
                    content: p.content.as_str().into(),
                    cache: p.cache,
                })
                .collect(),
        ));
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
    if reasoning.encrypted_output()
        || reasoning.presence == ReasoningPresence::Null
        || !reasoning.display.is_absent() && !reasoning.summary.is_absent()
    {
        return Err(unsupported());
    }
    if !reasoning.context.is_absent() || !reasoning.read_history.is_absent() {
        return Err(unsupported());
    }
    // An OpenAI summary request selects the display; without one the field stays absent,
    // which equals the 5.x default (`omitted`).
    let display = match (&reasoning.display, &reasoning.summary) {
        (Presence::Value(ReasoningDisplay::Summary), _)
        | (Presence::Absent, Presence::Value(ReasoningSummary::Auto)) => {
            Presence::Value(super::Display::Summarized)
        }
        (Presence::Absent, Presence::Value(ReasoningSummary::Concise))
        | (Presence::Absent, Presence::Value(ReasoningSummary::Detailed)) => {
            rules.insert(RequestRule::SummarizeReasoning);
            Presence::Value(super::Display::Summarized)
        }
        (Presence::Value(ReasoningDisplay::Omitted), _)
        | (Presence::Absent, Presence::Value(ReasoningSummary::Disabled))
        | (Presence::Absent, Presence::Null) => Presence::Value(super::Display::Omitted),
        (Presence::Null, _) => Presence::Null,
        (Presence::Absent, Presence::Absent) => Presence::Absent,
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
        .skip(usize::from(matches!(
            request.items().first(),
            Some((_, Item::Instruction(_)))
        )))
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
        messages.extend(system_messages(&run, rules)?);
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
                item_blocks(
                    *owner, item, request, fidelity, target, normalized, omitted, rules,
                )?
                .into_iter()
                .enumerate()
                .map(|(index, block)| super::CacheMarked {
                    cache: match item {
                        Item::Message(m) => m.parts[index].cache,
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
                }),
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
    messages.extend(system_messages(&run, rules)?);
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
        temperature: s.controls.temperature(),
        top_p: s.controls.top_p(),
        top_k: s.controls.top_k,
        user_id: Presence::Absent,
        service_policy: Presence::Absent,
        max_tokens: s
            .controls
            .max_output_tokens
            .ok_or_else(|| CodecError::Unsupported("Messages missing output ceiling".into()))?,
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
