//! Task settings shared by request parsing and provider-reported response echoes.
use super::{CodecError, Profile, common::*, function_tools, reasoning};
use crate::semantic::{task::generation::*, value::Text};
use serde_json::{Map, Value, json};
pub(super) const FIELDS: &[&str] = &[
    "instructions",
    "temperature",
    "max_output_tokens",
    "top_p",
    "top_logprobs",
    "truncation",
    "tools",
    "tool_choice",
    "parallel_tool_calls",
    "reasoning",
    "include",
    "text",
];
/// `reported` distinguishes response-echo admission from request admission: the
/// pinned response model marks `text` nullable, the create type does not.
pub(super) fn read(
    o: &Map<String, Value>,
    reported: bool,
) -> Result<GenerationSettings, CodecError> {
    let instructions = read_presence(o, "instructions", |v| {
        Text::allowing_empty(
            v.as_str().ok_or(CodecError::Invalid("instructions"))?,
            "instructions",
            MAX_TEXT_BYTES,
        )
        .map_err(|_| CodecError::Limit)
    })?;
    let mut s = GenerationSettings {
        instructions,
        controls: controls(o, "max_output_tokens")?,
        reasoning: reasoning::request(o, reported)?,
        ..Default::default()
    };
    if let Some(v) = o.get("top_p").filter(|v| !v.is_null()) {
        s.controls = s
            .controls
            .with_top_p(v.as_f64().ok_or(CodecError::Invalid("top_p"))?)?;
    }
    s.controls.top_logprobs = o
        .get("top_logprobs")
        .filter(|v| !v.is_null())
        .map(|v| {
            v.as_u64()
                .filter(|n| *n <= 20)
                .map(|n| n as u8)
                .ok_or(CodecError::Invalid("top_logprobs"))
        })
        .transpose()?;
    if o.get("include")
        .and_then(Value::as_array)
        .is_some_and(|a| a.iter().any(|v| v == "message.output_text.logprobs"))
    {
        s.controls.logprobs = crate::semantic::value::Presence::Value(true);
    }
    s.controls.truncation = o
        .get("truncation")
        .filter(|v| !v.is_null())
        .map(|v| match v.as_str() {
            Some("auto") => Ok(Truncation::Auto),
            Some("disabled") => Ok(Truncation::Disabled),
            _ => Err(CodecError::Invalid("truncation")),
        })
        .transpose()?;
    if o.get("text").is_some_and(Value::is_null) && !reported {
        // The pinned create type is non-nullable; an explicit null is not "no text".
        return Err(CodecError::Invalid("text"));
    }
    if let Some(t) = o.get("text").filter(|v| !v.is_null()) {
        let t = object(t)?;
        fields(t, &["format", "verbosity"])?;
        s.text.presence = true;
        s.text.format = read_presence(t, "format", read_format)?;
        s.text.verbosity = read_presence(t, "verbosity", |v| match v.as_str() {
            Some("low") => Ok(Verbosity::Low),
            Some("medium") => Ok(Verbosity::Medium),
            Some("high") => Ok(Verbosity::High),
            _ => Err(CodecError::Invalid("verbosity")),
        })?;
    }
    function_tools::read_settings(o, Profile::Responses, &mut s)?;
    s.validate()?;
    Ok(s)
}
pub(super) fn read_format(v: &Value) -> Result<OutputConstraint, CodecError> {
    let o = object(v)?;
    Ok(match string(o, "type")? {
        "text" => {
            fields(o, &["type"])?;
            OutputConstraint::Text
        }
        "json_object" => {
            fields(o, &["type"])?;
            OutputConstraint::JsonObject
        }
        "json_schema" => {
            fields(o, &["type", "name", "description", "schema", "strict"])?;
            read_schema_body(o)?
        }
        _ => return Err(CodecError::Unsupported("text format".into())),
    })
}
/// Shared json_schema body fields; each protocol shell owns its wrapper and key set.
pub(super) fn read_schema_body(o: &Map<String, Value>) -> Result<OutputConstraint, CodecError> {
    Ok(OutputConstraint::JsonSchema {
        name: text(string(o, "name")?, "schema name", 64)?,
        description: o
            .get("description")
            .filter(|v| !v.is_null())
            .map(|v| {
                Text::allowing_empty(
                    v.as_str().ok_or(CodecError::Invalid("description"))?,
                    "description",
                    MAX_TEXT_BYTES,
                )
                .map_err(|_| CodecError::Limit)
            })
            .transpose()?,
        schema: o
            .get("schema")
            .cloned()
            .ok_or(CodecError::Invalid("schema"))?,
        strict: o
            .get("strict")
            .filter(|v| !v.is_null())
            .map(|v| v.as_bool().ok_or(CodecError::Invalid("strict")))
            .transpose()?,
    })
}
/// Missing and null description/strict collapse to `None`; explicit `strict: false` stays visible.
pub(super) fn write_schema_body(
    o: &mut Map<String, Value>,
    name: &Text,
    description: Option<&Text>,
    schema: &Value,
    strict: Option<bool>,
) {
    o.insert("name".into(), json!(name.as_str()));
    if let Some(d) = description {
        o.insert("description".into(), json!(d.as_str()));
    }
    o.insert("schema".into(), schema.clone());
    if let Some(s) = strict {
        o.insert("strict".into(), json!(s));
    }
}
pub(super) fn write_format(f: &OutputConstraint) -> Value {
    match f {
        OutputConstraint::Text => json!({"type":"text"}),
        OutputConstraint::JsonObject => json!({"type":"json_object"}),
        OutputConstraint::JsonSchema {
            name,
            description,
            schema,
            strict,
        } => {
            let mut o = Map::new();
            o.insert("type".into(), json!("json_schema"));
            write_schema_body(&mut o, name, description.as_ref(), schema, *strict);
            Value::Object(o)
        }
    }
}
pub(super) fn write(s: &GenerationSettings, o: &mut Map<String, Value>) -> Result<(), CodecError> {
    put_presence(o, "instructions", &s.instructions, |t| json!(t.as_str()));
    write_control_values(&s.controls, o, "max_output_tokens");
    if let Some(v) = s.controls.top_p() {
        o.insert("top_p".into(), json!(v));
    }
    if let Some(v) = s.controls.top_logprobs {
        o.insert("top_logprobs".into(), json!(v));
    }
    if let Some(v) = s.controls.truncation {
        o.insert(
            "truncation".into(),
            json!(match v {
                Truncation::Auto => "auto",
                Truncation::Disabled => "disabled",
            }),
        );
    }
    function_tools::write_settings(s, Profile::Responses, o)?;
    reasoning::write_request(&s.reasoning, o, Profile::Responses)?;
    if s.controls.logprobs == crate::semantic::value::Presence::Value(true) {
        let a = o.entry("include").or_insert_with(|| json!([]));
        a.as_array_mut()
            .expect("typed include")
            .push(json!("message.output_text.logprobs"));
    }
    if s.text.presence {
        let mut t = Map::new();
        put_presence(&mut t, "format", &s.text.format, write_format);
        put_presence(&mut t, "verbosity", &s.text.verbosity, |v| {
            json!(match v {
                Verbosity::Low => "low",
                Verbosity::Medium => "medium",
                Verbosity::High => "high",
            })
        });
        o.insert("text".into(), Value::Object(t));
    }
    Ok(())
}
