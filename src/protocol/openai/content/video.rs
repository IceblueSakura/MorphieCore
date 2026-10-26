//! Video extensions: Chat `video_url` and Responses `input_video`, each a video URL or
//! base64 data URL, never a frame list. Chat's is shared by OpenRouter, DashScope, Zhipu,
//! vLLM and ModelBest; Responses' by Open Responses and OpenRouter. Extras such as `fps`
//! or `processing` are dialect-specific and ignored. Source: docs/refs/multimodal.md.
use crate::protocol::openai::{CodecError, shape::*};
use crate::semantic::task::generation::*;
use serde_json::{Map, Value, json};

pub(in crate::protocol::openai) fn read(
    part: &Map<String, Value>,
    direction: DecodeDirection,
) -> Result<Resource, CodecError> {
    direction.fields(part, &["type", "video_url"])?;
    let video = object(part.get("video_url").ok_or(CodecError::invalid())?)?;
    direction.fields(video, &["url"])?;
    resource(string(video, "url")?)
}
/// Responses `input_video`: `video_url` is a plain string, not Chat's object.
pub(in crate::protocol::openai) fn read_responses(
    part: &Map<String, Value>,
    direction: DecodeDirection,
) -> Result<Resource, CodecError> {
    direction.fields(part, &["type", "video_url"])?;
    resource(string(part, "video_url")?)
}
fn resource(raw: &str) -> Result<Resource, CodecError> {
    if raw.len() > MAX_TEXT_BYTES {
        return Err(CodecError::Limit);
    }
    let location = match raw.strip_prefix("data:") {
        Some(data) => {
            let (media_type, data) = data.split_once(";base64,").ok_or(CodecError::invalid())?;
            ResourceLocation::Inline {
                media_type: text(
                    media_type,
                    "video media type",
                    MAX_RESOURCE_MEDIA_TYPE_BYTES,
                )?,
                data_base64: text(data, "video data", MAX_TEXT_BYTES)?,
            }
        }
        None => ResourceLocation::Url(text(raw, "video URL", MAX_RESOURCE_URL_BYTES)?),
    };
    let resource = Resource {
        location,
        description: ResourceDescription::Video,
    };
    resource.validate()?;
    Ok(resource)
}

pub(in crate::protocol::openai) fn write(resource: ResourceView<'_>) -> Value {
    json!({"type":"video_url","video_url":{"url":url(resource)}})
}
pub(in crate::protocol::openai) fn write_responses(resource: ResourceView<'_>) -> Value {
    json!({"type":"input_video","video_url":url(resource)})
}
fn url(resource: ResourceView<'_>) -> String {
    match resource.location {
        ResourceLocation::Url(url) => url.as_str().to_owned(),
        ResourceLocation::Inline {
            media_type,
            data_base64,
        } => format!(
            "data:{};base64,{}",
            media_type.as_str(),
            data_base64.as_str()
        ),
        ResourceLocation::OpaqueReference(_) | ResourceLocation::NamespacedReference { .. } => {
            unreachable!("lowering rejects resource IDs")
        }
    }
}
