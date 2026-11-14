//! Qualified function-argument subset, separate from text response schemas.
//! Local Google SDK modes are request-wide, not per-definition strict flags.
use crate::{
    protocol::{
        CodecError,
        failure::{CarrierReason, ToolConstraint as Reason},
    },
    semantic::task::generation::*,
};
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) fn admits(schema: &SchemaDocument) -> bool {
    check_schema(schema).is_ok()
}
fn check_schema(schema: &SchemaDocument) -> Result<(), Reason> {
    let root = schema.root();
    let mut check = Check {
        root,
        nodes: BTreeMap::new(),
    };
    if let Some(definitions) = root.get("$defs") {
        for value in definitions.as_object().ok_or(Reason::Structure)?.values() {
            check.walk(value, 0)?;
        }
    }
    if check.walk(root, 0)?.object {
        Ok(())
    } else {
        Err(Reason::RootObject)
    }
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
    // None is an active visit. Cache height so shared DAGs cannot bypass depth bounds.
    nodes: BTreeMap<*const Value, Option<Checked>>,
}
impl<'a> Check<'a> {
    fn walk(&mut self, value: &'a Value, depth: usize) -> Result<Checked, Reason> {
        if depth > MAX_DEPTH {
            return Err(Reason::Depth);
        }
        let identity = std::ptr::from_ref(value);
        if let Some(cached) = self.nodes.get(&identity) {
            let cached = cached.ok_or(Reason::Reference)?;
            return if depth + cached.height <= MAX_DEPTH {
                Ok(cached)
            } else {
                Err(Reason::Depth)
            };
        }
        self.nodes.insert(identity, None);
        let checked = self.node(value, depth)?;
        self.nodes.insert(identity, Some(checked));
        Ok(checked)
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
    fn node(&mut self, value: &'a Value, depth: usize) -> Result<Checked, Reason> {
        let object = value.as_object().ok_or(Reason::Structure)?;
        let root = std::ptr::eq(value, self.root);
        if let Some(reference) = object.get("$ref") {
            // No sibling conjunction/suppression semantics are selected.
            if object
                .keys()
                .any(|key| key != "$ref" && !(root && key == "$defs"))
            {
                return Err(Reason::Reference);
            }
            let target = self
                .reference(reference.as_str().ok_or(Reason::Reference)?)
                .ok_or(Reason::Reference)?;
            let child = self.walk(target, depth + 1)?;
            return Ok(Checked {
                height: child.height + 1,
                object: child.object,
            });
        }
        // Exact nullable pairs, not general composition. A nullable root is not an object.
        if let Some(branches) = object.get("anyOf") {
            if !object.iter().all(|(key, value)| match key.as_str() {
                "anyOf" => true,
                "title" | "description" => value.is_string(),
                _ => false,
            }) {
                return Err(Reason::Composition);
            }
            let branches = branches
                .as_array()
                .filter(|b| b.len() == 2)
                .ok_or(Reason::Composition)?;
            let null = branches
                .iter()
                .position(|branch| {
                    branch.as_object().is_some_and(|null| {
                        null.len() == 1 && null.get("type").and_then(Value::as_str) == Some("null")
                    })
                })
                .ok_or(Reason::Composition)?;
            let child = self.walk(&branches[1 - null], depth + 1)?;
            return Ok(Checked {
                height: child.height + 1,
                object: false,
            });
        }
        let kind = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or(Reason::Type)?;
        if !matches!(
            kind,
            "object" | "array" | "string" | "integer" | "number" | "boolean"
        ) {
            return Err(Reason::Type);
        }
        if kind == "object" && object.get("additionalProperties") != Some(&Value::Bool(false)) {
            return Err(Reason::ObjectClosure);
        }
        if kind == "array" && !object.contains_key("items") {
            return Err(Reason::Items);
        }
        let mut height = 0;
        for (key, value) in object {
            let admitted = match key.as_str() {
                "type" => true,
                "$defs" if root => true,
                "title" | "description" => value.is_string(),
                "properties" if kind == "object" => {
                    for child in value.as_object().ok_or(Reason::Structure)?.values() {
                        height = height.max(self.walk(child, depth + 1)?.height + 1);
                    }
                    true
                }
                "required" if kind == "object" => {
                    let mut seen = std::collections::BTreeSet::new();
                    value
                        .as_array()
                        .ok_or(Reason::Structure)?
                        .iter()
                        .all(|name| {
                            name.as_str().is_some_and(|name| {
                                seen.insert(name)
                                    && object
                                        .get("properties")
                                        .and_then(Value::as_object)
                                        .is_some_and(|p| p.contains_key(name))
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
                // Unknown keys remain closed categories; never retain their spelling or values.
                _ => {
                    return Err(match key.as_str() {
                        "minLength" => Reason::MinLength,
                        "maxLength" => Reason::MaxLength,
                        "pattern" => Reason::Pattern,
                        "minimum" => Reason::Minimum,
                        "maximum" => Reason::Maximum,
                        "minItems" => Reason::MinItems,
                        "maxItems" => Reason::MaxItems,
                        "oneOf" | "allOf" | "not" => Reason::Composition,
                        _ => Reason::Keyword,
                    });
                }
            };
            if !admitted {
                return Err(Reason::Structure);
            }
        }
        Ok(Checked {
            height,
            object: kind == "object",
        })
    }
}
fn exact_integer(value: &Value) -> bool {
    // Preserve the qualified binary64-safe numeric vocabulary.
    const MAX: u64 = (1 << 53) - 1;
    value.as_i64().is_some_and(|n| n.unsigned_abs() <= MAX)
        || value.as_u64().is_some_and(|n| n <= MAX)
}
/// Every declaration shares the decoding mode. Validate the whole inventory.
pub(crate) fn validated(request: &GenerationRequest) -> Result<bool, CodecError> {
    // NONE makes argument/cardinality guarantees vacuous without deleting declarations.
    if request.tool_choice() == Some(&ToolChoice::None) {
        return Ok(false);
    }
    let unsupported = |declaration, constraint| {
        CodecError::Unsupported(CarrierReason::ToolConstraint {
            declaration,
            constraint,
        })
    };
    let mut strict = false;
    for (index, tool) in request.tools().iter().enumerate() {
        let ToolDefinition::Function(tool) = tool else {
            return Err(unsupported(index, Reason::Declaration));
        };
        match tool.strict {
            FunctionStrictness::Explicit(true) => strict = true,
            FunctionStrictness::Omitted(StrictDefault::Conditional) => {
                return Err(unsupported(index, Reason::ConditionalStrictness));
            }
            _ => {}
        }
    }
    if strict {
        for (index, tool) in request.tools().iter().enumerate() {
            if let ToolDefinition::Function(tool) = tool
                && let Some(schema) = &tool.parameters
            {
                check_schema(schema).map_err(|reason| unsupported(index, reason))?;
            }
        }
    }
    Ok(strict)
}
