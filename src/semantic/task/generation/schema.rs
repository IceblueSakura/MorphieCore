//! Immutable schema admission, not instance evaluation or remote reference resolution.
use super::{GenerationError, MAX_TEXT_BYTES, MAX_TOTAL_BYTES, pattern};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

const MAX_JSON_DEPTH: usize = 64;
const MAX_JSON_NODES: usize = 65_536;
const MAX_SCHEMA_NODES: usize = 16_384;
const MAX_REFS: usize = 8_192;
const MAX_ENUMS: usize = 8_192;
// Fixed local Structured Outputs profile, not universal model capability limits.
const STRICT_PROPERTIES: usize = 5_000;
const STRICT_ENUMS: usize = 1_000;
const STRICT_CHARS: usize = 120_000;
const STRICT_DEPTH: usize = 10;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Mode {
    General,
    Normalize,
    Explicit,
}
fn invalid() -> GenerationError {
    GenerationError::InvalidSchema
}
/// Closed format registry from the JSON Schema 2020-12 validation vocabulary. Unknown
/// names are rejected rather than ignored; this is not a claim that they are invalid
/// in every JSON Schema dialect.
const FORMATS: &[&str] = &[
    "date",
    "time",
    "date-time",
    "duration",
    "email",
    "idn-email",
    "hostname",
    "idn-hostname",
    "ipv4",
    "ipv6",
    "uri",
    "uri-reference",
    "uri-template",
    "iri",
    "iri-reference",
    "uuid",
    "json-pointer",
    "relative-json-pointer",
    "regex",
];
fn format_admitted(value: &str) -> bool {
    FORMATS.contains(&value)
}
fn charge(total: &mut usize, n: usize, max: usize) -> Result<(), GenerationError> {
    *total = total.checked_add(n).ok_or(GenerationError::Limit)?;
    if *total > max {
        return Err(GenerationError::Limit);
    }
    Ok(())
}

pub(super) fn validate(value: &Value, mode: Mode) -> Result<usize, GenerationError> {
    // Value callers bypass raw byte admission. Bound all data, including annotations,
    // before serde serialization, enum cloning or recursive schema traversal.
    let mut raw_nodes = 0;
    preflight(value, 0, &mut raw_nodes)?;
    let bytes = crate::semantic::value::json_size(value, MAX_TEXT_BYTES)
        .map_err(|_| GenerationError::Limit)?;
    let mut check = Check {
        mode,
        nodes: BTreeMap::new(),
        refs: vec![],
        path_bytes: 0,
        properties: 0,
        enums: 0,
        chars: 0,
        enum_bytes: 0,
    };
    check.walk(value, String::new(), 0)?;
    for target in &check.refs {
        if !check.nodes.contains_key(target) {
            return Err(invalid());
        }
    }
    if mode != Mode::General {
        // Only the root's alias chain needs a concrete object type. Other recursive
        // edges are checked by membership, never expanded into an infinite tree.
        let mut path = String::new();
        let mut seen = BTreeSet::new();
        loop {
            if !seen.insert(path.clone()) {
                return Err(invalid());
            }
            let node = check.nodes.get(&path).ok_or_else(invalid)?;
            let o = node.as_object().ok_or_else(invalid)?;
            if let Some(reference) = o.get("$ref") {
                path = reference_path(reference.as_str().ok_or_else(invalid)?)?;
            } else {
                if o.contains_key("anyOf")
                    || o.get("type").and_then(Value::as_str) != Some("object")
                {
                    return Err(invalid());
                }
                break;
            }
        }
    }
    Ok(bytes)
}
fn preflight(v: &Value, depth: usize, nodes: &mut usize) -> Result<(), GenerationError> {
    charge(nodes, 1, MAX_JSON_NODES)?;
    match v {
        Value::Object(o) => {
            if depth >= MAX_JSON_DEPTH {
                return Err(GenerationError::Limit);
            }
            for child in o.values() {
                charge(nodes, 1, MAX_JSON_NODES)?;
                preflight(child, depth + 1, nodes)?;
            }
        }
        Value::Array(a) => {
            if depth >= MAX_JSON_DEPTH {
                return Err(GenerationError::Limit);
            }
            for child in a {
                preflight(child, depth + 1, nodes)?;
            }
        }
        _ => {}
    }
    Ok(())
}
struct Check<'a> {
    mode: Mode,
    nodes: BTreeMap<String, &'a Value>,
    refs: Vec<String>,
    path_bytes: usize,
    properties: usize,
    enums: usize,
    chars: usize,
    enum_bytes: usize,
}
impl<'a> Check<'a> {
    fn strings(&mut self, n: usize) -> Result<(), GenerationError> {
        if self.mode != Mode::General {
            charge(&mut self.chars, n, STRICT_CHARS)?;
        }
        Ok(())
    }
    fn walk(&mut self, v: &'a Value, path: String, depth: usize) -> Result<(), GenerationError> {
        let o = v.as_object().ok_or_else(invalid)?;
        if self.nodes.len() == MAX_SCHEMA_NODES {
            return Err(GenerationError::Limit);
        }
        charge(&mut self.path_bytes, path.len(), MAX_TOTAL_BYTES)?;
        self.nodes.insert(path.clone(), v);
        let types = type_names(o.get("type"))?;
        if self.mode != Mode::General {
            self.strict_node(o, &types)?;
        }
        let nested = depth + usize::from(types.contains(&"object") || types.contains(&"array"));
        if self.mode != Mode::General && nested > STRICT_DEPTH {
            return Err(GenerationError::Limit);
        }
        for (key, value) in o {
            if self.mode != Mode::General
                && matches!(
                    key.as_str(),
                    "allOf"
                        | "oneOf"
                        | "not"
                        | "if"
                        | "then"
                        | "else"
                        | "patternProperties"
                        | "propertyNames"
                        | "dependentRequired"
                        | "dependentSchemas"
                        | "prefixItems"
                        | "contains"
                        | "minContains"
                        | "maxContains"
                        | "unevaluatedProperties"
                        | "unevaluatedItems"
                        | "uniqueItems"
                        | "minProperties"
                        | "maxProperties"
                )
            {
                return Err(invalid());
            }
            let child_path = || format!("{path}/{}", escape(key));
            match key.as_str() {
                "type" => {}
                "properties" | "patternProperties" | "$defs" | "dependentSchemas" => {
                    let map = value.as_object().ok_or_else(invalid)?;
                    if key == "properties" {
                        charge(
                            &mut self.properties,
                            map.len(),
                            if self.mode == Mode::General {
                                MAX_SCHEMA_NODES
                            } else {
                                STRICT_PROPERTIES
                            },
                        )?;
                    }
                    for (name, child) in map {
                        if key == "patternProperties" {
                            pattern::validate(name)?;
                        }
                        if key == "properties" || key == "$defs" {
                            self.strings(name.chars().count())?;
                        }
                        self.walk(
                            child,
                            format!("{}/{name}", child_path(), name = escape(name)),
                            if key == "$defs" {
                                0
                            } else if key == "dependentSchemas" {
                                depth
                            } else {
                                nested
                            },
                        )?;
                    }
                }
                "required" => {
                    unique_strings(value)?;
                }
                "dependentRequired" => {
                    for child in value.as_object().ok_or_else(invalid)?.values() {
                        unique_strings(child)?;
                    }
                }
                "items" | "contains" | "propertyNames" => self.walk(value, child_path(), nested)?,
                "not" | "if" | "then" | "else" => self.walk(value, child_path(), depth)?,
                "anyOf" | "allOf" | "oneOf" | "prefixItems" => {
                    let a = value
                        .as_array()
                        .filter(|a| !a.is_empty())
                        .ok_or_else(invalid)?;
                    for (index, child) in a.iter().enumerate() {
                        self.walk(
                            child,
                            format!("{}/{index}", child_path()),
                            if key == "prefixItems" { nested } else { depth },
                        )?;
                    }
                }
                "additionalProperties" | "unevaluatedProperties" | "unevaluatedItems" => {
                    if !value.is_boolean() {
                        self.walk(value, child_path(), nested)?;
                    }
                }
                "$ref" => {
                    if self.refs.len() == MAX_REFS {
                        return Err(GenerationError::Limit);
                    }
                    let target = reference_path(value.as_str().ok_or_else(invalid)?)?;
                    charge(&mut self.path_bytes, target.len(), MAX_TOTAL_BYTES)?;
                    self.refs.push(target);
                }
                "enum" => {
                    let a = value
                        .as_array()
                        .filter(|a| !a.is_empty())
                        .ok_or_else(invalid)?;
                    charge(
                        &mut self.enums,
                        a.len(),
                        if self.mode == Mode::General {
                            MAX_ENUMS
                        } else {
                            STRICT_ENUMS
                        },
                    )?;
                    let mut unique = BTreeSet::new();
                    for value in a {
                        self.strings(string_chars(value))?;
                        let canonical = canonical_enum(value)?;
                        charge(&mut self.enum_bytes, canonical.len(), MAX_TOTAL_BYTES)?;
                        if !unique.insert(canonical) {
                            return Err(invalid());
                        }
                    }
                }
                "const" => self.strings(string_chars(value))?,
                "default" => {}
                "examples" => {
                    if !value.is_array() {
                        return Err(invalid());
                    }
                }
                "title" | "description" => {
                    if !value.is_string() {
                        return Err(invalid());
                    }
                }
                "pattern" => {
                    pattern::validate(value.as_str().ok_or_else(invalid)?)?;
                }
                "format" => {
                    if !value.as_str().is_some_and(format_admitted) {
                        return Err(invalid());
                    }
                }
                "minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" => {
                    if !value.is_number() {
                        return Err(invalid());
                    }
                }
                "multipleOf" => {
                    if !super::schema_number::Decimal::new(value.as_number().ok_or_else(invalid)?)?
                        .positive()
                    {
                        return Err(invalid());
                    }
                }
                "minLength" | "maxLength" | "minItems" | "maxItems" | "minProperties"
                | "maxProperties" | "minContains" | "maxContains" => {
                    natural(value)?;
                }
                "uniqueItems" => {
                    if !value.is_boolean() {
                        return Err(invalid());
                    }
                }
                _ => return Err(invalid()),
            }
        }
        for (min, max) in [
            ("minLength", "maxLength"),
            ("minItems", "maxItems"),
            ("minProperties", "maxProperties"),
            ("minContains", "maxContains"),
        ] {
            if let (Some(a), Some(b)) = (o.get(min), o.get(max)) {
                let (a, b) = (natural(a)?, natural(b)?);
                if a.cmp(&b).is_gt() {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
    fn strict_node(&self, o: &Map<String, Value>, types: &[&str]) -> Result<(), GenerationError> {
        if o.contains_key("$ref") {
            if o.keys().any(|k| {
                !matches!(
                    k.as_str(),
                    "$ref" | "title" | "description" | "default" | "examples" | "$defs"
                )
            }) {
                return Err(invalid());
            }
            return Ok(());
        }
        if types.is_empty() && !o.contains_key("anyOf") {
            return Err(invalid());
        }
        if o.get("type").is_some_and(Value::is_array)
            && (types.len() != 2 || !types.contains(&"null"))
        {
            return Err(invalid());
        }
        let object = types.contains(&"object");
        let array = types.contains(&"array");
        if !object
            && ["properties", "required", "additionalProperties"]
                .iter()
                .any(|key| o.contains_key(*key))
            || !array && o.contains_key("items")
            || array && !o.contains_key("items")
        {
            return Err(invalid());
        }
        if object && self.mode == Mode::Explicit {
            if o.get("additionalProperties") != Some(&Value::Bool(false)) {
                return Err(invalid());
            }
            let props = o
                .get("properties")
                .map(|p| p.as_object().ok_or_else(invalid))
                .transpose()?;
            let required = o
                .get("required")
                .map(unique_strings)
                .transpose()?
                .unwrap_or_default();
            if required.len() != props.map_or(0, Map::len)
                || props.is_some_and(|p| p.keys().any(|k| !required.contains(k.as_str())))
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
fn type_names(value: Option<&Value>) -> Result<Vec<&str>, GenerationError> {
    let Some(value) = value else {
        return Ok(vec![]);
    };
    let names = if let Some(s) = value.as_str() {
        vec![s]
    } else {
        value
            .as_array()
            .filter(|a| !a.is_empty())
            .ok_or_else(invalid)?
            .iter()
            .map(|v| v.as_str().ok_or_else(invalid))
            .collect::<Result<_, _>>()?
    };
    let mut seen = BTreeSet::new();
    for name in &names {
        if !matches!(
            *name,
            "object" | "array" | "string" | "number" | "integer" | "boolean" | "null"
        ) || !seen.insert(*name)
        {
            return Err(invalid());
        }
    }
    Ok(names)
}
fn unique_strings(v: &Value) -> Result<BTreeSet<&str>, GenerationError> {
    let mut seen = BTreeSet::new();
    for v in v.as_array().ok_or_else(invalid)? {
        if !seen.insert(v.as_str().ok_or_else(invalid)?) {
            return Err(invalid());
        }
    }
    Ok(seen)
}
fn natural(v: &Value) -> Result<super::schema_number::Natural, GenerationError> {
    super::schema_number::Decimal::new(v.as_number().ok_or_else(invalid)?)?.natural()
}
fn escape(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}
fn reference_path(reference: &str) -> Result<String, GenerationError> {
    let fragment = reference.strip_prefix('#').ok_or_else(invalid)?;
    let mut bytes = Vec::new();
    let mut input = fragment.bytes();
    while let Some(b) = input.next() {
        if b == b'%' {
            let digit = |b: u8| {
                (b as char)
                    .to_digit(16)
                    .map(|n| n as u8)
                    .ok_or_else(invalid)
            };
            bytes.push(
                digit(input.next().ok_or_else(invalid)?)? * 16
                    + digit(input.next().ok_or_else(invalid)?)?,
            );
        } else {
            bytes.push(b);
        }
    }
    let pointer = String::from_utf8(bytes).map_err(|_| invalid())?;
    if pointer.is_empty() {
        return Ok(pointer);
    }
    let mut canonical = String::new();
    for segment in pointer.strip_prefix('/').ok_or_else(invalid)?.split('/') {
        let mut decoded = String::new();
        let mut chars = segment.chars();
        while let Some(c) = chars.next() {
            decoded.push(if c == '~' {
                match chars.next() {
                    Some('0') => '~',
                    Some('1') => '/',
                    _ => return Err(invalid()),
                }
            } else {
                c
            });
        }
        canonical.push('/');
        canonical.push_str(&escape(&decoded));
    }
    Ok(canonical)
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
fn canonical_enum(v: &Value) -> Result<String, GenerationError> {
    // Canonicalize only the comparison copy, never the authoritative schema.
    fn normalize(v: &mut Value) -> Result<(), GenerationError> {
        match v {
            Value::Object(o) => {
                o.sort_keys();
                for child in o.values_mut() {
                    normalize(child)?;
                }
            }
            Value::Array(a) => {
                for child in a {
                    normalize(child)?;
                }
            }
            Value::Number(n) => {
                *n = super::schema_number::Decimal::new(n)?
                    .canonical()
                    .parse()
                    .map_err(|_| invalid())?;
            }
            _ => {}
        }
        Ok(())
    }
    let mut copy = v.clone();
    normalize(&mut copy)?;
    serde_json::to_string(&copy).map_err(|_| invalid())
}
