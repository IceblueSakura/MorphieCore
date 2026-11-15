//! Source-only reports share Generation bounds; storage paths are not native data.
use super::*;
use crate::semantic::value::json_size;

pub(crate) fn check_budget<'a>(
    values: impl IntoIterator<Item = &'a Value>,
) -> Result<(), CodecError> {
    let mut remaining = MAX_TOTAL_BYTES;
    for value in values {
        let bytes = json_size(value, remaining).map_err(|_| CodecError::Limit)?;
        remaining = remaining.checked_sub(bytes).ok_or(CodecError::Limit)?;
    }
    Ok(())
}

/// Filtering known reports can leave a large, mostly empty object allocation.
/// Rebuild object tables before retaining them; values and field order are unchanged.
pub(super) fn compact(value: Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| (key, compact(value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(compact).collect()),
        value => value,
    }
}

fn text_bytes(value: &String) -> usize {
    value.capacity().max(value.len().saturating_mul(64))
}

/// Conservative live tree storage and serialization working space, not a fixed
/// maximum report charge. Object tables have been compacted at retention.
fn retained_value(value: &Value) -> usize {
    let dynamic = match value {
        Value::Null | Value::Bool(_) => 0,
        Value::Number(value) => json_size(value, MAX_TOTAL_BYTES)
            .unwrap_or(MAX_TOTAL_BYTES)
            .saturating_mul(128),
        Value::String(value) => text_bytes(value),
        Value::Array(values) => values.iter().fold(
            values
                .capacity()
                .saturating_mul(std::mem::size_of::<Value>()),
            |bytes, value| bytes.saturating_add(retained_value(value)),
        ),
        Value::Object(values) => retained_map(values.iter()),
    };
    std::mem::size_of_val(value).saturating_add(dynamic)
}

fn retained_map<'a>(values: impl Iterator<Item = (&'a String, &'a Value)>) -> usize {
    values.fold(1024usize, |bytes, (key, value)| {
        bytes
            .saturating_add(1024)
            .saturating_add(text_bytes(key))
            .saturating_add(retained_value(value))
    })
}

impl ResponseExtras {
    pub(super) fn retained_bytes(&self) -> usize {
        std::mem::size_of_val(self)
            .saturating_add(self.origin.as_str().len().saturating_mul(64))
            .saturating_add(self.response_id.as_str().len().saturating_mul(64))
            .saturating_add(retained_map(self.values.iter()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::openai::{Dialect, GenerationApi, GenerationCodec};
    use serde_json::json;

    fn decode(filters: Value) -> FidelityRecords {
        let source = GenerationCodec::new(
            GenerationApi::Responses,
            Dialect::Grok,
            Some(ReplayOrigin::new("synthetic").unwrap()),
        );
        let body = json!({"id":"r","object":"response","created_at":1,"model":"m",
            "status":"completed","output":[],"content_filters":filters});
        source
            .decode_response(body.to_string().as_bytes())
            .unwrap()
            .fidelity
    }

    #[test]
    fn report_collection_keeps_the_generation_byte_bound() {
        let value = Value::String("x".repeat(MAX_TOTAL_BYTES / 2));
        check_budget([&value]).unwrap();
        assert_eq!(check_budget([&value, &value]), Err(CodecError::Limit));
    }

    #[test]
    fn retained_report_growth_is_charged_to_delivery_context() {
        let small = decode(json!("x"));
        let large = decode(json!("x".repeat(10_000)));
        assert!(large.retained_bytes() >= small.retained_bytes() + 9_999);
    }
}
