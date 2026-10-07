//! Standard Chat nests URL citations; the shared text owner uses a flat typed value.
//! https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/chat_completion_message.py
use super::{CodecError, common::*};
use crate::semantic::{task::generation::*, value::Text};
use serde_json::{Value, json};

pub(super) fn attach(
    parts: &mut [Part],
    value: Option<&Value>,
    items: &mut Items,
) -> Result<(), CodecError> {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Ok(());
    };
    let rows = value.as_array().ok_or(CodecError::Invalid("annotations"))?;
    if rows.len() > MAX_ITEMS {
        return Err(CodecError::Limit);
    }
    if rows.is_empty() {
        return Ok(());
    }
    let [
        Part {
            content: ContentPart::Text(text),
            ..
        },
    ] = parts
    else {
        return Err(CodecError::Invalid("annotation owner"));
    };
    let mut annotations = Vec::new();
    for row in rows {
        let row = object(row)?;
        fields(row, &["type", "url_citation"])?;
        if string(row, "type")? != "url_citation" {
            return Err(CodecError::Unsupported("Chat annotation kind".into()));
        }
        let cite = object(
            row.get("url_citation")
                .ok_or(CodecError::Invalid("url citation"))?,
        )?;
        fields(cite, &["start_index", "end_index", "title", "url"])?;
        let mut flat = cite.clone();
        flat.insert("type".into(), json!("url_citation"));
        annotations.push(super::citations::read(&Value::Object(flat), items)?);
    }
    *text = TextContent::new(
        Text::allowing_empty(text.as_str(), "text", MAX_TEXT_BYTES)
            .map_err(|_| CodecError::Limit)?,
        annotations,
        text.logprobs().clone(),
    )?;
    Ok(())
}
pub(super) fn write(annotations: &[Annotation], resources: &ResourceTable) -> Value {
    json!(
        annotations
            .iter()
            .map(|a| {
                let mut v = super::citations::write(a, resources).expect("checked citation");
                v.as_object_mut()
                    .expect("citation object")
                    .shift_remove("type");
                json!({"type":"url_citation","url_citation":v})
            })
            .collect::<Vec<_>>()
    )
}
