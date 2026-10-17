//! Typed terminal details, shared by static and event codecs.
use crate::protocol::openai::{CodecError, common::*, lowering::projection::LossRule};
use crate::semantic::task::generation::*;
use serde_json::{Map, Value, json};
pub(super) fn decode_details(o: &Map<String, Value>) -> Result<TerminalDetails, CodecError> {
    let error = o
        .get("error")
        .filter(|v| !v.is_null())
        .map(|v| {
            let e = object(v)?;
            DecodeDirection::Upstream.fields(e, &["code", "message", "param"])?;
            Ok::<_, CodecError>(ResponseError {
                code: e
                    .get("code")
                    .filter(|v| !v.is_null())
                    .map(|v| {
                        text(
                            v.as_str().ok_or(CodecError::Invalid("error code"))?,
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
                            v.as_str().ok_or(CodecError::Invalid("param"))?,
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
        (Outcome::Incomplete, Some(IncompleteReason::MaxOutputTokens)) => Ok("length"),
        (
            Outcome::Incomplete,
            Some(IncompleteReason::ContentFilter | IncompleteReason::Recitation),
        ) => Ok("content_filter"),
        _ => Err(CodecError::Unsupported("Chat terminal".into())),
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
    match (&details.completion, profile) {
        (None, _) => {}
        (Some(CompletionCause::NaturalOrStopSequence), Profile::Chat) => {}
        (Some(_), Profile::Chat) => losses.push(LossRule::CoarsenChatCompletionCause),
        (Some(_), Profile::Responses) => losses.push(LossRule::OmitResponsesCompletionCause),
    }
    match details.incomplete {
        Some(IncompleteReason::ContextWindowExceeded) => {
            return Err(CodecError::Unsupported("context-window terminal".into()));
        }
        Some(IncompleteReason::Recitation) => losses.push(LossRule::RecitationAsContentFilter),
        _ => {}
    }
    Ok(losses)
}
pub(super) fn encode_error(error: Option<&ResponseError>) -> Value {
    error
        .map(|e| {
            let mut v =
                json!({"code":e.code.as_ref().map(|c|c.as_str()),"message":e.message.as_str()});
            if let Some(p) = &e.param {
                v["param"] = json!(p.as_str());
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
        IncompleteReason::Unspecified => Value::Null,
        IncompleteReason::MaxOutputTokens => json!("max_output_tokens"),
        IncompleteReason::ContentFilter | IncompleteReason::Recitation => json!("content_filter"),
        IncompleteReason::ContextWindowExceeded => {
            return Err(CodecError::Unsupported("context-window terminal".into()));
        }
        IncompleteReason::Other(t) => json!(t.as_str()),
    };
    Ok(json!({ "reason": reason }))
}
