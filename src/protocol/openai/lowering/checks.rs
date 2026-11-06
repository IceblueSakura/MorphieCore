//! Shared OpenAI representation constraints used by request, response and event lowering.
use super::RepresentationError;
use crate::{
    protocol::{fidelity::FidelityRecords, openai::GenerationApi},
    semantic::task::generation::*,
};

/// Fixed Responses has no text/prediction slots; explicit profiles may own text slots.
/// Even zero is reported: never omit it or invent undeclared prediction positions.
pub(super) fn check_usage(
    usage: Usage,
    profile: GenerationApi,
    rules: &crate::protocol::openai::dialect::adaptation::WireRules,
) -> Result<(), RepresentationError> {
    usage.validate()?;
    if usage.output_relation != OutputTokenRelation::IncludesReasoning
        || usage.total_relation != TotalTokenRelation::InputAndOutput
        || usage.input_relation != InputTokenRelation::IncludesCache
        || usage.input_tokens.is_none()
        || usage.output_tokens.is_none()
        || usage.total_tokens.is_none()
    {
        return Err(RepresentationError::UsageProjection);
    }
    if profile == GenerationApi::Responses
        && usage.input_image_tokens.is_some()
        && !rules.responses_image_usage
    {
        return Err(RepresentationError::UsageDetails);
    }
    if profile == GenerationApi::Responses
        && ((usage.input_text_tokens.is_some() || usage.output_text_tokens.is_some())
            && !rules.responses_text_usage
            || usage.accepted_prediction_tokens.is_some()
            || usage.rejected_prediction_tokens.is_some()
            || usage.input_audio_tokens.is_some()
            || usage.output_audio_tokens.is_some())
    {
        return Err(RepresentationError::UsageDetails);
    }
    Ok(())
}

/// History transfer and final validation share the target's actual owner shape.
pub(crate) fn chat_reasoning_carrier(
    reasoning: &ReasoningItem,
    next: Option<&Item>,
    rules: &crate::protocol::openai::dialect::adaptation::WireRules,
) -> bool {
    if rules.readable_reasoning_text_only
        && (reasoning.replay.is_some()
            || reasoning
                .parts
                .iter()
                .any(|(_, part)| !matches!(part, ReasoningContent::Text(_))))
    {
        return false;
    }
    let readable = rules.readable_reasoning
        && reasoning.replay.is_none()
        && (matches!(
            reasoning.parts.as_slice(),
            [(_, ReasoningContent::Text(text) | ReasoningContent::Summary(text))]
                if !text.as_str().is_empty()
        ) && matches!(next,
            Some(Item::Message(message)) if message.role == MessageRole::Assistant)
            || !reasoning.parts.is_empty()
                && matches!(next, Some(Item::ToolCall(_) | Item::CustomCall(_))));
    // Each retained reasoning owner must in turn have a carrier, so consecutive
    // structured owners ultimately share the following assistant message or call.
    let structured = rules.structured_chat_reasoning
        && (!reasoning.parts.is_empty() || reasoning.replay.is_some())
        && !reasoning.parts.iter().any(
            |(_, part)| matches!(part, ReasoningContent::Text(text) if text.as_str().is_empty()),
        )
        && next.is_some_and(|item| match item {
            Item::Reasoning(_) | Item::ToolCall(_) | Item::CustomCall(_) => true,
            Item::Message(message) => message.role == MessageRole::Assistant,
            _ => false,
        });
    readable || structured
}
pub(super) fn represent_reasoning(
    settings: &GenerationSettings,
    items: &[(ItemId, Item)],
    fidelity: &FidelityRecords,
    profile: GenerationApi,
    origin: Option<&crate::semantic::value::ReplayOrigin>,
    adaptation: &crate::protocol::openai::dialect::adaptation::Adaptation,
    history: Option<&GenerationRequest>,
) -> Result<(), RepresentationError> {
    let structured_chat = adaptation.rules.structured_chat_reasoning;
    let controls = &settings.reasoning;
    let request = history.is_some();
    if !controls.read_history.is_none()
        && (profile != GenerationApi::Chat || !adaptation.rules.chat_preserve_thinking)
    {
        return Err(RepresentationError::Reasoning);
    }
    if crate::protocol::openai::dialect::bailian::extended(
        settings,
        profile,
        request && adaptation.rules.thinking_controls,
    ) {
        return Err(RepresentationError::Reasoning);
    }
    if profile == GenerationApi::Responses
        && controls.presentation.visibility.as_ref() == Some(&ReasoningDisplay::Omitted)
    {
        return Err(RepresentationError::Reasoning);
    }
    if profile == GenerationApi::Responses
        && items.windows(2).any(|pair| {
            matches!(pair[0].1, Item::ConfigurationUpdate(_))
                && matches!(pair[1].1, Item::ConfigurationUpdate(_))
        })
    {
        return Err(RepresentationError::Reasoning);
    }
    for (id, item) in items {
        // Replacement/reference/removal changes need a separate projection.
        if matches!(item, Item::ToolChange(_))
            || profile != GenerationApi::Responses && matches!(item, Item::ToolBatch(_))
        {
            return Err(RepresentationError::Tools);
        }
        if let Item::ConfigurationUpdate(update) = item
            && (profile != GenerationApi::Responses
                || update.boundary != ConfigurationBoundary::FollowingResponse
                || crate::protocol::openai::reasoning::effort_label(update).is_err())
        {
            return Err(RepresentationError::Reasoning);
        }
        if let Item::Compaction(value) = item {
            if profile != GenerationApi::Responses {
                return Err(RepresentationError::UnmigratedSemantic);
            }
            if !fidelity.compaction_matches(*id, value, origin) {
                return Err(RepresentationError::ReplayOrigin);
            }
            // No partial marker exists on a static compaction carrier.
            if value.replay_token().is_none() {
                return Err(RepresentationError::ReplayPhase);
            }
        }
        if let Item::Reasoning(r) = item
            && let Some(value) = &r.replay
        {
            if value.format() != ReplayFormat::ResponsesEncrypted {
                return Err(RepresentationError::ReplayFormat);
            }
            if (profile == GenerationApi::Chat || request)
                && r.status != ItemLifecycle::Completed
                && !(profile == GenerationApi::Responses && r.status == ItemLifecycle::Incomplete)
            {
                return Err(RepresentationError::Terminal);
            }
        }
        if let Item::Reasoning(reasoning) = item
            && let Some(replay) = &reasoning.replay
            && (profile != GenerationApi::Responses
                && !(profile == GenerationApi::Chat && structured_chat)
                || !history.map_or_else(
                    || fidelity.replay_matches(*id, reasoning, origin),
                    |request| fidelity.replay_matches_request(*id, reasoning, origin, request),
                )
                || replay.replay_token().is_none()
                    && (request
                        || reasoning.status == ItemLifecycle::Completed
                        || profile == GenerationApi::Chat))
        {
            return Err(RepresentationError::ReplayOrigin);
        }
    }
    let has_reasoning_items = items
        .iter()
        .any(|(_, item)| matches!(item, Item::Reasoning(_)));
    if adaptation.rules.readable_reasoning_text_only
        && items.iter().any(|(_, item)| matches!(item,
            Item::Reasoning(reasoning) if reasoning.replay.is_some()
                || reasoning.parts.iter().any(|(_, part)| !matches!(part, ReasoningContent::Text(_)))
        ))
    {
        return Err(RepresentationError::Reasoning);
    }
    let native_budget = request
        && adaptation.rules.thinking_controls
        && controls.budget_tokens.is_some()
        && controls.mode == crate::semantic::value::Presence::Value(ReasoningMode::Budgeted);
    if profile == GenerationApi::Chat
        && (!controls.context.is_absent() || !controls.mode.is_absent() && !native_budget)
    {
        return Err(RepresentationError::Reasoning);
    }
    let chat_control = controls.presence() == ReasoningPresence::Present
        && (settings.effort.is_none()
            && controls.activation.is_none()
            && controls.read_history.is_none()
            && !native_budget
            || !controls.presentation.is_absent());
    if has_reasoning_items
        && profile != GenerationApi::Responses
        && !(profile == GenerationApi::Chat
            && items
                .iter()
                .enumerate()
                .all(|(index, (_, item))| match item {
                    Item::Reasoning(reasoning) => chat_reasoning_carrier(
                        reasoning,
                        items.get(index + 1).map(|(_, item)| item),
                        &adaptation.rules,
                    ),
                    _ => true,
                }))
    {
        return Err(RepresentationError::Reasoning);
    }
    if controls.encrypted_output() && profile != GenerationApi::Responses {
        return Err(RepresentationError::Reasoning);
    }
    if profile == GenerationApi::Chat && chat_control {
        return Err(RepresentationError::Reasoning);
    }
    Ok(())
}
pub(super) fn text_items(
    items: &[(ItemId, Item)],
    profile: GenerationApi,
    request: bool,
    resources: &ResourceTable,
) -> Result<(), RepresentationError> {
    crate::protocol::openai::content::citations::check_items(items, resources, profile)
        .map_err(|_| RepresentationError::TextMetadata)?;
    for (_, i) in items {
        if i.participant().is_some() && (!request || profile != GenerationApi::Chat) {
            return Err(RepresentationError::UnmigratedSemantic);
        }
        if !request
            && profile == GenerationApi::Chat
            && matches!(i, Item::ToolResult(_) | Item::CustomResult(_))
        {
            return Err(RepresentationError::Tools);
        }
        if !request
            && matches!(i, Item::ToolResult(r) | Item::CustomResult(r) if r.status.is_none())
            || matches!(i, Item::CustomResult(r) if r.call_id.is_none() || r.name.is_some())
        {
            return Err(RepresentationError::Tools);
        }
        // Public carriers have no source/reference-domain field. A domain
        // declaration is not permission to erase it or invent a private carrier.
        let alias_domain = match i {
            Item::ToolCall(call) => call.context.alias_domain.is_some(),
            Item::CustomCall(call) => call.context.alias_domain.is_some(),
            Item::ToolResult(result) | Item::CustomResult(result) => {
                result.context.alias_domain.is_some()
            }
            _ => false,
        };
        if alias_domain {
            return Err(RepresentationError::UnmigratedSemantic);
        }
        // These profiles have neither structured argument nor execution-report carriers.
        if matches!(i, Item::ToolCall(call) if !matches!(call.arguments, ToolArguments::Raw(_))) {
            return Err(RepresentationError::Tools);
        }
        // Responses admits image and file result parts in history and output.
        if let Item::ToolResult(result) | Item::CustomResult(result) = i
            && !result_representable(result, profile, resources)
        {
            return Err(RepresentationError::Tools);
        }
        if profile == GenerationApi::Chat {
            match i {
                Item::CustomCall(c) if !c.context.is_direct() => {
                    return Err(RepresentationError::Tools);
                }
                Item::CustomResult(r) if !r.context.is_direct() => {
                    return Err(RepresentationError::Tools);
                }
                Item::Program(_) | Item::ProgramOutput(_) => {
                    return Err(RepresentationError::Tools);
                }
                Item::ToolCall(c) if !c.context.is_direct() => {
                    return Err(RepresentationError::Tools);
                }
                Item::ToolResult(r) if !r.context.is_direct() => {
                    return Err(RepresentationError::Tools);
                }
                Item::ConfigurationUpdate(_) => {
                    return Err(RepresentationError::Reasoning);
                }
                Item::Instruction(i) if !request && i.parts.len() != 1 => {
                    return Err(RepresentationError::MessageGrouping);
                }
                Item::ToolResult(r)
                    if (!request && matches!(r.output, ToolOutput::Parts(_)))
                        || r.status.is_some_and(|s| s != ItemLifecycle::Completed) =>
                {
                    return Err(RepresentationError::Tools);
                }
                Item::Message(m)
                    if m.parts.iter().any(|p| match &p.content {
                        ContentPart::Text(t) => {
                            (request && !t.is_plain())
                                || t.annotations().iter().any(|a| {
                                    crate::protocol::openai::content::citations::check(
                                        a,
                                        resources,
                                        GenerationApi::Chat,
                                    )
                                    .is_err()
                                })
                        }
                        ContentPart::Refusal(t) => request && !t.logprobs().is_none(),
                        _ => false,
                    }) =>
                {
                    return Err(RepresentationError::TextMetadata);
                }
                _ => {}
            }
        }
        if let Item::Message(m) = i {
            for part in &m.parts {
                let probs = match &part.content {
                    ContentPart::Text(t) => t.logprobs(),
                    ContentPart::Refusal(t) => {
                        if profile == GenerationApi::Responses && !t.logprobs().is_none() {
                            return Err(RepresentationError::TextMetadata);
                        }
                        t.logprobs()
                    }
                    _ => continue,
                };
                if profile == GenerationApi::Chat
                    && let Some(probs) = probs.as_ref()
                {
                    crate::protocol::openai::chat::logprobs::validate(probs)
                        .map_err(|_| RepresentationError::TextMetadata)?;
                }
            }
            if profile == GenerationApi::Responses
                && m.parts.iter().any(|p| {
                    matches!(&p.content, ContentPart::Text(t)
                        if t.logprobs().as_ref().is_some_and(|v| !v.iter().all(complete_logprob)))
                })
            {
                return Err(RepresentationError::TextMetadata);
            }
            if m.parts.iter().any(|p| match &p.content {
                ContentPart::Text(_) | ContentPart::Refusal(_) => false,
                ContentPart::Audio(_) => profile != GenerationApi::Chat,
                ContentPart::AudioReference(_) => profile != GenerationApi::Chat || !request,
                ContentPart::Resource(resource) => {
                    let Ok(resource) = resource.media(resources) else {
                        return true;
                    };
                    if matches!(resource.description, ResourceDescription::File(file)
                        if file.context.is_some())
                    {
                        return true;
                    }
                    !request
                        || match resource.kind() {
                            ResourceKind::Image => {
                                matches!(
                                    resource.location,
                                    ResourceLocation::OpaqueReference(_)
                                        | ResourceLocation::NamespacedReference { .. }
                                )
                            }
                            // Chat files are inline PDFs only and have no detail slot.
                            // https://developers.openai.com/api/docs/guides/file-inputs
                            ResourceKind::File => match profile {
                                GenerationApi::Responses => matches!(
                                    resource.location,
                                    ResourceLocation::OpaqueReference(_)
                                        | ResourceLocation::NamespacedReference { .. }
                                ),
                                GenerationApi::Chat => {
                                    m.role != MessageRole::User
                                        || !matches!(
                                            resource.location,
                                            ResourceLocation::Inline { media_type, .. }
                                                if media_type.as_str() == "application/pdf"
                                        )
                                        || matches!(resource.description,
                                            ResourceDescription::File(f) if f.detail.is_some())
                                }
                            },
                            ResourceKind::Video => {
                                m.role != MessageRole::User
                                    || matches!(
                                        resource.location,
                                        ResourceLocation::OpaqueReference(_)
                                            | ResourceLocation::NamespacedReference { .. }
                                    )
                            }
                            ResourceKind::Audio => {
                                profile != GenerationApi::Chat
                                    || m.role != MessageRole::User
                                    || !crate::protocol::openai::content::input_audio::representable(
                                        resource,
                                    )
                            }
                        }
                }
            }) {
                return Err(RepresentationError::UnmigratedSemantic);
            }
            if profile == GenerationApi::Chat {
                let audio = m
                    .parts
                    .iter()
                    .filter(|p| {
                        matches!(
                            p.content,
                            ContentPart::Audio(_) | ContentPart::AudioReference(_)
                        )
                    })
                    .count();
                if audio > 1
                    || (audio == 1
                        && (m.parts.len() > 2
                            || !m.parts.last().is_some_and(|p| {
                                matches!(
                                    p.content,
                                    ContentPart::Audio(_) | ContentPart::AudioReference(_)
                                )
                            })
                            || m.parts.iter().any(|p| {
                                matches!(
                                    p.content,
                                    ContentPart::Refusal(_) | ContentPart::Resource(_)
                                )
                            })))
                {
                    return Err(RepresentationError::MessageGrouping);
                }
            }
            if profile == GenerationApi::Chat
                && m.parts
                    .iter()
                    .filter(|p| {
                        !matches!(
                            p.content,
                            ContentPart::Audio(_) | ContentPart::AudioReference(_)
                        )
                    })
                    .count()
                    > 1
                && (!request
                    || m.parts.iter().any(|p| {
                        !matches!(
                            p.content,
                            ContentPart::Text(_)
                                | ContentPart::Refusal(_)
                                | ContentPart::Resource(_)
                        )
                    }))
            {
                return Err(RepresentationError::MessageGrouping);
            }
        }
    }
    Ok(())
}
pub(super) fn validate_wire_ids(
    items: &[(ItemId, Item)],
    fidelity: &FidelityRecords,
    generate: bool,
) -> Result<(), RepresentationError> {
    fidelity
        .check_wire_item_ids(items, generate)
        .map_err(|_| RepresentationError::Metadata)
}
pub(super) fn check_tool_selection(choice: Option<&ToolChoice>) -> Result<(), RepresentationError> {
    // The pinned standard named-tool types have no qualified-reference carrier.
    // https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/tool_choice_function_param.py
    let qualified = match choice {
        Some(ToolChoice::Qualified(_)) => true,
        Some(ToolChoice::Allowed { tools, .. }) => tools.iter().any(|r| r.namespace.is_some()),
        _ => false,
    };
    if qualified {
        return Err(RepresentationError::Tools);
    }
    Ok(())
}
/// Responses logprobs require bytes and complete alternatives.
fn complete_logprob(p: &Logprob) -> bool {
    p.bytes.is_some()
        && p.top_logprobs.as_ref().is_some_and(|alternatives| {
            alternatives
                .iter()
                .all(|a| a.token.is_some() && a.logprob.is_some() && a.bytes.is_some())
        })
}
fn result_representable(
    result: &ToolResult,
    profile: GenerationApi,
    resources: &ResourceTable,
) -> bool {
    let unsupported_media = match &result.output {
        ToolOutput::Parts(parts) => parts.iter().any(|part| {
            matches!(&part.content, InputPart::Resource(r)
            if r.media(resources).is_ok_and(|r| {
                matches!(r.kind(), ResourceKind::Audio | ResourceKind::Video)
                    || matches!(r.description, ResourceDescription::File(file)
                        if file.context.is_some())
            }))
        }),
        _ => false,
    };
    result.execution.is_none()
        && result.is_error.is_none()
        && !matches!(result.output, ToolOutput::Structured(_))
        && (profile != GenerationApi::Chat || result.output.is_text_only())
        && !unsupported_media
}
