//! Candidate-local native request/history projection.
use super::{
    Block, InputMessage, Media, MessageContent, ResultBlock, ResultContent, Role, Selection,
    TextContent as WireText,
};
use crate::{
    protocol::{CodecError, fidelity::FidelityRecords, native::ReplayTarget},
    semantic::{task::generation::*, value::Presence},
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
/// The cap opencode's Messages client sends by default (`DEFAULT_MAX_TOKENS` in
/// https://github.com/anomalyco/opencode/blob/e5a557efeee02c94486d391afc48f24a49ebc9a5/packages/ai/src/protocols/anthropic-messages.ts).
const DEFAULT_MAX_TOKENS: u64 = 32_000;
pub(super) fn unsupported() -> CodecError {
    CodecError::Unsupported("native semantic carrier".into())
}
/// Named degradable rules applied to one candidate's request copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RequestRule {
    ReasoningPolicy(&'static str),
    Cache(crate::protocol::cache::CacheRule),
    OmitVerbosity,
    /// Messages has one summarized display, without concise or detailed variants.
    SummarizeReasoning,
    /// Claude 5.x models reject non-default `temperature` and `top_p`.
    OmitSampling,
    ProjectCallIds,
    /// Opaque reasoning issued for another target is dropped.
    OmitForeignReplay,
    /// Reasoning without this target's signature is sent as assistant text, as pi does.
    ReasoningAsText,
    /// A leading developer instruction (pi, Codex) is the system prompt.
    DeveloperAsSystem,
    /// A Messages output format has no name; OpenAI requires one as a label.
    OmitSchemaName,
}
impl RequestRule {
    pub const fn name(self) -> &'static str {
        match self {
            Self::ReasoningPolicy(rule) => rule,
            Self::Cache(rule) => rule.name(),
            Self::OmitVerbosity => "messages.omit-verbosity.v1",
            Self::SummarizeReasoning => "messages.summarize-reasoning.v1",
            Self::OmitSampling => "messages.omit-sampling.v1",
            Self::ProjectCallIds => "messages.project-call-ids.v1",
            Self::OmitForeignReplay => "messages.omit-foreign-replay.v1",
            Self::ReasoningAsText => "messages.reasoning-as-text.v1",
            Self::DeveloperAsSystem => "messages.developer-as-system.v1",
            Self::OmitSchemaName => "messages.omit-schema-name.v1",
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
    if request.cached_context().is_some() {
        return Err(unsupported());
    }
    if s.controls.top_k.is_some()
        || s.controls.presence_penalty().is_some()
        || s.controls.frequency_penalty().is_some()
        || s.controls.seed.is_some()
    {
        return Err(unsupported());
    }
    if s.controls.temperature().is_some() || s.controls.top_p().is_some() {
        rules.insert(RequestRule::OmitSampling);
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
                .map(|p| {
                    Ok(super::CacheMarked {
                        content: p.content.as_text().ok_or_else(unsupported)?.into(),
                        cache: p.cache,
                    })
                })
                .collect::<Result<_, CodecError>>()?,
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
                            FunctionStrictness::Explicit(true) => Presence::Value(true),
                            FunctionStrictness::Omitted(StrictDefault::NormalizeSchema) => {
                                return Err(unsupported());
                            }
                        },
                        cache_control: t.cache.map_or(Presence::Absent, Presence::Value),
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
    if reasoning.budget_tokens.is_some() {
        return Err(unsupported());
    }
    let thinking = match (&reasoning.activation, &reasoning.mode) {
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
            Presence::Value(super::Thinking::Adaptive { display })
        }
        (Presence::Absent, Presence::Absent) if display.is_absent() => Presence::Absent,
        _ => return Err(unsupported()),
    };
    let effort = match &s.effort {
        Presence::Absent => Presence::Absent,
        Presence::Value(control) if control.scope == EffortScope::Response => {
            Presence::Value(match control.level {
                EffortLevel::Low => super::Effort::Low,
                EffortLevel::Medium => super::Effort::Medium,
                EffortLevel::High => super::Effort::High,
                EffortLevel::XHigh => super::Effort::XHigh,
                EffortLevel::Max => super::Effort::Max,
                EffortLevel::Minimal => return Err(unsupported()),
            })
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
        service_policy: Presence::Absent,
        // Messages requires max_tokens; other sources may leave the cap to the model.
        max_tokens: s.controls.max_output_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
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
        // Always requested, like OpenAI's implicit caching; below the model's minimum
        // prompt length nothing is cached or charged.
        cache_control: Presence::Value(super::CacheControl::from_ttl(
            s.automatic_prefix_cache
                .unwrap_or(PrefixCacheTtl::FiveMinutes),
        )),
        messages,
        stream: Presence::Absent,
    })
}
#[allow(clippy::too_many_arguments)]
pub(super) fn item_blocks(
    id: ItemId,
    item: &Item,
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
    normalized: &mut Vec<ItemId>,
    omitted: &mut Vec<ItemId>,
    _rules: &mut BTreeSet<RequestRule>,
) -> Result<Vec<Block>, CodecError> {
    let media = |part: PartId, resource: &ResourceUse| {
        Media::from_resource(
            resource.media(request.resources())?,
            fidelity.strict_image_size(part),
        )
    };
    Ok(match item {
        Item::Message(m) => {
            if m.status != ItemLifecycle::Completed || m.phase.is_some() {
                return Err(unsupported());
            }
            m.parts
                .iter()
                .map(|p| match &p.content {
                    ContentPart::Text(t) if t.is_plain() => Ok(Block::Text(t.as_str().into())),
                    ContentPart::Resource(r) if m.role == MessageRole::User => {
                        Ok(Block::Media(media(p.id, r)?))
                    }
                    _ => Err(unsupported()),
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
            let value = replay.replay_token().ok_or_else(unsupported)?.into();
            match (replay.format(), r.parts.first()) {
                (ReplayFormat::AnthropicMessagesRedactedThinking, None) => {
                    vec![Block::RedactedThinking(value)]
                }
                (ReplayFormat::AnthropicMessagesThinking, part) => vec![Block::Thinking {
                    thinking: match part {
                        None => String::new(),
                        Some((_, ReasoningContent::Summary(t))) => t.as_str().into(),
                        _ => return Err(unsupported()),
                    },
                    signature: value,
                }],
                _ => return Err(unsupported()),
            }
        }
        Item::ToolCall(c) => {
            if c.status != ItemLifecycle::Completed
                || !c.context.is_direct()
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
            if !r.context.is_direct() || r.context.alias_domain.is_some() || r.status.is_some() {
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
                ToolOutput::Text(s) => ResultContent::Text(s.clone()),
                ToolOutput::Parts(parts) => ResultContent::Blocks(
                    parts
                        .iter()
                        .map(|part| {
                            Ok(super::CacheMarked {
                                cache: part.cache,
                                content: match &part.content {
                                    InputPart::Text(t) => ResultBlock::Text(t.as_str().into()),
                                    InputPart::Resource(r) => {
                                        ResultBlock::Media(media(part.id, r)?)
                                    }
                                },
                            })
                        })
                        .collect::<Result<_, CodecError>>()?,
                ),
                _ => return Err(unsupported()),
            };
            vec![Block::ToolResult {
                tool_use_id: r.required_call_id()?.into(),
                content,
                is_error: r.is_error.map_or(Presence::Absent, Presence::Value),
            }]
        }
        _ => return Err(unsupported()),
    })
}
