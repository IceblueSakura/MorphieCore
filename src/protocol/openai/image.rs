//! Standard input image values. File IDs need issuer/resource ownership before admission.
use super::{CodecError, Profile, common::*};
use crate::semantic::{task::generation::*, value::Text};
use serde_json::{Map, Value, json};

pub(super) fn read(part: &Map<String, Value>, profile: Profile) -> Result<Resource, CodecError> {
    let image = match profile {
        Profile::Responses => {
            fields(part, &["type", "image_url", "detail"])?;
            part
        }
        Profile::Chat => {
            fields(part, &["type", "image_url"])?;
            let image = object(
                part.get("image_url")
                    .ok_or(CodecError::Invalid("image_url"))?,
            )?;
            fields(image, &["url", "detail"])?;
            image
        }
    };
    let raw = string(
        image,
        if profile == Profile::Chat {
            "url"
        } else {
            "image_url"
        },
    )?;
    if raw.len() > MAX_TEXT_BYTES {
        return Err(CodecError::Limit);
    }
    let location = if let Some(data) = raw.strip_prefix("data:") {
        let (media_type, data_base64) = data
            .split_once(";base64,")
            .ok_or(CodecError::Invalid("image data URL"))?;
        ResourceLocation::Inline {
            media_type: Text::new(media_type, "image media type", 64)
                .map_err(|_| CodecError::Invalid("image media type"))?,
            data_base64: Text::new(data_base64, "image data", MAX_TEXT_BYTES)
                .map_err(|_| CodecError::Invalid("image data"))?,
        }
    } else {
        ResourceLocation::Url(
            Text::new(raw, "image URL", MAX_RESOURCE_URL_BYTES)
                .map_err(|_| CodecError::Invalid("image URL"))?,
        )
    };
    let image_detail = match image.get("detail") {
        None => None,
        Some(Value::String(s)) => Some(
            s.parse::<ImageDetail>()
                .map_err(|_| CodecError::Invalid("image detail"))?,
        ),
        _ => return Err(CodecError::Invalid("image detail")),
    };
    let resource = Resource {
        location,
        description: ResourceDescription::Image {
            detail: image_detail,
        },
    };
    resource.validate()?;
    Ok(resource)
}
pub(super) fn write(resource: &Resource, profile: Profile) -> Value {
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
        ResourceLocation::OpaqueReference(_) => unreachable!("lowering rejects resource IDs"),
    };
    let mut image = match profile {
        Profile::Chat => json!({"url":url}),
        Profile::Responses => json!({"type":"input_image","image_url":url}),
    };
    if let Some(detail) = resource.image_detail() {
        image["detail"] = json!(detail.label());
    }
    if profile == Profile::Chat {
        json!({"type":"image_url","image_url":image})
    } else {
        image
    }
}
