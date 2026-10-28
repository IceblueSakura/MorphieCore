//! Response fact requirements and projection before static or event delivery.
use super::{
    ReportedFactPolicy, RepresentationError, TargetContract,
    checks::{
        check_tool_selection, check_usage, represent_reasoning, text_items, validate_wire_ids,
    },
    message_groups, projection,
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
            (loss).effect(
                crate::protocol::conversion::generation::ConversionBoundary::ResponseProject,
            )
        })
        .collect();
    if profile == GenerationApi::Chat
        && let Some((view, effects)) = projection::chat_text_status(r, c.reported_facts)?
    {
        semantic = std::borrow::Cow::Owned(view);
        projection.extend(effects);
    }
    if profile == GenerationApi::Responses
        && let Some(rule) = projection::request_metadata_echo(&metadata, c.reported_facts)?
    {
        projection.push(
            rule.effect(
                crate::protocol::conversion::generation::ConversionBoundary::ResponseProject,
            ),
        );
    }
    if let Some(loss) = projection::reported_settings(metadata.to_mut(), profile)? {
        if c.reported_facts == ReportedFactPolicy::StrictComplete {
            return Err(RepresentationError::ReportedFacts);
        }
        projection.push(
            (loss).effect(
                crate::protocol::conversion::generation::ConversionBoundary::ResponseProject,
            ),
        );
    }
    crate::protocol::reports::relation(&metadata.context.reports, true)
        .map_err(|_| RepresentationError::Metadata)?;
    if let Some(loss) = projection::reported_service_class(metadata.to_mut(), profile) {
        if c.reported_facts == ReportedFactPolicy::StrictComplete {
            return Err(RepresentationError::ReportedFacts);
        }
        projection.push(
            loss.effect(
                crate::protocol::conversion::generation::ConversionBoundary::ResponseProject,
            ),
        );
    }
    if profile == GenerationApi::Responses
        && crate::protocol::reports::terminal_view(&mut metadata.to_mut().context.reports)
            .map_err(|_| RepresentationError::Metadata)?
    {
        if c.reported_facts == ReportedFactPolicy::StrictComplete {
            return Err(RepresentationError::ReportedFacts);
        }
        projection.push(
            (projection::ProjectionRule::OmitOpeningReport).effect(
                crate::protocol::conversion::generation::ConversionBoundary::ResponseProject,
            ),
        );
    }
    if profile == GenerationApi::Responses && !metadata.context.system_fingerprint.is_absent() {
        let observed = metadata.context.system_fingerprint.value().is_some();
        metadata.to_mut().context.system_fingerprint = crate::semantic::value::Presence::Absent;
        if observed {
            projection.push(
                (projection::ProjectionRule::OmitResponsesSystemFingerprint).effect(
                    crate::protocol::conversion::generation::ConversionBoundary::ResponseProject,
                ),
            );
        }
    }
    if let Some((groups, loss)) =
        message_groups::project(r.items(), r.message_envelopes(), profile)?
    {
        semantic = std::borrow::Cow::Owned(semantic.into_owned().with_message_envelopes(groups)?);
        projection.push(
            (loss).effect(
                crate::protocol::conversion::generation::ConversionBoundary::ResponseProject,
            ),
        );
    }
    if r.usage_reports().has_unrepresented_details() {
        if c.reported_facts == ReportedFactPolicy::StrictComplete {
            return Err(RepresentationError::ReportedFacts);
        }
        semantic = std::borrow::Cow::Owned(semantic.into_owned().with_usage(r.usage())?);
        projection.push(
            (projection::ProjectionRule::UsageReportView).effect(
                crate::protocol::conversion::generation::ConversionBoundary::ResponseProject,
            ),
        );
    }
    if let Some(usage) = r.usage() {
        let (projected, losses) = match profile {
            GenerationApi::Chat => (usage, vec![]),
            GenerationApi::Responses => projection::responses_usage(usage, &c.adaptation.rules)?,
        };
        if !losses.is_empty() {
            semantic = std::borrow::Cow::Owned(semantic.into_owned().with_usage(Some(projected))?);
            projection.extend(losses.into_iter().map(|loss| {
                (loss).effect(
                    crate::protocol::conversion::generation::ConversionBoundary::ResponseProject,
                )
            }));
        }
    }
    if !semantic.details().refusal.is_absent() || !semantic.details().explanation.is_absent() {
        let mut details = semantic.details().clone();
        details.refusal = crate::semantic::value::Presence::Absent;
        details.explanation = crate::semantic::value::Presence::Absent;
        semantic = std::borrow::Cow::Owned(semantic.into_owned().with_details(details)?);
    }
    validate_response(&semantic, fidelity, &metadata, profile, &c)?;

    Ok(ResponseRepresentation {
        semantic,
        fidelity,
        metadata,
        profile,
        adaptation: c.adaptation,
        projection_steps: projection.len().max(1),
        projection,
    })
}

fn validate_response(
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
    if let Some(class) = metadata.context.service_class().value() {
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
