//! OpenRouter's dedicated Images operation, not its Chat image projection.
//! Sources: https://openrouter.ai/docs/api/api-reference/images/generate-an-image
//! https://openrouter.ai/api/v1/images/models/openai/gpt-image-2.5-flare/endpoints
use crate::{
    adapter::images::Request,
    protocol::{
        CodecError,
        openai::images::{number, object, optional, required, string},
    },
    semantic::{
        task::image_generation::*,
        value::{ImageFormat, Presence},
    },
};
use serde_json::{Map, Value, json};
fn invalid() -> CodecError {
    CodecError::Invalid("OpenRouter image generation")
}
pub fn encode_request(request: &Request, model: &str) -> Result<Value, CodecError> {
    // A supported aspect ratio is not an exact pixel-size or encoding guarantee.
    // Neither size nor output_format is declared by this endpoint. Compression
    // cannot be honored without an admitted compatible encoding.
    if model != "openai/gpt-image-2.5-flare"
        || request.task.format.value().is_some()
        || request.task.size.value().is_some()
        || request.task.compression.value().is_some()
    {
        return Err(invalid());
    }
    super::openai::images::validate_request(request)?;
    // Upstream n is an upper bound; exact local satisfaction is checked after EOF.
    // OpenAI null defaults do not authorize nullable upstream controls.
    let mut value = json!({"model":model,"prompt":request.task.prompt(),"n":request.task.requested_count(),"stream":false,
        "provider":{"only":["openai"],"allow_fallbacks":false}});
    if let Some(quality) = request.task.quality.value() {
        value["quality"] = json!(quality.label());
    }
    if let Some(background) = request.task.background.value() {
        value["background"] = json!(background.label());
    }
    if let Some(moderation) = request.task.moderation.value() {
        let label: &str = (*moderation).into();
        value["provider"]["options"] = json!({"openai":{"moderation":label}});
    }
    if let Some(user) = request.identity.user.value() {
        value["user"] = json!(user);
    }
    Ok(value)
}
fn input_details(value: &Value) -> Result<ImageTokenBreakdown, CodecError> {
    let m = object(
        value,
        &[
            "audio_tokens",
            "cached_tokens",
            "cache_write_tokens",
            "file_tokens",
            "video_tokens",
        ],
    )?;
    Ok(ImageTokenBreakdown {
        audio: optional(m, "audio_tokens", number)?,
        cached: optional(m, "cached_tokens", number)?,
        cache_write: optional(m, "cache_write_tokens", number)?,
        file: optional(m, "file_tokens", number)?,
        video: optional(m, "video_tokens", number)?,
        ..Default::default()
    })
}
fn output_details(value: &Value) -> Result<ImageTokenBreakdown, CodecError> {
    let m = object(value, &["audio_tokens", "image_tokens", "reasoning_tokens"])?;
    Ok(ImageTokenBreakdown {
        audio: optional(m, "audio_tokens", number)?,
        image: optional(m, "image_tokens", number)?,
        reasoning: optional(m, "reasoning_tokens", number)?,
        ..Default::default()
    })
}
fn amount(value: &Value) -> Result<UsdAmount, CodecError> {
    let Value::Number(value) = value else {
        return Err(invalid());
    };
    UsdAmount::new(value.clone()).map_err(|_| invalid())
}
fn alias(m: &Map<String, Value>, a: &str, b: &str) -> Result<Presence<UsdAmount>, CodecError> {
    if m.contains_key(a) && m.contains_key(b) {
        return Err(invalid());
    }
    optional(m, if m.contains_key(a) { a } else { b }, amount)
}
fn cost_details(value: &Value) -> Result<ImageCostBreakdown, CodecError> {
    let m = object(
        value,
        &[
            "upstream_inference_cost",
            "upstream_inference_prompt_cost",
            "upstream_inference_completions_cost",
            "upstream_inference_input_cost",
            "upstream_inference_output_cost",
        ],
    )?;
    Ok(ImageCostBreakdown {
        upstream_total: optional(m, "upstream_inference_cost", amount)?,
        upstream_input: alias(
            m,
            "upstream_inference_prompt_cost",
            "upstream_inference_input_cost",
        )?,
        upstream_output: alias(
            m,
            "upstream_inference_completions_cost",
            "upstream_inference_output_cost",
        )?,
    })
}
fn usage(value: &Value) -> Result<ImageUsage, CodecError> {
    let m = object(
        value,
        &[
            "prompt_tokens",
            "completion_tokens",
            "total_tokens",
            "prompt_tokens_details",
            "completion_tokens_details",
            "cost",
            "cost_details",
            "is_byok",
        ],
    )?;
    let usage = ImageUsage {
        input: number(required(m, "prompt_tokens")?)?,
        output: number(required(m, "completion_tokens")?)?,
        total: number(required(m, "total_tokens")?)?,
        input_details: optional(m, "prompt_tokens_details", input_details)?,
        output_details: optional(m, "completion_tokens_details", output_details)?,
        billing: ImageBilling {
            cost: optional(m, "cost", amount)?,
            upstream: optional(m, "cost_details", cost_details)?,
            byok: optional(m, "is_byok", |v| v.as_bool().ok_or_else(invalid))?,
        },
    };
    usage.validate().map_err(|_| invalid())?;
    Ok(usage)
}
pub fn decode_response(bytes: &[u8]) -> Result<ImageGenerationResponse, CodecError> {
    let value = super::openai::json::decode_image_response(bytes)?;
    let map = object(&value, &["created", "data", "usage"])?;
    let data = required(map, "data")?.as_array().ok_or_else(invalid)?;
    if !(1..=usize::from(MAX_IMAGE_COUNT)).contains(&data.len()) {
        return Err(invalid());
    }
    let mut total = 0;
    let mut images = Vec::with_capacity(data.len());
    for item in data {
        let item = object(item, &["b64_json", "media_type"])?;
        let mut image = GeneratedImage::new(super::openai::images::image_data(
            required(item, "b64_json")?,
            &mut total,
        )?);
        if let Some(v) = item.get("media_type") {
            image.format =
                Presence::Value(string(v)?.parse::<ImageFormat>().map_err(|_| invalid())?);
        }
        image.validate().map_err(|_| invalid())?;
        images.push(image);
    }
    let mut result = ImageGenerationResponse::new(number(required(map, "created")?)?, images)
        .map_err(|_| invalid())?;
    result.usage = optional(map, "usage", usage)?;
    result.validate().map_err(|_| invalid())?;
    Ok(result)
}
