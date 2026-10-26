//! Shared stop-list parsing; protocol callers select their own shape and count limit.
use crate::{
    protocol::CodecError,
    semantic::{task::generation::MAX_TEXT_BYTES, value::Text},
};
use serde_json::Value;

pub(super) fn stops(
    value: Option<&Value>,
    string_allowed: bool,
    max: usize,
) -> Result<Vec<Text>, CodecError> {
    let values = match value {
        None | Some(Value::Null) => return Ok(vec![]),
        Some(Value::String(text)) if string_allowed && text.is_empty() => return Ok(vec![]),
        Some(value @ Value::String(_)) if string_allowed => std::slice::from_ref(value),
        Some(Value::Array(values)) => values.as_slice(),
        _ => {
            return Err(CodecError::invalid());
        }
    };
    if values.len() > max {
        return Err(CodecError::Limit);
    }
    values
        .iter()
        .map(|value| {
            Text::new(
                value.as_str().ok_or(CodecError::invalid())?,
                "stop sequence",
                MAX_TEXT_BYTES,
            )
            .map_err(|_| CodecError::invalid())
        })
        .collect()
}
