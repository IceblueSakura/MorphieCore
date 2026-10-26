//! Shared scalar validation for standalone embedding codecs, not a wire schema.
use crate::protocol::CodecError;
use serde_json::{Map, Value};
pub(crate) fn required<'a>(
    map: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Value, CodecError> {
    map.get(key).ok_or_else(CodecError::invalid)
}
pub(crate) fn string(value: &Value) -> Result<&str, CodecError> {
    value.as_str().ok_or_else(CodecError::invalid)
}
pub(crate) fn number(value: &Value) -> Result<u64, CodecError> {
    value.as_u64().ok_or_else(CodecError::invalid)
}
pub(crate) fn optional_count(
    map: &Map<String, Value>,
    name: &str,
) -> Result<Option<u64>, CodecError> {
    map.get(name).map(number).transpose()
}
pub(crate) fn optional_string(
    map: &Map<String, Value>,
    name: &str,
) -> Result<Option<String>, CodecError> {
    map.get(name)
        .map(|v| string(v).map(str::to_owned))
        .transpose()
}
