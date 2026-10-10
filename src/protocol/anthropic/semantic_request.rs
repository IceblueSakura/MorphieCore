//! Candidate-local native request/history projection and stable prefix encoding.
use super::{
    Block, InputMessage, MessageContent, Role, Selection, TextContent as WireText,
    target::ReplayTarget,
};
use crate::{
    protocol::{CodecError, fidelity::FidelityRecords},
    semantic::{task::generation::*, value::Presence},
};
use serde_json::Value;
use std::collections::BTreeSet;
pub(super) fn unsupported() -> CodecError {
    CodecError::Unsupported("native semantic carrier".into())
}
pub(super) fn project(
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
    normalized: &mut Vec<ItemId>,
    omitted: &mut Vec<ItemId>,
    until: Option<ItemId>,
) -> Result<super::Request, CodecError> {
    let s = request.settings();
    if s.controls.temperature().is_some()
        || s.controls.top_p().is_some()
        || s.controls.top_logprobs.is_some()
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
        if i.authority != InstructionAuthority::System || i.status.is_some() || !system.is_absent()
        {
            return Err(unsupported());
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
        if s.parallel_tool_calls.is_some() {
            return Err(unsupported());
        }
        Presence::Absent
    };
    let reasoning = &s.reasoning;
    if reasoning.budget.is_some()
        || !reasoning.summary.is_absent()
        || !reasoning.context.is_absent()
        || reasoning.encrypted_output()
        || reasoning.presence == ReasoningPresence::Null
    {
        return Err(unsupported());
    }
    let thinking = match reasoning.mode {
        Presence::Absent if reasoning.display.is_absent() => Presence::Absent,
        Presence::Value(ReasoningMode::Adaptive) => Presence::Value(super::Thinking {
            display: match reasoning.display {
                Presence::Absent => Presence::Absent,
                Presence::Null => Presence::Null,
                Presence::Value(ReasoningDisplay::Summary) => {
                    Presence::Value(super::Display::Summarized)
                }
                Presence::Value(ReasoningDisplay::Omitted) => {
                    Presence::Value(super::Display::Omitted)
                }
            },
        }),
        _ => return Err(unsupported()),
    };
    let output_config = match reasoning.effort {
        Presence::Absent => Presence::Absent,
        Presence::Value(ReasoningEffort::Low) => Presence::Value(super::Effort::Low),
        Presence::Value(ReasoningEffort::Medium) => Presence::Value(super::Effort::Medium),
        Presence::Value(ReasoningEffort::High) => Presence::Value(super::Effort::High),
        _ => return Err(unsupported()),
    };
    if reasoning.presence == ReasoningPresence::Present
        && thinking.is_absent()
        && output_config.is_absent()
    {
        return Err(unsupported());
    }
    let mut included = BTreeSet::new();
    let mut messages = vec![];
    let mut stopped = false;
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
            if Some(*owner) == until {
                stopped = true;
                break;
            }
            if !included.insert(*owner) {
                return Err(unsupported());
            }
            let item = &request
                .items()
                .iter()
                .find(|(id, _)| id == owner)
                .ok_or_else(unsupported)?
                .1;
            blocks.extend(item_blocks(
                *owner, item, request, fidelity, target, normalized, omitted,
            )?);
        }
        messages.push(InputMessage {
            role,
            content: MessageContent::Blocks(blocks),
        });
        if stopped {
            break;
        }
    }
    let expected = request
        .items()
        .iter()
        .take_while(|(id, _)| Some(*id) != until)
        .filter(|(_, item)| !matches!(item, Item::Instruction(_)))
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    let actual = request
        .message_envelopes()
        .iter()
        .flat_map(|g| g.members())
        .take_while(|id| Some(**id) != until)
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
    if until.is_some() && !stopped {
        return Err(CodecError::Invalid("native replay owner"));
    }
    Ok(super::Request {
        model: target.model().into(),
        max_tokens: s.controls.max_output_tokens.ok_or_else(unsupported)?,
        system,
        tools,
        tool_choice,
        thinking,
        output_config,
        messages,
        stream: Presence::Absent,
    })
}
/// Version-one prefix encoding excludes local IDs and unrelated sampling controls.
/// This is a stable selected wire projection, not Rust Debug or a serialized IR.
pub(crate) fn prefix_digest(
    request: &GenerationRequest,
    owner: ItemId,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
) -> Result<[u8; 32], CodecError> {
    use sha2::{Digest, Sha256};
    request.validate()?;
    let wire = project(
        request,
        fidelity,
        target,
        &mut vec![],
        &mut vec![],
        Some(owner),
    )?;
    let value = wire.wire();
    let mut prefix = serde_json::Map::new();
    prefix.insert("version".into(), serde_json::json!(1));
    for field in ["system", "tools", "messages"] {
        if let Some(value) = value.get(field) {
            prefix.insert(field.into(), value.clone());
        }
    }
    let value = Value::Object(prefix);
    super::shape::bounded(&value)?;
    Ok(Sha256::digest(
        serde_json::to_vec(&value).map_err(|_| CodecError::Invalid("native prefix"))?,
    )
    .into())
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
