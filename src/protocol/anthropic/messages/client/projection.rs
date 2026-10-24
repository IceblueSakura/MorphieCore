//! Client Messages delivery projection, named losses and JSON rendering.
use crate::protocol::{CodecError, DecodedResponse, anthropic::messages as wire, replay_envelope};
use crate::semantic::{task::generation::*, value::Presence};
use serde_json::Value;

/// Selected only for a trusted Google binding whose effective request has no
/// stop sequences. This is client delivery policy, never native source decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GoogleTerminalPolicy {
    GenerateContentWithoutStops,
    InteractionsWithoutStops,
}
impl GoogleTerminalPolicy {
    pub(crate) fn project(
        self,
        source: &GenerationResponse,
        direction: &'static str,
    ) -> Result<Option<(TerminalDetails, crate::lowering::generation::Effect)>, CodecError> {
        source.details().validate(source.outcome())?;
        if source.outcome() != Outcome::Completed {
            return Ok(None);
        }
        let calls = source
            .items()
            .iter()
            .any(|(_, i)| matches!(i, Item::ToolCall(_)));
        let (completion, rule, action) = match (
            self,
            source.progress(),
            calls,
            source.details().completion.as_ref(),
        ) {
            (
                Self::GenerateContentWithoutStops,
                InteractionProgress::AwaitingToolResults,
                true,
                Some(CompletionCause::NaturalOrStopSequence),
            ) => (None, "messages.google-tool-stop.v1", "omit"),
            (
                Self::GenerateContentWithoutStops,
                InteractionProgress::TurnFinished,
                false,
                Some(CompletionCause::NaturalOrStopSequence),
            ) => (
                Some(CompletionCause::Natural),
                "messages.google-stop-without-sequences.v1",
                "resolve",
            ),
            (Self::InteractionsWithoutStops, InteractionProgress::TurnFinished, false, None) => (
                Some(CompletionCause::Natural),
                "messages.interactions-completed-as-end-turn.v1",
                "approximate",
            ),
            _ => return Ok(None),
        };
        let mut details = source.details().clone();
        details.completion = completion;
        Ok(Some((
            details,
            crate::lowering::generation::Effect::Field {
                rule: crate::lowering::generation::RuleId::new(rule),
                field: ("generation.completion_cause").into(),
                direction: (direction).into(),
                action: (action).into(),
                owner: crate::lowering::generation::Owner::Root,
            },
        )))
    }
}

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
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Safety,
        ));
    }
    crate::protocol::reports::single_view(&mut response.metadata.context.reports)?;
    if let Some(class) = response.metadata.context.service_class().value() {
        wire::service::class_wire(*class)?;
    }
    if response.metadata.context.execution != crate::semantic::context::ExecutionHints::default() {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
    }
    for report in response.metadata.context.reports.iter_mut() {
        report.settings = None;
    }
    response.metadata.context.instruction_messages = None;
    response.metadata.instruction_fidelity = Default::default();
    response.metadata.context.system_fingerprint = Presence::Absent;
    let selected = match wire::accounting::project_reports(response.semantic.usage_reports()) {
        Ok(selected) => selected,
        Err(CodecError::Unsupported(_))
            if response
                .metadata
                .delivery
                .as_ref()
                .is_some_and(|d| d.usage.is_some()) =>
        {
            UsageReports::default()
        }
        Err(error) => return Err(error),
    };
    if selected.operation().is_none()
        && response
            .metadata
            .delivery
            .as_ref()
            .is_none_or(|d| d.usage.is_none())
    {
        return Err(CodecError::Invalid(
            crate::protocol::failure::ShapeReason::Usage,
        ));
    }
    response.semantic = response.semantic.clone().with_usage_reports(selected)?;
    let mut details = response.semantic.details().clone();
    details.validate(response.semantic.outcome())?;
    details.explanation = Presence::Absent;
    response.semantic = response.semantic.clone().with_details(details)?;
    Ok(())
}
pub(crate) fn metadata_losses(
    metadata: &crate::protocol::ResponseMetadata,
) -> Vec<crate::lowering::generation::Effect> {
    let mut losses = vec![];
    if metadata
        .context
        .reports
        .iter()
        .any(|r| r.settings.is_some())
        || metadata.context.instruction_messages.is_some()
    {
        losses.push(crate::lowering::generation::Effect::Field {
            rule: crate::lowering::generation::RuleId::new("messages.omit-incidental-settings.v1"),
            field: ("response.reported.settings").into(),
            direction: ("response.project").into(),
            action: ("omit").into(),
            owner: crate::lowering::generation::Owner::Root,
        });
    }
    if !metadata.context.system_fingerprint.is_absent() {
        losses.push(crate::lowering::generation::Effect::Field {
            rule: crate::lowering::generation::RuleId::new("messages.omit-system-fingerprint.v1"),
            field: ("response.context.system_fingerprint").into(),
            direction: ("response.project").into(),
            action: ("omit").into(),
            owner: crate::lowering::generation::Owner::Root,
        });
    }
    losses
}
/// Messages has no refusal content block; `stop_reason: "refusal"` reports classifier
/// intervention, not a model-authored refusal message, so the text is kept as text.
pub(crate) fn refusal_loss(direction: &'static str) -> crate::lowering::generation::Effect {
    crate::lowering::generation::Effect::Field {
        rule: crate::lowering::generation::RuleId::new("messages.refusal-as-text.v1"),
        field: ("generation.content.refusal").into(),
        direction: (direction).into(),
        action: ("approximate").into(),
        owner: crate::lowering::generation::Owner::Root,
    }
}
pub(crate) fn content_losses(
    response: &GenerationResponse,
) -> Vec<crate::lowering::generation::Effect> {
    let mut losses = Vec::new();
    if !response.details().explanation.is_absent() {
        losses.push(explanation_loss("response.project"));
    }
    if wire::accounting::project_reports(response.usage_reports())
        .is_ok_and(|selected| selected != *response.usage_reports())
        && (response.usage_reports().has_details()
            || response
                .usage_reports()
                .operation()
                .is_some_and(|r| r.stage != UsageStage::Final))
    {
        losses.push(crate::protocol::usage::report_loss("response.project"));
    }
    let refusal = response.items().iter().any(|(_, item)| {
        matches!(item, Item::Message(m)
            if m.parts.iter().any(|p| matches!(p.content, ContentPart::Refusal(_))))
    });
    losses.extend(refusal.then(|| refusal_loss("response.project")));
    losses
}
pub(super) fn explanation_loss(direction: &'static str) -> crate::lowering::generation::Effect {
    crate::lowering::generation::Effect::Field {
        rule: crate::lowering::generation::RuleId::new("messages.omit-terminal-explanation.v1"),
        field: ("generation.terminal.explanation").into(),
        direction: (direction).into(),
        action: ("omit").into(),
        owner: crate::lowering::generation::Owner::Root,
    }
}
/// Shared by JSON and SSE delivery. Lost report detail/scope and a changed scalar usage
/// view are independent receipts; a report without a selected view records only the former.
pub(crate) fn report_losses(
    source: &UsageReport,
    selected: Option<&UsageReport>,
    direction: &'static str,
) -> Vec<crate::lowering::generation::Effect> {
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
        losses.push(usage_loss().with_boundary(direction.into()));
    }
    losses
}
pub(crate) fn usage_loss() -> crate::lowering::generation::Effect {
    crate::lowering::generation::Effect::Field {
        rule: crate::lowering::generation::RuleId::new("messages.usage-view.v1"),
        field: ("generation.usage").into(),
        direction: ("response.project").into(),
        action: crate::lowering::generation::Change::Normalize,
        owner: crate::lowering::generation::Owner::Root,
    }
}
pub(super) fn tool_input(call: &ToolCall) -> Result<Value, CodecError> {
    if !call.context.is_direct() || call.context.alias_domain.is_some() {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
    }
    let input = match &call.arguments {
        ToolArguments::Structured(value) => value.clone(),
        ToolArguments::Raw(raw) => StructuredValue::from_bytes(raw.as_bytes())?,
        ToolArguments::StructuredPartial(_) => {
            return Err(CodecError::Unsupported(
                crate::protocol::failure::CarrierReason::Tools,
            ));
        }
    };
    if !input.value().is_object() {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
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
