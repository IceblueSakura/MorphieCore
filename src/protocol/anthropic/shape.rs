//! Closed native shapes over the shared exact, duplicate-rejecting JSON parser.
use crate::{
    protocol::CodecError,
    semantic::{
        task::generation::{MAX_ITEMS, MAX_TEXT_BYTES, MAX_TOTAL_BYTES},
        value::*,
    },
};
use serde_json::{Map, Value};

pub(super) fn parse(bytes: &[u8]) -> Result<Value, CodecError> {
    parse_json(bytes, JsonLimits::ENVELOPE).map_err(|e| match e {
        JsonError::Invalid => CodecError::Invalid("JSON"),
        JsonError::Limit => CodecError::Limit,
    })
}
pub(super) fn bounded(value: &Value) -> Result<(), CodecError> {
    json_size(value, MAX_TOTAL_BYTES)
        .map(|_| ())
        .map_err(|_| CodecError::Limit)
}
pub(super) fn object<'a>(
    v: &'a Value,
    allowed: &[&str],
) -> Result<&'a Map<String, Value>, CodecError> {
    let o = v
        .as_object()
        .ok_or(CodecError::Invalid("Anthropic object"))?;
    if o.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(CodecError::Unsupported("Anthropic field".into()));
    }
    Ok(o)
}
pub(super) fn required<'a>(
    o: &'a Map<String, Value>,
    key: &'static str,
) -> Result<&'a Value, CodecError> {
    o.get(key).ok_or(CodecError::Invalid(key))
}
pub(super) fn string(v: &Value) -> Result<String, CodecError> {
    let s = v.as_str().ok_or(CodecError::Invalid("Anthropic string"))?;
    text(s)?;
    Ok(s.into())
}
pub(super) fn text(s: &str) -> Result<usize, CodecError> {
    if s.len() > MAX_TEXT_BYTES {
        return Err(CodecError::Limit);
    }
    Ok(s.len())
}
pub(super) fn label(s: &str) -> Result<(), CodecError> {
    if s.is_empty() || s.len() > 256 || s.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(CodecError::Invalid("Anthropic label"));
    }
    Ok(())
}
pub(super) fn name(s: &str, limit: usize) -> Result<(), CodecError> {
    if s.is_empty()
        || s.len() > limit
        || !s
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
    {
        return Err(CodecError::Invalid("Anthropic name or call id"));
    }
    Ok(())
}
pub(super) fn uint(v: &Value) -> Result<u64, CodecError> {
    v.as_u64().ok_or(CodecError::Invalid("Anthropic counter"))
}
pub(super) fn boolean(v: &Value) -> Result<bool, CodecError> {
    v.as_bool().ok_or(CodecError::Invalid("Anthropic boolean"))
}
pub(super) fn list(v: &Value) -> Result<&[Value], CodecError> {
    let a = v.as_array().ok_or(CodecError::Invalid("Anthropic array"))?;
    if a.len() > MAX_ITEMS {
        return Err(CodecError::Limit);
    }
    Ok(a)
}
pub(super) fn presence<T>(
    o: &Map<String, Value>,
    key: &str,
    read: impl FnOnce(&Value) -> Result<T, CodecError>,
) -> Result<Presence<T>, CodecError> {
    match o.get(key) {
        None => Ok(Presence::Absent),
        Some(Value::Null) => Ok(Presence::Null),
        Some(v) => Ok(Presence::Value(read(v)?)),
    }
}
pub(super) fn put<T>(
    o: &mut Map<String, Value>,
    key: &str,
    p: &Presence<T>,
    wire: impl FnOnce(&T) -> Value,
) {
    match p {
        Presence::Absent => {}
        Presence::Null => {
            o.insert(key.into(), Value::Null);
        }
        Presence::Value(v) => {
            o.insert(key.into(), wire(v));
        }
    }
}
pub(super) fn nonnull<T>(p: &Presence<T>) -> Result<(), CodecError> {
    if matches!(p, Presence::Null) {
        return Err(CodecError::Invalid("Anthropic null"));
    }
    Ok(())
}
pub(super) fn charge(total: &mut usize, n: usize) -> Result<(), CodecError> {
    *total = total.checked_add(n).ok_or(CodecError::Limit)?;
    if *total > MAX_TOTAL_BYTES {
        return Err(CodecError::Limit);
    }
    Ok(())
}
pub(super) fn count(n: usize) -> Result<(), CodecError> {
    if n > MAX_ITEMS {
        return Err(CodecError::Limit);
    }
    Ok(())
}
pub(super) fn node_charge(total: &mut usize, n: usize) -> Result<(), CodecError> {
    *total = total.checked_add(n).ok_or(CodecError::Limit)?;
    if *total > JsonLimits::ENVELOPE.nodes {
        return Err(CodecError::Limit);
    }
    Ok(())
}
/// Count borrowed, already validated JSON trees before cloning them into wire output.
pub(super) fn value_nodes(value: &Value) -> Result<usize, CodecError> {
    let mut nodes = 0;
    let mut stack = vec![value];
    while let Some(value) = stack.pop() {
        node_charge(&mut nodes, 1)?;
        match value {
            Value::Object(o) => {
                node_charge(&mut nodes, o.len())?;
                stack.extend(o.values());
            }
            Value::Array(a) => stack.extend(a),
            _ => {}
        }
    }
    Ok(nodes)
}
