//! Final typed SIWC admission, before cache projection or wire encoding.
use crate::adapter::Request;
use crate::{
    protocol::CodecError,
    semantic::task::generation::{InstructionAuthority, Item, ToolDefinition},
};

pub(in crate::protocol::openai) fn check(
    request: &Request,
    cache: crate::protocol::cache::CacheProjection,
) -> Result<(), CodecError> {
    let context = &request.context;
    let controls = request.task.semantic.controls();
    if controls.max_output_tokens.is_some()
        || controls.temperature().is_some()
        || controls.top_p().is_some()
        || controls.top_logprobs.is_some()
        || !controls.logprobs.is_absent()
        || controls.truncation.is_some()
        || !context.background.is_absent()
        || !context.conversation.is_absent()
        || !context.previous_response_id.is_absent()
        || !context.metadata.is_absent()
        || !context.max_tool_calls.is_absent()
        || !context.prompt.is_absent()
        || !context.moderation.is_absent()
        || !context.context_management.is_absent()
        || (!context.cache.prompt_cache_retention.is_absent()
            && !cache.omits_day_retention(context))
        || !context.cache.prompt_cache_options.is_absent()
    {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
    }
    for tool in request.task.semantic.tools() {
        if !matches!(tool, ToolDefinition::Namespace(_)) || !tool.dispatch_inactive() {
            return Err(CodecError::Unsupported(
                crate::protocol::failure::CarrierReason::Tools,
            ));
        }
    }
    for (_, item) in request.task.semantic.items() {
        let unsupported = match item {
            Item::Instruction(i) => i.authority == InstructionAuthority::System,
            Item::ToolCall(c) => {
                c.context.namespace.is_none() || c.context.async_call || c.context.caller.is_some()
            }
            Item::CustomCall(c) => {
                c.context.namespace.is_none() || c.context.async_call || c.context.caller.is_some()
            }
            Item::Program(_) | Item::ProgramOutput(_) => true,
            _ => false,
        };
        if unsupported {
            return Err(CodecError::Unsupported(
                crate::protocol::failure::CarrierReason::Unavailable,
            ));
        }
    }
    Ok(())
}
