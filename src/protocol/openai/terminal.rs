//! Typed terminal details, shared by static and event codecs.
use crate::protocol::openai::{CodecError, lowering::projection::LossRule, shape::*};
use crate::semantic::task::generation::*;
use serde_json::{Map, Value, json};
pub(super) fn decode_details(o: &Map<String, Value>) -> Result<TerminalDetails, CodecError> {
    let error = o
        .get("error")
        .filter(|v| !v.is_null())
        .map(|v| {
            let e = object(v)?;
            if e.get("misalignment").is_some_and(|v| !v.is_null()) {
                return Err(CodecError::Unsupported(
                    crate::protocol::failure::CarrierReason::Unavailable,
                ));
            }
            DecodeDirection::Upstream.fields(e, &["code", "message", "param", "type"])?;
            Ok::<_, CodecError>(ResponseError {
                original_type: e
                    .get("type")
                    .filter(|v| !v.is_null())
                    .map(|v| {
                        crate::semantic::value::Text::allowing_empty(
                            v.as_str().ok_or(CodecError::Invalid(
                                crate::protocol::failure::ShapeReason::InvalidValue,
                            ))?,
                            "original error type",
                            256,
                        )
                        .map_err(|_| CodecError::Limit)
                    })
                    .transpose()?,
                code: e
                    .get("code")
                    .filter(|v| !v.is_null())
                    .map(|v| {
                        text(
                            v.as_str().ok_or(CodecError::Invalid(
                                crate::protocol::failure::ShapeReason::InvalidValue,
                            ))?,
                            "error code",
                            128,
                        )
                    })
                    .transpose()?,
                message: crate::semantic::value::Text::allowing_empty(
                    string(e, "message")?,
                    "error message",
                    MAX_TEXT_BYTES,
                )
                .map_err(|_| CodecError::Limit)?,
                param: e
                    .get("param")
                    .filter(|v| !v.is_null())
                    .map(|v| {
                        text(
                            v.as_str().ok_or(CodecError::Invalid(
                                crate::protocol::failure::ShapeReason::InvalidValue,
                            ))?,
                            "param",
                            256,
                        )
                    })
                    .transpose()?,
            })
        })
        .transpose()?;
    let incomplete = o
        .get("incomplete_details")
        .filter(|v| !v.is_null())
        .map(|v| {
            let d = object(v)?;
            DecodeDirection::Upstream.fields(d, &["reason"])?;
            if d.get("reason").is_none_or(|v| v.is_null()) {
                return Ok(IncompleteReason::Unspecified);
            }
            Ok::<_, CodecError>(match string(d, "reason")? {
                "max_output_tokens" => IncompleteReason::MaxOutputTokens,
                "content_filter" => IncompleteReason::ContentFilter,
                s => IncompleteReason::Other(text(s, "incomplete reason", 128)?),
            })
        })
        .transpose()?;
    Ok(TerminalDetails {
        error,
        incomplete,
        completion: None,
        ..Default::default()
    })
}
/// Chat has two distinct non-success finish reasons; neither is a successful stop.
/// Completion-cause approximations are recorded by [`terminal_losses`].
pub(super) fn chat_finish(response: &GenerationResponse) -> Result<&'static str, CodecError> {
    match (response.outcome(), response.details().incomplete.as_ref()) {
        // `stop` reports a natural or matched stop, also beside forced tool calls.
        (Outcome::Completed, _) if response.details().completion.is_some() => Ok("stop"),
        (Outcome::Completed, _) => Ok(match response.continuation() {
            Continuation::Unreported => "stop",
            Continuation::ToolResults(_) => "tool_calls",
        }),
        // A context-window stop is also a length limit; the cause is recorded as a loss.
        (
            Outcome::Incomplete,
            Some(IncompleteReason::MaxOutputTokens | IncompleteReason::ContextWindowExceeded),
        ) => Ok("length"),
        (
            Outcome::Incomplete,
            Some(IncompleteReason::ContentFilter | IncompleteReason::Recitation),
        ) => Ok("content_filter"),
        _ => Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Terminal,
        )),
    }
}
/// Whether Chat has a finish reason for this incomplete state, exactly or under a
/// named rule. An incomplete outcome needs a reason; a successful one admits none.
pub(super) fn chat_incomplete(incomplete: bool, details: &TerminalDetails) -> bool {
    match &details.incomplete {
        None => !incomplete,
        Some(reason) => matches!(
            reason,
            IncompleteReason::MaxOutputTokens
                | IncompleteReason::ContentFilter
                | IncompleteReason::Recitation
                | IncompleteReason::ContextWindowExceeded
        ),
    }
}
/// Selected terminal approximations for a profile; an empty list means exact.
/// Chat `stop` officially covers a natural stop and a matched stop sequence:
/// https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/chat_completion.py
pub(super) fn terminal_losses(
    details: &TerminalDetails,
    profile: crate::protocol::openai::Profile,
) -> Result<Vec<LossRule>, CodecError> {
    use crate::protocol::openai::Profile;
    let mut losses = vec![];
    if details
        .refusal
        .value()
        .is_some_and(|report| report.category.value().is_some())
    {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
    }
    if !details.refusal.is_absent() {
        losses.push(LossRule::OmitExplanatoryRefusalDetails);
    }
    if !details.explanation.is_absent() {
        losses.push(LossRule::OmitTerminalExplanation);
    }
    match (&details.completion, profile) {
        (None, _) => {}
        (Some(CompletionCause::NaturalOrStopSequence), Profile::Chat) => {}
        (Some(_), Profile::Chat) => losses.push(LossRule::CoarsenChatCompletionCause),
        (Some(_), Profile::Responses) => losses.push(LossRule::OmitResponsesCompletionCause),
    }
    match (&details.incomplete, profile) {
        (Some(IncompleteReason::ContextWindowExceeded), Profile::Chat) => {
            losses.push(LossRule::ContextWindowAsLength)
        }
        (Some(IncompleteReason::ContextWindowExceeded), Profile::Responses) => {
            losses.push(LossRule::OmitResponsesContextWindowReason)
        }
        (Some(IncompleteReason::Recitation), _) => losses.push(LossRule::RecitationAsContentFilter),
        _ => {}
    }
    Ok(losses)
}
/// Classify an OpenAI-compatible error reported inside a successful reply. A
/// numeric HTTP-like `code` (OpenRouter) wins, then a named code, then the type.
pub(crate) fn reported_failure(
    numeric: Option<u64>,
    code: Option<&str>,
    kind: Option<&str>,
    message: Option<&str>,
) -> crate::protocol::failure::SourceFailureEvidence {
    use crate::protocol::failure::RecognitionBasis;
    let named = |value: Option<&str>| match value? {
        "invalid_request_error" | "context_length_exceeded" => Some(400),
        "authentication_error" | "invalid_api_key" => Some(401),
        "permission_error" | "permission_denied" => Some(403),
        "not_found_error" | "model_not_found" => Some(404),
        "rate_limit_exceeded" | "rate_limit_error" | "insufficient_quota" => Some(429),
        "overloaded" | "server_is_overloaded" => Some(503),
        _ => None,
    };
    let (status, basis) = match numeric.filter(|n| (400..600).contains(n)) {
        Some(n) => (n as u16, RecognitionBasis::EnvelopeCode),
        None => match (named(code), named(kind)) {
            (Some(status), _) => (status, RecognitionBasis::EnvelopeCode),
            (None, Some(status)) => (status, RecognitionBasis::EnvelopeType),
            (None, None) => (500, RecognitionBasis::EnvelopeFields),
        },
    };
    crate::protocol::failure::SourceFailureEvidence::envelope(status, basis, code, kind, message)
}
/// A failed Responses outcome classified as an upstream failure, for clients
/// whose protocol has no failed-response carrier.
pub fn failed_response(
    response: &GenerationResponse,
) -> Option<crate::protocol::failure::SourceFailureEvidence> {
    if response.outcome() != Outcome::Failed {
        return None;
    }
    let error = response.details().error.as_ref();
    Some(reported_failure(
        None,
        error.and_then(|e| e.code.as_ref()).map(|c| c.as_str()),
        error
            .and_then(|e| e.original_type.as_ref())
            .map(|t| t.as_str()),
        error.map(|e| e.message.as_str()),
    ))
}
pub(super) fn encode_error(error: Option<&ResponseError>) -> Value {
    error
        .map(|e| {
            let mut v =
                json!({"code":e.code.as_ref().map(|c|c.as_str()),"message":e.message.as_str()});
            if let Some(p) = &e.param {
                v["param"] = json!(p.as_str());
            }
            if let Some(value) = &e.original_type {
                v["type"] = json!(value.as_str());
            }
            v
        })
        .unwrap_or(Value::Null)
}
pub(super) fn encode_incomplete(reason: Option<&IncompleteReason>) -> Result<Value, CodecError> {
    let Some(reason) = reason else {
        return Ok(Value::Null);
    };
    let reason = match reason {
        // Responses has no context-window reason; the stop stays incomplete.
        IncompleteReason::Unspecified | IncompleteReason::ContextWindowExceeded => Value::Null,
        IncompleteReason::MaxOutputTokens => json!("max_output_tokens"),
        IncompleteReason::ContentFilter | IncompleteReason::Recitation => json!("content_filter"),
        IncompleteReason::Other(t) => json!(t.as_str()),
    };
    Ok(json!({ "reason": reason }))
}
