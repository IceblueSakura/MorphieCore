//! Shared Token Plan and DashScope synchronous Qwen Image 3.0 wire.
use crate::{
    CodecError,
    image::Request,
    semantic::{
        task::image_generation::{
            GeneratedImage, ImageBackgroundRequest, ImageData, ImageGenerationResponse,
            ImageQualityRequest, ImageSize, ImageSizeRequest,
        },
        value::{ImageFormat, Presence},
    },
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};

pub fn encode_request(request: &Request, model: &str) -> Result<Value, CodecError> {
    crate::openai::images::validate_request(request)?;
    let task = &request.task;
    if !crate::openai::speech::valid_model(model)
        || task.requested_count() > 6
        || task.format.value().is_some_and(|f| *f != ImageFormat::Png)
        || matches!(task.quality.value(), Some(ImageQualityRequest::Exact(_)))
        || matches!(
            task.background.value(),
            Some(ImageBackgroundRequest::Exact(_))
        )
        || task.compression.value().is_some()
        || task.moderation.value().is_some()
        || request.identity.user.is_some()
        || request.identity.safety_identifier.is_some()
    {
        return Err(CodecError::invalid());
    }
    let mut content = Vec::new();
    if let Some(edit) = task.edit() {
        if edit.mask.is_some() || !edit.input_fidelity.is_absent() {
            return Err(CodecError::invalid());
        }
        for source in &edit.sources {
            let data_url = format!(
                "data:{};base64,{}",
                source.mime(),
                STANDARD.encode(source.data())
            );
            content.push(json!({"image": data_url}));
        }
    }
    content.push(json!({"text": task.prompt()}));
    let mut parameters = json!({"n": task.requested_count(), "response_format": "b64_json"});
    if let Some(ImageSizeRequest::Exact(size)) = task.size.value() {
        parameters["size"] = json!(format!("{}*{}", size.width, size.height));
    }
    Ok(json!({
        "model": model,
        "input": {"messages": [{"role": "user", "content": content}]},
        "parameters": parameters,
    }))
}

#[derive(Deserialize)]
struct Envelope {
    output: Output,
    usage: Option<Metering>,
    code: Option<String>,
    message: Option<String>,
}
#[derive(Deserialize)]
struct Output {
    choices: Vec<Choice>,
}
#[derive(Deserialize)]
struct Choice {
    finish_reason: String,
    message: Message,
}
#[derive(Deserialize)]
struct Message {
    role: String,
    content: Vec<Artifact>,
}
#[derive(Deserialize)]
struct Artifact {
    image: String,
}
#[derive(Deserialize)]
struct Metering {
    output_width: u32,
    output_height: u32,
    output_image_count: usize,
    input_image_count: u32,
    input_image_type: String,
    output_image_type: String,
}

pub fn decode_response(
    bytes: &[u8],
    expected_inputs: usize,
) -> Result<ImageGenerationResponse, CodecError> {
    // Accounted dimensions are reports, not inferred from the requested size.
    let envelope: Envelope = serde_json::from_value(crate::json::decode_image_response(bytes)?)
        .map_err(|_| CodecError::invalid())?;
    if envelope.code.is_some() || envelope.message.is_some() {
        return Err(CodecError::invalid());
    }
    let reported_size = envelope
        .usage
        .as_ref()
        .map_or(Ok(Presence::Absent), |usage| {
            if u32::try_from(expected_inputs).ok() != Some(usage.input_image_count)
                || usage.input_image_type.is_empty()
                || usage.output_image_type.is_empty()
            {
                return Err(CodecError::invalid());
            }
            let size = ImageSize {
                width: usage.output_width,
                height: usage.output_height,
            };
            size.validate().map_err(|_| CodecError::invalid())?;
            Ok(Presence::Value(size))
        })?;
    let mut images = Vec::new();
    for choice in envelope.output.choices {
        if choice.finish_reason != "stop"
            || choice.message.role != "assistant"
            || choice.message.content.is_empty()
        {
            return Err(CodecError::invalid());
        }
        for artifact in choice.message.content {
            if images.len() == 6 {
                return Err(CodecError::invalid());
            }
            let encoded = artifact
                .image
                .strip_prefix("data:image/png;base64,")
                .ok_or_else(CodecError::invalid)?;
            let mut image =
                GeneratedImage::new(ImageData::new(encoded).map_err(|_| CodecError::invalid())?);
            image.format = Presence::Value(ImageFormat::Png);
            image.size = reported_size.clone();
            images.push(image);
        }
    }
    if envelope
        .usage
        .is_some_and(|usage| usage.output_image_count != images.len())
    {
        return Err(CodecError::invalid());
    }
    let response = ImageGenerationResponse {
        created: Presence::Absent,
        model: Presence::Absent,
        safety_identifier: Presence::Absent,
        images,
        usage: Presence::Absent,
    };
    response.validate().map_err(|_| CodecError::invalid())?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_data_requires_successful_choices_and_consistent_metering() {
        let response = json!({
            "output":{"choices":[{"finish_reason":"stop","message":{"role":"assistant",
                "content":[{"image":"data:image/png;base64,AQID"},{"image":"data:image/png;base64,BAUG"}]}}]},
            "usage":{"output_width":512,"output_height":640,"output_image_count":2,
                "input_image_count":0,"input_image_type":"qima_input_1k","output_image_type":"qima_output_1k"}
        });
        let decode = |value: &Value| decode_response(&serde_json::to_vec(value).unwrap(), 0);
        let result = decode(&response).unwrap();
        assert_eq!(result.images[1].data.as_base64(), "BAUG");
        assert_eq!(
            result.images[0].size,
            Presence::Value(ImageSize {
                width: 512,
                height: 640
            })
        );
        assert!(result.created.is_absent() && result.usage.is_absent());

        for (at, value) in [
            ("/output/choices/0/finish_reason", json!("length")),
            (
                "/output/choices/0/message/content/0/image",
                json!("https://example.invalid/asset.png"),
            ),
            ("/usage/output_image_count", json!(1)),
            ("/usage/output_width", json!("512")),
        ] {
            let mut invalid = response.clone();
            *invalid.pointer_mut(at).unwrap() = value;
            assert!(decode(&invalid).is_err(), "{at}");
        }
    }
}
