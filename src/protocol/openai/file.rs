//! Standard inline/URL file input. No file I/O, fetching or issuer-bound IDs.
//! Source: https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_input_file_param.py
use super::{CodecError, common::*};
use crate::semantic::{task::generation::*, value::Text};
use serde_json::{Map, Value, json};

pub(super) fn read(part: &Map<String, Value>) -> Result<Resource, CodecError> {
    fields(
        part,
        &["type", "file_data", "file_url", "filename", "detail"],
    )?;
    let location = match (
        part.contains_key("file_data"),
        part.contains_key("file_url"),
    ) {
        (true, false) => {
            let raw = string(part, "file_data")?;
            if raw.len() > MAX_TEXT_BYTES {
                return Err(CodecError::Limit);
            }
            let (media_type, data) = raw
                .strip_prefix("data:")
                .and_then(|v| v.split_once(";base64,"))
                .ok_or(CodecError::Invalid("file data URL"))?;
            ResourceLocation::Inline {
                media_type: text(media_type, "file media type", MAX_RESOURCE_MEDIA_TYPE_BYTES)?,
                data_base64: text(data, "file data", MAX_TEXT_BYTES)?,
            }
        }
        (false, true) => ResourceLocation::Url(text(
            string(part, "file_url")?,
            "file URL",
            MAX_RESOURCE_URL_BYTES,
        )?),
        _ => return Err(CodecError::Invalid("file source")),
    };
    let filename = part
        .get("filename")
        .map(|v| {
            let name = v.as_str().ok_or(CodecError::Invalid("filename"))?;
            Text::allowing_empty(name, "filename", MAX_FILE_NAME_BYTES)
                .map_err(|_| CodecError::Limit)
        })
        .transpose()?;
    let detail = part
        .get("detail")
        .map(|v| {
            v.as_str()
                .ok_or(CodecError::Invalid("file detail"))?
                .parse::<FileDetail>()
                .map_err(|_| CodecError::Invalid("file detail"))
        })
        .transpose()?;
    let resource = Resource {
        location,
        description: ResourceDescription::File(FileDescription { filename, detail }),
    };
    resource.validate()?;
    Ok(resource)
}
pub(super) fn write(resource: &Resource) -> Value {
    let ResourceDescription::File(file) = &resource.description else {
        unreachable!("file lowering requires a file description")
    };
    let mut value = match &resource.location {
        ResourceLocation::Url(url) => json!({"type":"input_file","file_url":url.as_str()}),
        ResourceLocation::Inline {
            media_type,
            data_base64,
        } => {
            json!({"type":"input_file","file_data":format!("data:{};base64,{}",media_type.as_str(),data_base64.as_str())})
        }
        ResourceLocation::OpaqueReference(_) => unreachable!("file lowering rejects IDs"),
    };
    if let Some(name) = &file.filename {
        value["filename"] = json!(name.as_str());
    }
    if let Some(detail) = file.detail {
        value["detail"] = json!(detail.label());
    }
    value
}
