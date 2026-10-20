//! Typed text metadata; no unknown annotation/probability object is retained.
use crate::protocol::openai::{CodecError, common::*};
use crate::semantic::{
    task::generation::*,
    value::{Presence, Text},
};
use serde_json::{Map, Value, json};
pub(in crate::protocol::openai) fn read(
    o: &Map<String, Value>,
    replay: bool,
    items: &mut Items,
) -> Result<TextContent, CodecError> {
    read_existing(o, replay, items, &[])
}
pub(in crate::protocol::openai) fn read_existing(
    o: &Map<String, Value>,
    replay: bool,
    items: &mut Items,
    existing: &[Annotation],
) -> Result<TextContent, CodecError> {
    items.direction.fields(
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
        Some(v) if !v.is_null() => read_annotations(v, items, existing)?,
        None if replay && items.direction.client() => {
            // Selected client-input shorthand only. Closed request decoding and
            // every upstream snapshot still require the official array.
            items.fidelity.record_history_annotations_default();
            vec![]
        }
        _ => return Err(CodecError::Invalid("annotations")),
    };
    let logprobs = read_presence(o, "logprobs", read_logprobs)?;
    Ok(TextContent::new(text, annotations, logprobs)?)
}
pub(in crate::protocol::openai) fn read_annotations(
    v: &Value,
    items: &mut Items,
    existing: &[Annotation],
) -> Result<Vec<Annotation>, CodecError> {
    let a = v.as_array().ok_or(CodecError::Invalid("annotations"))?;
    if a.len() > MAX_ITEMS {
        return Err(CodecError::Limit);
    }
    if a.len() < existing.len() {
        return Err(CodecError::Invalid("annotation snapshot"));
    }
    a.iter()
        .enumerate()
        .map(|(index, value)| {
            if let Some(known) = existing.get(index) {
                crate::protocol::openai::content::citations::check_snapshot(known, value, items)?;
                Ok(known.clone())
            } else {
                crate::protocol::openai::content::citations::read(value, items)
            }
        })
        .collect()
}
pub(in crate::protocol::openai) fn read_logprobs(v: &Value) -> Result<Vec<Logprob>, CodecError> {
    let a = v.as_array().ok_or(CodecError::Invalid("logprobs"))?;
    if a.len() > MAX_ITEMS {
        return Err(CodecError::Limit);
    }
    // Protocol extensions are ignored; the semantic structs remain closed.
    let mut known = a.clone();
    for token in &mut known {
        let token = token
            .as_object_mut()
            .ok_or(CodecError::Invalid("logprobs"))?;
        token.retain(|key, _| {
            matches!(key.as_str(), "token" | "logprob" | "bytes" | "top_logprobs")
        });
        if let Some(top) = token.get_mut("top_logprobs").filter(|v| !v.is_null()) {
            let top = top.as_array_mut().ok_or(CodecError::Invalid("logprobs"))?;
            if top.len() > MAX_ITEMS {
                return Err(CodecError::Limit);
            }
            for entry in top {
                let entry = entry
                    .as_object_mut()
                    .ok_or(CodecError::Invalid("logprobs"))?;
                entry.retain(|key, _| matches!(key.as_str(), "token" | "logprob" | "bytes"));
            }
        }
    }
    let p: Vec<Logprob> =
        serde_json::from_value(Value::Array(known)).map_err(|_| CodecError::Invalid("logprobs"))?;
    validate_logprobs(&p)?;
    Ok(p)
}
pub(in crate::protocol::openai) fn event_logprobs(probs: &[Logprob]) -> Value {
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
pub(in crate::protocol::openai) fn write(
    t: &TextContent,
    kind: &str,
    resources: &ResourceTable,
) -> Value {
    let mut v = json!({"type":kind,"text":t.as_str()});
    if kind == "output_text" || !t.annotations().is_empty() {
        v["annotations"] = json!(
            t.annotations()
                .iter()
                .map(
                    |a| crate::protocol::openai::content::citations::write(a, resources)
                        .expect("checked citation")
                )
                .collect::<Vec<_>>()
        );
    }
    match t.logprobs() {
        Presence::Absent => {}
        Presence::Null => v["logprobs"] = Value::Null,
        Presence::Value(p) => v["logprobs"] = json!(p),
    }
    v
}
