//! Client Messages delivery projection, named losses and JSON rendering.
use crate::protocol::{CodecError, DecodedResponse, anthropic::messages as wire, replay_envelope};
use crate::semantic::{task::generation::*, value::Presence};
use serde_json::Value;

/// Client values are sealed envelopes; the format tag keeps a redacted block's type.
pub(crate) fn redacted(sealed: &str) -> bool {
    matches!(
        replay_envelope::open(sealed),
        Some((_, ReplayFormat::AnthropicMessagesRedactedThinking, _))
    )
}

/// Retained metadata and accounting reflect the actual Messages carrier.
pub(crate) fn project_snapshot(response: &mut DecodedResponse) -> Result<(), CodecError> {
    response.metadata.context.validate()?;
    if !response.semantic.safety_reports().is_empty()
        || response
            .metadata
            .context
            .reports
            .iter()
            .any(|r| r.settings.as_ref().is_some_and(|s| !s.safety.is_empty()))
    {
        return Err(CodecError::Unsupported("Messages safety report".into()));
    }
    crate::protocol::reports::single_view(&mut response.metadata.context.reports)?;
    if let Some(class) = response.metadata.context.service_class().value() {
        wire::service::class_wire(*class)?;
    }
    if response.metadata.context.execution != crate::semantic::context::ExecutionHints::default() {
        return Err(CodecError::Unsupported(
            "Messages reported execution context".into(),
        ));
    }
    for report in response.metadata.context.reports.iter_mut() {
        report.settings = None;
    }
    response.metadata.context.instruction_messages = None;
    response.metadata.instruction_fidelity = Default::default();
    response.metadata.context.system_fingerprint = Presence::Absent;
    let selected = wire::accounting::project_reports(response.semantic.usage_reports())?;
    if selected.operation().is_none() {
        return Err(CodecError::Invalid("Messages usage"));
    }
    response.semantic = response.semantic.clone().with_usage_reports(selected)?;
    Ok(())
}
pub(crate) fn metadata_losses(
    metadata: &crate::protocol::ResponseMetadata,
) -> Vec<crate::lowering::generation::LossEvidence> {
    let mut losses = vec![];
    if metadata
        .context
        .reports
        .iter()
        .any(|r| r.settings.is_some())
        || metadata.context.instruction_messages.is_some()
    {
        losses.push(crate::lowering::generation::LossEvidence {
            rule: "messages.omit-incidental-settings.v1",
            field: "response.reported.settings",
            direction: "response.encode",
            action: "omit",
        });
    }
    if !metadata.context.system_fingerprint.is_absent() {
        losses.push(crate::lowering::generation::LossEvidence {
            rule: "messages.omit-system-fingerprint.v1",
            field: "response.context.system_fingerprint",
            direction: "response.encode",
            action: "omit",
        });
    }
    losses
}
/// Messages has no refusal content block; `stop_reason: "refusal"` reports classifier
/// intervention, not a model-authored refusal message, so the text is kept as text.
pub(crate) fn refusal_loss(direction: &'static str) -> crate::lowering::generation::LossEvidence {
    crate::lowering::generation::LossEvidence {
        rule: "messages.refusal-as-text.v1",
        field: "generation.content.refusal",
        direction,
        action: "approximate",
    }
}
pub(crate) fn content_losses(
    response: &GenerationResponse,
) -> Vec<crate::lowering::generation::LossEvidence> {
    let mut losses = Vec::new();
    if wire::accounting::project_reports(response.usage_reports())
        .is_ok_and(|selected| selected != *response.usage_reports())
        && (response.usage_reports().has_details()
            || response
                .usage_reports()
                .operation()
                .is_some_and(|r| r.stage != UsageStage::Final))
    {
        losses.push(crate::protocol::usage::report_loss("response.encode"));
    }
    let refusal = response.items().iter().any(|(_, item)| {
        matches!(item, Item::Message(m)
            if m.parts.iter().any(|p| matches!(p.content, ContentPart::Refusal(_))))
    });
    losses.extend(refusal.then(|| refusal_loss("response.encode")));
    losses
}
/// Shared by JSON and SSE delivery. Lost report detail/scope and a changed scalar usage
/// view are independent receipts; a report without a selected view records only the former.
pub(crate) fn report_losses(
    source: &UsageReport,
    selected: Option<&UsageReport>,
    direction: &'static str,
) -> Vec<crate::lowering::generation::LossEvidence> {
    if selected == Some(source) {
        return vec![];
    }
    let detailed = source.details != Default::default()
        || source.scope != UsageScope::Operation
        || source.accounting != crate::semantic::task::generation::UsageAccounting::Direct;
    let mut losses = Vec::new();
    if detailed {
        losses.push(crate::protocol::usage::report_loss(direction));
    }
    if !detailed || selected.is_some_and(|s| s.scalar_summary() != source.scalar_summary()) {
        losses.push(crate::lowering::generation::LossEvidence {
            direction,
            ..usage_loss()
        });
    }
    losses
}
pub(crate) fn usage_loss() -> crate::lowering::generation::LossEvidence {
    crate::lowering::generation::LossEvidence {
        rule: "messages.usage-view.v1",
        field: "generation.usage",
        direction: "response.encode",
        action: "project",
    }
}
pub(super) fn tool_input(call: &ToolCall) -> Result<Value, CodecError> {
    if !call.context.is_direct() || call.context.alias_domain.is_some() {
        return Err(CodecError::Unsupported("Messages call context".into()));
    }
    let input = match &call.arguments {
        ToolArguments::Structured(value) => value.clone(),
        ToolArguments::Raw(raw) => StructuredValue::from_bytes(raw.as_bytes())?,
        ToolArguments::StructuredPartial(_) => {
            return Err(CodecError::Unsupported("Messages tool input".into()));
        }
    };
    if !input.value().is_object() {
        return Err(CodecError::Unsupported("Messages object input".into()));
    }
    Ok(input.value().clone())
}
/// Reuse the existing EOF-validated materialization; never buffer streamed arguments again.
pub(crate) fn validate_tool_calls(response: &DecodedResponse) -> Result<(), CodecError> {
    for (_, item) in response.semantic.items() {
        if let Item::ToolCall(call) = item {
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
