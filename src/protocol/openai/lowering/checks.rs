//! Shared OpenAI representation constraints used by request, response and event lowering.
use super::RepresentationError;
use crate::{
    protocol::{fidelity::FidelityRecords, openai::Profile},
    semantic::task::generation::*,
};

/// Fixed Responses has no text/prediction slots; explicit profiles may own text slots.
/// Even zero is reported: never omit it or invent undeclared prediction positions.
pub(super) fn check_usage(
    usage: Usage,
    profile: Profile,
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
    if profile == Profile::Responses
        && usage.input_image_tokens.is_some()
        && !rules.responses_image_usage
    {
        return Err(RepresentationError::UsageDetails);
    }
    if profile == Profile::Responses
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

/// Chat can carry readable reasoning text on its carrier message: each reasoning
/// item must hold exactly one non-empty text part and sit immediately before the
/// message item that carries it.
fn chat_reasoning_shape(items: &[(ItemId, Item)]) -> bool {
    items
        .iter()
        .enumerate()
        .all(|(index, (_, item))| match item {
            // Messages thinking is a summary and often precedes a call without text;
            // either way it rides the next assistant message as its readable view.
            Item::Reasoning(reasoning) => {
                matches!(
                    reasoning.parts.as_slice(),
                    [(_, ReasoningContent::Text(text) | ReasoningContent::Summary(text))]
                        if !text.as_str().is_empty()
                ) && matches!(items.get(index + 1),
                    Some((_, Item::Message(message))) if message.role == MessageRole::Assistant)
                    || matches!(
                        items.get(index + 1),
                        Some((_, Item::ToolCall(_) | Item::CustomCall(_)))
                    ) && !reasoning.parts.is_empty()
                        && reasoning.replay.is_none()
            }
            _ => true,
        })
}
pub(super) fn represent_reasoning(
    settings: &GenerationSettings,
    items: &[(ItemId, Item)],
    fidelity: &FidelityRecords,
    profile: Profile,
    origin: Option<&crate::semantic::value::ReplayOrigin>,
    adaptation: &crate::protocol::openai::dialect::adaptation::Adaptation,
    history: Option<&GenerationRequest>,
) -> Result<(), RepresentationError> {
    let structured_chat = adaptation.rules.structured_chat_reasoning;
    let controls = &settings.reasoning;
    let request = history.is_some();
    if !controls.read_history.is_absent()
        && (profile != Profile::Chat || !adaptation.rules.chat_preserve_thinking)
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
    if profile == Profile::Responses
        && controls.presentation.visibility.value() == Some(&ReasoningDisplay::Omitted)
    {
        return Err(RepresentationError::Reasoning);
    }
    for (id, item) in items {
        // No OpenAI carrier orders tool availability; collapsing it is a separate policy.
        if matches!(item, Item::ToolChange(_)) {
            return Err(RepresentationError::Tools);
        }
        if let Item::ConfigurationUpdate(update) = item
            && (profile != Profile::Responses
                || crate::protocol::openai::reasoning::effort_label(update).is_err())
        {
            return Err(RepresentationError::Reasoning);
        }
        if let Item::Compaction(value) = item {
            if profile != Profile::Responses {
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
            if (profile == Profile::Chat || request) && r.status != ItemLifecycle::Completed {
                return Err(RepresentationError::Terminal);
            }
        }
        if let Item::Reasoning(reasoning) = item
            && let Some(replay) = &reasoning.replay
            && (profile != Profile::Responses && !(profile == Profile::Chat && structured_chat)
                || !history.map_or_else(
                    || fidelity.replay_matches(*id, reasoning, origin),
                    |request| fidelity.replay_matches_request(*id, reasoning, origin, request),
                )
                || replay.replay_token().is_none()
                    && (request
                        || reasoning.status == ItemLifecycle::Completed
                        || profile == Profile::Chat))
        {
            return Err(RepresentationError::ReplayOrigin);
        }
    }
    let has_reasoning_items = items
        .iter()
        .any(|(_, item)| matches!(item, Item::Reasoning(_)));
    let native_budget = request
        && adaptation.rules.thinking_controls
        && controls.budget_tokens.is_some()
        && controls.mode == crate::semantic::value::Presence::Value(ReasoningMode::Budgeted);
    if profile == Profile::Chat
        && (!controls.context.is_absent() || !controls.mode.is_absent() && !native_budget)
    {
        return Err(RepresentationError::Reasoning);
    }
    let chat_control = controls.presence() == ReasoningPresence::Present
        && (settings.effort.is_absent()
            && controls.activation.is_absent()
            && controls.read_history.is_absent()
            && !native_budget
            || !controls.presentation.is_absent());
    if has_reasoning_items
        && profile != Profile::Responses
        && !(profile == Profile::Chat
            && (chat_reasoning_shape(items)
                || structured_chat && structured_chat_reasoning_shape(items)))
    {
        return Err(RepresentationError::Reasoning);
    }
    if controls.encrypted_output() && profile != Profile::Responses {
        return Err(RepresentationError::Reasoning);
    }
    if profile == Profile::Chat && chat_control {
        return Err(RepresentationError::Reasoning);
    }
    Ok(())
}
pub(super) fn text_items(
    items: &[(ItemId, Item)],
    profile: Profile,
    request: bool,
    resources: &ResourceTable,
) -> Result<(), RepresentationError> {
    crate::protocol::openai::content::citations::check_items(items, resources, profile)
        .map_err(|_| RepresentationError::TextMetadata)?;
    for (_, i) in items {
        if !request
            && profile == Profile::Chat
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
        if profile == Profile::Chat {
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
                                        Profile::Chat,
                                    )
                                    .is_err()
                                })
                        }
                        ContentPart::Refusal(t) => request && !t.logprobs().is_absent(),
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
                        if profile == Profile::Responses && !t.logprobs().is_absent() {
                            return Err(RepresentationError::TextMetadata);
                        }
                        t.logprobs()
                    }
                    _ => continue,
                };
                if profile == Profile::Chat
                    && let Some(probs) = probs.value()
                {
                    crate::protocol::openai::chat::logprobs::validate(probs)
                        .map_err(|_| RepresentationError::TextMetadata)?;
                }
            }
            if profile == Profile::Responses
                && m.parts.iter().any(|p| {
                    matches!(&p.content, ContentPart::Text(t)
                        if t.logprobs().value().is_some_and(|v| !v.iter().all(complete_logprob)))
                })
            {
                return Err(RepresentationError::TextMetadata);
            }
            if m.parts.iter().any(|p| match &p.content {
                ContentPart::Text(_) | ContentPart::Refusal(_) => false,
                ContentPart::Audio(_) => profile != Profile::Chat,
                ContentPart::AudioReference(_) => profile != Profile::Chat || !request,
                ContentPart::Resource(resource) => {
                    let Ok(resource) = resource.media(resources) else {
                        return true;
                    };
                    if matches!(resource.description, ResourceDescription::File(file)
                        if !file.context.is_absent())
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
                                Profile::Responses => matches!(
                                    resource.location,
                                    ResourceLocation::OpaqueReference(_)
                                        | ResourceLocation::NamespacedReference { .. }
                                ),
                                Profile::Chat => {
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
                                profile != Profile::Chat
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
            if profile == Profile::Chat {
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
            if profile == Profile::Chat
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
/// Structured details carry several items and parts ahead of one carrier.
fn structured_chat_reasoning_shape(items: &[(ItemId, Item)]) -> bool {
    items.iter().enumerate().all(|(n, (_, item))| {
        let Item::Reasoning(r) = item else {
            return true;
        };
        let empty_text = r
            .parts
            .iter()
            .any(|(_, p)| matches!(p, ReasoningContent::Text(t) if t.as_str().is_empty()));
        let followed = match items.get(n + 1) {
            Some((_, Item::Reasoning(_) | Item::ToolCall(_) | Item::CustomCall(_))) => true,
            Some((_, Item::Message(m))) => m.role == MessageRole::Assistant,
            _ => false,
        };
        !empty_text && (!r.parts.is_empty() || r.replay.is_some()) && followed
    })
}
fn result_representable(result: &ToolResult, profile: Profile, resources: &ResourceTable) -> bool {
    let unsupported_media = match &result.output {
        ToolOutput::Parts(parts) => parts.iter().any(|part| {
            matches!(&part.content, InputPart::Resource(r)
            if r.media(resources).is_ok_and(|r| {
                matches!(r.kind(), ResourceKind::Audio | ResourceKind::Video)
                    || matches!(r.description, ResourceDescription::File(file)
                        if !file.context.is_absent())
            }))
        }),
        _ => false,
    };
    result.execution.is_none()
        && result.is_error.is_none()
        && !matches!(result.output, ToolOutput::Structured(_))
        && (profile != Profile::Chat || result.output.is_text_only())
        && !unsupported_media
}
