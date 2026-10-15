//! Selected responseJsonSchema subset; never simplify a caller's constraints.
//! https://ai.google.dev/api/generate-content#GenerationConfig
use serde_json::Value;

pub(super) fn admits(schema: &Value) -> bool {
    let Some(object) = schema.as_object() else {
        return false;
    };
    object.iter().all(|(key, value)| match key.as_str() {
        "type" => match value {
            Value::String(t) => kind(t),
            Value::Array(types) => {
                !types.is_empty() && types.iter().all(|t| t.as_str().is_some_and(kind))
            }
            _ => false,
        },
        "title" | "description" => value.is_string(),
        "format" => matches!(value.as_str(), Some("date" | "time" | "date-time")),
        "enum" => value.as_array().is_some_and(|values| {
            !values.is_empty() && values.iter().all(|v| v.is_string() || v.is_number())
        }),
        "minimum" | "maximum" => value.as_f64().is_some_and(f64::is_finite),
        "minItems" | "maxItems" => value.as_u64().is_some(),
        "required" | "propertyOrdering" => value
            .as_array()
            .is_some_and(|values| values.iter().all(Value::is_string)),
        "properties" => value
            .as_object()
            .is_some_and(|properties| properties.values().all(admits)),
        "items" => admits(value),
        "additionalProperties" => value.is_boolean() || admits(value),
        "anyOf" | "prefixItems" => value
            .as_array()
            .is_some_and(|values| !values.is_empty() && values.iter().all(admits)),
        // oneOf is treated as anyOf upstream. References need a separately
        // checked expansion contract; neither may silently weaken constraints.
        _ => false,
    })
}

fn kind(value: &str) -> bool {
    matches!(
        value,
        "object" | "array" | "string" | "number" | "integer" | "boolean" | "null"
    )
}
