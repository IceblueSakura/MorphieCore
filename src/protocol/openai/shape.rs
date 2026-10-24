//! Shared OpenAI wire-shape readers, field admission and bounds.
use crate::protocol::openai::CodecError;
use crate::semantic::{
    task::generation::{MAX_TEXT_BYTES, MAX_TOTAL_BYTES},
    value::{Text, json_size},
};
use serde_json::{Map, Value};

pub(super) fn bounded(v: &Value) -> Result<(), CodecError> {
    json_size(v, MAX_TOTAL_BYTES)
        .map(|_| ())
        .map_err(|_| CodecError::Limit)
}
pub(super) fn read_presence<T>(
    o: &Map<String, Value>,
    key: &'static str,
    parse: impl FnOnce(&Value) -> Result<T, CodecError>,
) -> Result<crate::semantic::value::Presence<T>, CodecError> {
    use crate::semantic::value::Presence;
    match o.get(key) {
        None => Ok(Presence::Absent),
        Some(Value::Null) => Ok(Presence::Null),
        Some(v) => parse(v).map(Presence::Value),
    }
}
pub(super) fn put_presence<T>(
    o: &mut Map<String, Value>,
    key: &str,
    value: &crate::semantic::value::Presence<T>,
    encode: impl FnOnce(&T) -> Value,
) {
    use crate::semantic::value::Presence;
    match value {
        Presence::Absent => {}
        Presence::Null => {
            o.insert(key.into(), Value::Null);
        }
        Presence::Value(v) => {
            o.insert(key.into(), encode(v));
        }
    }
}
pub(super) fn object(v: &Value) -> Result<&Map<String, Value>, CodecError> {
    v.as_object().ok_or(CodecError::Invalid(
        crate::protocol::failure::ShapeReason::Object,
    ))
}
/// Operation-local parsing policy. Payload contents never select this direction.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum DecodeDirection<'a> {
    /// Closed shape, for gateway-built requests.
    #[default]
    Request,
    /// Downstream client request: unregistered fields are ignored and their names
    /// recorded. Discriminators (`type`, `role`) are values and stay strict.
    Client(&'a IgnoredFields),
    Upstream,
    Encoded,
}
impl DecodeDirection<'_> {
    pub(crate) fn client(self) -> bool {
        matches!(self, Self::Client(_))
    }
    pub(crate) fn fields(self, o: &Map<String, Value>, allowed: &[&str]) -> Result<(), CodecError> {
        match self {
            Self::Upstream => Ok(()),
            Self::Client(ignored) => {
                for key in o.keys().filter(|key| !allowed.contains(&key.as_str())) {
                    ignored.record(key);
                }
                Ok(())
            }
            Self::Request | Self::Encoded => fields(o, allowed),
        }
    }
}
/// Names of ignored client request fields, deduplicated and bounded; values are never kept.
#[derive(Debug, Default)]
pub struct IgnoredFields(std::cell::RefCell<std::collections::BTreeSet<String>>);
impl IgnoredFields {
    const MAX_NAMES: usize = 32;
    const MAX_NAME_BYTES: usize = 64;
    fn record(&self, name: &str) {
        let mut names = self.0.borrow_mut();
        if names.len() < Self::MAX_NAMES {
            let mut end = name.len().min(Self::MAX_NAME_BYTES);
            while !name.is_char_boundary(end) {
                end -= 1;
            }
            names.insert(name[..end].to_owned());
        }
    }
    pub fn into_names(self) -> Vec<String> {
        self.0.into_inner().into_iter().collect()
    }
}
/// Closed request shape; shared readers use their explicitly supplied direction.
pub(super) fn fields(o: &Map<String, Value>, allowed: &[&str]) -> Result<(), CodecError> {
    if let Some(_key) = o.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::UnknownField,
        ));
    }
    Ok(())
}
/// A replayed SDK derived view (`parsed`, `parsed_arguments`) must equal the naive JSON
/// parse of its authoritative raw body, mirroring the pinned SDK's derivation over this
/// opaque string; null means no derived value. The view is validated then dropped: it
/// never becomes IR state, is never emitted, and an unparseable body cannot carry a
/// non-null view.
pub(super) fn admit_parsed(
    _key: &'static str,
    parsed: Option<&Value>,
    raw: Option<&str>,
) -> Result<(), CodecError> {
    match (parsed, raw) {
        (None | Some(Value::Null), _) => Ok(()),
        (Some(_), None) => Err(CodecError::Invalid(
            crate::protocol::failure::ShapeReason::InvalidValue,
        )),
        (Some(v), Some(raw)) => {
            let expected: Value =
                crate::semantic::value::parse_json_view(raw.as_bytes()).map_err(|_| {
                    CodecError::Invalid(crate::protocol::failure::ShapeReason::InvalidValue)
                })?;
            if *v != expected {
                return Err(CodecError::Invalid(
                    crate::protocol::failure::ShapeReason::InvalidValue,
                ));
            }
            Ok(())
        }
    }
}
pub(super) fn string<'a>(
    o: &'a Map<String, Value>,
    key: &'static str,
) -> Result<&'a str, CodecError> {
    o.get(key)
        .and_then(Value::as_str)
        .ok_or(CodecError::Invalid(
            crate::protocol::failure::ShapeReason::InvalidValue,
        ))
}
pub(super) fn text(value: &str, kind: &'static str, max: usize) -> Result<Text, CodecError> {
    Text::new(value, kind, max)
        .map_err(|_| CodecError::Invalid(crate::protocol::failure::ShapeReason::InvalidValue))
}
pub(super) fn nullable_bool(
    o: &Map<String, Value>,
    key: &'static str,
) -> Result<Option<bool>, CodecError> {
    o.get(key)
        .filter(|v| !v.is_null())
        .map(|v| {
            v.as_bool().ok_or(CodecError::Invalid(
                crate::protocol::failure::ShapeReason::InvalidValue,
            ))
        })
        .transpose()
}
pub(super) fn raw_string(o: &Map<String, Value>, key: &'static str) -> Result<String, CodecError> {
    let s = string(o, key)?;
    if s.len() > MAX_TEXT_BYTES {
        return Err(CodecError::Limit);
    }
    Ok(s.to_owned())
}
pub(super) fn accept_status(o: &Map<String, Value>, expected: &str) -> Result<(), CodecError> {
    let actual = o.get("status").and_then(Value::as_str);
    let allowed = if expected == "incomplete" {
        actual.is_none() || matches!(actual, Some("completed" | "incomplete" | "in_progress"))
    } else {
        actual.is_none() || actual == Some(expected)
    };
    if !allowed {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_budget_counts_utf8_and_escaped_bytes_at_the_limit() {
        // Quotes, one three-byte character and one escaped newline occupy seven bytes.
        let mut value = Value::String(format!("你\n{}", "x".repeat(MAX_TOTAL_BYTES - 7)));
        assert!(bounded(&value).is_ok());
        let Value::String(text) = &mut value else {
            unreachable!()
        };
        text.push('x');
        assert!(matches!(bounded(&value), Err(CodecError::Limit)));
    }
}
