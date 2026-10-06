//! Static GPT Image profile; unknown fields and unsupported options fail closed.
//! Sources: https://developers.openai.com/api/reference/resources/images/methods/generate
//! https://github.com/openai/openai-python/tree/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types
use crate::{
    protocol::CodecError,
    semantic::{
        task::image_generation::*,
        value::{ImageFormat, Presence},
    },
};
use serde_json::{Map, Value, json};

use crate::{
    adapter::images::Request,
    semantic::context::{ClientIdentityHints, DeliveryIntent},
};
fn invalid() -> CodecError {
    CodecError::Invalid("image generation")
}
pub(crate) fn object<'a>(
    value: &'a Value,
    allowed: &[&str],
) -> Result<&'a Map<String, Value>, CodecError> {
    let map = value.as_object().ok_or_else(invalid)?;
    if map.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid());
    }
    Ok(map)
}
pub(crate) fn required<'a>(
    map: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Value, CodecError> {
    map.get(key).ok_or_else(invalid)
}
pub(crate) fn string(value: &Value) -> Result<&str, CodecError> {
    value.as_str().ok_or_else(invalid)
}
pub(crate) fn number(value: &Value) -> Result<u64, CodecError> {
    value.as_u64().ok_or_else(invalid)
}
pub(crate) fn optional<T>(
    map: &Map<String, Value>,
    key: &str,
    parse: impl FnOnce(&Value) -> Result<T, CodecError>,
) -> Result<Presence<T>, CodecError> {
    Ok(match map.get(key) {
        None => Presence::Absent,
        Some(Value::Null) => Presence::Null,
        Some(v) => Presence::Value(parse(v)?),
    })
}
fn put<T>(
    map: &mut Map<String, Value>,
    key: &str,
    value: &Presence<T>,
    encode: impl FnOnce(&T) -> Value,
) {
    match value {
        Presence::Absent => {}
        Presence::Null => {
            map.insert(key.into(), Value::Null);
        }
        Presence::Value(v) => {
            map.insert(key.into(), encode(v));
        }
    }
}
fn format(value: &Value) -> Result<ImageFormat, CodecError> {
    match string(value)? {
        "png" => Ok(ImageFormat::Png),
        "jpeg" => Ok(ImageFormat::Jpeg),
        "webp" => Ok(ImageFormat::Webp),
        _ => Err(invalid()),
    }
}
fn format_value(value: &ImageFormat) -> Value {
    json!(match value {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::Webp => "webp",
        _ => unreachable!("validated format"),
    })
}
fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 256
        && !model.chars().any(|c| c.is_control() || c.is_whitespace())
}
pub fn decode_request(bytes: &[u8]) -> Result<Request, CodecError> {
    let value = super::json::decode(bytes)?;
    let map = object(
        &value,
        &[
            "model",
            "prompt",
            "n",
            "stream",
            "output_format",
            "size",
            "quality",
            "background",
            "output_compression",
            "moderation",
            "user",
        ],
    )?;
    let mut task =
        ImageGenerationRequest::new(string(required(map, "prompt")?)?).map_err(|_| invalid())?;
    task.count = optional(map, "n", |v| {
        u8::try_from(number(v)?).map_err(|_| invalid())
    })?;
    task.format = optional(map, "output_format", format)?;
    task.size = optional(map, "size", |v| {
        if string(v)? == "auto" {
            Ok(ImageSizeRequest::Auto)
        } else {
            size(v).map(ImageSizeRequest::Exact)
        }
    })?;
    task.quality = optional(map, "quality", |v| {
        if string(v)? == "auto" {
            Ok(ImageQualityRequest::Auto)
        } else {
            Ok(ImageQualityRequest::Exact(
                string(v)?.parse().map_err(|_| invalid())?,
            ))
        }
    })?;
    task.background = optional(map, "background", |v| {
        if string(v)? == "auto" {
            Ok(ImageBackgroundRequest::Auto)
        } else {
            Ok(ImageBackgroundRequest::Exact(
                string(v)?.parse().map_err(|_| invalid())?,
            ))
        }
    })?;
    task.compression = optional(map, "output_compression", |v| {
        u8::try_from(number(v)?).map_err(|_| invalid())
    })?;
    task.moderation = optional(map, "moderation", |v| {
        string(v)?.parse().map_err(|_| invalid())
    })?;
    let request = Request {
        model: string(required(map, "model")?)?.into(),
        task,
        delivery: DeliveryIntent {
            stream: optional(map, "stream", |v| v.as_bool().ok_or_else(invalid))?,
            options: Presence::Absent,
        },
        identity: ClientIdentityHints {
            user: optional(map, "user", |v| Ok(string(v)?.into()))?,
            ..Default::default()
        },
    };
    validate_request(&request)?;
    Ok(request)
}
pub fn validate_request(request: &Request) -> Result<(), CodecError> {
    request.task.validate().map_err(|_| invalid())?;
    request.identity.validate().map_err(|_| invalid())?;
    if !valid_model(&request.model)
        || request.delivery.streaming()
        || !request.delivery.options.is_absent()
        || !request.identity.safety_identifier.is_absent()
        || matches!(request.identity.user, Presence::Null)
        // This profile defaults to PNG, unlike the protocol-neutral request.
        || request.task.compression.value().is_some()
            && !matches!(request.task.format.value(),Some(ImageFormat::Jpeg|ImageFormat::Webp))
    {
        return Err(invalid());
    }
    Ok(())
}
pub fn encode_request(request: &Request, model: &str) -> Result<Value, CodecError> {
    validate_request(request)?;
    if !valid_model(model) {
        return Err(invalid());
    }
    let mut map = Map::new();
    map.insert("model".into(), json!(model));
    map.insert("prompt".into(), json!(request.task.prompt()));
    put(&mut map, "n", &request.task.count, |n| json!(n));
    put(&mut map, "stream", &request.delivery.stream, |v| json!(v));
    put(
        &mut map,
        "output_format",
        &request.task.format,
        format_value,
    );
    put(&mut map, "size", &request.task.size, |v| match v {
        ImageSizeRequest::Auto => json!("auto"),
        ImageSizeRequest::Exact(s) => json!(format!("{}x{}", s.width, s.height)),
    });
    put(&mut map, "quality", &request.task.quality, |v| {
        json!(v.label())
    });
    put(&mut map, "background", &request.task.background, |v| {
        json!(v.label())
    });
    put(
        &mut map,
        "output_compression",
        &request.task.compression,
        |v| json!(v),
    );
    put(&mut map, "moderation", &request.task.moderation, |v| {
        let label: &str = (*v).into();
        json!(label)
    });
    put(&mut map, "user", &request.identity.user, |v| json!(v));
    Ok(Value::Object(map))
}
fn size(value: &Value) -> Result<ImageSize, CodecError> {
    let raw = string(value)?;
    let (w, h) = raw.split_once('x').ok_or_else(invalid)?;
    let size = ImageSize {
        width: w.parse().map_err(|_| invalid())?,
        height: h.parse().map_err(|_| invalid())?,
    };
    size.validate().map_err(|_| invalid())?;
    if raw != format!("{}x{}", size.width, size.height) {
        return Err(invalid());
    }
    Ok(size)
}
fn breakdown(value: &Value) -> Result<ImageTokenBreakdown, CodecError> {
    let map = object(value, &["text_tokens", "image_tokens"])?;
    Ok(ImageTokenBreakdown::text_image(
        number(required(map, "text_tokens")?)?,
        number(required(map, "image_tokens")?)?,
    ))
}
fn usage(value: &Value) -> Result<ImageUsage, CodecError> {
    let map = object(
        value,
        &[
            "input_tokens",
            "input_tokens_details",
            "output_tokens",
            "output_tokens_details",
            "total_tokens",
        ],
    )?;
    let usage = ImageUsage {
        input: number(required(map, "input_tokens")?)?,
        input_details: Presence::Value(breakdown(required(map, "input_tokens_details")?)?),
        output: number(required(map, "output_tokens")?)?,
        output_details: optional(map, "output_tokens_details", breakdown)?,
        total: number(required(map, "total_tokens")?)?,
        billing: ImageBilling::default(),
    };
    usage.validate().map_err(|_| invalid())?;
    if !crate::lowering::images::standard_tokens_representable(&usage) {
        return Err(invalid());
    }
    Ok(usage)
}
pub fn decode_response(bytes: &[u8]) -> Result<ImageGenerationResponse, CodecError> {
    let value = super::json::decode_image_response(bytes)?;
    let map = object(
        &value,
        &[
            "created",
            "data",
            "output_format",
            "size",
            "background",
            "quality",
            "usage",
        ],
    )?;
    let data = required(map, "data")?.as_array().ok_or_else(invalid)?;
    if !(1..=usize::from(MAX_IMAGE_COUNT)).contains(&data.len()) {
        return Err(invalid());
    }
    let format = optional(map, "output_format", format)?;
    let size = optional(map, "size", size)?;
    let background = optional(map, "background", |v| {
        string(v)?.parse().map_err(|_| invalid())
    })?;
    let quality = optional(map, "quality", |v| {
        string(v)?.parse().map_err(|_| invalid())
    })?;
    let mut total = 0;
    let mut images = Vec::with_capacity(data.len());
    for item in data {
        let item = object(item, &["b64_json"])?;
        let data = image_data(required(item, "b64_json")?, &mut total)?;
        images.push(GeneratedImage {
            data,
            format: format.clone(),
            size: size.clone(),
            background: background.clone(),
            quality: quality.clone(),
        });
    }
    let mut response = ImageGenerationResponse::new(number(required(map, "created")?)?, images)
        .map_err(|_| invalid())?;
    response.usage = optional(map, "usage", usage)?;
    response.validate().map_err(|_| invalid())?;
    Ok(response)
}
/// Charge aggregate bytes before retaining another encoded artifact.
pub(crate) fn image_data(value: &Value, total: &mut usize) -> Result<ImageData, CodecError> {
    let encoded = string(value)?;
    let remaining = MAX_IMAGES_BYTES.checked_sub(*total).ok_or_else(invalid)?;
    if encoded.len() > remaining.div_ceil(3) * 4 {
        return Err(CodecError::Limit);
    }
    let data = ImageData::new(encoded).map_err(|_| invalid())?;
    *total = total
        .checked_add(data.decoded_bytes())
        .ok_or_else(invalid)?;
    if *total > MAX_IMAGES_BYTES {
        return Err(CodecError::Limit);
    }
    Ok(data)
}
fn breakdown_value(v: &ImageTokenBreakdown) -> Value {
    json!({"text_tokens":v.text.value().expect("validated text count"),"image_tokens":v.image.value().expect("validated image count")})
}
fn usage_value(v: &ImageUsage) -> Value {
    let mut map = Map::new();
    map.insert("input_tokens".into(), json!(v.input));
    map.insert(
        "input_tokens_details".into(),
        breakdown_value(v.input_details.value().expect("validated input details")),
    );
    map.insert("output_tokens".into(), json!(v.output));
    map.insert("total_tokens".into(), json!(v.total));
    put(
        &mut map,
        "output_tokens_details",
        &v.output_details,
        breakdown_value,
    );
    Value::Object(map)
}
pub fn encode_response(response: &ImageGenerationResponse) -> Result<Value, CodecError> {
    response.validate().map_err(|_| invalid())?;
    let image = &response.images[0];
    if response
        .images
        .iter()
        .any(|other| !image.same_reports(other))
    {
        return Err(invalid());
    }
    if response.usage.value().is_some_and(|u| {
        !crate::lowering::images::standard_tokens_representable(u) || !u.billing.is_absent()
    }) {
        return Err(invalid());
    }
    let mut map = Map::new();
    map.insert("created".into(), json!(response.created));
    map.insert(
        "data".into(),
        Value::Array(
            response
                .images
                .iter()
                .map(|image| json!({"b64_json":image.data.as_base64()}))
                .collect(),
        ),
    );
    put(&mut map, "output_format", &image.format, format_value);
    put(&mut map, "size", &image.size, |s| {
        json!(format!("{}x{}", s.width, s.height))
    });
    put(&mut map, "background", &image.background, |v| {
        let label: &str = (*v).into();
        json!(label)
    });
    put(&mut map, "quality", &image.quality, |v| {
        let label: &str = (*v).into();
        json!(label)
    });
    put(&mut map, "usage", &response.usage, usage_value);
    Ok(Value::Object(map))
}
