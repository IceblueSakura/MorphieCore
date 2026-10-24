//! Final carrier constraints used by OpenAI request and response encoders.
//! Callers retain their original validation stages after target projection.
use crate::protocol::openai::{CodecError, Profile};
use crate::semantic::task::generation::*;

pub(in crate::protocol::openai) fn check_response_carriers(
    response: &GenerationResponse,
) -> Result<(), CodecError> {
    if response.usage().is_some_and(|usage| {
        usage.output_relation != OutputTokenRelation::IncludesReasoning
            || usage.total_relation != TotalTokenRelation::InputAndOutput
            || usage.input_relation != InputTokenRelation::IncludesCache
            || usage.input_tokens.is_none()
            || usage.output_tokens.is_none()
            || usage.total_tokens.is_none()
    }) {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Usage,
        ));
    }
    if response.progress() != InteractionProgress::Unreported {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
    }
    Ok(())
}

pub(in crate::protocol::openai) fn check_item_carriers(
    items: &[(ItemId, Item)],
) -> Result<(), CodecError> {
    if items
        .iter()
        .any(|(_, item)| matches!(item, Item::ProviderTool(_)))
    {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
    }
    if items.iter().any(|(_, item)| match item {
        Item::ToolCall(call) => call.context.alias_domain.is_some(),
        Item::CustomCall(call) => call.context.alias_domain.is_some(),
        Item::ToolResult(result) | Item::CustomResult(result) => {
            result.context.alias_domain.is_some()
        }
        _ => false,
    }) {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
    }
    let foreign_replay = |item: &Item| match item {
        Item::Reasoning(r) => r
            .replay
            .as_ref()
            .is_some_and(|value| value.format() != ReplayFormat::ResponsesEncrypted),
        _ => false,
    };
    if items.iter().any(|(_, item)| foreign_replay(item)) {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Replay,
        ));
    }
    if items
        .iter()
        .any(|(_, item)| matches!(item, Item::Compaction(value) if value.replay_token().is_none()))
    {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
    }
    if items.iter().any(|(_, item)| {
        matches!(item, Item::ToolCall(call) if !matches!(call.arguments, ToolArguments::Raw(_)))
    }) {
        return Err(CodecError::Unsupported(crate::protocol::failure::CarrierReason::Unavailable));
    }
    let unrepresentable_result = |item: &Item| match item {
        Item::ToolResult(r) => {
            r.execution.is_some()
                || r.is_error.is_some()
                || matches!(r.output, ToolOutput::Structured(_))
        }
        Item::CustomResult(r) => {
            r.execution.is_some()
                || r.is_error.is_some()
                || matches!(r.output, ToolOutput::Structured(_))
                || r.call_id.is_none()
                || r.name.is_some()
        }
        _ => false,
    };
    if items.iter().any(|(_, item)| unrepresentable_result(item)) {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Tools,
        ));
    }
    Ok(())
}
pub(in crate::protocol::openai) fn check_message_carriers(
    items: &[(ItemId, Item)],
    groups: &[MessageEnvelope],
    profile: Profile,
) -> Result<(), CodecError> {
    if profile != Profile::Chat && items.iter().any(|(_, i)| i.participant().is_some()) {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
    }
    if items.iter().any(|(_, item)| {
        item.cache_boundaries()
            .any(|cache| profile == Profile::Chat || matches!(cache, PrefixCacheBoundary::Ttl(_)))
    }) {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
    }
    if profile == Profile::Chat
        && items
            .iter()
            .any(|(_, item)| matches!(item, Item::Compaction(_)))
    {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
    }
    if profile == Profile::Responses && !groups.is_empty() {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
    }
    for group in groups {
        if crate::protocol::openai::chat::owners::owner(group, items).is_none() {
            return Err(CodecError::Unsupported(
                crate::protocol::failure::CarrierReason::Unavailable,
            ));
        }
    }
    let owners = crate::protocol::openai::chat::owners::owners(items, groups);
    for (member, parent) in &owners {
        let first = items
            .iter()
            .position(|(id, _)| id == parent)
            .ok_or(CodecError::Invalid(
                crate::protocol::failure::ShapeReason::InvalidValue,
            ))?;
        let last = items
            .iter()
            .position(|(id, _)| id == member)
            .ok_or(CodecError::Invalid(
                crate::protocol::failure::ShapeReason::InvalidValue,
            ))?;
        if last <= first
            || items[first + 1..=last].iter().any(|(id, item)| {
                !matches!(item, Item::ToolCall(_) | Item::CustomCall(_))
                    || owners.get(id) != Some(parent)
            })
        {
            return Err(CodecError::Unsupported(
                crate::protocol::failure::CarrierReason::Unavailable,
            ));
        }
    }
    Ok(())
}
