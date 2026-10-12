//! Candidate-local native request/history projection.
use super::{Block, InputMessage, MessageContent, Role, Selection, TextContent as WireText};
use crate::{
    protocol::{CodecError, fidelity::FidelityRecords, native::ReplayTarget},
    semantic::{task::generation::*, value::Presence},
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
pub(super) fn unsupported() -> CodecError {
    CodecError::Unsupported("native semantic carrier".into())
}
/// Named degradable rules applied to one candidate's request copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RequestRule {
    /// `none` and `minimal` have no Messages level; `low` is the least effort.
    LowerEffort,
    /// Messages has one summarized display, without concise or detailed variants.
    SummarizeReasoning,
    OmitReasoningContext,
    /// Claude 5.x models reject non-default `temperature` and `top_p`.
    OmitSampling,
    ProjectCallIds,
    /// Opaque reasoning issued for another target is dropped.
    OmitForeignReplay,
    /// Reasoning without this target's signature is sent as assistant text, as pi does.
    ReasoningAsText,
    /// Claude 5.x thinks adaptively; a token budget becomes the nearest effort.
    BudgetAsEffort,
    /// A leading developer instruction (pi, Codex) is the system prompt.
    DeveloperAsSystem,
    /// An omitted `strict` defaulting to schema normalization (Responses) is sent
    /// as a plain tool; Messages has no strict tools.
    DefaultStrictAsPlain,
}
impl RequestRule {
    pub const fn name(self) -> &'static str {
        match self {
            Self::LowerEffort => "messages.lower-effort.v1",
            Self::SummarizeReasoning => "messages.summarize-reasoning.v1",
            Self::OmitReasoningContext => "messages.omit-reasoning-context.v1",
            Self::OmitSampling => "messages.omit-sampling.v1",
            Self::ProjectCallIds => "messages.project-call-ids.v1",
            Self::OmitForeignReplay => "messages.omit-foreign-replay.v1",
            Self::ReasoningAsText => "messages.reasoning-as-text.v1",
            Self::BudgetAsEffort => "messages.budget-as-effort.v1",
            Self::DeveloperAsSystem => "messages.developer-as-system.v1",
            Self::DefaultStrictAsPlain => "messages.default-strict-as-plain.v1",
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
pub(super) fn project(
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
    normalized: &mut Vec<ItemId>,
    omitted: &mut Vec<ItemId>,
    rules: &mut BTreeSet<RequestRule>,
) -> Result<super::Request, CodecError> {
    let s = request.settings();
    if s.controls.temperature().is_some() || s.controls.top_p().is_some() {
        rules.insert(RequestRule::OmitSampling);
    }
    if s.controls.top_logprobs.is_some()
        || !s.controls.logprobs.is_absent()
        || s.controls.truncation.is_some()
        || s.text != TextOptions::default()
        || !s.output_modalities.is_absent()
        || !s.audio.is_absent()
        || !request.replay_groups().is_empty()
        || !request.call_derivations().is_empty()
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
            i.parts.iter().map(|(_, t)| t.as_str().into()).collect(),
        ));
    }
    let tools = if s.tools.is_some() {
        Presence::Value(
            request
                .tools()
                .iter()
                .map(|tool| {
                    let ToolDefinition::Function(t) = tool else {
                        return Err(unsupported());
                    };
                    if !t.dispatch.is_inactive() || t.output_schema.is_some() {
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
                            FunctionStrictness::Omitted(StrictDefault::NonStrict) => {
                                Presence::Absent
                            }
                            FunctionStrictness::Explicit(false) => Presence::Value(false),
                            FunctionStrictness::Omitted(StrictDefault::NormalizeSchema) => {
                                rules.insert(RequestRule::DefaultStrictAsPlain);
                                Presence::Absent
                            }
                            _ => return Err(unsupported()),
                        },
                    })
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
    if !reasoning.context.is_absent() {
        rules.insert(RequestRule::OmitReasoningContext);
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
    let thinking = match reasoning.mode {
        Presence::Absent if display.is_absent() => Presence::Absent,
        Presence::Absent | Presence::Value(ReasoningMode::Adaptive) => {
            Presence::Value(super::Thinking { display })
        }
        Presence::Value(ReasoningMode::Budgeted) => {
            rules.insert(RequestRule::BudgetAsEffort);
            Presence::Value(super::Thinking { display })
        }
        _ => return Err(unsupported()),
    };
    let effort = match (reasoning.effort.clone(), reasoning.budget_as_effort()) {
        (Presence::Absent, Some(effort)) => Presence::Value(effort),
        (effort, _) => effort,
    };
    let output_config = match effort {
        Presence::Absent => Presence::Absent,
        Presence::Value(ReasoningEffort::None | ReasoningEffort::Minimal) => {
            rules.insert(RequestRule::LowerEffort);
            Presence::Value(super::Effort::Low)
        }
        Presence::Value(ReasoningEffort::Low) => Presence::Value(super::Effort::Low),
        Presence::Value(ReasoningEffort::Medium) => Presence::Value(super::Effort::Medium),
        Presence::Value(ReasoningEffort::High) => Presence::Value(super::Effort::High),
        Presence::Value(ReasoningEffort::XHigh) => Presence::Value(super::Effort::XHigh),
        Presence::Value(ReasoningEffort::Max) => Presence::Value(super::Effort::Max),
        Presence::Null => return Err(unsupported()),
    };
    if reasoning.presence == ReasoningPresence::Present
        && thinking.is_absent()
        && output_config.is_absent()
    {
        return Err(unsupported());
    }
    let mut included = BTreeSet::new();
    let mut messages = vec![];
    for group in request.message_envelopes() {
        if group.members().is_empty() {
            return Err(unsupported());
        }
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
                for (_, part) in &r.parts {
                    let (ReasoningContent::Summary(t) | ReasoningContent::Text(t)) = part;
                    if !t.as_str().trim().is_empty() {
                        rules.insert(RequestRule::ReasoningAsText);
                        blocks.push(Block::Text(t.as_str().into()));
                    }
                }
                continue;
            }
            blocks.extend(
                item_blocks(*owner, item, request, fidelity, target, normalized, omitted)?
                    .into_iter()
                    .map(|block| match block {
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
                    }),
            );
        }
        messages.push(InputMessage {
            role,
            content: MessageContent::Blocks(blocks),
        });
    }
    let expected = request
        .items()
        .iter()
        .filter(|(_, item)| !matches!(item, Item::Instruction(_)))
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    let actual = request
        .message_envelopes()
        .iter()
        .flat_map(|g| g.members())
        .copied()
        .collect::<Vec<_>>();
    if actual != expected
        || request
            .items()
            .iter()
            .skip(1)
            .any(|(_, i)| matches!(i, Item::Instruction(_)))
    {
        return Err(unsupported());
    }
    Ok(super::Request {
        model: target.model().into(),
        max_tokens: s.controls.max_output_tokens.ok_or_else(unsupported)?,
        system,
        tools,
        tool_choice,
        thinking,
        output_config,
        // Always requested, like OpenAI's implicit caching; below the model's minimum
        // prompt length nothing is cached or charged.
        cache_control: Presence::Value(super::CacheControl::Ephemeral),
        messages,
        stream: Presence::Absent,
    })
}
pub(super) fn item_blocks(
    id: ItemId,
    item: &Item,
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
    normalized: &mut Vec<ItemId>,
    omitted: &mut Vec<ItemId>,
) -> Result<Vec<Block>, CodecError> {
    Ok(match item {
        Item::Message(m) => {
            if m.status != ItemLifecycle::Completed || m.phase.is_some() {
                return Err(unsupported());
            }
            m.parts
                .iter()
                .map(|p| {
                    let ContentPart::Text(t) = &p.content else {
                        return Err(unsupported());
                    };
                    if !t.is_plain() || p.replay.is_some() {
                        return Err(unsupported());
                    }
                    Ok(Block::Text(t.as_str().into()))
                })
                .collect::<Result<_, _>>()?
        }
        Item::Reasoning(r) => {
            if r.status != ItemLifecycle::Completed
                || r.parts.len() > 1
                || !fidelity.replay_matches_request(id, r, Some(&target.origin()), request)
            {
                return Err(CodecError::Invalid("native replay dependency"));
            }
            let replay = r.replay.as_ref().ok_or_else(unsupported)?;
            if replay.format() != ReplayFormat::AnthropicMessagesThinking {
                return Err(unsupported());
            }
            let thinking = match r.parts.first() {
                None => String::new(),
                Some((_, ReasoningContent::Summary(t))) => t.as_str().into(),
                _ => return Err(unsupported()),
            };
            vec![Block::Thinking {
                thinking,
                signature: replay.replay_token().ok_or_else(unsupported)?.into(),
            }]
        }
        Item::ToolCall(c) => {
            if c.status != ItemLifecycle::Completed
                || !c.context.is_direct()
                || c.context.replay.is_some()
                || c.context.definition.is_some()
                || c.context.alias_domain.is_some()
            {
                return Err(unsupported());
            }
            let input = match &c.arguments {
                ToolArguments::Structured(v) => v.clone(),
                ToolArguments::Raw(s) => {
                    let value = crate::semantic::value::parse_json(
                        s.as_bytes(),
                        crate::semantic::value::JsonLimits::STRUCTURED,
                    )
                    .map_err(|_| CodecError::Invalid("native object arguments"))?;
                    normalized.push(id);
                    StructuredValue::new(value)?
                }
                ToolArguments::StructuredPartial(_) => return Err(unsupported()),
            };
            if !input.value().is_object() {
                return Err(unsupported());
            }
            vec![Block::ToolUse {
                id: c.call_id.as_str().into(),
                name: c.name.as_str().into(),
                input,
                caller: Presence::Absent,
            }]
        }
        Item::ToolResult(r) => {
            if !r.context.is_direct()
                || r.context.alias_domain.is_some()
                || r.context.definition.is_some()
                || r.status.is_some()
            {
                return Err(unsupported());
            }
            if let Some(execution) = &r.execution {
                let allowed = match execution {
                    ToolExecution::Succeeded => r.is_error != Some(true),
                    ToolExecution::NotExecuted | ToolExecution::Failed { code: None } => {
                        r.is_error == Some(true)
                    }
                    _ => false,
                };
                if !allowed {
                    return Err(unsupported());
                }
                omitted.push(id);
            }
            let content = match &r.output {
                ToolOutput::Text(s) => WireText::Text(s.clone()),
                ToolOutput::Parts(parts) => WireText::Blocks(
                    parts
                        .iter()
                        .map(|(_, part)| {
                            let ToolResultPart::Text(t) = part else {
                                return Err(unsupported());
                            };
                            Ok(t.as_str().into())
                        })
                        .collect::<Result<_, _>>()?,
                ),
                _ => return Err(unsupported()),
            };
            vec![Block::ToolResult {
                tool_use_id: r.call_id.as_str().into(),
                content,
                is_error: r.is_error.map_or(Presence::Absent, Presence::Value),
            }]
        }
        _ => return Err(unsupported()),
    })
}
