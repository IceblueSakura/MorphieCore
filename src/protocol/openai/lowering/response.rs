//! Response fact requirements and projection before static or event delivery.
use super::{
    ReportedFactPolicy, RepresentationError, TargetContract,
    checks::{
        check_tool_selection, check_usage, represent_reasoning, text_items, validate_wire_ids,
    },
};
use crate::{
    protocol::{
        ResponseMetadata,
        fidelity::FidelityRecords,
        openai::{GenerationApi, ResponseRepresentation},
    },
    semantic::task::generation::*,
};

/// A strict-complete target cannot deliver a response whose reported facts are
/// absent: the settings echo and, when usage is reported, its sub-details.
pub fn require_reported_facts(
    r: &GenerationResponse,
    metadata: &ResponseMetadata,
    c: &TargetContract,
) -> Result<(), RepresentationError> {
    if c.reported_facts != ReportedFactPolicy::StrictComplete {
        return Ok(());
    }
    let echo_complete = metadata.context.settings().is_some_and(|settings| {
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

pub fn lower_response<'a>(
    r: &'a GenerationResponse,
    fidelity: &'a FidelityRecords,
    metadata: &'a ResponseMetadata,
    profile: GenerationApi,
    c: TargetContract,
) -> Result<ResponseRepresentation<'a>, crate::protocol::ConversionFailure> {
    let client = crate::adapter::ClientAdapter::openai(
        crate::protocol::openai::GenerationCodec {
            protocol: profile,
            adaptation: c.adaptation.clone(),
        },
        c.clone(),
    );
    let decoded = crate::protocol::DecodedResponse {
        semantic: r.clone(),
        fidelity: fidelity.clone(),
        metadata: metadata.clone(),
        evidence: Default::default(),
    };
    let (projected, projection) = client.project_response(&decoded)?;
    Ok(ResponseRepresentation {
        semantic: std::borrow::Cow::Owned(projected.semantic),
        fidelity,
        metadata: std::borrow::Cow::Owned(projected.metadata),
        profile,
        adaptation: c.adaptation,
        projection_steps: projection.len().max(1),
        projection,
    })
}

pub(crate) fn validate_response(
    r: &GenerationResponse,
    fidelity: &FidelityRecords,
    metadata: &ResponseMetadata,
    profile: GenerationApi,
    c: &TargetContract,
) -> Result<(), RepresentationError> {
    if let Some(settings) = metadata.context.settings() {
        settings.validate()?;
        if crate::protocol::openai::reasoning::requires_extended_controls(settings) {
            return Err(RepresentationError::Reasoning);
        }
        check_tool_selection(settings.tool_choice.as_ref())?;
    }
    if r.progress() != InteractionProgress::Unreported {
        return Err(RepresentationError::InteractionProgress);
    }
    if metadata
        .context
        .settings()
        .is_some_and(|s| !s.audio.is_absent() || !s.output_modalities.is_absent())
    {
        return Err(RepresentationError::UnmigratedSemantic);
    }
    require_reported_facts(r, metadata, c)?;
    if profile == GenerationApi::Responses
        && metadata.context.settings().is_some_and(|s| {
            s.reasoning.presentation.visibility.value() == Some(&ReasoningDisplay::Omitted)
        })
    {
        return Err(RepresentationError::Reasoning);
    }
    if let Some(usage) = r.usage() {
        check_usage(usage, profile, &c.adaptation.rules)?;
    }
    if profile == GenerationApi::Chat
        && !c.adaptation.rules.readable_reasoning
        && r.items()
            .iter()
            .any(|(_, item)| matches!(item, Item::Reasoning(_)))
    {
        return Err(RepresentationError::Reasoning);
    }
    // Phase labels are Responses-only; reject instead of dropping the label.
    if profile == GenerationApi::Chat
        && r.items()
            .iter()
            .any(|(_, i)| matches!(i, Item::Message(m) if m.phase.is_some()))
    {
        return Err(RepresentationError::UnmigratedSemantic);
    }
    if r.items()
        .iter()
        .any(|(_, item)| matches!(item, Item::ProviderTool(_) | Item::ConfigurationUpdate(_)))
    {
        return Err(RepresentationError::UnmigratedSemantic);
    }
    c.dispositions.check_response(r)?;
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
    if profile == GenerationApi::Chat
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
    if (profile == GenerationApi::Responses && !r.message_envelopes().is_empty())
        || (profile == GenerationApi::Chat
            && !crate::protocol::openai::chat::owners::contiguous(r.items(), r.message_envelopes()))
    {
        return Err(RepresentationError::MessageGrouping);
    }
    text_items(r.items(), profile, false, r.resources())?;
    let owners = crate::protocol::openai::chat::owners::owners(r.items(), r.message_envelopes());
    if profile == GenerationApi::Chat && chat_message_count(r.items(), &owners) != 1 {
        return Err(RepresentationError::MessageGrouping);
    }
    if metadata.validate().is_err() {
        return Err(RepresentationError::Metadata);
    }
    if profile == GenerationApi::Chat
        && (matches!(r.outcome(), Outcome::Failed | Outcome::Cancelled)
            || r.details().error.is_some()
            || !crate::protocol::openai::terminal::chat_incomplete(
                r.outcome() == Outcome::Incomplete,
                r.details(),
            ))
    {
        return Err(RepresentationError::Terminal);
    }
    if let Some(class) = metadata.context.service_class().as_ref() {
        crate::protocol::openai::service::class_wire(*class, profile)
            .map_err(|_| RepresentationError::Metadata)?;
    }
    if profile == GenerationApi::Chat
        && crate::protocol::openai::chat::envelope::validate_context(&metadata.context.execution)
            .is_err()
    {
        return Err(RepresentationError::Metadata);
    }
    if metadata
        .created
        .as_ref()
        .is_none_or(|created| profile == GenerationApi::Chat && created.as_u64().is_none())
    {
        return Err(RepresentationError::Metadata);
    }
    validate_wire_ids(r.items(), fidelity, true)?;
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
