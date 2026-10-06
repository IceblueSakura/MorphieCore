//! Standard Speech audio branch. No custom voice resources, SSE, sniffing or transcoding.
//! Sources: https://developers.openai.com/api/reference/resources/audio/subresources/speech/methods/create
//! https://developers.openai.com/api/docs/guides/text-to-speech
use crate::{
    adapter::speech::Request,
    protocol::CodecError,
    semantic::{
        task::speech_synthesis::{SpeechRequest, Speed},
        value::{AudioArtifact, AudioEncoding},
    },
};
use bytes::Bytes;
use serde_json::{Value, json};

fn invalid() -> CodecError {
    CodecError::Invalid("speech")
}
pub(crate) fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 256
        && !model.chars().any(|c| c.is_control() || c.is_whitespace())
}
fn format(label: &str) -> Result<AudioEncoding, CodecError> {
    Ok(match label {
        "mp3" => AudioEncoding::Mp3,
        "opus" => AudioEncoding::Opus,
        "aac" => AudioEncoding::Aac,
        "flac" => AudioEncoding::Flac,
        "wav" => AudioEncoding::Wav,
        "pcm" => AudioEncoding::PcmS16Le24KhzMono,
        _ => return Err(invalid()),
    })
}
fn format_label(format: AudioEncoding) -> &'static str {
    match format {
        AudioEncoding::Mp3 => "mp3",
        AudioEncoding::Opus => "opus",
        AudioEncoding::Aac => "aac",
        AudioEncoding::Flac => "flac",
        AudioEncoding::Wav => "wav",
        AudioEncoding::PcmS16Le24KhzMono => "pcm",
    }
}
pub fn decode_request(bytes: &[u8]) -> Result<Request, CodecError> {
    let value = super::json::decode(bytes)?;
    let map = value.as_object().ok_or_else(invalid)?;
    if map.keys().any(|k| {
        ![
            "model",
            "input",
            "voice",
            "instructions",
            "speed",
            "response_format",
            "stream_format",
        ]
        .contains(&k.as_str())
    }) {
        return Err(invalid());
    }
    let required = |key| map.get(key).and_then(Value::as_str).ok_or_else(invalid);
    let mut task =
        SpeechRequest::new(required("input")?, required("voice")?).map_err(|_| invalid())?;
    task.instructions = map
        .get("instructions")
        .map(|v| v.as_str().map(String::from).ok_or_else(invalid))
        .transpose()?;
    task.speed = map
        .get("speed")
        .map(|v| Speed::new(v.as_number().ok_or_else(invalid)?.clone()).map_err(|_| invalid()))
        .transpose()?;
    task.format = map
        .get("response_format")
        .map(|v| format(v.as_str().ok_or_else(invalid)?))
        .transpose()?;
    let mut request = Request::new(required("model")?, task);
    if let Some(delivery) = map.get("stream_format") {
        if delivery.as_str() != Some("audio") {
            return Err(invalid());
        }
        request.explicit_audio_delivery = true;
    }
    validate_request(&request)?;
    Ok(request)
}
pub fn validate_request(request: &Request) -> Result<(), CodecError> {
    request.task.validate().map_err(|_| invalid())?;
    if !valid_model(&request.model) {
        return Err(invalid());
    }
    Ok(())
}
pub fn encode_request(request: &Request, model: &str) -> Result<Value, CodecError> {
    validate_request(request)?;
    if !valid_model(model) {
        return Err(invalid());
    }
    let mut value =
        json!({"model":model, "input":request.task.input(), "voice":request.task.voice()});
    if let Some(instructions) = &request.task.instructions {
        value["instructions"] = json!(instructions);
    }
    if let Some(speed) = &request.task.speed {
        value["speed"] = Value::Number(speed.number().clone());
    }
    if let Some(format) = request.task.format {
        value["response_format"] = json!(format_label(format));
    }
    if request.explicit_audio_delivery {
        value["stream_format"] = json!("audio");
    }
    Ok(value)
}
/// Only a format-specific report establishes encoding. Generic bytes remain unreported.
/// PCM layout follows this selected standard profile, not a universal MIME assumption.
pub fn reported_encoding(
    content_type: &str,
    task: &SpeechRequest,
) -> Result<Option<AudioEncoding>, CodecError> {
    task.validate().map_err(|_| invalid())?;
    let mime: mime::Mime = content_type.parse().map_err(|_| invalid())?;
    if mime.params().next().is_some() {
        return Err(invalid());
    }
    let encoding = match mime.essence_str() {
        "application/octet-stream" => None,
        "audio/mpeg" => Some(AudioEncoding::Mp3),
        "audio/opus" => Some(AudioEncoding::Opus),
        "audio/aac" => Some(AudioEncoding::Aac),
        "audio/flac" => Some(AudioEncoding::Flac),
        "audio/wav" | "audio/x-wav" => Some(AudioEncoding::Wav),
        "audio/pcm" => Some(AudioEncoding::PcmS16Le24KhzMono),
        _ => return Err(invalid()),
    };
    if encoding.is_some_and(|e| e != task.requested_format()) {
        return Err(invalid());
    }
    Ok(encoding)
}
pub fn decode_response(
    bytes: Bytes,
    content_type: &str,
    task: &SpeechRequest,
) -> Result<AudioArtifact, CodecError> {
    AudioArtifact::new(bytes, reported_encoding(content_type, task)?).map_err(|_| invalid())
}
pub fn response_content_type(artifact: &AudioArtifact) -> &'static str {
    match artifact.encoding() {
        None => "application/octet-stream",
        Some(AudioEncoding::Mp3) => "audio/mpeg",
        Some(AudioEncoding::Opus) => "audio/opus",
        Some(AudioEncoding::Aac) => "audio/aac",
        Some(AudioEncoding::Flac) => "audio/flac",
        Some(AudioEncoding::Wav) => "audio/wav",
        Some(AudioEncoding::PcmS16Le24KhzMono) => "audio/pcm",
    }
}

/// Named standard-binary projection: omit ancillary synthesis reports, not audio.
/// The source result remains available to typed consumers and is never mutated.
pub fn project_result(
    result: &crate::semantic::task::speech_synthesis::SpeechResult,
) -> Result<crate::lowering::speech::BinaryProjection, CodecError> {
    crate::lowering::speech::project_binary(result).map_err(|_| invalid())
}
