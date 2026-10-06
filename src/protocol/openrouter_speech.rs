//! Named MP3 Speech mapping; the upstream PCM default must not change client intent.
//! Source: https://openrouter.ai/docs/api/api-reference/tts/create-speech.md
//! https://openrouter.ai/docs/guides/overview/multimodal/tts
use crate::{
    adapter::speech::Request,
    protocol::{CodecError, openai::speech},
    semantic::{task::speech_synthesis::SpeechRequest, value::AudioEncoding},
};
use serde_json::{Value, json};

pub(crate) fn validate_task(task: &SpeechRequest) -> Result<(), CodecError> {
    task.validate().map_err(|_| CodecError::Invalid("speech"))?;
    if task.requested_format() != AudioEncoding::Mp3
        || task.instructions.is_some()
        || task.speed.is_some()
    {
        return Err(CodecError::Unsupported("OpenRouter speech control".into()));
    }
    Ok(())
}
pub fn encode_request(request: &Request, model: &str) -> Result<Value, CodecError> {
    speech::validate_request(request)?;
    validate_task(&request.task)?;
    if !speech::valid_model(model) {
        return Err(CodecError::Invalid("speech model"));
    }
    // This operation always delivers audio bytes. The explicit standard audio
    // carrier needs no upstream field; it is not a low-latency or SSE request.
    Ok(json!({
        "model": model,
        "input": request.task.input(),
        "voice": request.task.voice(),
        "response_format": "mp3"
    }))
}
