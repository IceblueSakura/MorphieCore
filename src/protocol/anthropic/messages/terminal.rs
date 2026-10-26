//! Messages stop reasons and semantic outcome/progress mappings in both directions.
use crate::protocol::anthropic::messages::items::text;
use crate::protocol::failure::CarrierReason;
use crate::{protocol::CodecError, semantic::task::generation::*};

pub(super) fn decode(
    message: &super::Message,
) -> Result<(Outcome, InteractionProgress, TerminalDetails), CodecError> {
    let stop = *message.stop_reason.value().ok_or(CodecError::invalid())?;
    let (outcome, progress, mut details) = match stop {
        super::StopReason::EndTurn | super::StopReason::StopSequence => (
            Outcome::Completed,
            InteractionProgress::TurnFinished,
            TerminalDetails::default(),
        ),
        super::StopReason::ToolUse => (
            Outcome::Completed,
            InteractionProgress::AwaitingToolResults,
            TerminalDetails::default(),
        ),
        super::StopReason::MaxTokens => (
            Outcome::Incomplete,
            InteractionProgress::Unreported,
            TerminalDetails {
                incomplete: Some(IncompleteReason::MaxOutputTokens),
                error: None,
                completion: None,
                ..Default::default()
            },
        ),
        super::StopReason::ModelContextWindowExceeded => (
            Outcome::Incomplete,
            InteractionProgress::Unreported,
            TerminalDetails {
                incomplete: Some(IncompleteReason::ContextWindowExceeded),
                error: None,
                completion: None,
                ..Default::default()
            },
        ),
        super::StopReason::Refusal => (
            Outcome::Incomplete,
            InteractionProgress::Unreported,
            TerminalDetails {
                incomplete: Some(IncompleteReason::ContentFilter),
                error: None,
                completion: None,
                ..Default::default()
            },
        ),
    };
    details.completion = match stop {
        super::StopReason::EndTurn => Some(CompletionCause::Natural),
        super::StopReason::StopSequence => Some(CompletionCause::StopSequence(text(
            message.stop_sequence.value().ok_or(CodecError::invalid())?,
        )?)),
        _ => None,
    };
    details.refusal = message.stop_details.clone();
    Ok((outcome, progress, details))
}

pub(super) fn encode(
    outcome: Outcome,
    progress: InteractionProgress,
    calls: bool,
    details: &TerminalDetails,
) -> Result<(super::StopReason, Option<&str>), CodecError> {
    details.validate(outcome)?;
    if !details.explanation.is_absent() {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    if progress == InteractionProgress::NeedsContinuation {
        return Err(CodecError::Unsupported(CarrierReason::Terminal));
    }
    if let Some(cause) = &details.completion {
        return match (cause, progress, calls) {
            (CompletionCause::Natural, InteractionProgress::TurnFinished, false) => {
                Ok((super::StopReason::EndTurn, None))
            }
            (CompletionCause::StopSequence(text), InteractionProgress::TurnFinished, false) => {
                Ok((super::StopReason::StopSequence, Some(text.as_str())))
            }
            _ => Err(CodecError::Unsupported(CarrierReason::Terminal)),
        };
    }
    let stop_reason = match (outcome, progress, calls, &details.incomplete) {
        // Completed calls are pending result dependencies in this carrier (ordinary
        // result items reject above), independently of a reported disposition.
        (
            Outcome::Completed,
            InteractionProgress::Unreported | InteractionProgress::AwaitingToolResults,
            true,
            _,
        ) => super::StopReason::ToolUse,
        (
            Outcome::Incomplete,
            InteractionProgress::Unreported,
            _,
            Some(IncompleteReason::MaxOutputTokens),
        ) => super::StopReason::MaxTokens,
        (
            Outcome::Incomplete,
            InteractionProgress::Unreported,
            _,
            Some(IncompleteReason::ContextWindowExceeded),
        ) => super::StopReason::ModelContextWindowExceeded,
        (
            Outcome::Incomplete,
            InteractionProgress::Unreported,
            _,
            Some(IncompleteReason::ContentFilter),
        ) => super::StopReason::Refusal,
        _ => {
            return Err(CodecError::Unsupported(CarrierReason::Terminal));
        }
    };
    Ok((stop_reason, None))
}
