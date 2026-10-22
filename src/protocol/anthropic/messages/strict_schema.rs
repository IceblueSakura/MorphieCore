//! Target-only strict schemas and request-wide complexity. Client parsing keeps the schema.
//! https://platform.claude.com/docs/en/build-with-claude/structured-outputs#json-schema-limitations
use serde_json::Value;

pub(super) fn admits_request(request: &super::Request) -> bool {
    use crate::semantic::value::Presence;
    // Definitions added mid-conversation share the request-wide grammar limits.
    let added = request.messages.iter().flat_map(|m| match &m.content {
        super::MessageContent::Blocks(blocks) => blocks.as_slice(),
        super::MessageContent::Text(_) => &[],
    });
    let tools: Vec<_> = request
        .tools
        .value()
        .into_iter()
        .flatten()
        .chain(added.filter_map(|block| match &block.content {
            super::Block::ToolAddition(super::ToolTarget::Definition(tool)) => Some(tool),
            _ => None,
        }))
        .filter(|tool| tool.strict == Presence::Value(true))
        .collect();
    // These native grammar limits are request-wide, including JSON output.
    if tools.len() > 20 {
        return false;
    }
    let mut budget = Budget::default();
    for tool in tools {
        if !admits(tool.input_schema.root()) || !budget.charge(tool.input_schema.root()) {
            return false;
        }
    }
    // JSON outputs share the native subset/casing limits, not only its grammar budget.
    request
        .output_config
        .value()
        .and_then(|config| config.format.value())
        .is_none_or(|schema| admits(schema.root()) && budget.charge(schema.root()))
}

#[derive(Default)]
struct Budget {
    optional: usize,
    unions: usize,
}
impl Budget {
    fn charge(&mut self, schema: &Value) -> bool {
        let Some(object) = schema.as_object() else {
            return false;
        };
        if let Some(properties) = object.get("properties").and_then(Value::as_object) {
            let required = object.get("required").and_then(Value::as_array);
            for (name, property) in properties {
                if !required.is_some_and(|names| names.iter().any(|n| n.as_str() == Some(name))) {
                    self.optional += 1;
                }
                if property.get("anyOf").is_some()
                    || property.get("type").is_some_and(Value::is_array)
                {
                    self.unions += 1;
                }
                if self.optional > 24 || self.unions > 16 || !self.charge(property) {
                    return false;
                }
            }
        }
        for (key, value) in object {
            let valid = match key.as_str() {
                "items" => self.charge(value),
                "$defs" | "definitions" => value
                    .as_object()
                    .is_some_and(|map| map.values().all(|v| self.charge(v))),
                "anyOf" | "allOf" => value
                    .as_array()
                    .is_some_and(|values| values.iter().all(|v| self.charge(v))),
                _ => true,
            };
            if !valid {
                return false;
            }
        }
        true
    }
}

fn admits(schema: &Value) -> bool {
    let Some(object) = schema.as_object() else {
        return false;
    };
    let is_object = object.get("type").is_some_and(|t| {
        t.as_str() == Some("object")
            || t.as_array()
                .is_some_and(|types| types.iter().any(|t| t.as_str() == Some("object")))
    }) || object.contains_key("properties");
    if is_object && object.get("additionalProperties") != Some(&Value::Bool(false)) {
        return false;
    }
    object.iter().all(|(key, value)| match key.as_str() {
        "type" => match value {
            Value::String(t) => kind(t),
            Value::Array(types) => {
                !types.is_empty() && types.iter().all(|t| t.as_str().is_some_and(kind))
            }
            _ => false,
        },
        "title" | "description" => value.is_string(),
        "default" => true,
        // Native strict output does not guarantee enum/const string casing.
        "const" => !value.is_string() && !value.is_object() && !value.is_array(),
        "enum" => value.as_array().is_some_and(|values| {
            !values.is_empty()
                && values
                    .iter()
                    .all(|v| !v.is_string() && !v.is_object() && !v.is_array())
        }),
        "format" => matches!(
            value.as_str(),
            Some(
                "date-time"
                    | "time"
                    | "date"
                    | "duration"
                    | "email"
                    | "hostname"
                    | "uri"
                    | "ipv4"
                    | "ipv6"
                    | "uuid"
            )
        ),
        "minItems" => matches!(value.as_u64(), Some(0 | 1)),
        "required" => value
            .as_array()
            .is_some_and(|values| values.iter().all(Value::is_string)),
        "properties" => value
            .as_object()
            .is_some_and(|properties| properties.values().all(admits)),
        "items" => admits(value),
        "additionalProperties" => value == &Value::Bool(false),
        "anyOf" | "allOf" => value
            .as_array()
            .is_some_and(|values| !values.is_empty() && values.iter().all(admits)),
        // References and regex need their own native validation; reject rather
        // than run an SDK's constraint-removing simplification.
        _ => false,
    })
}

fn kind(value: &str) -> bool {
    matches!(
        value,
        "object" | "array" | "string" | "number" | "integer" | "boolean" | "null"
    )
}
