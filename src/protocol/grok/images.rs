//! Static Grok Imagine Image 2.0 projection; no network or binding activation.
//! Sources checked 2026-10-22: https://docs.x.ai/developers/model-capabilities/images/generation
//! https://docs.x.ai/developers/rest-api-reference/inference/images.md
use crate::{
    adapter::images::Request,
    protocol::{CodecError, openai::images::image_data},
    semantic::{
        task::image_generation::*,
        value::{ImageFormat, Presence},
    },
};
use serde::Deserialize;
use serde_json::{Value, json};

fn invalid() -> CodecError {
    CodecError::Invalid(crate::protocol::failure::ShapeReason::InvalidValue)
}

/// Project only controls with an exact carrier. Resolution tiers and aspect
/// ratios are not exact pixel dimensions; no size-to-ratio approximation occurs.
pub fn encode_request(request: &Request, model: &str) -> Result<Value, CodecError> {
    request.task.validate().map_err(|_| invalid())?;
    request.identity.validate().map_err(|_| invalid())?;
    if model != "grok-imagine-image-2.0"
        || request.model.is_empty()
        || request.model.len() > 256
        || request
            .model
            .chars()
            .any(|c| c.is_control() || c.is_whitespace())
        || request.delivery.streaming()
        || !request.delivery.options.is_absent()
        || !request.identity.safety_identifier.is_absent()
        || matches!(request.identity.user, Presence::Null)
        || request.task.format.value().is_some()
        || request.task.size.value().is_some()
        || request.task.background.value().is_some()
        || request.task.compression.value().is_some()
        || request.task.moderation.value().is_some()
    {
        return Err(invalid());
    }
    let mut wire = json!({
        "model": model,
        "prompt": request.task.prompt(),
        "n": request.task.requested_count(),
        "response_format": "b64_json"
    });
    if let Some(quality) = request.task.quality.value() {
        wire["quality"] = match quality {
            ImageQualityRequest::Auto => json!("auto"),
            ImageQualityRequest::Exact(ImageQuality::Low) => json!("low"),
            ImageQualityRequest::Exact(ImageQuality::Medium) => json!("medium"),
            _ => return Err(invalid()),
        };
    }
    if let Some(user) = request.identity.user.value() {
        wire["user"] = json!(user);
    }
    Ok(wire)
}

#[derive(Deserialize)]
struct Response {
    data: Vec<Artifact>,
    #[serde(default)]
    created: Presence<u64>,
    #[serde(default)]
    model: Presence<String>,
    #[serde(default)]
    usage: Presence<Usage>,
    // A deferred/error envelope is never a static image completion.
    #[serde(default)]
    request_id: Option<Value>,
    #[serde(default)]
    status: Option<Value>,
    #[serde(default)]
    error: Option<Value>,
}
#[derive(Deserialize)]
struct Artifact {
    b64_json: String,
    #[serde(default)]
    mime_type: Presence<String>,
    #[serde(default)]
    respect_moderation: Option<bool>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    file_output: Option<Value>,
    #[serde(default)]
    storage_error: Option<String>,
}
#[derive(Deserialize)]
struct Usage {
    cost_in_usd_ticks: u64,
    #[serde(default)]
    input_tokens: Presence<u64>,
    #[serde(default)]
    output_tokens: Presence<u64>,
    #[serde(default)]
    total_tokens: Presence<u64>,
    #[serde(default)]
    input_tokens_details: Presence<InputDetails>,
    #[serde(default)]
    output_tokens_details: Presence<OutputDetails>,
}
#[derive(Deserialize)]
struct InputDetails {
    text_tokens: u64,
    image_tokens: u64,
    cached_tokens: u64,
}
#[derive(Deserialize)]
struct OutputDetails {
    text_tokens: u64,
    image_tokens: u64,
    reasoning_tokens: u64,
}
fn map_presence<T, U>(
    value: Presence<T>,
    convert: impl FnOnce(T) -> Result<U, CodecError>,
) -> Result<Presence<U>, CodecError> {
    Ok(match value {
        Presence::Absent => Presence::Absent,
        Presence::Null => Presence::Null,
        Presence::Value(value) => Presence::Value(convert(value)?),
    })
}
fn usage(wire: Usage) -> Result<ImageUsage, CodecError> {
    // Derived detail sums constrain consistency only; never fill missing reports.
    let mut input = wire.input_tokens.value().copied();
    let mut output = wire.output_tokens.value().copied();
    // Decimal arithmetic preserves ticks exactly, including values beyond f64's
    // integer precision. One USD is 10^10 ticks, not 10^8 (one cent).
    let cost = format!(
        "{}.{:010}",
        wire.cost_in_usd_ticks / 10_000_000_000,
        wire.cost_in_usd_ticks % 10_000_000_000
    );
    let result = ImageUsage {
        input: wire.input_tokens,
        output: wire.output_tokens,
        total: wire.total_tokens,
        input_details: map_presence(wire.input_tokens_details, |d| {
            if d.cached_tokens > d.text_tokens {
                return Err(invalid());
            }
            let sum = d
                .text_tokens
                .checked_add(d.image_tokens)
                .ok_or_else(invalid)?;
            if input.is_some_and(|n| n != sum) {
                return Err(invalid());
            }
            input = Some(sum);
            Ok(ImageTokenBreakdown {
                text: Presence::Value(d.text_tokens),
                image: Presence::Value(d.image_tokens),
                cached: Presence::Value(d.cached_tokens),
                ..Default::default()
            })
        })?,
        output_details: map_presence(wire.output_tokens_details, |d| {
            let sum = d
                .text_tokens
                .checked_add(d.image_tokens)
                .and_then(|n| n.checked_add(d.reasoning_tokens))
                .ok_or_else(invalid)?;
            if output.is_some_and(|n| n != sum) {
                return Err(invalid());
            }
            output = Some(sum);
            Ok(ImageTokenBreakdown {
                text: Presence::Value(d.text_tokens),
                image: Presence::Value(d.image_tokens),
                reasoning: Presence::Value(d.reasoning_tokens),
                ..Default::default()
            })
        })?,
        billing: ImageBilling {
            cost: Presence::Value(
                UsdAmount::new(cost.parse().map_err(|_| invalid())?).map_err(|_| invalid())?,
            ),
            ..Default::default()
        },
    };
    if let Some(total) = result.total.value() {
        if [input, output].into_iter().flatten().any(|n| n > *total) {
            return Err(invalid());
        }
        if let (Some(input), Some(output)) = (input, output)
            && input.checked_add(output) != Some(*total)
        {
            return Err(invalid());
        }
    }
    result.validate().map_err(|_| invalid())?;
    Ok(result)
}

pub fn decode_response(bytes: &[u8]) -> Result<ImageGenerationResponse, CodecError> {
    // Strict JSON intake rejects duplicate keys before typed deserialization.
    // Unknown upstream fields are ignored; known fields retain strict types.
    let wire: Response = serde_json::from_value(super::super::json::decode_image_response(bytes)?)
        .map_err(|_| invalid())?;
    if wire.request_id.is_some()
        || wire.status.is_some()
        || wire.error.is_some()
        || !(1..=usize::from(MAX_IMAGE_COUNT)).contains(&wire.data.len())
    {
        return Err(invalid());
    }
    let mut total = 0;
    let mut images = Vec::with_capacity(wire.data.len());
    for artifact in wire.data {
        if artifact.respect_moderation == Some(false)
            || artifact.url.is_some()
            || artifact.file_output.is_some()
            || artifact.storage_error.is_some()
        {
            return Err(invalid());
        }
        let mut image =
            GeneratedImage::new(image_data(&Value::String(artifact.b64_json), &mut total)?);
        image.format = map_presence(artifact.mime_type, |mime| {
            let format = mime.parse::<ImageFormat>().map_err(|_| invalid())?;
            if !matches!(
                format,
                ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Webp
            ) {
                return Err(invalid());
            }
            Ok(format)
        })?;
        images.push(image);
    }
    let result = ImageGenerationResponse {
        created: wire.created,
        model: wire.model,
        images,
        usage: map_presence(wire.usage, usage)?,
    };
    result.validate().map_err(|_| invalid())?;
    Ok(result)
}

#[cfg(test)]
mod tests;
