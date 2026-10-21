//! Compare supplied lifecycle steps with closed streamed owners; never materialize content.
use super::{
    replay::Signed,
    sse::{check_annotations, field, invalid, object, required, string},
};
use crate::{
    protocol::{CodecError, stream::SseError},
    semantic::task::generation::*,
};
use serde_json::Value;

fn mismatch() -> SseError {
    invalid("Interactions snapshot steps")
}

pub(super) fn reconcile(
    value: Option<&Value>,
    owners: &[Option<ItemId>],
    state: &StreamState,
) -> Result<(), SseError> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(());
    };
    let steps = value
        .as_array()
        .ok_or_else(|| invalid("Interactions steps"))?;
    if steps.len() > MAX_ITEMS {
        return Err(SseError::Limit);
    }
    if steps.len() != owners.len() {
        return Err(mismatch());
    }
    let mut streamed = state.items().iter();
    for (step, owner) in steps.iter().zip(owners) {
        let step = object(step, "Interactions snapshot step")?;
        if step.get("error").is_some_and(|value| !value.is_null()) {
            return Err(CodecError::Unsupported("Interactions model_output error".into()).into());
        }
        let item = match owner {
            None => None,
            Some(owner) => {
                let item = streamed.next().ok_or_else(mismatch)?;
                if item.id != *owner {
                    return Err(mismatch());
                }
                Some(item)
            }
        };
        let kind = required(step, "type")?;
        let matches = matches!(
            (kind, item.map(|item| &item.kind)),
            (
                "model_output",
                None | Some(ItemKind::Message { phase: None })
            ) | ("thought", Some(ItemKind::Reasoning))
                | ("function_call", Some(ItemKind::ToolCall { .. }))
        );
        if !matches {
            return Err(mismatch());
        }
        if kind == "function_call" {
            let item = item.ok_or_else(mismatch)?;
            let ItemKind::ToolCall { call_id, name, .. } = &item.kind else {
                return Err(mismatch());
            };
            let arguments = object(field(step, "arguments")?, "snapshot call arguments")?;
            let [part] = item.parts.as_slice() else {
                return Err(mismatch());
            };
            let StreamPartValue::Json(actual) = part.value() else {
                return Err(mismatch());
            };
            if required(step, "id")? != call_id.as_str()
                || required(step, "name")? != name.as_str()
                || actual.value().as_object() != Some(arguments)
            {
                return Err(mismatch());
            }
        }
        if let Some(signature) = string(step, "signature")? {
            let format = match kind {
                "thought" => ReplayFormat::GoogleInteractionsV1Thought,
                "function_call" => ReplayFormat::GoogleInteractionsV1FunctionCall,
                _ => {
                    return Err(
                        CodecError::Unsupported("Interactions signature owner".into()).into(),
                    );
                }
            };
            if signature.is_empty() {
                return Err(invalid("step signature"));
            }
            let replay = item
                .and_then(|item| item.replay.as_ref())
                .ok_or_else(mismatch)?;
            if replay.value.format() != format {
                return Err(mismatch());
            }
            let actual = Signed::decode(replay.value.replay_token().ok_or_else(mismatch)?, format)?;
            if actual.signature() != Some(signature) {
                return Err(mismatch());
            }
        }
        for (key, expected_kind) in [("content", "model_output"), ("summary", "thought")] {
            if let Some(value) = step.get(key) {
                if kind != expected_kind {
                    return Err(mismatch());
                }
                text(value, item)?;
            }
        }
    }
    if streamed.next().is_some() {
        return Err(mismatch());
    }
    Ok(())
}

fn text(value: &Value, item: Option<&StreamItem>) -> Result<(), SseError> {
    let parts = value
        .as_array()
        .ok_or_else(|| invalid("snapshot content"))?;
    // Deltas carry no content-part index. Do not assert unknown segmentation.
    if parts.len() > 1 {
        return Err(
            CodecError::Unsupported("Interactions snapshot multipart content".into()).into(),
        );
    }
    let actual = match item.map(|item| item.parts.as_slice()).unwrap_or_default() {
        [] => "",
        [part] => part.text().ok_or_else(mismatch)?,
        _ => return Err(mismatch()),
    };
    let expected = if let Some(part) = parts.first() {
        let part = object(part, "snapshot content part")?;
        check_annotations(part)?;
        if required(part, "type")? != "text" {
            return Err(CodecError::Unsupported("Interactions content type".into()).into());
        }
        string(part, "text")?.ok_or_else(|| invalid("snapshot text"))?
    } else {
        ""
    };
    if expected != actual {
        return Err(mismatch());
    }
    Ok(())
}
