//! Single-artifact native Generate and unmasked Edit profiles with Provider-selected encoding.
use super::Request;
use crate::{
    CodecError,
    semantic::{
        task::image_generation::{
            GeneratedImage, ImageBackground, ImageBackgroundRequest, ImageBilling, ImageData,
            ImageGenerationResponse, ImageSizeRequest, ImageTokenBreakdown, ImageUsage, UsdAmount,
        },
        value::{ImageFormat, Presence},
    },
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};

fn admit(request: &Request) -> Result<(), CodecError> {
    crate::openai::images::validate_request(request)?;
    let task = &request.task;
    if let Some(edit) = task.edit()
        && edit.mask.is_some()
    {
        return Err(CodecError::invalid());
    }
    if request.delivery.streaming()
        || task.requested_count() != 1
        || task.format.value().is_some()
        || matches!(task.size.value(), Some(ImageSizeRequest::Exact(_)))
        || matches!(
            task.background.value(),
            Some(ImageBackgroundRequest::Exact(ImageBackground::Transparent))
        )
    {
        return Err(CodecError::invalid());
    }
    Ok(())
}

pub(crate) fn gemini_request(request: &Request, model: &str) -> Result<Value, CodecError> {
    admit(request)?;
    if !crate::openai::speech::valid_model(model) {
        return Err(CodecError::invalid());
    }
    let mut content = Vec::new();
    if let Some(edit) = request.task.edit() {
        for source in &edit.sources {
            content.push(json!({
                "type": "image",
                "mime_type": source.mime(),
                "data": STANDARD.encode(source.data()),
            }));
        }
    }
    content.push(json!({"type": "text", "text": request.task.prompt()}));
    Ok(json!({
        "model": model,
        "store": false,
        "input": [{"type": "user_input", "content": content}],
        "response_format": {"type": "image"}
    }))
}

pub(crate) fn grok_request(request: &Request, model: &str) -> Result<Value, CodecError> {
    admit(request)?;
    if !crate::openai::speech::valid_model(model) {
        return Err(CodecError::invalid());
    }
    let mut body = json!({
        "model": model, "prompt": request.task.prompt(), "n": 1, "response_format": "b64_json"
    });
    if let Some(edit) = request.task.edit() {
        let mut refs = edit.sources.iter().map(|source| {
            let data_url = format!(
                "data:{};base64,{}",
                source.mime(),
                STANDARD.encode(source.data())
            );
            json!({"type": "image_url", "url": data_url})
        });
        if edit.sources.len() == 1 {
            body["image"] = refs.next().expect("single source");
        } else {
            body["images"] = Value::Array(refs.collect());
        }
    }
    if let Some(user) = &request.identity.user {
        body["user"] = json!(user);
    }
    Ok(body)
}

fn completed(encoded: &str, mime: &str) -> Result<ImageGenerationResponse, CodecError> {
    let format = mime
        .parse::<ImageFormat>()
        .map_err(|_| CodecError::invalid())?;
    let mut image =
        GeneratedImage::new(ImageData::new(encoded).map_err(|_| CodecError::invalid())?);
    image.format = Presence::Value(format);
    let response = ImageGenerationResponse {
        created: Presence::Absent,
        model: Presence::Absent,
        safety_identifier: Presence::Absent,
        images: vec![image],
        usage: Presence::Absent,
    };
    response.validate().map_err(|_| CodecError::invalid())?;
    Ok(response)
}

#[derive(Deserialize)]
struct GeminiInteraction {
    status: String,
    #[serde(default)]
    errors: Vec<Value>,
    #[serde(default)]
    steps: Vec<GeminiStep>,
}
#[derive(Deserialize)]
struct GeminiStep {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    content: Vec<GeminiPart>,
}
#[derive(Deserialize)]
struct GeminiPart {
    #[serde(rename = "type")]
    kind: String,
    data: Option<String>,
    mime_type: Option<String>,
}
pub(crate) fn gemini_response(bytes: &[u8]) -> Result<ImageGenerationResponse, CodecError> {
    let interaction: GeminiInteraction =
        serde_json::from_value(crate::json::decode_image_response(bytes)?)
            .map_err(|_| CodecError::invalid())?;
    if interaction.status != "completed" || !interaction.errors.is_empty() {
        return Err(CodecError::invalid());
    }
    let mut image = None;
    for step in interaction.steps {
        match step.kind.as_str() {
            "thought" => continue,
            "model_output" => {
                for part in step.content {
                    match part.kind.as_str() {
                        "text" => {}
                        "image" if image.is_none() => {
                            let Some(data) = part.data else {
                                return Err(CodecError::invalid());
                            };
                            let Some(mime_type) = part.mime_type else {
                                return Err(CodecError::invalid());
                            };
                            image = Some((data, mime_type));
                        }
                        _ => return Err(CodecError::invalid()),
                    }
                }
            }
            _ => return Err(CodecError::invalid()),
        }
    }
    let (data, mime_type) = image.ok_or_else(CodecError::invalid)?;
    completed(&data, &mime_type)
}

#[derive(Deserialize)]
struct Grok {
    data: Vec<GrokImage>,
    usage: Option<GrokUsage>,
}
#[derive(Deserialize)]
struct GrokImage {
    b64_json: String,
    mime_type: String,
    url: Option<String>,
}
#[derive(Deserialize)]
struct GrokUsage {
    cost_in_usd_ticks: u64,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
    input_tokens_details: Option<GrokDetails>,
    output_tokens_details: Option<GrokDetails>,
}
#[derive(Deserialize)]
struct GrokDetails {
    text_tokens: u64,
    image_tokens: u64,
    cached_tokens: Option<u64>,
    reasoning_tokens: Option<u64>,
}
impl From<GrokDetails> for ImageTokenBreakdown {
    fn from(raw: GrokDetails) -> Self {
        Self {
            text: Presence::Value(raw.text_tokens),
            image: Presence::Value(raw.image_tokens),
            cached: present(raw.cached_tokens),
            reasoning: present(raw.reasoning_tokens),
            ..Self::default()
        }
    }
}
fn present<T>(value: Option<T>) -> Presence<T> {
    value.map_or(Presence::Absent, Presence::Value)
}
pub(crate) fn grok_response(bytes: &[u8]) -> Result<ImageGenerationResponse, CodecError> {
    let Grok { data, usage } = serde_json::from_value(crate::json::decode_image_response(bytes)?)
        .map_err(|_| CodecError::invalid())?;
    let [image] = data.as_slice() else {
        return Err(CodecError::invalid());
    };
    if image.url.is_some() {
        return Err(CodecError::invalid());
    }
    let mut response = completed(&image.b64_json, &image.mime_type)?;
    if let Some(usage) = usage {
        let ticks = usage.cost_in_usd_ticks;
        let scale = 10_000_000_000;
        let cost = format!("{}.{:010}", ticks / scale, ticks % scale)
            .parse()
            .map_err(|_| CodecError::invalid())?;
        response.usage = Presence::Value(ImageUsage {
            input: present(usage.input_tokens),
            input_details: present(usage.input_tokens_details.map(Into::into)),
            output: present(usage.output_tokens),
            output_details: present(usage.output_tokens_details.map(Into::into)),
            total: present(usage.total_tokens),
            billing: ImageBilling {
                cost: Presence::Value(UsdAmount::new(cost).map_err(|_| CodecError::invalid())?),
                ..ImageBilling::default()
            },
        });
        response.validate().map_err(|_| CodecError::invalid())?;
    }
    Ok(response)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::images::decode_request;

    #[test]
    fn native_image_cannot_promise_unselected_pixel_controls() {
        for control in [
            json!({"size":"1024x1024"}),
            json!({"n":2}),
            json!({"output_format":"jpeg"}),
        ] {
            let mut request = json!({"model":"fixture","prompt":"pattern"});
            request
                .as_object_mut()
                .unwrap()
                .extend(control.as_object().unwrap().clone());
            let request = decode_request(&serde_json::to_vec(&request).unwrap()).unwrap();
            assert!(gemini_request(&request, "fixture").is_err());
            assert!(grok_request(&request, "fixture").is_err());
        }
    }

    #[test]
    fn gemini_requires_one_final_image_not_text_or_a_thought_artifact() {
        let response = |status, steps| json!({"id":"i1","status":status,"steps":steps});
        let decode = |value| gemini_response(&serde_json::to_vec(&value).unwrap());
        let thought = json!({"type":"thought","summary":[{"type":"image","mime_type":"image/png","data":"AQID"}]});
        let image = json!({"type":"image","mime_type":"image/jpeg","data":"BAUG"});
        let result = decode(response(
            "completed",
            json!([thought, {"type":"model_output","content":[{"type":"text","text":"Done."}, image]}]),
        ))
        .unwrap();
        assert_eq!(result.images[0].format, Presence::Value(ImageFormat::Jpeg));
        assert_eq!(result.images[0].data.as_base64(), "BAUG");
        assert!(
            decode(response(
                "completed",
                json!([thought, {"type":"model_output","content":[{"type":"text","text":"Done."}]}])
            ))
            .is_err()
        );
        assert!(
            decode(response(
                "incomplete",
                json!([{"type":"model_output","content":[image]}])
            ))
            .is_err()
        );
        assert!(
            decode(response(
                "completed",
                json!([{"type":"model_output","content":[image, image]}])
            ))
            .is_err()
        );
        assert!(decode(json!({"id":"i1","status":"completed","errors":[{"message":"fault"}],"steps":[{"type":"model_output","content":[image]}]})).is_err());
        assert!(decode(response("completed", json!([{"type":"model_output","content":[{"type":"image","uri":"https://example.invalid/out.png"}]}]))).is_err());
    }

    #[test]
    fn grok_cost_is_exact_and_omitted_only_by_native_delivery_projection() {
        let value = br#"{"data":[{"b64_json":"AQID","mime_type":"image/jpeg"}],
                       "usage":{"cost_in_usd_ticks":400000001,"input_tokens":2,
                       "output_tokens":5,"total_tokens":7}}"#;
        let result = grok_response(value).unwrap();
        let usage = result.usage.value().unwrap();
        assert_eq!(
            usage.billing.cost.value().unwrap().amount().to_string(),
            "0.0400000001"
        );
        assert_eq!(result.images[0].format, Presence::Value(ImageFormat::Jpeg));
        let projected = crate::image::projection::project_response(
            &result,
            crate::image::projection::AccountingPolicy::NativeStaticArtifacts,
        )
        .unwrap();
        assert!(projected.loss.billing_omitted && projected.response.usage.is_absent());
        assert!(
            grok_response(br#"{"data":[{"b64_json":"AQID","mime_type":"image/gif"}]}"#).is_err()
        );
        assert!(grok_response(br#"{"data":[{"b64_json":"AQID"}]}"#).is_err());
    }
}
