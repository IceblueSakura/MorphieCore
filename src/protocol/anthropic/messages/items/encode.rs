//! IR item → Messages block encoding shared by request history and response output.
use crate::protocol::anthropic::messages::{
    Block, Media, ResultBlock, ResultContent, items::unsupported,
};
use crate::protocol::{CodecError, fidelity::FidelityRecords, native::ReplayTarget};
use crate::semantic::{task::generation::*, value::Presence};

pub(in crate::protocol::anthropic::messages) fn item_blocks(
    id: ItemId,
    item: &Item,
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
    normalized: &mut Vec<ItemId>,
    omitted: &mut Vec<ItemId>,
) -> Result<Vec<Block>, CodecError> {
    let media = |resource: &ResourceUse| Media::from_resource(resource.media(request.resources())?);
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
                        Ok(Block::Media(media(r)?))
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
                            Ok(crate::protocol::anthropic::messages::CacheMarked {
                                cache: part.cache,
                                content: match &part.content {
                                    InputPart::Text(t) => ResultBlock::Text(t.as_str().into()),
                                    InputPart::Resource(r) => ResultBlock::Media(media(r)?),
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
