//! Request admission and candidate-local projection before OpenAI wire encoding.
use super::{
    RepresentationError, TargetCaps,
    checks::{check_tool_selection, represent_reasoning, text_items, validate_wire_ids},
    message_groups, projection,
};
use crate::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{Profile, RequestRepresentation},
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
    if profile == Profile::Chat
        && let Some(projected) = crate::protocol::phase::omit_request(r)?
    {
        semantic = std::borrow::Cow::Owned(projected);
        let mut stage = projection::ProjectionStage::response(
            profile,
            c.adaptation.profile_id,
            Some(projection::LossRule::OmitChatMessagePhase),
        );
        stage.direction = projection::ProjectionDirection::Request;
        projection.push(stage);
    }
    if let Some((groups, loss)) =
        message_groups::project(semantic.items(), semantic.message_envelopes(), profile)?
    {
        // Replay dependencies must be checked against the final target value below.
        semantic = std::borrow::Cow::Owned(semantic.into_owned().with_message_envelopes(groups)?);
        let mut stage =
            projection::ProjectionStage::response(profile, c.adaptation.profile_id, Some(loss));
        stage.direction = projection::ProjectionDirection::Request;
        projection.push(stage);
    }
    if semantic.settings().prediction.is_some() && (profile != Profile::Chat || !c.prediction) {
        let mut settings = semantic.settings().clone();
        settings.prediction = None;
        semantic = std::borrow::Cow::Owned(semantic.into_owned().with_settings(settings)?);
        let mut stage = projection::ProjectionStage::response(
            profile,
            c.adaptation.profile_id,
            Some(projection::LossRule::OmitPrediction),
        );
        stage.direction = projection::ProjectionDirection::Request;
        projection.push(stage);
    }
    let r = semantic.as_ref();
    let q = check(r, c.clone())?;
    if r.controls().top_k.is_some()
        && !(profile == Profile::Chat && c.adaptation.rules.thinking_controls)
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
    if crate::protocol::image_constraints::rejects_downsize(r.items()) {
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
        crate::protocol::openai::responses::request::validate_program_history(r)
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
        if item
            .cache_boundaries()
            .any(|cache| profile == Profile::Chat || matches!(cache, PrefixCacheBoundary::Ttl(_)))
        {
            return Err(RepresentationError::Controls);
        }
        if let Item::Message(m) = item
            && m.parts.iter().any(|p| {
                matches!(&p.content, ContentPart::Text(t)
                    if !t.is_plain()
                        && (m.role == MessageRole::User || p.cache.is_some()))
            })
        {
            return Err(RepresentationError::TextMetadata);
        }
    }
    validate_wire_ids(r.items(), fidelity, false)?;
    c.model_limits
        .check_request(r)
        .map_err(|_| RepresentationError::Controls)?;
    Ok(RequestRepresentation {
        adaptation: c.adaptation,
        semantic,
        fidelity,
        profile,
        projection,
    })
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
        if tool.strict == FunctionStrictness::Explicit(true)
            && tool.parameters.as_ref().is_some_and(|p| !admits(p, true))
        {
            return Err(RepresentationError::Tools);
        }
    }
    Ok(())
}
