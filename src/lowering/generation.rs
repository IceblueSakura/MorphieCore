//! Candidate-local representability over immutable final IR. No route or credential access.
use crate::{
    protocol::{
        ResponseMetadata,
        fidelity::FidelityRecords,
        openai::{Profile, RequestRepresentation, ResponseRepresentation},
    },
    semantic::task::generation::*,
};
use std::collections::BTreeSet;

/// Delivery policy for reported response facts (ADR 0008). Absence is a
/// reported fact; a target that demands the SDK-strict complete form fails
/// lowering instead of omitting the fact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportedFactPolicy {
    /// Absent facts stay absent and are never filled or synthesized.
    Faithful,
    /// The target requires complete reported facts; absence is a
    /// representability failure, not an omission.
    StrictComplete,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationRepresentationContract {
    pub adaptation: crate::protocol::adaptation::Adaptation,
    pub replay_origin: Option<crate::semantic::value::ReplayOrigin>,
    /// Caller-supplied clock for checking a reported remote-resource expiry.
    pub resource_time: Option<u64>,
    /// Delivery policy for reported facts; `Faithful` unless a consumer
    /// explicitly demands the strict complete form.
    pub reported_facts: ReportedFactPolicy,
    pub semantics: GenerationSemanticContract,
    pub images: crate::protocol::image_constraints::ImageConstraints,
    pub files: crate::protocol::file_constraints::FileConstraints,
    pub cache: crate::protocol::cache::CacheProjection,
    /// Identity/safety acceptance is independent of optional cache hints.
    pub identity_hints: bool,
    /// Target admits metadata/service-tier/tool-budget context fields.
    pub standard_context: bool,
}
impl GenerationRepresentationContract {
    pub fn full() -> Self {
        Self {
            adaptation: Default::default(),
            replay_origin: None,
            resource_time: None,
            reported_facts: ReportedFactPolicy::Faithful,
            semantics: GenerationSemanticContract::full(),
            images: crate::protocol::image_constraints::ImageConstraints::all(),
            files: crate::protocol::file_constraints::FileConstraints::all(),
            cache: crate::protocol::cache::CacheProjection::all(),
            identity_hints: true,
            standard_context: true,
        }
    }
}
pub fn check(
    r: &GenerationRequest,
    c: GenerationRepresentationContract,
) -> Result<GenerationRequirements, RepresentationError> {
    r.validate()?;
    c.semantics.check(r).map_err(RepresentationError::Admission)
}
pub fn lower_request<'a>(
    r: &'a GenerationRequest,
    fidelity: &'a FidelityRecords,
    profile: Profile,
    c: GenerationRepresentationContract,
) -> Result<RequestRepresentation<'a>, RepresentationError> {
    let q = check(r, c.clone())?;
    check_tool_selection(r.tool_choice())?;
    if profile != Profile::Chat
        && (!r.settings().audio.is_absent() || !r.settings().output_modalities.is_absent())
    {
        return Err(RepresentationError::UnmigratedSemantic);
    }
    for (_, item) in r.items() {
        if let Item::Message(m) = item {
            for p in &m.parts {
                if matches!(
                    p.content,
                    ContentPart::Audio(_) | ContentPart::AudioReference(_)
                ) {
                    fidelity
                        .check_audio(p.id, &p.content, c.replay_origin.as_ref(), c.resource_time)
                        .map_err(|_| RepresentationError::ReplayOrigin)?;
                }
            }
        }
    }
    if profile == Profile::Chat
        && q.reasoning
        && !c.adaptation.rules.readable_reasoning
        && r.items()
            .iter()
            .any(|(_, item)| matches!(item, Item::Reasoning(_)))
    {
        return Err(RepresentationError::Reasoning);
    }
    if profile == Profile::Chat
        && r.items()
            .iter()
            .any(|(_, i)| i.lifecycle().is_some_and(|s| s != ItemLifecycle::Completed))
    {
        return Err(RepresentationError::Terminal);
    }
    if profile == Profile::Chat && (q.truncation || !r.text_options().verbosity.is_absent()) {
        return Err(RepresentationError::UnmigratedSemantic);
    }
    if profile == Profile::Chat
        && r.items()
            .iter()
            .any(|(_, i)| matches!(i, Item::Message(m) if m.phase.is_some()))
    {
        // Phase labels are Responses-only; reject instead of dropping the label.
        return Err(RepresentationError::UnmigratedSemantic);
    }
    if profile == Profile::Chat
        && r.controls().top_logprobs.is_some()
        && r.controls().logprobs != crate::semantic::value::Presence::Value(true)
    {
        return Err(RepresentationError::Controls);
    }
    represent_reasoning(
        r.reasoning(),
        r.items(),
        fidelity,
        profile,
        c.replay_origin.as_ref(),
        &c.adaptation,
        Some(r),
    )?;
    c.images
        .check(r)
        .map_err(|_| RepresentationError::ImageInput)?;
    c.files
        .check(r)
        .map_err(|_| RepresentationError::FileInput)?;
    for (_, item) in r.items() {
        if let Item::Message(message) = item {
            for part in &message.parts {
                if let ContentPart::Resource(resource) = &part.content
                    && profile == Profile::Chat
                    && resource.image_detail() == Some(ImageDetail::Original)
                    && !c.adaptation.rules.chat_original_image_detail
                {
                    return Err(RepresentationError::ImageInput);
                }
            }
        }
    }
    if profile == Profile::Responses {
        crate::protocol::openai::responses::validate_program_history(r)
            .map_err(|_| RepresentationError::Tools)?;
    }
    text_items(r.items(), profile, true)?;
    let expected_default = if profile == Profile::Chat {
        StrictDefault::NonStrict
    } else {
        StrictDefault::NormalizeSchema
    };
    if profile == Profile::Chat
        && r.tools()
            .iter()
            .any(|t| matches!(t, ToolDefinition::Namespace(_)))
    {
        return Err(RepresentationError::Tools);
    }
    for tool in r.tools().iter().flat_map(ToolDefinition::leaves) {
        if !tool.dispatch_inactive() && profile == Profile::Chat {
            return Err(RepresentationError::Tools);
        }
        if let ToolDefinition::Function(t) = tool {
            if matches!(t.strict, FunctionStrictness::Omitted(d) if d != expected_default) {
                return Err(RepresentationError::StrictDefault);
            }
            if profile == Profile::Chat && t.output_schema.is_some() {
                return Err(RepresentationError::Tools);
            }
        } else if profile == Profile::Chat {
            return Err(RepresentationError::Tools);
        }
    }
    if profile == Profile::Chat
        && (matches!(
            r.tool_choice(),
            Some(ToolChoice::Custom(_) | ToolChoice::Qualified(_))
        ) || matches!(r.tool_choice(), Some(ToolChoice::Allowed { tools, .. }) if tools.iter().any(|r| r.kind != ToolKind::Function || r.namespace.is_some())))
    {
        return Err(RepresentationError::Tools);
    }
    for (_, item) in r.items() {
        let parts: Vec<_> = match item {
            Item::Instruction(i) => i.parts.iter().map(|(id, _)| *id).collect(),
            Item::Message(m) => m.parts.iter().map(|p| p.id).collect(),
            Item::ToolResult(r) | Item::CustomResult(r) => match &r.output {
                ToolOutput::Parts(p) => p.iter().map(|(id, _)| *id).collect(),
                _ => vec![],
            },
            _ => vec![],
        };
        if profile == Profile::Chat && parts.iter().any(|id| fidelity.cache_breakpoint(*id)) {
            return Err(RepresentationError::Controls);
        }
        if matches!(item, Item::ToolResult(result) | Item::CustomResult(result)
            if matches!(&result.output, ToolOutput::Parts(parts)
                if parts.iter().any(|(id, part)| matches!(part, ToolResultPart::Resource(_)) && fidelity.cache_breakpoint(*id))))
        {
            // Image breakpoints need an explicit image-carrier admission;
            // the text-only breakpoint path cannot silently omit one.
            return Err(RepresentationError::Controls);
        }
        if let Item::Message(m)=item && m.parts.iter().any(|p|matches!(&p.content,ContentPart::Text(t) if !t.is_plain() && (m.role==MessageRole::User || fidelity.cache_breakpoint(p.id)))){return Err(RepresentationError::TextMetadata);}
    }
    validate_wire_ids(r.items(), fidelity, false)?;
    Ok(RequestRepresentation {
        adaptation: c.adaptation,
        semantic: r,
        fidelity,
        profile,
    })
}
/// A strict-complete target cannot deliver a response whose reported facts are
/// absent: the settings echo and, when usage is reported, its sub-details.
pub fn require_reported_facts(
    r: &GenerationResponse,
    metadata: &ResponseMetadata,
    c: &GenerationRepresentationContract,
) -> Result<(), RepresentationError> {
    if c.reported_facts != ReportedFactPolicy::StrictComplete {
        return Ok(());
    }
    let echo_complete = metadata.context.settings.as_ref().is_some_and(|settings| {
        settings.tools.is_some()
            && settings.tool_choice.is_some()
            && settings.parallel_tool_calls.is_some()
    });
    let usage_complete = match r.usage() {
        None => true,
        Some(usage) => {
            usage.cached_input_tokens.is_some()
                && usage.input_cache_write_tokens.is_some()
                && usage.reasoning_tokens.is_some()
        }
    };
    if echo_complete && usage_complete {
        Ok(())
    } else {
        Err(RepresentationError::ReportedFacts)
    }
}

/// Fixed Responses has no text/prediction slots; explicit profiles may own text slots.
/// Even zero is reported: never omit it or invent undeclared prediction positions.
pub(super) fn check_usage(
    usage: Usage,
    profile: Profile,
    rules: &crate::protocol::adaptation::WireRules,
) -> Result<(), RepresentationError> {
    usage.validate()?;
    if usage.scope != UsageScope::Operation
        || usage.basis != UsageBasis::Final
        || usage.output_relation != OutputTokenRelation::IncludesReasoning
        || usage.total_relation != TotalTokenRelation::InputAndOutput
        || usage.input_tokens.is_none()
        || usage.output_tokens.is_none()
        || usage.total_tokens.is_none()
    {
        return Err(RepresentationError::UsageProjection);
    }
    if usage.input_image_tokens.is_some()
        && !(match profile {
            Profile::Chat => rules.chat_image_usage,
            Profile::Responses => rules.responses_image_usage,
        })
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

pub fn lower_response<'a>(
    r: &'a GenerationResponse,
    fidelity: &'a FidelityRecords,
    metadata: &'a ResponseMetadata,
    profile: Profile,
    c: GenerationRepresentationContract,
) -> Result<ResponseRepresentation<'a>, RepresentationError> {
    if let Some(settings) = &metadata.context.settings {
        check_tool_selection(settings.tool_choice.as_ref())?;
    }
    if r.progress() != InteractionProgress::Unreported {
        return Err(RepresentationError::InteractionProgress);
    }
    if r.usage_reports().len() > 1
        || r.usage_reports().iter().any(|report| {
            report.scope != UsageScope::Operation || report.basis != UsageBasis::Final
        })
    {
        return Err(RepresentationError::UsageProjection);
    }
    if metadata
        .context
        .settings
        .as_ref()
        .is_some_and(|s| !s.audio.is_absent() || !s.output_modalities.is_absent())
    {
        return Err(RepresentationError::UnmigratedSemantic);
    }
    require_reported_facts(r, metadata, &c)?;
    if profile == Profile::Responses
        && metadata
            .context
            .settings
            .as_ref()
            .is_some_and(|s| s.reasoning.summary() == Some(ReasoningSummary::Disabled))
    {
        return Err(RepresentationError::Reasoning);
    }
    if let Some(usage) = r.usage() {
        check_usage(usage, profile, &c.adaptation.rules)?;
    }
    if profile == Profile::Chat
        && !c.adaptation.rules.readable_reasoning
        && r.items()
            .iter()
            .any(|(_, item)| matches!(item, Item::Reasoning(_)))
    {
        return Err(RepresentationError::Reasoning);
    }
    // `system_fingerprint` is a Chat reported fact with no Responses wire position.
    if profile == Profile::Responses && !metadata.context.system_fingerprint.is_absent() {
        return Err(RepresentationError::UnmigratedSemantic);
    }
    // Phase labels are Responses-only; reject instead of dropping the label.
    if profile == Profile::Chat
        && r.items()
            .iter()
            .any(|(_, i)| matches!(i, Item::Message(m) if m.phase.is_some()))
    {
        return Err(RepresentationError::UnmigratedSemantic);
    }
    c.semantics.check_response(r)?;
    represent_reasoning(
        &ReasoningRequest::absent(),
        r.items(),
        fidelity,
        profile,
        c.replay_origin.as_ref(),
        &c.adaptation,
        None,
    )?;
    let chat_status = if r.outcome() == Outcome::Incomplete {
        ItemLifecycle::Incomplete
    } else {
        ItemLifecycle::Completed
    };
    if profile == Profile::Chat
        && r.items().iter().any(|(_, item)| {
            let completed_replay_owner = c.adaptation.rules.structured_chat_reasoning
                && chat_status == ItemLifecycle::Incomplete
                && matches!(item, Item::Reasoning(r) if r.status == ItemLifecycle::Completed
                    && r.replay.as_ref().and_then(ReplayValue::replay_token).is_some());
            item.lifecycle().is_some_and(|status| status != chat_status) && !completed_replay_owner
        })
    {
        return Err(RepresentationError::Terminal);
    }
    text_items(r.items(), profile, false)?;
    if profile == Profile::Chat && chat_message_count(r.items()) != 1 {
        return Err(RepresentationError::MessageGrouping);
    }
    if metadata.id.is_empty()
        || metadata.model.is_empty()
        || metadata.id.len() > 256
        || metadata.model.len() > 256
    {
        return Err(RepresentationError::Metadata);
    }
    if profile == Profile::Chat
        && (matches!(r.outcome(), Outcome::Failed | Outcome::Cancelled)
            || r.outcome() == Outcome::Incomplete
                && !matches!(
                    r.details().incomplete,
                    Some(IncompleteReason::MaxOutputTokens | IncompleteReason::ContentFilter)
                )
            || r.details().error.is_some()
            || r.details().incomplete.as_ref().is_some_and(|reason| {
                !matches!(
                    reason,
                    IncompleteReason::MaxOutputTokens | IncompleteReason::ContentFilter
                )
            }))
    {
        return Err(RepresentationError::Terminal);
    }
    if profile == Profile::Chat
        && crate::protocol::openai::chat_envelope::validate_context(&metadata.context.execution)
            .is_err()
    {
        return Err(RepresentationError::Metadata);
    }
    if metadata.context.validate().is_err()
        || !crate::semantic::value::valid_timestamp(&metadata.created)
        || profile == Profile::Chat && metadata.created.as_u64().is_none()
    {
        return Err(RepresentationError::Metadata);
    }
    validate_wire_ids(r.items(), fidelity, true)?;
    Ok(ResponseRepresentation {
        semantic: r,
        fidelity,
        metadata,
        profile,
        adaptation: c.adaptation,
    })
}
/// Chat can carry readable reasoning text on its carrier message: each reasoning
/// item must hold exactly one non-empty text part and sit immediately before the
/// message item that carries it.
fn chat_reasoning_shape(items: &[(ItemId, Item)]) -> bool {
    items
        .iter()
        .enumerate()
        .all(|(index, (_, item))| match item {
            Item::Reasoning(reasoning) => {
                matches!(
                    reasoning.parts.as_slice(),
                    [(_, ReasoningContent::Text(text))] if !text.as_str().is_empty()
                ) && matches!(items.get(index + 1), Some((_, Item::Message(message))) if message.role == MessageRole::Assistant)
            }
            _ => true,
        })
}
fn represent_reasoning(
    controls: &ReasoningRequest,
    items: &[(ItemId, Item)],
    fidelity: &FidelityRecords,
    profile: Profile,
    origin: Option<&crate::semantic::value::ReplayOrigin>,
    adaptation: &crate::protocol::adaptation::Adaptation,
    history: Option<&GenerationRequest>,
) -> Result<(), RepresentationError> {
    let structured_chat = adaptation.rules.structured_chat_reasoning;
    let request = history.is_some();
    if profile == Profile::Responses && controls.summary() == Some(ReasoningSummary::Disabled) {
        return Err(RepresentationError::Reasoning);
    }
    for (id, item) in items {
        if matches!(item, Item::Reasoning(r) if r.replay.as_ref().is_some_and(|value| value.format() != ReplayFormat::ResponsesEncrypted))
        {
            return Err(RepresentationError::ReplayFormat);
        }
        if (profile == Profile::Chat || request)
            && matches!(item, Item::Reasoning(r) if r.replay.is_some() && r.status != ItemLifecycle::Completed)
        {
            return Err(RepresentationError::Terminal);
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
    if profile == Profile::Chat && (!controls.context.is_absent() || !controls.mode.is_absent()) {
        return Err(RepresentationError::Reasoning);
    }
    let chat_control = controls.presence() == ReasoningPresence::Present
        && (controls.effort.is_absent() || !controls.summary.is_absent());
    if has_reasoning_items
        && profile != Profile::Responses
        && !(profile == Profile::Chat && (chat_reasoning_shape(items)
            || structured_chat && items.iter().enumerate().all(|(n,(_,item))| match item {
                Item::Reasoning(r) => r.parts.len() <= 1
                    && !r.parts.iter().any(|(_,p)|matches!(p,ReasoningContent::Text(t) if t.as_str().is_empty()))
                    && (!r.parts.is_empty() || r.replay.is_some())
                    && matches!(items.get(n+1),Some((_,Item::Message(m))) if m.role==MessageRole::Assistant),
                _ => true,
            })))
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
fn text_items(
    items: &[(ItemId, Item)],
    profile: Profile,
    request: bool,
) -> Result<(), RepresentationError> {
    for (_, i) in items {
        // These profiles have neither structured argument nor execution-report carriers.
        if matches!(i, Item::ToolCall(call) if !matches!(call.arguments, ToolArguments::Raw(_))) {
            return Err(RepresentationError::Tools);
        }
        // Standard Responses admits image result parts in history; Chat does not.
        if matches!(i, Item::ToolResult(result) | Item::CustomResult(result)
            if result.execution.is_some() || matches!(result.output, ToolOutput::Structured(_))
                || (!request || profile == Profile::Chat) && !result.output.is_text_only())
        {
            return Err(RepresentationError::Tools);
        }
        // Standard Responses has no message-call membership carrier. Keeping
        // both items is insufficient to preserve this relation through history.
        if profile == Profile::Responses
            && matches!(i, Item::ToolCall(call) if call.message.is_some())
        {
            return Err(RepresentationError::MessageGrouping);
        }
        if profile == Profile::Chat {
            match i {
                Item::CustomCall(c) if !c.context.is_direct() => {
                    return Err(RepresentationError::Tools);
                }
                Item::CustomResult(r) if !r.context.is_direct() => {
                    return Err(RepresentationError::Tools);
                }
                Item::CustomCall(_) | Item::CustomResult(_) => {
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
                                || t.annotations()
                                    .iter()
                                    .any(|a| !matches!(a, Annotation::UrlCitation { .. }))
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
                    crate::protocol::openai::chat_logprobs::validate(probs)
                        .map_err(|_| RepresentationError::TextMetadata)?;
                }
            }
            if profile==Profile::Responses && m.parts.iter().any(|p|matches!(&p.content,ContentPart::Text(t) if t.logprobs().value().is_some_and(|v|v.iter().any(|p|p.bytes.is_none() || p.top_logprobs.as_ref().is_none_or(|v|v.iter().any(|p|p.token.is_none()||p.logprob.is_none()||p.bytes.is_none())))))){return Err(RepresentationError::TextMetadata);}
            if m.parts.iter().any(|p| match &p.content {
                ContentPart::Text(_) | ContentPart::Refusal(_) => false,
                ContentPart::Audio(_) => profile != Profile::Chat,
                ContentPart::AudioReference(_) => profile != Profile::Chat || !request,
                ContentPart::Resource(resource) => {
                    !request
                        || match resource.kind() {
                            ResourceKind::Image => {
                                matches!(resource.location, ResourceLocation::OpaqueReference(_))
                            }
                            ResourceKind::File => {
                                profile != Profile::Responses
                                    || matches!(
                                        resource.location,
                                        ResourceLocation::OpaqueReference(_)
                                    )
                            }
                            ResourceKind::Audio => true,
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
                        !matches!(p.content, ContentPart::Text(_) | ContentPart::Resource(_))
                    }))
            {
                return Err(RepresentationError::MessageGrouping);
            }
        }
    }
    Ok(())
}
/// Explicit normalization: a contiguous run of independent function calls is one Chat call message.
/// Independent text messages are never merged into that run by positional guessing.
fn chat_message_count(items: &[(ItemId, Item)]) -> usize {
    let mut count = 0;
    let mut run = false;
    for (_, item) in items {
        match item {
            Item::ToolCall(c) if c.message.is_none() => {
                if !run {
                    count += 1;
                }
                run = true;
            }
            Item::ToolCall(_) => run = false,
            // Readable reasoning rides its carrier message and is validated as
            // part of that message's representable shape.
            Item::Reasoning(_) => {}
            _ => {
                count += 1;
                run = false;
            }
        }
    }
    count
}
fn validate_wire_ids(
    items: &[(ItemId, Item)],
    fidelity: &FidelityRecords,
    generate: bool,
) -> Result<(), RepresentationError> {
    let mut ids = BTreeSet::new();
    for (id, _) in items {
        let value = fidelity
            .response_item_id(*id)
            .map(str::to_owned)
            .or_else(|| generate.then(|| format!("item_{}", id.get())));
        if value.is_some_and(|value| !ids.insert(value)) {
            return Err(RepresentationError::Metadata);
        }
    }
    Ok(())
}
fn check_tool_selection(choice: Option<&ToolChoice>) -> Result<(), RepresentationError> {
    // The pinned standard named-tool types have no qualified-reference carrier.
    // https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/tool_choice_function_param.py
    if matches!(choice, Some(ToolChoice::Qualified(_)))
        || matches!(choice, Some(ToolChoice::Allowed { tools, .. }) if tools.iter().any(|r| r.namespace.is_some()))
    {
        return Err(RepresentationError::Tools);
    }
    Ok(())
}
#[derive(Clone, Debug, Eq, thiserror::Error, PartialEq)]
pub enum RepresentationError {
    #[error(transparent)]
    Admission(#[from] GenerationFeature),
    #[error(transparent)]
    Semantic(#[from] GenerationError),
    #[error(transparent)]
    Event(#[from] EventError),
    #[error("target cannot represent reported usage details")]
    UsageDetails,
    #[error("target requires complete reported facts")]
    ReportedFacts,
    #[error("target cannot represent text metadata")]
    TextMetadata,
    #[error("target has no reported interaction-progress carrier")]
    InteractionProgress,
    #[error("target cannot represent generation controls")]
    Controls,
    #[error("target cannot represent instructions")]
    Instructions,
    #[error("target cannot represent temperature")]
    Temperature,
    #[error("target cannot represent maximum output tokens")]
    MaxOutputTokens,
    #[error("target cannot represent tools")]
    Tools,
    #[error("target cannot represent parallel tools")]
    ParallelTools,
    #[error("target cannot represent strict tools")]
    StrictTools,
    #[error("omitted strict has a different default in the target profile")]
    StrictDefault,
    #[error("target cannot represent structured output")]
    StructuredOutput,
    #[error("target cannot represent reasoning")]
    Reasoning,
    #[error("target cannot represent this replay-value phase at item start")]
    ReplayPhase,
    #[error("target has no carrier for this replay format")]
    ReplayFormat,
    #[error("opaque replay requires a final token and a matching trusted origin")]
    ReplayOrigin,
    #[error("target cannot represent image input")]
    ImageInput,
    #[error("target cannot represent audio input")]
    AudioInput,
    #[error("target cannot represent file input")]
    FileInput,
    #[error("semantic domain has no migrated codec yet")]
    UnmigratedSemantic,
    #[error("message grouping cannot be preserved in this target")]
    MessageGrouping,
    #[error("target cannot represent the queued response lifecycle")]
    Lifecycle,
    #[error("terminal cannot be represented by the target profile")]
    Terminal,
    #[error("usage conversion is not part of this migration slice")]
    UsageProjection,
    #[error("invalid or conflicting response representation metadata")]
    Metadata,
}
