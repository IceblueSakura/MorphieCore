//! Bounded, duplicate-rejecting JSON with exact numeric values.
use serde::{
    Deserialize,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Map, Number, Value, value::RawValue};
use std::fmt;

/// Callers may narrow these hard resource bounds, never enlarge them.
#[derive(Clone, Copy)]
pub struct JsonLimits {
    pub bytes: usize,
    pub depth: usize,
    pub nodes: usize,
}
impl JsonLimits {
    pub const ENVELOPE: Self = Self {
        bytes: 64 << 20,
        depth: 64,
        nodes: 1 << 20,
    };
    /// Reserved for the image-response entry point; ordinary parse_json still caps at ENVELOPE.
    pub const IMAGE_RESPONSE: Self = Self {
        bytes: 16 << 20,
        depth: 64,
        nodes: 65_536,
    };
    pub const STRUCTURED: Self = Self {
        bytes: 1 << 20,
        depth: 32,
        nodes: 8_192,
    };
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JsonResource {
    Bytes,
    Depth,
    Nodes,
}
#[derive(Clone, Debug, Eq, thiserror::Error, PartialEq)]
pub enum JsonError {
    #[error("invalid JSON")]
    Invalid,
    #[error("JSON exceeds resource limits")]
    Limit(JsonResource),
}

/// Raw fragments borrow the bounded input; no second body or floating-point parse.
pub fn parse_json(input: &[u8], limits: JsonLimits) -> Result<Value, JsonError> {
    parse(input, limits, true)
}
/// Validate unfinished JSON without completing or materializing it. Known
/// duplicate keys and budgets remain errors before the outer value closes.
pub(crate) fn validate_json_prefix(input: &[u8], limits: JsonLimits) -> Result<(), JsonError> {
    let limits = JsonLimits {
        bytes: limits.bytes.min(JsonLimits::ENVELOPE.bytes),
        depth: limits.depth.min(JsonLimits::ENVELOPE.depth),
        nodes: limits.nodes.min(JsonLimits::ENVELOPE.nodes),
    };
    if input.len() > limits.bytes {
        return Err(JsonError::Limit(JsonResource::Bytes));
    }
    depth_preflight(input, limits.depth)?;
    // Count wire tokens, not serde's arbitrary-precision Number pseudo-map.
    let (mut nodes, mut string, mut escaped, mut scalar) = (0usize, false, false, false);
    let mut unicode_remaining = 0u8;
    for byte in input {
        if string {
            if unicode_remaining > 0 {
                if !byte.is_ascii_hexdigit() {
                    return Err(JsonError::Invalid);
                }
                unicode_remaining -= 1;
            } else if escaped {
                // serde may report EOF before checking the already supplied
                // digits of a short Unicode escape; EOF alone is not prefix validity.
                match *byte {
                    b'u' => unicode_remaining = 4,
                    b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => {}
                    _ => return Err(JsonError::Invalid),
                }
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                string = false;
            } else if *byte < 0x20 {
                return Err(JsonError::Invalid);
            }
            continue;
        }
        let starts = match *byte {
            b'"' => {
                string = true;
                scalar = false;
                true
            }
            b'{' | b'[' => {
                scalar = false;
                true
            }
            b'}' | b']' | b':' | b',' | b' ' | b'\n' | b'\r' | b'\t' => {
                scalar = false;
                false
            }
            _ => {
                let starts = !scalar;
                scalar = true;
                starts
            }
        };
        nodes += usize::from(starts);
        if nodes > limits.nodes {
            return Err(JsonError::Limit(JsonResource::Nodes));
        }
    }
    let mut parser = serde_json::Deserializer::from_slice(input);
    match Prefix.deserialize(&mut parser).and_then(|()| parser.end()) {
        Ok(()) => Ok(()),
        Err(error) if error.is_eof() => Ok(()),
        Err(_) => Err(JsonError::Invalid),
    }
}
struct Prefix;
impl<'de> DeserializeSeed<'de> for Prefix {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, parser: D) -> Result<(), D::Error> {
        parser.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Prefix {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON prefix")
    }
    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<(), E> {
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while seq.next_element_seed(Prefix)?.is_some() {}
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut keys = std::collections::BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(de::Error::custom("duplicate JSON key"));
            }
            map.next_value_seed(Prefix)?;
        }
        Ok(())
    }
}
/// Derived views use last-key-wins without changing raw or structured authority.
pub(crate) fn parse_json_view(input: &[u8]) -> Result<Value, JsonError> {
    parse(input, JsonLimits::ENVELOPE, false)
}
pub(crate) fn parse_image_response_json(input: &[u8]) -> Result<Value, JsonError> {
    parse(input, JsonLimits::IMAGE_RESPONSE, true)
}
fn parse(input: &[u8], limits: JsonLimits, reject_duplicates: bool) -> Result<Value, JsonError> {
    let limits = JsonLimits {
        bytes: limits.bytes.min(JsonLimits::ENVELOPE.bytes),
        depth: limits.depth.min(JsonLimits::ENVELOPE.depth),
        nodes: limits.nodes.min(JsonLimits::ENVELOPE.nodes),
    };
    if input.len() > limits.bytes {
        return Err(JsonError::Limit(JsonResource::Bytes));
    }
    depth_preflight(input, limits.depth)?;
    let mut budget = Budget {
        nodes: 0,
        exhausted: None,
        limits,
        reject_duplicates,
    };
    let mut parser = serde_json::Deserializer::from_slice(input);
    let result = Seed {
        budget: &mut budget,
        depth: 0,
    }
    .deserialize(&mut parser)
    .and_then(|value| {
        parser.end()?;
        Ok(value)
    });
    result.map_err(|_| {
        budget
            .exhausted
            .map_or(JsonError::Invalid, JsonError::Limit)
    })
}

/// Bound recursion before RawValue scans a container, even when its syntax is invalid.
fn depth_preflight(input: &[u8], maximum: usize) -> Result<(), JsonError> {
    let (mut depth, mut string, mut escaped) = (0usize, false, false);
    for byte in input {
        if string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                string = false;
            }
        } else {
            match byte {
                b'"' => string = true,
                b'{' | b'[' => {
                    depth += 1;
                    if depth > maximum {
                        return Err(JsonError::Limit(JsonResource::Depth));
                    }
                }
                b'}' | b']' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    Ok(())
}
struct Budget {
    nodes: usize,
    exhausted: Option<JsonResource>,
    limits: JsonLimits,
    reject_duplicates: bool,
}
impl Budget {
    fn charge<E: de::Error>(&mut self) -> Result<(), E> {
        if self.nodes >= self.limits.nodes {
            self.exhausted = Some(JsonResource::Nodes);
            return Err(E::custom("JSON limit"));
        }
        self.nodes += 1;
        Ok(())
    }
}
struct Key<'a>(&'a mut Budget);
impl<'de> DeserializeSeed<'de> for Key<'_> {
    type Value = String;
    fn deserialize<D: de::Deserializer<'de>>(self, parser: D) -> Result<String, D::Error> {
        self.0.charge()?;
        String::deserialize(parser)
    }
}
struct Seed<'a> {
    budget: &'a mut Budget,
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(self, parser: D) -> Result<Value, D::Error> {
        self.budget.charge()?;
        let raw = <&RawValue>::deserialize(parser)?;
        let value = raw.get();
        let mut parser = serde_json::Deserializer::from_str(value);
        // Dispatch by the actual JSON token: arbitrary_precision's private Number
        // map must never reinterpret an ordinary object with the same key.
        match value.as_bytes()[0] {
            b'{' => de::Deserializer::deserialize_map(&mut parser, self).map_err(de::Error::custom),
            b'[' => de::Deserializer::deserialize_seq(&mut parser, self).map_err(de::Error::custom),
            b'-' | b'0'..=b'9'
                if !self.budget.reject_duplicates && value.contains(['.', 'e', 'E']) =>
            {
                // SDK views use ordinary binary floats; rounding belongs only to
                // this discarded view, never to raw/structured value authority.
                let value = value
                    .parse::<f64>()
                    .ok()
                    .and_then(Number::from_f64)
                    .ok_or_else(|| de::Error::custom("JSON view number"))?;
                Ok(Value::Number(value))
            }
            // Number's integer fast path otherwise erases the sign of bare -0.
            b'-' if value == "-0" => Ok(Value::Number(
                "-0.0".parse::<Number>().map_err(de::Error::custom)?,
            )),
            b'-' | b'0'..=b'9' => value
                .parse::<Number>()
                .map(Value::Number)
                .map_err(de::Error::custom),
            _ => serde_json::from_str::<Value>(value).map_err(de::Error::custom),
        }
    }
}
impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON container")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        if self.depth >= self.budget.limits.depth {
            self.budget.exhausted = Some(JsonResource::Depth);
            return Err(de::Error::custom("JSON depth"));
        }
        let mut values = Vec::new();
        while let Some(value) = seq.next_element_seed(Seed {
            budget: &mut *self.budget,
            depth: self.depth + 1,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        if self.depth >= self.budget.limits.depth {
            self.budget.exhausted = Some(JsonResource::Depth);
            return Err(de::Error::custom("JSON depth"));
        }
        let mut values = Map::new();
        while let Some(key) = map.next_key_seed(Key(&mut *self.budget))? {
            if self.budget.reject_duplicates && values.contains_key(&key) {
                return Err(de::Error::custom("duplicate JSON key"));
            }
            let value = map.next_value_seed(Seed {
                budget: &mut *self.budget,
                depth: self.depth + 1,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_zero_survives_exact_numeric_ingestion_without_loosening_json() {
        let value = parse_json(br#"[-0,-0.0,0,0.1]"#, JsonLimits::STRUCTURED).unwrap();
        assert_eq!(value[0].to_string(), "-0.0");
        assert_eq!(value[1].to_string(), "-0.0");
        assert_eq!(value[2].to_string(), "0");
        assert_eq!(value[3].to_string(), "0.1");
        assert!(parse_json(br#"{"n":-0,"n":0}"#, JsonLimits::STRUCTURED).is_err());
        assert!(parse_json(b"-00", JsonLimits::STRUCTURED).is_err());
    }
}
