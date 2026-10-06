//! Standard Chat nests URL citations; the shared text owner uses a flat typed value.
//! https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/chat_completion_message.py
use super::{CodecError, common::*};
use crate::semantic::{task::generation::*, value::Text};
use serde_json::{Value, json};

pub(super) fn attach(parts: &mut [Part], value: Option<&Value>) -> Result<(), CodecError> {
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
        annotations.push(Annotation::UrlCitation {
            start_index: usize::try_from(
                cite.get("start_index")
                    .and_then(Value::as_u64)
                    .ok_or(CodecError::Invalid("citation index"))?,
            )
            .map_err(|_| CodecError::Limit)?,
            end_index: usize::try_from(
                cite.get("end_index")
                    .and_then(Value::as_u64)
                    .ok_or(CodecError::Invalid("citation index"))?,
            )
            .map_err(|_| CodecError::Limit)?,
            title: string(cite, "title")?.into(),
            url: string(cite, "url")?.into(),
        });
    }
    *text = TextContent::new(
        Text::allowing_empty(text.as_str(), "text", MAX_TEXT_BYTES)
            .map_err(|_| CodecError::Limit)?,
        annotations,
        text.logprobs().clone(),
    )?;
    Ok(())
}
pub(super) fn write(annotations: &[Annotation]) -> Value {
    json!(annotations.iter().map(|annotation|match annotation {
        Annotation::UrlCitation{start_index,end_index,title,url}=>json!({"type":"url_citation","url_citation":{"start_index":start_index,"end_index":end_index,"title":title,"url":url}}),
        _=>unreachable!("lowering checks Chat annotation kind"),
    }).collect::<Vec<_>>())
}
