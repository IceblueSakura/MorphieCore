//! Request admission and candidate-local projection before OpenAI wire encoding.
use super::{
    RepresentationError, TargetContract,
    checks::{check_tool_selection, represent_reasoning, text_items, validate_wire_ids},
};
use crate::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{GenerationApi, RequestRepresentation},
    },
    semantic::task::generation::*,
};

pub fn lower_request<'a>(
    request: &'a GenerationRequest,
    fidelity: &'a FidelityRecords,
    profile: GenerationApi,
    contract: TargetContract,
) -> Result<RequestRepresentation<'a>, crate::protocol::ConversionFailure> {
    let projection = crate::lowering::request::project_semantic(
        request,
        crate::lowering::request::Target::OpenAi(profile, &contract),
        "",
    )?;
    let mut rendered = render_request(projection.value, fidelity, profile, contract)?;
    rendered
        .projection
        .extend(projection.evidence.into_effects());
    Ok(rendered)
}

pub(crate) fn render_request<'a>(
    semantic: std::borrow::Cow<'a, GenerationRequest>,
    fidelity: &'a FidelityRecords,
    profile: GenerationApi,
    c: TargetContract,
) -> Result<RequestRepresentation<'a>, RepresentationError> {
    let r = semantic.as_ref();
    r.validate()?;
    if r.items()
        .iter()
        .any(|(_, item)| item.has_replay_attachment())
    {
        return Err(RepresentationError::ReplayFormat);
    }
    let projection = Vec::new();
    let q = r.features();
    if r.controls().top_k.is_some()
        && !(profile == GenerationApi::Chat && c.adaptation.rules.thinking_controls)
        || profile == GenerationApi::Responses && r.controls().has_extended_controls()
        || profile == GenerationApi::Chat
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
    if q.contains_key(&Feature::AutomaticPrefixCache) || q.contains_key(&Feature::CachedContext) {
        return Err(RepresentationError::Controls);
    }
    if q.contains_key(&Feature::Safety) {
        return Err(RepresentationError::Safety);
    }
    if q.contains_key(&Feature::ToolResultError) {
        return Err(RepresentationError::Tools);
    }
    check_strict_schemas(r.settings())?;
    if q.contains_key(&Feature::ProviderObservation) {
        return Err(RepresentationError::UnmigratedSemantic);
    }
    check_tool_selection(r.tool_choice())?;
    if profile != GenerationApi::Chat
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
    if profile == GenerationApi::Chat
        && q.contains_key(&Feature::Reasoning)
        && !c.adaptation.rules.readable_reasoning
        && r.items()
            .iter()
            .any(|(_, item)| matches!(item, Item::Reasoning(_)))
    {
        return Err(RepresentationError::Reasoning);
    }
    if profile == GenerationApi::Chat
        && r.items()
            .iter()
            .any(|(_, i)| i.lifecycle().is_some_and(|s| s != ItemLifecycle::Completed))
    {
        return Err(RepresentationError::Terminal);
    }
    if profile == GenerationApi::Chat && q.contains_key(&Feature::Truncation) {
        return Err(RepresentationError::UnmigratedSemantic);
    }
    if profile == GenerationApi::Chat
        && r.items()
            .iter()
            .any(|(_, i)| matches!(i, Item::Message(m) if m.phase.is_some()))
    {
        // Phase labels are Responses-only; reject instead of dropping the label.
        return Err(RepresentationError::UnmigratedSemantic);
    }
    if profile == GenerationApi::Chat
        && r.controls().top_logprobs.is_some()
        && r.controls().logprobs != Some(true)
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
        GenerationApi::Chat => c.adaptation.rules.chat_video_url,
        GenerationApi::Responses => c.adaptation.rules.responses_input_video,
    };
    if video && !video_carrier {
        return Err(RepresentationError::UnmigratedSemantic);
    }
    for (_, item) in r.items() {
        if let Item::Message(message) = item {
            for part in &message.parts {
                if let ContentPart::Resource(resource) = &part.content
                    && profile == GenerationApi::Chat
                    && resource.media(r.resources())?.image_detail() == Some(ImageDetail::Original)
                    && !c.adaptation.rules.chat_original_image_detail
                {
                    return Err(RepresentationError::ImageInput);
                }
            }
        }
    }
    if profile == GenerationApi::Responses {
        crate::protocol::openai::responses::request::validate_program_history(r)
            .map_err(|_| RepresentationError::Tools)?;
    }
    if (profile == GenerationApi::Responses && !r.message_envelopes().is_empty())
        || (profile == GenerationApi::Chat
            && !crate::protocol::openai::chat::owners::contiguous(r.items(), r.message_envelopes()))
    {
        return Err(RepresentationError::MessageGrouping);
    }
    text_items(r.items(), profile, true, r.resources())?;
    if profile == GenerationApi::Chat
        && r.tools()
            .iter()
            .any(|t| matches!(t, ToolDefinition::Namespace(_)))
    {
        return Err(RepresentationError::Tools);
    }
    for tool in r.tools().iter().flat_map(ToolDefinition::leaves) {
        if !tool.dispatch_inactive() && profile == GenerationApi::Chat {
            return Err(RepresentationError::Tools);
        }
        if let ToolDefinition::Function(t) = tool {
            if profile == GenerationApi::Chat
                && !crate::protocol::openai::function_tools::valid_chat_function_name(
                    t.name.as_str(),
                )
            {
                return Err(RepresentationError::Tools);
            }
            if t.cache.is_some() {
                return Err(RepresentationError::Controls);
            }
            if profile == GenerationApi::Chat && t.output_schema.is_some() {
                return Err(RepresentationError::Tools);
            }
        }
    }
    for (_, item) in r.items() {
        if item.cache_boundaries().any(|cache| {
            profile == GenerationApi::Chat || matches!(cache, PrefixCacheBoundary::Ttl(_))
        }) {
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
        && !admits(schema)
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
            && tool.parameters.as_ref().is_some_and(|p| !admits(p))
        {
            return Err(RepresentationError::Tools);
        }
    }
    Ok(())
}
