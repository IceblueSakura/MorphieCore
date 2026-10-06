//! Typed text metadata; no unknown annotation/probability object is retained.
use super::{CodecError, common::*};
use crate::semantic::{
    task::generation::*,
    value::{Presence, Text},
};
use serde_json::{Map, Value, json};
pub(super) fn read(o: &Map<String, Value>, replay: bool) -> Result<TextContent, CodecError> {
    fields(
        o,
        if replay {
            &["type", "text", "annotations", "logprobs", "parsed"]
        } else {
            &["type", "text", "annotations", "logprobs"]
        },
    )?;
    let text = Text::allowing_empty(string(o, "text")?, "text", MAX_TEXT_BYTES)
        .map_err(|_| CodecError::Limit)?;
    if replay {
        admit_parsed("parsed", o.get("parsed"), Some(text.as_str()))?;
    }
    let annotations = match o.get("annotations") {
        Some(v) if !v.is_null() => read_annotations(v)?,
        // Both the output model and the request param require this array.
        _ => return Err(CodecError::Invalid("annotations")),
    };
    let logprobs = read_presence(o, "logprobs", read_logprobs)?;
    Ok(TextContent::new(text, annotations, logprobs)?)
}
pub(super) fn read_annotations(v: &Value) -> Result<Vec<Annotation>, CodecError> {
    let a = v.as_array().ok_or(CodecError::Invalid("annotations"))?;
    if a.len() > MAX_ITEMS {
        return Err(CodecError::Limit);
    }
    serde_json::from_value(v.clone()).map_err(|_| CodecError::Invalid("annotations"))
}
pub(super) fn read_logprobs(v: &Value) -> Result<Vec<Logprob>, CodecError> {
    let a = v.as_array().ok_or(CodecError::Invalid("logprobs"))?;
    if a.len() > MAX_ITEMS {
        return Err(CodecError::Limit);
    }
    let p: Vec<Logprob> =
        serde_json::from_value(v.clone()).map_err(|_| CodecError::Invalid("logprobs"))?;
    validate_logprobs(&p)?;
    Ok(p)
}
pub(super) fn event_logprobs(probs: &[Logprob]) -> Value {
    json!(
        probs
            .iter()
            .map(|p| {
                let mut v = json!({"token":p.token,"logprob":p.logprob});
                if let Some(top) = &p.top_logprobs {
                    v["top_logprobs"] = json!(
                        top.iter()
                            .map(|t| {
                                let mut v = Map::new();
                                if let Some(token) = &t.token {
                                    v.insert("token".into(), json!(token));
                                }
                                if let Some(prob) = &t.logprob {
                                    v.insert("logprob".into(), json!(prob));
                                }
                                Value::Object(v)
                            })
                            .collect::<Vec<_>>()
                    );
                }
                v
            })
            .collect::<Vec<_>>()
    )
}
pub(super) fn write(t: &TextContent, kind: &str) -> Value {
    let mut v = json!({"type":kind,"text":t.as_str()});
    if kind == "output_text" || !t.annotations().is_empty() {
        v["annotations"] = json!(t.annotations());
    }
    match t.logprobs() {
        Presence::Absent => {}
        Presence::Null => v["logprobs"] = Value::Null,
        Presence::Value(p) => v["logprobs"] = json!(p),
    }
    v
}
