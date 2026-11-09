//! Qualified function-argument subset, separate from text response schemas.
//! Local Google SDK modes are request-wide, not per-definition strict flags.
use crate::{
    protocol::{CodecError, failure::CarrierReason},
    semantic::task::generation::*,
};
use serde_json::Value;

pub(crate) fn admits(schema: &SchemaDocument) -> bool {
    schema.root().get("type").and_then(Value::as_str) == Some("object") && node(schema.root())
}

fn node(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    // Current Agent strict serializers express optional fields as required
    // nullable values. Admit that union, not arbitrary schema composition.
    if let Some(branches) = object.get("anyOf") {
        return object.iter().all(|(key, value)| match key.as_str() {
            "anyOf" => true,
            "title" | "description" => value.is_string(),
            _ => false,
        }) && branches.as_array().is_some_and(|branches| {
            branches.len() == 2
                && branches.iter().enumerate().any(|(index, branch)| {
                    branch.as_object().is_some_and(|null| {
                        null.len() == 1 && null.get("type").and_then(Value::as_str) == Some("null")
                    }) && node(&branches[1 - index])
                })
        });
    }
    let Some(kind) = object.get("type").and_then(Value::as_str) else {
        return false;
    };
    if !matches!(
        kind,
        "object" | "array" | "string" | "integer" | "number" | "boolean"
    ) {
        return false;
    }
    if kind == "object" && object.get("additionalProperties") != Some(&Value::Bool(false))
        || kind == "array" && !object.contains_key("items")
    {
        return false;
    }
    object.iter().all(|(key, value)| match key.as_str() {
        "type" => true,
        "title" | "description" => value.is_string(),
        "properties" if kind == "object" => value
            .as_object()
            .is_some_and(|properties| properties.values().all(node)),
        "required" if kind == "object" => value.as_array().is_some_and(|names| {
            let mut seen = std::collections::BTreeSet::new();
            names.iter().all(|name| {
                name.as_str().is_some_and(|name| {
                    seen.insert(name)
                        && object
                            .get("properties")
                            .and_then(Value::as_object)
                            .is_some_and(|properties| properties.contains_key(name))
                })
            })
        }),
        "additionalProperties" if kind == "object" => value == false,
        "items" if kind == "array" => node(value),
        "enum" => value.as_array().is_some_and(|values| {
            !values.is_empty()
                && values.iter().all(|value| match kind {
                    "string" => value.is_string(),
                    "integer" => exact_integer(value),
                    "number" => {
                        exact_integer(value)
                            || value.as_f64().is_some_and(|number| {
                                number.is_finite()
                                    && serde_json::to_value(number).ok().as_ref() == Some(value)
                            })
                    }
                    _ => false,
                })
        }),
        // In particular, length, pattern, references and compositions must not
        // inherit the text-output subset or silently disappear in validated mode.
        _ => false,
    })
}

fn exact_integer(value: &Value) -> bool {
    // Keep the qualified numeric vocabulary within binary64's safe integer
    // range; larger native enum literals are not an exact carrier.
    const MAX: u64 = (1 << 53) - 1;
    value.as_i64().is_some_and(|n| n.unsigned_abs() <= MAX)
        || value.as_u64().is_some_and(|n| n <= MAX)
}

/// Every declaration shares the decoding mode. Only switch to validation when
/// the whole inventory has a qualified schema; otherwise keep honest rejection.
pub(crate) fn validated(request: &GenerationRequest) -> Result<bool, CodecError> {
    let unsupported = || CodecError::Unsupported(CarrierReason::Unavailable);
    let mut strict = false;
    for tool in request.tools() {
        let ToolDefinition::Function(tool) = tool else {
            return Err(unsupported());
        };
        match tool.strict {
            FunctionStrictness::Explicit(true) => strict = true,
            FunctionStrictness::Omitted(StrictDefault::Conditional) => return Err(unsupported()),
            _ => {}
        }
    }
    if strict
        && request.tools().iter().any(|tool| {
            let ToolDefinition::Function(tool) = tool else {
                return true;
            };
            tool.parameters
                .as_ref()
                .is_some_and(|schema| !admits(schema))
        })
    {
        return Err(unsupported());
    }
    Ok(strict)
}
