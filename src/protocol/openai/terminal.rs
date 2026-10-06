//! Typed terminal details, shared by static and event codecs.
use super::{CodecError, common::*};
use crate::semantic::task::generation::*;
use serde_json::{Map, Value, json};
pub(super) fn decode_details(o: &Map<String, Value>) -> Result<TerminalDetails, CodecError> {
    let error = o
        .get("error")
        .filter(|v| !v.is_null())
        .map(|v| {
            let e = object(v)?;
            fields(e, &["code", "message", "param"])?;
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
            fields(d, &["reason"])?;
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
    Ok(TerminalDetails { error, incomplete })
}
/// Chat has two distinct non-success finish reasons; neither is a successful stop.
pub(super) fn chat_finish(response: &GenerationResponse) -> Result<&'static str, CodecError> {
    match (response.outcome(), response.details().incomplete.as_ref()) {
        (Outcome::Completed, _) => Ok(match response.continuation() {
            Continuation::Unreported => "stop",
            Continuation::ToolResults(_) => "tool_calls",
        }),
        (Outcome::Incomplete, Some(IncompleteReason::MaxOutputTokens)) => Ok("length"),
        (Outcome::Incomplete, Some(IncompleteReason::ContentFilter)) => Ok("content_filter"),
        _ => Err(CodecError::Unsupported("Chat terminal".into())),
    }
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
pub(super) fn encode_incomplete(reason: Option<&IncompleteReason>) -> Value {
    reason.map(|r|json!({"reason":match r {IncompleteReason::Unspecified=>Value::Null,IncompleteReason::MaxOutputTokens=>json!("max_output_tokens"),IncompleteReason::ContentFilter=>json!("content_filter"),IncompleteReason::Other(t)=>json!(t.as_str())}})).unwrap_or(Value::Null)
}
