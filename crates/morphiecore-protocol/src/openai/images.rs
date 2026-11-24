//! Static GPT Image profile; request admission is independent of upstream report extensions.
//! Sources: https://developers.openai.com/api/reference/resources/images/methods/generate
//! https://developers.openai.com/api/reference/resources/images/methods/edit
//! https://github.com/openai/openai-python/tree/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types
mod multipart;
mod stream;
pub use multipart::{MAX_EDIT_UPLOAD_BYTES, decode_edit_request, encode_edit_request};
pub use stream::{StreamDecoder, encode_sse_event};

use crate::{
    CodecError,
    image::Request,
    semantic::{
        context::{ClientIdentityHints, DeliveryIntent},
        task::image_generation::*,
        value::{ImageFormat, Presence},
    },
};
use serde_json::{Map, Value, json};
fn required<'a>(map: &'a Map<String, Value>, key: &str) -> Result<&'a Value, CodecError> {
    map.get(key).ok_or_else(CodecError::invalid)
}
fn string(value: &Value) -> Result<&str, CodecError> {
    value.as_str().ok_or_else(CodecError::invalid)
}
fn number(value: &Value) -> Result<u64, CodecError> {
    value.as_u64().ok_or_else(CodecError::invalid)
}
fn optional<T>(
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
pub(super) fn format_str(raw: &str) -> Result<ImageFormat, CodecError> {
    match raw {
        "png" => Ok(ImageFormat::Png),
        "jpeg" => Ok(ImageFormat::Jpeg),
        "webp" => Ok(ImageFormat::Webp),
        _ => Err(CodecError::invalid()),
    }
}
fn format(value: &Value) -> Result<ImageFormat, CodecError> {
    format_str(string(value)?)
}
pub(super) fn format_label(value: &ImageFormat) -> &'static str {
    match value {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::Webp => "webp",
        _ => unreachable!("validated format"),
    }
}
fn format_value(value: &ImageFormat) -> Value {
    json!(format_label(value))
}
pub(super) fn size_request_str(raw: &str) -> Result<ImageSizeRequest, CodecError> {
    if raw == "auto" {
        Ok(ImageSizeRequest::Auto)
    } else {
        size_str(raw).map(ImageSizeRequest::Exact)
    }
}
pub(super) fn quality_request_str(raw: &str) -> Result<ImageQualityRequest, CodecError> {
    if raw == "auto" {
        Ok(ImageQualityRequest::Auto)
    } else {
        Ok(ImageQualityRequest::Exact(
            raw.parse().map_err(|_| CodecError::invalid())?,
        ))
    }
}
pub(super) fn background_request_str(raw: &str) -> Result<ImageBackgroundRequest, CodecError> {
    if raw == "auto" {
        Ok(ImageBackgroundRequest::Auto)
    } else {
        Ok(ImageBackgroundRequest::Exact(
            raw.parse().map_err(|_| CodecError::invalid())?,
        ))
    }
}
pub(super) fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 256
        && !model.chars().any(|c| c.is_control() || c.is_whitespace())
}
pub fn decode_request(bytes: &[u8]) -> Result<Request, CodecError> {
    let value = crate::json::decode(bytes)?;
    let map = value.as_object().ok_or_else(CodecError::invalid)?;
    let allowed = [
        "model",
        "prompt",
        "n",
        "partial_images",
        "stream",
        "output_format",
        "size",
        "quality",
        "background",
        "output_compression",
        "moderation",
        "safety_identifier",
        "user",
    ];
    if map.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(CodecError::invalid());
    }
    let mut task = ImageGenerationRequest::new(string(required(map, "prompt")?)?)
        .map_err(|_| CodecError::invalid())?;
    task.count = optional(map, "n", |v| {
        u8::try_from(number(v)?).map_err(|_| CodecError::invalid())
    })?;
    task.partial_images = optional(map, "partial_images", |v| {
        u8::try_from(number(v)?).map_err(|_| CodecError::invalid())
    })?;
    task.format = optional(map, "output_format", format)?;
    task.size = optional(map, "size", |v| size_request_str(string(v)?))?;
    task.quality = optional(map, "quality", |v| quality_request_str(string(v)?))?;
    task.background = optional(map, "background", |v| background_request_str(string(v)?))?;
    task.compression = optional(map, "output_compression", |v| {
        u8::try_from(number(v)?).map_err(|_| CodecError::invalid())
    })?;
    task.moderation = optional(map, "moderation", |v| {
        string(v)?.parse().map_err(|_| CodecError::invalid())
    })?;
    let request = Request {
        model: string(required(map, "model")?)?.into(),
        task,
        delivery: DeliveryIntent {
            stream: optional(map, "stream", |v| {
                v.as_bool().ok_or_else(CodecError::invalid)
            })?
            .value()
            .copied(),
            options: None,
        },
        identity: ClientIdentityHints {
            safety_identifier: optional(map, "safety_identifier", |v| Ok(string(v)?.into()))?
                .value()
                .cloned(),
            user: {
                let user = optional(map, "user", |v| Ok(string(v)?.into()))?;
                if user == Presence::Null {
                    return Err(CodecError::invalid());
                }
                user.value().cloned()
            },
        },
    };
    validate_request(&request)?;
    Ok(request)
}
pub fn validate_request(request: &Request) -> Result<(), CodecError> {
    request.task.validate().map_err(|_| CodecError::invalid())?;
    request
        .identity
        .validate()
        .map_err(|_| CodecError::invalid())?;
    if !valid_model(&request.model)
        || request.delivery.options.is_some()
        || (request.delivery.streaming() && request.task.requested_count() != 1)
        || (!request.delivery.streaming() && request.task.partial_images.value().is_some())
        // This profile defaults to PNG, unlike the protocol-neutral request.
        || request.task.compression.value().is_some()
            && !matches!(request.task.format.value(),Some(ImageFormat::Jpeg|ImageFormat::Webp))
    {
        return Err(CodecError::invalid());
    }
    Ok(())
}
pub fn encode_request(request: &Request, model: &str) -> Result<Value, CodecError> {
    validate_request(request)?;
    if request.task.is_edit() || !valid_model(model) {
        return Err(CodecError::invalid());
    }
    let mut map = Map::new();
    map.insert("model".into(), json!(model));
    map.insert("prompt".into(), json!(request.task.prompt()));
    put(&mut map, "n", &request.task.count, |n| json!(n));
    put(
        &mut map,
        "partial_images",
        &request.task.partial_images,
        |n| json!(n),
    );
    if let Some(stream) = request.delivery.stream {
        map.insert("stream".into(), json!(stream));
    }
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
    if let Some(value) = &request.identity.safety_identifier {
        map.insert("safety_identifier".into(), json!(value));
    }
    if let Some(value) = &request.identity.user {
        map.insert("user".into(), json!(value));
    }
    Ok(Value::Object(map))
}
fn size_str(raw: &str) -> Result<ImageSize, CodecError> {
    let (w, h) = raw.split_once('x').ok_or_else(CodecError::invalid)?;
    let size = ImageSize {
        width: w.parse().map_err(|_| CodecError::invalid())?,
        height: h.parse().map_err(|_| CodecError::invalid())?,
    };
    size.validate().map_err(|_| CodecError::invalid())?;
    if raw != format!("{}x{}", size.width, size.height) {
        return Err(CodecError::invalid());
    }
    Ok(size)
}
fn size(value: &Value) -> Result<ImageSize, CodecError> {
    size_str(string(value)?)
}
fn breakdown(value: &Value) -> Result<ImageTokenBreakdown, CodecError> {
    let map = value.as_object().ok_or_else(CodecError::invalid)?;
    Ok(ImageTokenBreakdown::text_image(
        number(required(map, "text_tokens")?)?,
        number(required(map, "image_tokens")?)?,
    ))
}
fn usage(value: &Value) -> Result<ImageUsage, CodecError> {
    let map = value.as_object().ok_or_else(CodecError::invalid)?;
    let usage = ImageUsage {
        input: Presence::Value(number(required(map, "input_tokens")?)?),
        input_details: Presence::Value(breakdown(required(map, "input_tokens_details")?)?),
        output: Presence::Value(number(required(map, "output_tokens")?)?),
        output_details: optional(map, "output_tokens_details", breakdown)?,
        total: Presence::Value(number(required(map, "total_tokens")?)?),
        billing: ImageBilling::default(),
    };
    usage.validate().map_err(|_| CodecError::invalid())?;
    if !crate::image::projection::standard_tokens_representable(&usage) {
        return Err(CodecError::invalid());
    }
    Ok(usage)
}
pub fn decode_response(bytes: &[u8]) -> Result<ImageGenerationResponse, CodecError> {
    let value = crate::json::decode_image_response(bytes)?;
    let map = value.as_object().ok_or_else(CodecError::invalid)?;
    if map.contains_key("svg_usage") {
        return Err(CodecError::invalid());
    }
    let data = required(map, "data")?
        .as_array()
        .ok_or_else(CodecError::invalid)?;
    if !(1..=usize::from(MAX_IMAGE_COUNT)).contains(&data.len()) {
        return Err(CodecError::invalid());
    }
    let format = optional(map, "output_format", format)?;
    let size = optional(map, "size", size)?;
    let background = optional(map, "background", |v| {
        string(v)?.parse().map_err(|_| CodecError::invalid())
    })?;
    let quality = optional(map, "quality", |v| {
        string(v)?.parse().map_err(|_| CodecError::invalid())
    })?;
    let mut total = 0;
    let mut images = Vec::with_capacity(data.len());
    for item in data {
        let item = item.as_object().ok_or_else(CodecError::invalid)?;
        if item.contains_key("url") || item.contains_key("revised_prompt") {
            return Err(CodecError::invalid());
        }
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
        .map_err(|_| CodecError::invalid())?;
    response.safety_identifier = optional(map, "safety_identifier", |v| Ok(string(v)?.into()))?;
    response.usage = optional(map, "usage", usage)?;
    response.validate().map_err(|_| CodecError::invalid())?;
    Ok(response)
}
/// Charge aggregate bytes before retaining another encoded artifact.
fn image_data(value: &Value, total: &mut usize) -> Result<ImageData, CodecError> {
    let encoded = string(value)?;
    let remaining = MAX_IMAGES_BYTES
        .checked_sub(*total)
        .ok_or_else(CodecError::invalid)?;
    if encoded.len() > remaining.div_ceil(3) * 4 {
        return Err(CodecError::Limit);
    }
    let data = ImageData::new(encoded).map_err(|_| CodecError::invalid())?;
    *total = total
        .checked_add(data.decoded_bytes())
        .ok_or_else(CodecError::invalid)?;
    if *total > MAX_IMAGES_BYTES {
        return Err(CodecError::Limit);
    }
    Ok(data)
}
fn breakdown_value(v: &ImageTokenBreakdown) -> Value {
    json!({
        "text_tokens": v.text.value().expect("validated text count"),
        "image_tokens": v.image.value().expect("validated image count"),
    })
}
fn usage_value(v: &ImageUsage) -> Value {
    let mut map = Map::new();
    map.insert(
        "input_tokens".into(),
        json!(v.input.value().expect("validated input")),
    );
    map.insert(
        "input_tokens_details".into(),
        breakdown_value(v.input_details.value().expect("validated input details")),
    );
    map.insert(
        "output_tokens".into(),
        json!(v.output.value().expect("validated output")),
    );
    map.insert(
        "total_tokens".into(),
        json!(v.total.value().expect("validated total")),
    );
    put(
        &mut map,
        "output_tokens_details",
        &v.output_details,
        breakdown_value,
    );
    Value::Object(map)
}
pub fn encode_response(response: &ImageGenerationResponse) -> Result<Value, CodecError> {
    if response.created.value().is_none() {
        return Err(CodecError::invalid());
    }
    encode_native_static_response(response)
}
/// Explicit native-static client profile. An upstream creation report may be absent.
pub fn encode_native_static_response(
    response: &ImageGenerationResponse,
) -> Result<Value, CodecError> {
    response.validate().map_err(|_| CodecError::invalid())?;
    if !response.model.is_absent() {
        return Err(CodecError::invalid());
    }
    let image = &response.images[0];
    if response
        .images
        .iter()
        .any(|other| !image.same_reports(other))
    {
        return Err(CodecError::invalid());
    }
    if response.usage.value().is_some_and(|u| {
        !crate::image::projection::standard_tokens_representable(u) || !u.billing.is_absent()
    }) {
        return Err(CodecError::invalid());
    }
    let mut map = Map::new();
    put(&mut map, "created", &response.created, |value| json!(value));
    put(
        &mut map,
        "safety_identifier",
        &response.safety_identifier,
        |v| json!(v),
    );
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
