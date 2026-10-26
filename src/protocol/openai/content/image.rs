//! Standard input image values. File IDs need issuer/resource ownership before admission.
use crate::{
    protocol::{
        failure::CarrierReason,
        openai::{CodecError, GenerationApi, shape::*},
    },
    semantic::{task::generation::*, value::Text},
};
use serde_json::{Map, Value, json};

pub(in crate::protocol::openai) fn read(
    part: &Map<String, Value>,
    profile: GenerationApi,
    direction: DecodeDirection,
) -> Result<Resource, CodecError> {
    let image = match profile {
        GenerationApi::Responses => {
            if part.get("file_id").is_some_and(|v| !v.is_null()) {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
            direction.fields(
                part,
                &[
                    "type",
                    "image_url",
                    "file_id",
                    "detail",
                    "prompt_cache_breakpoint",
                ],
            )?;
            part
        }
        GenerationApi::Chat => {
            direction.fields(part, &["type", "image_url"])?;
            let image = object(part.get("image_url").ok_or(CodecError::invalid())?)?;
            direction.fields(image, &["url", "detail"])?;
            image
        }
    };
    let raw = string(
        image,
        if profile == GenerationApi::Chat {
            "url"
        } else {
            "image_url"
        },
    )?;
    if raw.len() > MAX_TEXT_BYTES {
        return Err(CodecError::Limit);
    }
    let location = if let Some(data) = raw.strip_prefix("data:") {
        let (media_type, data_base64) = data.split_once(";base64,").ok_or(CodecError::invalid())?;
        ResourceLocation::Inline {
            media_type: Text::new(
                media_type,
                "image media type",
                MAX_RESOURCE_MEDIA_TYPE_BYTES,
            )
            .map_err(|_| CodecError::invalid())?,
            data_base64: Text::new(data_base64, "image data", MAX_TEXT_BYTES)
                .map_err(|_| CodecError::invalid())?,
        }
    } else {
        ResourceLocation::Url(
            Text::new(raw, "image URL", MAX_RESOURCE_URL_BYTES)
                .map_err(|_| CodecError::invalid())?,
        )
    };
    let image_detail = match image.get("detail") {
        None => None,
        Some(Value::String(s)) => Some(
            s.parse::<ImageDetail>()
                .map_err(|_| CodecError::invalid())?,
        ),
        _ => {
            return Err(CodecError::invalid());
        }
    };
    let resource = Resource {
        location,
        description: ResourceDescription::Image {
            detail: image_detail,
            size_policy: ImageSizePolicy::AllowDownsize,
        },
    };
    resource.validate()?;
    Ok(resource)
}
pub(in crate::protocol::openai) fn write(
    resource: ResourceView<'_>,
    profile: GenerationApi,
) -> Value {
    let url = match &resource.location {
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
    };
    let mut image = match profile {
        GenerationApi::Chat => json!({"url":url}),
        GenerationApi::Responses => json!({"type":"input_image","image_url":url}),
    };
    if let Some(detail) = resource.image_detail() {
        image["detail"] = json!(detail.label());
    }
    if profile == GenerationApi::Chat {
        json!({"type":"image_url","image_url":image})
    } else {
        image
    }
}
