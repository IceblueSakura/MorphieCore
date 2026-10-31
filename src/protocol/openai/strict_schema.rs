//! The OpenAI Structured Outputs subset, checked only when encoding to an OpenAI target.
//! Other targets carry the same IR schema under their own rules.
//! https://developers.openai.com/api/docs/guides/structured-outputs#supported-schemas
use crate::semantic::task::generation::SchemaDocument;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

const MAX_PROPERTIES: usize = 5_000;
const MAX_ENUMS: usize = 1_000;
const MAX_CHARS: usize = 120_000;
const MAX_DEPTH: usize = 10;
/// Keywords outside the subset; anything else is left to the upstream.
const UNSUPPORTED: &[&str] = &[
    "allOf",
    "oneOf",
    "not",
    "if",
    "then",
    "else",
    "patternProperties",
    "propertyNames",
    "dependentRequired",
    "dependentSchemas",
    "prefixItems",
    "contains",
    "minContains",
    "maxContains",
    "unevaluatedProperties",
    "unevaluatedItems",
    "uniqueItems",
    "minProperties",
    "maxProperties",
];

/// Explicit strict carriage requires closed objects with every property required.
pub(super) fn admits(schema: &SchemaDocument) -> bool {
    check(schema, false)
}

/// Inferring strictness needs a known equivalent vocabulary; explicit strict
/// requests instead leave unknown keywords to upstream validation.
pub(super) fn admits_conditional(schema: &SchemaDocument) -> bool {
    check(schema, true)
}

fn check(schema: &SchemaDocument, known_keywords: bool) -> bool {
    let root = schema.root();
    let mut check = Check {
        known_keywords,
        visited: BTreeSet::new(),
        refs: vec![],
        properties: 0,
        enums: 0,
        chars: 0,
    };
    if check.walk(root, 0).is_none() {
        return false;
    }
    // Targets must be schema positions, not arbitrary JSON inside one.
    let resolve = |reference: &str| {
        target(root, reference).filter(|v| check.visited.contains(&std::ptr::from_ref(*v)))
    };
    if check.refs.iter().any(|r| resolve(r).is_none()) {
        return false;
    }
    // Only the root's alias chain needs a concrete object type. Other recursive
    // edges are checked by membership, never expanded into an infinite tree.
    let mut node = root;
    let mut seen = BTreeSet::new();
    loop {
        let Some(o) = node.as_object() else {
            return false;
        };
        if !seen.insert(std::ptr::from_ref(node)) {
            return false;
        }
        match o.get("$ref").map(|r| r.as_str().and_then(resolve)) {
            Some(Some(next)) => node = next,
            Some(None) => return false,
            None => {
                return !o.contains_key("anyOf")
                    && o.get("type").and_then(Value::as_str) == Some("object");
            }
        }
    }
}

struct Check<'a> {
    known_keywords: bool,
    visited: BTreeSet<*const Value>,
    refs: Vec<&'a str>,
    properties: usize,
    enums: usize,
    chars: usize,
}
impl<'a> Check<'a> {
    fn charge(total: &mut usize, n: usize, max: usize) -> Option<()> {
        *total = total.checked_add(n)?;
        (*total <= max).then_some(())
    }
    fn walk(&mut self, v: &'a Value, depth: usize) -> Option<()> {
        self.visited.insert(std::ptr::from_ref(v));
        let o = v.as_object()?;
        let types = types(o.get("type"))?;
        self.node(o, &types)?;
        let nested = depth + usize::from(types.contains(&"object") || types.contains(&"array"));
        if nested > MAX_DEPTH || o.keys().any(|k| UNSUPPORTED.contains(&k.as_str())) {
            return None;
        }
        for (key, value) in o {
            match key.as_str() {
                "properties" | "$defs" => {
                    let map = value.as_object()?;
                    if key == "properties" {
                        Self::charge(&mut self.properties, map.len(), MAX_PROPERTIES)?;
                    }
                    for (name, child) in map {
                        Self::charge(&mut self.chars, name.chars().count(), MAX_CHARS)?;
                        self.walk(child, if key == "$defs" { 0 } else { nested })?;
                    }
                }
                "items" => self.walk(value, nested)?,
                "additionalProperties" if !value.is_boolean() => self.walk(value, nested)?,
                "anyOf" => {
                    for child in value.as_array().filter(|a| !a.is_empty())? {
                        self.walk(child, depth)?;
                    }
                }
                "$ref" => self.refs.push(value.as_str()?),
                "enum" => {
                    let values = value.as_array()?;
                    Self::charge(&mut self.enums, values.len(), MAX_ENUMS)?;
                    for value in values {
                        Self::charge(&mut self.chars, string_chars(value), MAX_CHARS)?;
                    }
                }
                "const" => Self::charge(&mut self.chars, string_chars(value), MAX_CHARS)?,
                "type"
                | "required"
                | "additionalProperties"
                | "title"
                | "description"
                | "default"
                | "examples"
                | "$schema"
                | "pattern"
                | "format"
                | "minLength"
                | "maxLength"
                | "minimum"
                | "maximum"
                | "exclusiveMinimum"
                | "exclusiveMaximum"
                | "multipleOf"
                | "minItems"
                | "maxItems" => {}
                _ if self.known_keywords => return None,
                _ => {}
            }
        }
        Some(())
    }
    fn node(&self, o: &Map<String, Value>, types: &[&str]) -> Option<()> {
        if o.contains_key("$ref") {
            let annotation = |k: &String| {
                matches!(
                    k.as_str(),
                    "$ref" | "title" | "description" | "default" | "examples" | "$defs" | "$schema"
                )
            };
            return o.keys().all(annotation).then_some(());
        }
        if types.is_empty() && !o.contains_key("anyOf") {
            return None;
        }
        if o.get("type").is_some_and(Value::is_array)
            && (types.len() != 2 || !types.contains(&"null"))
        {
            return None;
        }
        let object = types.contains(&"object");
        let array = types.contains(&"array");
        if !object
            && ["properties", "required", "additionalProperties"]
                .iter()
                .any(|key| o.contains_key(*key))
            || array != o.contains_key("items")
        {
            return None;
        }
        if object {
            if o.get("additionalProperties") != Some(&Value::Bool(false)) {
                return None;
            }
            let names: BTreeSet<&str> = match o.get("properties") {
                Some(v) => v.as_object()?.keys().map(String::as_str).collect(),
                None => BTreeSet::new(),
            };
            let required: Vec<&str> = match o.get("required") {
                Some(v) => v
                    .as_array()?
                    .iter()
                    .map(Value::as_str)
                    .collect::<Option<_>>()?,
                None => vec![],
            };
            // Every property is required exactly once.
            if required.len() != names.len()
                || required.into_iter().collect::<BTreeSet<_>>() != names
            {
                return None;
            }
        }
        Some(())
    }
}
fn types(value: Option<&Value>) -> Option<Vec<&str>> {
    let names: Vec<&str> = match value {
        None => return Some(vec![]),
        Some(Value::String(s)) => vec![s],
        Some(v) => v
            .as_array()
            .filter(|a| !a.is_empty())?
            .iter()
            .map(Value::as_str)
            .collect::<Option<_>>()?,
    };
    let mut seen = BTreeSet::new();
    names
        .iter()
        .all(|name| {
            matches!(
                *name,
                "object" | "array" | "string" | "number" | "integer" | "boolean" | "null"
            ) && seen.insert(*name)
        })
        .then_some(names)
}
fn string_chars(v: &Value) -> usize {
    match v {
        Value::String(s) => s.chars().count(),
        Value::Array(a) => a.iter().map(string_chars).sum(),
        Value::Object(o) => o
            .iter()
            .map(|(k, v)| k.chars().count() + string_chars(v))
            .sum(),
        _ => 0,
    }
}
/// A local `#` fragment: percent-decoded, then a JSON pointer into the same document.
fn target<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    let fragment = reference.strip_prefix('#')?.as_bytes();
    let mut bytes = Vec::with_capacity(fragment.len());
    let mut i = 0;
    while i < fragment.len() {
        if fragment[i] == b'%' {
            let hex = std::str::from_utf8(fragment.get(i + 1..i + 3)?).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            bytes.push(fragment[i]);
            i += 1;
        }
    }
    let pointer = String::from_utf8(bytes).ok()?;
    // serde_json treats an unknown `~` escape as a literal key; the standard does not.
    if pointer
        .split('~')
        .skip(1)
        .any(|s| !s.starts_with(['0', '1']))
    {
        return None;
    }
    root.pointer(&pointer)
}
