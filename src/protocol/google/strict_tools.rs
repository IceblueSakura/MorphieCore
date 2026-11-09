//! Qualified function-argument subset, separate from text response schemas.
//! Local Google SDK modes are request-wide, not per-definition strict flags.
use crate::{
    protocol::{CodecError, failure::CarrierReason},
    semantic::task::generation::*,
};
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) fn admits(schema: &SchemaDocument) -> bool {
    let root = schema.root();
    let mut check = Check {
        root,
        nodes: BTreeMap::new(),
    };
    if let Some(definitions) = root.get("$defs") {
        let Some(definitions) = definitions.as_object() else {
            return false;
        };
        if definitions
            .values()
            .any(|value| check.walk(value, 0).is_none())
        {
            return false;
        }
    }
    check.walk(root, 0).is_some_and(|node| node.object)
}

// Physical JSON bounds do not bound a shallow reference graph's path length.
const MAX_DEPTH: usize = 64;

#[derive(Clone, Copy)]
struct Checked {
    height: usize,
    object: bool,
}

struct Check<'a> {
    root: &'a Value,
    // None is an active visit. Cache subtree height as well as validity so a
    // shared DAG is checked once without bypassing the expanded-path bound.
    nodes: BTreeMap<*const Value, Option<Checked>>,
}

impl<'a> Check<'a> {
    fn walk(&mut self, value: &'a Value, depth: usize) -> Option<Checked> {
        if depth > MAX_DEPTH {
            return None;
        }
        let identity = std::ptr::from_ref(value);
        if let Some(cached) = self.nodes.get(&identity) {
            let cached = (*cached)?;
            return (depth + cached.height <= MAX_DEPTH).then_some(cached);
        }
        self.nodes.insert(identity, None);
        let checked = self.node(value, depth)?;
        self.nodes.insert(identity, Some(checked));
        Some(checked)
    }

    fn reference(&self, reference: &str) -> Option<&'a Value> {
        let token = reference.strip_prefix("#/$defs/")?;
        if token.contains(['/', '%']) {
            return None;
        }
        let mut chars = token.chars();
        while let Some(c) = chars.next() {
            if c == '~' && !matches!(chars.next(), Some('0' | '1')) {
                return None;
            }
        }
        self.root.pointer(reference.strip_prefix('#')?)
    }

    fn node(&mut self, value: &'a Value, depth: usize) -> Option<Checked> {
        let object = value.as_object()?;
        let root = std::ptr::eq(value, self.root);
        if let Some(reference) = object.get("$ref") {
            // Do not choose sibling conjunction/suppression semantics. Only
            // the root's definition repository may accompany a reference.
            if object
                .keys()
                .any(|key| key != "$ref" && !(root && key == "$defs"))
            {
                return None;
            }
            let target = self.reference(reference.as_str()?)?;
            let child = self.walk(target, depth + 1)?;
            return Some(Checked {
                height: child.height + 1,
                object: child.object,
            });
        }
        // Current Agent optional fields use a supported node and exact null,
        // not arbitrary composition. A root nullable union is not an object.
        if let Some(branches) = object.get("anyOf") {
            if !object.iter().all(|(key, value)| match key.as_str() {
                "anyOf" => true,
                "title" | "description" => value.is_string(),
                _ => false,
            }) {
                return None;
            }
            let branches = branches.as_array().filter(|branches| branches.len() == 2)?;
            let null = branches.iter().position(|branch| {
                branch.as_object().is_some_and(|null| {
                    null.len() == 1 && null.get("type").and_then(Value::as_str) == Some("null")
                })
            })?;
            let child = self.walk(&branches[1 - null], depth + 1)?;
            return Some(Checked {
                height: child.height + 1,
                object: false,
            });
        }
        let kind = object.get("type")?.as_str()?;
        if !matches!(
            kind,
            "object" | "array" | "string" | "integer" | "number" | "boolean"
        ) || kind == "object" && object.get("additionalProperties") != Some(&Value::Bool(false))
            || kind == "array" && !object.contains_key("items")
        {
            return None;
        }
        let mut height = 0;
        for (key, value) in object {
            let admitted = match key.as_str() {
                "type" => true,
                "$defs" if root => true, // Every definition was checked before the root.
                "title" | "description" => value.is_string(),
                "properties" if kind == "object" => {
                    for child in value.as_object()?.values() {
                        height = height.max(self.walk(child, depth + 1)?.height + 1);
                    }
                    true
                }
                "required" if kind == "object" => {
                    let mut seen = std::collections::BTreeSet::new();
                    value.as_array()?.iter().all(|name| {
                        name.as_str().is_some_and(|name| {
                            seen.insert(name)
                                && object
                                    .get("properties")
                                    .and_then(Value::as_object)
                                    .is_some_and(|properties| properties.contains_key(name))
                        })
                    })
                }
                "additionalProperties" if kind == "object" => value == false,
                "items" if kind == "array" => {
                    height = height.max(self.walk(value, depth + 1)?.height + 1);
                    true
                }
                "enum" => value.as_array().is_some_and(|values| {
                    !values.is_empty()
                        && values.iter().all(|value| match kind {
                            "string" => value.is_string(),
                            "integer" => exact_integer(value),
                            "number" => {
                                exact_integer(value)
                                    || value.as_f64().is_some_and(|number| {
                                        number.is_finite()
                                            && serde_json::to_value(number).ok().as_ref()
                                                == Some(value)
                                    })
                            }
                            _ => false,
                        })
                }),
                // Do not inherit the separate text-output subset.
                _ => false,
            };
            if !admitted {
                return None;
            }
        }
        Some(Checked {
            height,
            object: kind == "object",
        })
    }
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
    // Native NONE prohibits every call, so argument and cardinality guarantees
    // are vacuous. Keep the declarations; do not request validated decoding.
    if request.tool_choice() == Some(&ToolChoice::None) {
        return Ok(false);
    }
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
