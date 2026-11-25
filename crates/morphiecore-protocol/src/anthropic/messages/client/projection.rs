//! Client Messages delivery projection, named losses and JSON rendering.
use crate::{
    CodecError, DecodedResponse, anthropic::messages as wire, failure::CarrierReason,
    replay_envelope, semantic::task::generation::*,
};
use serde_json::Value;

/// The caller supplies the admitted/effective no-stop premise. Source reports
/// remain unchanged; the Messages target derives both end-turn facts.
pub fn completed_without_stops(
    source: &GenerationResponse,
    direction: crate::conversion::generation::ConversionBoundary,
) -> Result<Option<(TerminalDetails, crate::conversion::generation::Effect)>, CodecError> {
    source.details().validate(source.outcome())?;
    if source.outcome() != Outcome::Completed
        || !matches!(
            source.progress(),
            InteractionProgress::Unreported | InteractionProgress::TurnFinished
        )
        || source
            .items()
            .iter()
            .any(|(_, item)| !matches!(item, Item::Message(_) | Item::Reasoning(_)))
        || !matches!(
            source.details().completion,
            None | Some(CompletionCause::Natural | CompletionCause::NaturalOrStopSequence)
        )
        || source.progress() == InteractionProgress::TurnFinished
            && source.details().completion == Some(CompletionCause::Natural)
    {
        return Ok(None);
    }
    let mut details = source.details().clone();
    details.completion = Some(CompletionCause::Natural);
    Ok(Some((
        details,
        crate::conversion::generation::Effect::Field {
            rule: "messages.infer-end-turn-from-request.v1".into(),
            field: crate::conversion::generation::Field::Completion,
            boundary: direction,
            change: crate::conversion::generation::Change::InferFromRequest,
            owner: crate::conversion::generation::Owner::Root,
        },
    )))
}

/// Client values are sealed envelopes; the format tag keeps a redacted block's type.
pub fn redacted(sealed: &str) -> bool {
    matches!(
        replay_envelope::open(sealed),
        Some((_, ReplayFormat::AnthropicMessagesRedactedThinking, _))
    )
}

/// Signed thinking is one unchanged summary or a redacted block. Unsigned
/// reasoning has no standard history-safe Messages carrier.
pub(super) fn project_reasoning(reasoning: &ReasoningItem) -> Result<&ReasoningItem, CodecError> {
    let signature = reasoning
        .replay
        .as_ref()
        .filter(|r| !r.as_str().is_empty())
        .map(|r| redacted(r.as_str()));
    let summary = matches!(
        reasoning.parts.as_slice(),
        [(_, ReasoningContent::Summary(_))]
    );
    let nonempty = matches!(reasoning.parts.as_slice(), [(_, ReasoningContent::Summary(t))] if !t.as_str().is_empty());
    if reasoning_carried(signature, reasoning.parts.len(), summary, nonempty) {
        Ok(reasoning)
    } else {
        Err(CodecError::Unsupported(CarrierReason::Replay))
    }
}

/// One carrier predicate for a completed owner in JSON and the bounded event queue.
pub(super) fn reasoning_carried(
    signature_redacted: Option<bool>,
    parts: usize,
    summary: bool,
    nonempty: bool,
) -> bool {
    signature_redacted.is_some()
        && (parts == 0 || parts == 1 && summary && nonempty && signature_redacted == Some(false))
}
pub(super) fn reasoning_omission(
    item: ItemId,
    boundary: crate::conversion::generation::ConversionBoundary,
) -> crate::conversion::generation::Effect {
    crate::lowering::disposition::OMIT_MESSAGES_REASONING
        .effect(boundary)
        .at(crate::conversion::generation::Owner::item(item))
}

pub fn validate_metadata(metadata: &crate::ResponseMetadata) -> Result<(), CodecError> {
    metadata.context.validate()?;
    if metadata
        .context
        .reports
        .iter()
        .any(|r| r.settings.as_ref().is_some_and(|s| !s.safety.is_empty()))
    {
        return Err(CodecError::Unsupported(CarrierReason::Safety));
    }
    Ok(())
}
pub fn project_metadata(metadata: &mut crate::ResponseMetadata) -> Result<(), CodecError> {
    validate_metadata(metadata)?;
    for report in metadata.context.reports.iter_mut() {
        if report
            .service_class
            .as_ref()
            .is_some_and(|class| wire::service::class_wire(*class).is_err())
        {
            report.service_class = None;
        }
    }
    crate::reports::single_view(&mut metadata.context.reports)?;
    Ok(())
}

pub fn metadata_losses(
    metadata: &crate::ResponseMetadata,
) -> Vec<crate::conversion::generation::Effect> {
    let mut losses = vec![];
    if metadata.context.reports.iter().any(|r| {
        r.service_class
            .as_ref()
            .is_some_and(|class| wire::service::class_wire(*class).is_err())
    }) {
        losses.push(crate::conversion::generation::Effect::Field {
            rule: crate::conversion::generation::RuleId::new(
                "messages.omit-reported-service-class.v1",
            ),
            field: crate::conversion::generation::Field::ServiceClass,
            boundary: crate::conversion::generation::ConversionBoundary::ResponseProject,
            change: crate::conversion::generation::Change::Omit,
            owner: crate::conversion::generation::Owner::Root,
        });
    }
    losses
}
pub(super) fn explanation_loss(
    direction: crate::conversion::generation::ConversionBoundary,
) -> crate::conversion::generation::Effect {
    crate::conversion::generation::Effect::Field {
        rule: crate::conversion::generation::RuleId::new("messages.omit-terminal-explanation.v1"),
        field: crate::conversion::generation::Field::TerminalExplanation,
        boundary: direction,
        change: crate::conversion::generation::Change::Omit,
        owner: crate::conversion::generation::Owner::Root,
    }
}
/// Shared by JSON and SSE delivery. Lost report detail/scope and a changed scalar usage
/// view are independent adaptations; a report without a selected view records only the former.
pub fn report_losses(
    source: &UsageReport,
    selected: Option<&UsageReport>,
    direction: crate::conversion::generation::ConversionBoundary,
) -> Vec<crate::conversion::generation::Effect> {
    if selected == Some(source) {
        return vec![];
    }
    let detailed = source.details != Default::default()
        || source.scope != UsageScope::Operation
        || source.accounting != crate::semantic::task::generation::UsageAccounting::Direct;
    let mut losses = Vec::new();
    if detailed {
        losses.push(crate::usage::report_loss(direction));
    }
    if !detailed || selected.is_some_and(|s| s.scalar_summary() != source.scalar_summary()) {
        losses.push(usage_loss().with_boundary(direction));
    }
    losses
}
pub fn usage_loss() -> crate::conversion::generation::Effect {
    crate::conversion::generation::Effect::Field {
        rule: crate::conversion::generation::RuleId::new("messages.usage-view.v1"),
        field: crate::conversion::generation::Field::Usage,
        boundary: crate::conversion::generation::ConversionBoundary::ResponseProject,
        change: crate::conversion::generation::Change::Normalize,
        owner: crate::conversion::generation::Owner::Root,
    }
}
/// Callers check labels: delivered IDs may be projected from the source call.
pub(super) fn tool_input(call: &ToolCall) -> Result<Value, CodecError> {
    if !call.context.is_direct() || call.context.alias_domain.is_some() {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    let input = match &call.arguments {
        ToolArguments::Structured(value) => value.clone(),
        ToolArguments::Raw(raw) if raw.is_empty() => StructuredValue::new(serde_json::json!({}))?,
        ToolArguments::Raw(raw) => StructuredValue::from_bytes(raw.as_bytes())?,
        ToolArguments::StructuredPartial(_) => {
            return Err(CodecError::Unsupported(CarrierReason::Tools));
        }
    };
    if !input.value().is_object() {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    Ok(input.value().clone())
}
/// Delivery must not publish identities that public Messages history cannot admit.
pub(super) fn tool_labels(id: &str, tool: &str) -> Result<(), CodecError> {
    wire::shape::name(id, 64)?;
    wire::shape::name(tool, 128)
}
/// Reuse the existing EOF-validated materialization; never buffer streamed arguments again.
/// Source call IDs are checked after delivery projection, which may replace them.
pub fn validate_tool_calls(response: &DecodedResponse) -> Result<(), CodecError> {
    for (_, item) in response.semantic.items() {
        if let Item::ToolCall(call) = item {
            wire::shape::name(call.name.as_str(), 128)?;
            if matches!(call.arguments, ToolArguments::StructuredPartial(_))
                && call.status == ItemLifecycle::Incomplete
                && response.semantic.outcome() == Outcome::Incomplete
                && response.semantic.details().incomplete == Some(IncompleteReason::MaxOutputTokens)
                && call.context.is_direct()
                && call.context.alias_domain.is_none()
            {
                continue;
            }
            tool_input(call)?;
        }
    }
    Ok(())
}
