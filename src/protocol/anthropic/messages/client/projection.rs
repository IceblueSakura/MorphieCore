//! Client Messages delivery projection, named losses and JSON rendering.
use crate::{
    protocol::{
        CodecError, DecodedResponse, anthropic::messages as wire, failure::CarrierReason,
        replay_envelope,
    },
    semantic::{task::generation::*, value::Presence},
};
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
    ) -> Result<
        Option<(
            TerminalDetails,
            crate::protocol::conversion::generation::Effect,
        )>,
        CodecError,
    > {
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
                "infer",
            ),
            (Self::InteractionsWithoutStops, InteractionProgress::TurnFinished, false, None) => (
                Some(CompletionCause::Natural),
                "messages.interactions-completed-as-end-turn.v1",
                "infer",
            ),
            _ => return Ok(None),
        };
        let mut details = source.details().clone();
        details.completion = completion;
        Ok(Some((
            details,
            crate::protocol::conversion::generation::Effect::Field {
                rule: crate::protocol::conversion::generation::RuleId::new(rule),
                field: ("generation.completion_cause").into(),
                boundary: (direction).into(),
                change: (action).into(),
                owner: crate::protocol::conversion::generation::Owner::Root,
            },
        )))
    }
}

/// The caller supplies the admitted/effective no-stop premise. Source reports
/// remain unchanged; the Messages target derives both end-turn facts.
pub(crate) fn completed_without_stops(
    source: &GenerationResponse,
    direction: &'static str,
) -> Result<
    Option<(
        TerminalDetails,
        crate::protocol::conversion::generation::Effect,
    )>,
    CodecError,
> {
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
        crate::protocol::conversion::generation::Effect::Field {
            rule: "messages.infer-end-turn-from-request.v1".into(),
            field: crate::protocol::conversion::generation::Field::Completion,
            boundary: direction.into(),
            change: crate::protocol::conversion::generation::Change::InferFromRequest,
            owner: crate::protocol::conversion::generation::Owner::Root,
        },
    )))
}

/// Client values are sealed envelopes; the format tag keeps a redacted block's type.
pub(crate) fn redacted(sealed: &str) -> bool {
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
    boundary: crate::protocol::conversion::generation::ConversionBoundary,
) -> crate::protocol::conversion::generation::Effect {
    crate::lowering::disposition::OMIT_MESSAGES_REASONING
        .effect(boundary)
        .at(crate::protocol::conversion::generation::Owner::item(item))
}

pub(crate) fn project_metadata(
    metadata: &mut crate::protocol::ResponseMetadata,
) -> Result<(), CodecError> {
    metadata.context.validate()?;
    if metadata
        .context
        .reports
        .iter()
        .any(|r| r.settings.as_ref().is_some_and(|s| !s.safety.is_empty()))
    {
        return Err(CodecError::Unsupported(CarrierReason::Safety));
    }
    for report in metadata.context.reports.iter_mut() {
        if report
            .service_class
            .as_ref()
            .is_some_and(|class| wire::service::class_wire(*class).is_err())
        {
            report.service_class = None;
        }
        report.settings = None;
    }
    crate::protocol::reports::single_view(&mut metadata.context.reports)?;
    metadata.context.execution = Default::default();
    metadata.context.instruction_messages = None;
    metadata.instruction_fidelity = Default::default();
    metadata.context.system_fingerprint = None;
    Ok(())
}

pub(crate) fn metadata_losses(
    metadata: &crate::protocol::ResponseMetadata,
) -> Vec<crate::protocol::conversion::generation::Effect> {
    let mut losses = vec![];
    let mut execution = metadata.context.execution.clone();
    if execution.metadata == Presence::Null {
        execution.metadata = Presence::Absent;
    }
    if execution != Default::default() {
        losses.push(crate::protocol::conversion::generation::Effect::Field {
            rule: crate::protocol::conversion::generation::RuleId::new(
                "messages.omit-reported-execution-hints.v1",
            ),
            field: crate::protocol::conversion::generation::Field::Representation,
            boundary: ("response.project").into(),
            change: ("omit").into(),
            owner: crate::protocol::conversion::generation::Owner::Root,
        });
    }
    if metadata.context.reports.iter().any(|r| {
        r.service_class
            .as_ref()
            .is_some_and(|class| wire::service::class_wire(*class).is_err())
    }) {
        losses.push(crate::protocol::conversion::generation::Effect::Field {
            rule: crate::protocol::conversion::generation::RuleId::new(
                "messages.omit-reported-service-class.v1",
            ),
            field: crate::protocol::conversion::generation::Field::ServiceClass,
            boundary: ("response.project").into(),
            change: ("omit").into(),
            owner: crate::protocol::conversion::generation::Owner::Root,
        });
    }
    if metadata
        .context
        .reports
        .iter()
        .any(|r| r.settings.is_some())
        || metadata.context.instruction_messages.is_some()
    {
        losses.push(crate::protocol::conversion::generation::Effect::Field {
            rule: crate::protocol::conversion::generation::RuleId::new(
                "messages.omit-incidental-settings.v1",
            ),
            field: ("response.reported.settings").into(),
            boundary: ("response.project").into(),
            change: ("omit").into(),
            owner: crate::protocol::conversion::generation::Owner::Root,
        });
    }
    if metadata.context.system_fingerprint.as_ref().is_some() {
        losses.push(crate::protocol::conversion::generation::Effect::Field {
            rule: crate::protocol::conversion::generation::RuleId::new(
                "messages.omit-system-fingerprint.v1",
            ),
            field: ("response.context.system_fingerprint").into(),
            boundary: ("response.project").into(),
            change: ("omit").into(),
            owner: crate::protocol::conversion::generation::Owner::Root,
        });
    }
    losses
}
pub(super) fn explanation_loss(
    direction: &'static str,
) -> crate::protocol::conversion::generation::Effect {
    crate::protocol::conversion::generation::Effect::Field {
        rule: crate::protocol::conversion::generation::RuleId::new(
            "messages.omit-terminal-explanation.v1",
        ),
        field: ("generation.terminal.explanation").into(),
        boundary: (direction).into(),
        change: ("omit").into(),
        owner: crate::protocol::conversion::generation::Owner::Root,
    }
}
/// Shared by JSON and SSE delivery. Lost report detail/scope and a changed scalar usage
/// view are independent adaptations; a report without a selected view records only the former.
pub(crate) fn report_losses(
    source: &UsageReport,
    selected: Option<&UsageReport>,
    direction: &'static str,
) -> Vec<crate::protocol::conversion::generation::Effect> {
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
pub(crate) fn usage_loss() -> crate::protocol::conversion::generation::Effect {
    crate::protocol::conversion::generation::Effect::Field {
        rule: crate::protocol::conversion::generation::RuleId::new("messages.usage-view.v1"),
        field: ("generation.usage").into(),
        boundary: ("response.project").into(),
        change: crate::protocol::conversion::generation::Change::Normalize,
        owner: crate::protocol::conversion::generation::Owner::Root,
    }
}
pub(super) fn tool_input(call: &ToolCall) -> Result<Value, CodecError> {
    tool_labels(call.call_id.as_str(), call.name.as_str())?;
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
pub(crate) fn validate_tool_calls(response: &DecodedResponse) -> Result<(), CodecError> {
    for (_, item) in response.semantic.items() {
        if let Item::ToolCall(call) = item {
            tool_labels(call.call_id.as_str(), call.name.as_str())?;
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
