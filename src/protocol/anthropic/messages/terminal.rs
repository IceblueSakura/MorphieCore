//! Messages stop reasons and semantic outcome/progress mappings in both directions.
use crate::protocol::anthropic::messages::items::text;
use crate::{protocol::CodecError, semantic::task::generation::*};

pub(super) fn decode(
    message: &super::Message,
) -> Result<(Outcome, InteractionProgress, TerminalDetails), CodecError> {
    let stop = *message
        .stop_reason
        .value()
        .ok_or(CodecError::Invalid("native stop"))?;
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
            },
        ),
        super::StopReason::ModelContextWindowExceeded => (
            Outcome::Incomplete,
            InteractionProgress::Unreported,
            TerminalDetails {
                incomplete: Some(IncompleteReason::ContextWindowExceeded),
                error: None,
                completion: None,
            },
        ),
        super::StopReason::Refusal => (
            Outcome::Incomplete,
            InteractionProgress::Unreported,
            TerminalDetails {
                incomplete: Some(IncompleteReason::ContentFilter),
                error: None,
                completion: None,
            },
        ),
    };
    details.completion = match stop {
        super::StopReason::EndTurn => Some(CompletionCause::Natural),
        super::StopReason::StopSequence => Some(CompletionCause::StopSequence(text(
            message
                .stop_sequence
                .value()
                .ok_or(CodecError::Invalid("native stop sequence"))?,
        )?)),
        _ => None,
    };
    Ok((outcome, progress, details))
}

pub(super) fn encode(
    outcome: Outcome,
    progress: InteractionProgress,
    calls: bool,
    details: &TerminalDetails,
) -> Result<(&'static str, Option<&str>), CodecError> {
    details.validate(outcome)?;
    if progress == InteractionProgress::NeedsContinuation {
        return Err(CodecError::Unsupported("Messages continuation".into()));
    }
    if let Some(cause) = &details.completion {
        return match (cause, progress, calls) {
            (CompletionCause::Natural, InteractionProgress::TurnFinished, false) => {
                Ok(("end_turn", None))
            }
            (CompletionCause::StopSequence(text), InteractionProgress::TurnFinished, false) => {
                Ok(("stop_sequence", Some(text.as_str())))
            }
            _ => Err(CodecError::Unsupported("Messages completion cause".into())),
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
        ) => "tool_use",
        (
            Outcome::Incomplete,
            InteractionProgress::Unreported,
            _,
            Some(IncompleteReason::MaxOutputTokens),
        ) => "max_tokens",
        (
            Outcome::Incomplete,
            InteractionProgress::Unreported,
            _,
            Some(IncompleteReason::ContextWindowExceeded),
        ) => "model_context_window_exceeded",
        (
            Outcome::Incomplete,
            InteractionProgress::Unreported,
            _,
            Some(IncompleteReason::ContentFilter),
        ) => "refusal",
        _ => return Err(CodecError::Unsupported("Messages terminal".into())),
    };
    Ok((stop_reason, None))
}
