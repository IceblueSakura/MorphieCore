//! OpenAI request, response and stream representability over immutable IR.
pub mod events;
mod message_groups;
pub(crate) mod native_messages;
pub mod projection;
pub use crate::lowering::generation::{ReportedFactPolicy, RepresentationError};
pub use crate::protocol::openai::TargetCaps;
use crate::{
    protocol::{
        ResponseMetadata,
        fidelity::FidelityRecords,
        openai::{Profile, RequestRepresentation, ResponseRepresentation},
    },
    semantic::task::generation::*,
};
pub fn check(
    r: &GenerationRequest,
    c: TargetCaps,
) -> Result<GenerationRequirements, RepresentationError> {
    r.validate()?;
    c.semantics.check(r).map_err(RepresentationError::Admission)
}
pub fn lower_request<'a>(
    r: &'a GenerationRequest,
    fidelity: &'a FidelityRecords,
    profile: Profile,
    c: TargetCaps,
) -> Result<RequestRepresentation<'a>, RepresentationError> {
    r.validate()?;
    if r.items()
        .iter()
        .any(|(_, item)| item.has_replay_attachment())
    {
        return Err(RepresentationError::ReplayFormat);
    }
    let mut semantic = std::borrow::Cow::Borrowed(r);
    let mut projection = Vec::new();
    if let Some((groups, loss)) =
        message_groups::project(r.items(), r.message_envelopes(), profile)?
    {
        // Replay dependencies must be checked against the final target value below.
        semantic = std::borrow::Cow::Owned(r.clone().with_message_envelopes(groups)?);
        let mut stage =
            projection::ProjectionStage::response(profile, c.adaptation.profile_id, Some(loss));
        stage.direction = projection::ProjectionDirection::Request;
        projection.push(stage);
    }
    let r = semantic.as_ref();
    let q = check(r, c.clone())?;
    if r.controls().top_k.is_some()
        || profile == Profile::Responses && r.controls().has_extended_controls()
        || profile == Profile::Chat
            && (r.controls().stop_sequences.len() > 4
                || [
                    r.controls().presence_penalty(),
                    r.controls().frequency_penalty(),
                ]
                .into_iter()
                .flatten()
                .any(|v| !(-2.0..=2.0).contains(&v)))
    {
        return Err(RepresentationError::Controls);
    }
    if q.automatic_prefix_cache || q.cached_context {
        return Err(RepresentationError::Controls);
    }
    if q.safety_settings {
        return Err(RepresentationError::Safety);
    }
    if q.tool_result_errors {
        return Err(RepresentationError::Tools);
    }
    check_strict_schemas(r.settings())?;
    if q.provider_observations {
        return Err(RepresentationError::UnmigratedSemantic);
    }
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
    if profile == Profile::Chat && q.truncation {
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
        r.settings(),
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
    if fidelity.has_strict_image_sizes(r.items()) {
        return Err(RepresentationError::ImageInput);
    }
    c.files
        .check(r)
        .map_err(|_| RepresentationError::FileInput)?;
    // Video has no standard OpenAI carrier; only declared extensions receive it. A file
    // with a video MIME type is a file carrier, checked by the file constraints instead.
    let video = r
        .items()
        .iter()
        .flat_map(|(_, i)| i.resource_uses())
        .any(|u| {
            u.media(r.resources())
                .is_ok_and(|v| v.kind() == ResourceKind::Video)
        });
    let video_carrier = match profile {
        Profile::Chat => c.adaptation.rules.chat_video_url,
        Profile::Responses => c.adaptation.rules.responses_input_video,
    };
    if video && !video_carrier {
        return Err(RepresentationError::UnmigratedSemantic);
    }
    for (_, item) in r.items() {
        if let Item::Message(message) = item {
            for part in &message.parts {
                if let ContentPart::Resource(resource) = &part.content
                    && profile == Profile::Chat
                    && resource.media(r.resources())?.image_detail() == Some(ImageDetail::Original)
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
    if (profile == Profile::Responses && !r.message_envelopes().is_empty())
        || (profile == Profile::Chat
            && !crate::protocol::openai::chat::owners::contiguous(r.items(), r.message_envelopes()))
    {
        return Err(RepresentationError::MessageGrouping);
    }
    text_items(r.items(), profile, true, r.resources())?;
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
            if t.cache.is_some() {
                return Err(RepresentationError::Controls);
            }
            // Non-strict source defaults have an exact explicit-false carrier in
            // Responses. The inverse requires schema normalization, not omission.
            if profile == Profile::Chat
                && t.strict == FunctionStrictness::Omitted(StrictDefault::NormalizeSchema)
            {
                return Err(RepresentationError::StrictDefault);
            }
            if profile == Profile::Chat && t.output_schema.is_some() {
                return Err(RepresentationError::Tools);
            }
        }
    }
    for (_, item) in r.items() {
        if item.has_cache_breakpoints() {
            return Err(RepresentationError::Controls);
        }
        let parts: Vec<_> = match item {
            Item::Instruction(i) => i.parts.iter().map(|p| p.id).collect(),
            Item::Message(m) => m.parts.iter().map(|p| p.id).collect(),
            Item::ToolResult(r) | Item::CustomResult(r) => match &r.output {
                ToolOutput::Parts(p) => p.iter().map(|p| p.id).collect(),
                _ => vec![],
            },
            _ => vec![],
        };
        if profile == Profile::Chat && parts.iter().any(|id| fidelity.cache_breakpoint(*id)) {
            return Err(RepresentationError::Controls);
        }
        if let Item::Message(m) = item
            && m.parts.iter().any(|p| {
                matches!(&p.content, ContentPart::Text(t)
                    if !t.is_plain()
                        && (m.role == MessageRole::User || fidelity.cache_breakpoint(p.id)))
            })
        {
            return Err(RepresentationError::TextMetadata);
        }
    }
    validate_wire_ids(r.items(), fidelity, false)?;
    Ok(RequestRepresentation {
        adaptation: c.adaptation,
        semantic,
        fidelity,
        profile,
        projection,
    })
}
/// A strict-complete target cannot deliver a response whose reported facts are
/// absent: the settings echo and, when usage is reported, its sub-details.
pub fn require_reported_facts(
    r: &GenerationResponse,
    metadata: &ResponseMetadata,
    c: &TargetCaps,
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

pub fn lower_response<'a>(
    r: &'a GenerationResponse,
    fidelity: &'a FidelityRecords,
    metadata: &'a ResponseMetadata,
    profile: Profile,
    c: TargetCaps,
) -> Result<ResponseRepresentation<'a>, RepresentationError> {
    if r.items()
        .iter()
        .any(|(_, item)| item.has_replay_attachment())
    {
        return Err(RepresentationError::ReplayFormat);
    }
    if !r.safety_reports().is_empty() {
        return Err(RepresentationError::Safety);
    }
    let terminal_losses = crate::protocol::openai::terminal::terminal_losses(r.details(), profile)
        .map_err(|_| RepresentationError::Terminal)?;
    metadata
        .context
        .validate()
        .map_err(|_| RepresentationError::Metadata)?;
    let mut metadata = std::borrow::Cow::Borrowed(metadata);
    let mut semantic = std::borrow::Cow::Borrowed(r);
    let mut projection: Vec<_> = terminal_losses
        .into_iter()
        .map(|loss| {
            projection::ProjectionStage::response(profile, c.adaptation.profile_id, Some(loss))
        })
        .collect();
    if let Some(loss) = projection::reported_settings(metadata.to_mut(), profile)? {
        if c.reported_facts == ReportedFactPolicy::StrictComplete {
            return Err(RepresentationError::ReportedFacts);
        }
        projection.push(projection::ProjectionStage::response(
            profile,
            c.adaptation.profile_id,
            Some(loss),
        ));
    }
    if profile == Profile::Responses && !metadata.context.system_fingerprint.is_absent() {
        metadata.to_mut().context.system_fingerprint = crate::semantic::value::Presence::Absent;
        projection.push(projection::ProjectionStage::response(
            profile,
            c.adaptation.profile_id,
            Some(projection::LossRule::OmitResponsesSystemFingerprint),
        ));
    }
    if let Some((groups, loss)) =
        message_groups::project(r.items(), r.message_envelopes(), profile)?
    {
        semantic = std::borrow::Cow::Owned(r.clone().with_message_envelopes(groups)?);
        projection.push(projection::ProjectionStage::response(
            profile,
            c.adaptation.profile_id,
            Some(loss),
        ));
    }
    if r.usage_reports().has_details() {
        if c.reported_facts == ReportedFactPolicy::StrictComplete {
            return Err(RepresentationError::ReportedFacts);
        }
        semantic = std::borrow::Cow::Owned(semantic.into_owned().with_usage(r.usage())?);
        projection.push(projection::ProjectionStage::response(
            profile,
            c.adaptation.profile_id,
            Some(projection::LossRule::UsageReportView),
        ));
    }
    if let Some(usage) = r.usage() {
        let (projected, losses) = match profile {
            Profile::Chat => (usage, vec![]),
            Profile::Responses => projection::responses_usage(usage, &c.adaptation.rules)?,
        };
        if !losses.is_empty() {
            semantic = std::borrow::Cow::Owned(semantic.into_owned().with_usage(Some(projected))?);
            projection.extend(losses.into_iter().map(|loss| {
                projection::ProjectionStage::response(profile, c.adaptation.profile_id, Some(loss))
            }));
        }
    }
    validate_response(&semantic, fidelity, &metadata, profile, &c)?;
    if projection.is_empty() {
        projection.push(projection::ProjectionStage::response(
            profile,
            c.adaptation.profile_id,
            None,
        ));
    }
    Ok(ResponseRepresentation {
        semantic,
        fidelity,
        metadata,
        profile,
        adaptation: c.adaptation,
        projection,
        attachment: None,
    })
}

fn validate_response(
    r: &GenerationResponse,
    fidelity: &FidelityRecords,
    metadata: &ResponseMetadata,
    profile: Profile,
    c: &TargetCaps,
) -> Result<(), RepresentationError> {
    if let Some(settings) = &metadata.context.settings {
        settings.validate()?;
        if super::reasoning::requires_extended_controls(settings) {
            return Err(RepresentationError::Reasoning);
        }
        check_tool_selection(settings.tool_choice.as_ref())?;
    }
    if r.progress() != InteractionProgress::Unreported {
        return Err(RepresentationError::InteractionProgress);
    }
    if metadata
        .context
        .settings
        .as_ref()
        .is_some_and(|s| !s.audio.is_absent() || !s.output_modalities.is_absent())
    {
        return Err(RepresentationError::UnmigratedSemantic);
    }
    require_reported_facts(r, metadata, c)?;
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
        return Err(RepresentationError::SystemFingerprint);
    }
    // Phase labels are Responses-only; reject instead of dropping the label.
    if profile == Profile::Chat
        && r.items()
            .iter()
            .any(|(_, i)| matches!(i, Item::Message(m) if m.phase.is_some()))
    {
        return Err(RepresentationError::UnmigratedSemantic);
    }
    if r.items()
        .iter()
        .any(|(_, item)| matches!(item, Item::ProviderTool(_)))
    {
        return Err(RepresentationError::UnmigratedSemantic);
    }
    c.semantics.check_response(r)?;
    represent_reasoning(
        &GenerationSettings::default(),
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
    if (profile == Profile::Responses && !r.message_envelopes().is_empty())
        || (profile == Profile::Chat
            && !crate::protocol::openai::chat::owners::contiguous(r.items(), r.message_envelopes()))
    {
        return Err(RepresentationError::MessageGrouping);
    }
    text_items(r.items(), profile, false, r.resources())?;
    let owners = crate::protocol::openai::chat::owners::owners(r.items(), r.message_envelopes());
    if profile == Profile::Chat && chat_message_count(r.items(), &owners) != 1 {
        return Err(RepresentationError::MessageGrouping);
    }
    if metadata.validate().is_err() {
        return Err(RepresentationError::Metadata);
    }
    if profile == Profile::Chat
        && (matches!(r.outcome(), Outcome::Failed | Outcome::Cancelled)
            || r.details().error.is_some()
            || !crate::protocol::openai::terminal::chat_incomplete(
                r.outcome() == Outcome::Incomplete,
                r.details(),
            ))
    {
        return Err(RepresentationError::Terminal);
    }
    if let Some(class) = metadata.context.service_class.value() {
        crate::protocol::openai::service::class_wire(*class, profile)
            .map_err(|_| RepresentationError::Metadata)?;
    }
    if profile == Profile::Chat
        && crate::protocol::openai::chat::envelope::validate_context(&metadata.context.execution)
            .is_err()
    {
        return Err(RepresentationError::Metadata);
    }
    if metadata
        .created
        .as_ref()
        .is_none_or(|created| profile == Profile::Chat && created.as_u64().is_none())
    {
        return Err(RepresentationError::Metadata);
    }
    validate_wire_ids(r.items(), fidelity, true)?;
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
fn represent_reasoning(
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
    if super::reasoning::requires_extended_controls(settings) {
        return Err(RepresentationError::Reasoning);
    }
    if profile == Profile::Responses && controls.summary() == Some(ReasoningSummary::Disabled) {
        return Err(RepresentationError::Reasoning);
    }
    for (id, item) in items {
        if let Item::ConfigurationUpdate(update) = item
            && (profile != Profile::Responses || super::reasoning::effort_label(update).is_err())
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
    if profile == Profile::Chat && (!controls.context.is_absent() || !controls.mode.is_absent()) {
        return Err(RepresentationError::Reasoning);
    }
    let chat_control = controls.presence() == ReasoningPresence::Present
        && (settings.effort.is_absent() && controls.activation.is_absent()
            || !controls.summary.is_absent());
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
fn text_items(
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
/// Explicit normalization: a contiguous run of independent calls is one Chat call message.
/// Independent text messages are never merged into that run by positional guessing.
fn chat_message_count(
    items: &[(ItemId, Item)],
    owners: &std::collections::BTreeMap<ItemId, ItemId>,
) -> usize {
    let mut count = 0;
    let mut run = false;
    for (id, item) in items {
        match item {
            Item::ToolCall(_) | Item::CustomCall(_) if !owners.contains_key(id) => {
                if !run {
                    count += 1;
                }
                run = true;
            }
            Item::ToolCall(_) | Item::CustomCall(_) => run = false,
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
    fidelity
        .check_wire_item_ids(items, generate)
        .map_err(|_| RepresentationError::Metadata)
}
/// The IR admits any bounded schema; only an OpenAI request target checks its strict subset.
fn check_strict_schemas(settings: &GenerationSettings) -> Result<(), RepresentationError> {
    use crate::protocol::openai::strict_schema::admits;
    if let OutputConstraint::JsonSchema {
        schema,
        strict: Some(true),
        ..
    } = settings.output()
        && !admits(schema, true)
    {
        return Err(RepresentationError::StructuredOutput);
    }
    for tool in settings
        .tools
        .as_deref()
        .unwrap_or_default()
        .iter()
        .flat_map(ToolDefinition::leaves)
    {
        let ToolDefinition::Function(tool) = tool else {
            continue;
        };
        let closed = match tool.strict {
            FunctionStrictness::Explicit(true) => true,
            FunctionStrictness::Omitted(StrictDefault::NormalizeSchema) => false,
            _ => continue,
        };
        if tool.parameters.as_ref().is_some_and(|p| !admits(p, closed)) {
            return Err(RepresentationError::Tools);
        }
    }
    Ok(())
}
fn check_tool_selection(choice: Option<&ToolChoice>) -> Result<(), RepresentationError> {
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
    let audio_or_video = match &result.output {
        ToolOutput::Parts(parts) => parts.iter().any(|part| {
            matches!(&part.content, InputPart::Resource(r)
            if r.media(resources).is_ok_and(|r| {
                matches!(r.kind(), ResourceKind::Audio | ResourceKind::Video)
            }))
        }),
        _ => false,
    };
    result.execution.is_none()
        && result.is_error.is_none()
        && !matches!(result.output, ToolOutput::Structured(_))
        && (profile != Profile::Chat || result.output.is_text_only())
        && !audio_or_video
}
