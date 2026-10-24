//! Independent MiMo Speech and Transcription codecs over the Chat JSON envelope.
//! Sources: https://mimo.mi.com/docs/en-US/api/audio/tts
//! https://mimo.mi.com/docs/en-US/api/audio/Speech-Recognition

use crate::{
    protocol::CodecError,
    semantic::{
        task::{
            speech_recognition::{InputFormat, RecognitionReport, RecognitionResult},
            speech_synthesis::{AudioReference, SpeechReport, SpeechRequest, SpeechResult},
        },
        value::{AudioArtifact, AudioEncoding},
    },
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};

pub const VOICES: &[&str] = &[
    "mimo_default",
    "冰糖",
    "茉莉",
    "苏打",
    "白桦",
    "Mia",
    "Chloe",
    "Milo",
    "Dean",
];
pub const FORMATS: &[AudioEncoding] = &[
    AudioEncoding::Mp3,
    AudioEncoding::Wav,
    AudioEncoding::PcmS16Le24KhzMono,
];

fn invalid() -> CodecError {
    CodecError::Invalid(crate::protocol::failure::ShapeReason::InvalidValue)
}

pub fn encode_speech(
    request: &crate::adapter::speech::Request,
    model: &str,
) -> Result<Value, CodecError> {
    super::openai::speech::validate_request(request)?;
    if !super::openai::speech::valid_model(model) {
        return Err(invalid());
    }
    let task = &request.task;
    if task.speed.is_some() || !VOICES.contains(&task.voice()) {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
    }
    let format = match task.requested_format() {
        AudioEncoding::Mp3 => "mp3",
        AudioEncoding::Wav => "wav",
        AudioEncoding::PcmS16Le24KhzMono => "pcm16",
        _ => {
            return Err(CodecError::Unsupported(
                crate::protocol::failure::CarrierReason::Unavailable,
            ));
        }
    };
    let mut messages = Vec::new();
    if let Some(instructions) = &task.instructions {
        messages.push(json!({"role":"user","content":instructions}));
    }
    messages.push(json!({"role":"assistant","content":task.input()}));
    Ok(json!({
        "model": model,
        "messages": messages,
        "audio": {"format": format, "voice": task.voice()},
        "stream": false,
    }))
}

pub fn encode_transcription(
    request: &crate::adapter::transcription::Request,
    model: &str,
) -> Result<Value, CodecError> {
    request.task.validate().map_err(|_| invalid())?;
    if !super::openai::speech::valid_model(&request.model)
        || !super::openai::speech::valid_model(model)
    {
        return Err(invalid());
    }
    if request
        .task
        .language
        .as_deref()
        .is_some_and(|v| !["zh", "en"].contains(&v))
    {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
    }
    let (format, mime) = match request.task.audio.format() {
        InputFormat::Wav => ("wav", "audio/wav"),
        InputFormat::Mp3 => ("mp3", "audio/mpeg"),
    };
    let data = format!(
        "data:{mime};base64,{}",
        STANDARD.encode(request.task.audio.data())
    );
    let mut value = json!({"model":model,"stream":false,"messages":[{"role":"user","content":[
        {"type":"input_audio","input_audio":{"data":data,"format":format}}
    ]}]});
    if let Some(language) = &request.task.language {
        value["asr_options"] = json!({"language":language});
    }
    Ok(value)
}

#[derive(Deserialize)]
struct Completion {
    id: Option<String>,
    object: Option<String>,
    #[serde(rename = "created")]
    _created: Option<u64>,
    #[serde(rename = "model")]
    _model: Option<String>,
    choices: Vec<Choice>,
    usage: Option<Usage>,
}
#[derive(Deserialize)]
struct Choice {
    index: u64,
    finish_reason: String,
    message: Message,
}
#[derive(Deserialize)]
struct Message {
    role: String,
    content: Option<String>,
    audio: Option<Audio>,
    tool_calls: Option<Vec<Value>>,
    refusal: Option<String>,
    #[serde(rename = "audio_tokens")]
    _audio_tokens: Option<Vec<u64>>,
}
#[derive(Deserialize)]
struct Audio {
    data: String,
    id: Option<String>,
    expires_at: Option<u64>,
    transcript: Option<String>,
}
#[derive(Deserialize)]
struct Usage {
    seconds: Option<u64>,
    #[serde(rename = "prompt_tokens")]
    _prompt_tokens: Option<u64>,
    #[serde(rename = "completion_tokens")]
    _completion_tokens: Option<u64>,
    #[serde(rename = "total_tokens")]
    _total_tokens: Option<u64>,
    #[serde(rename = "prompt_tokens_details")]
    _prompt_details: Option<TokenDetails>,
    #[serde(rename = "completion_tokens_details")]
    _completion_details: Option<TokenDetails>,
}
#[derive(Deserialize)]
struct TokenDetails {
    #[serde(rename = "reasoning_tokens")]
    _reasoning_tokens: Option<u64>,
    #[serde(rename = "audio_tokens")]
    _audio_tokens: Option<u64>,
    #[serde(rename = "cached_tokens")]
    _cached_tokens: Option<u64>,
}
fn decode(bytes: &[u8]) -> Result<(Completion, Message), CodecError> {
    let value = super::json::decode(bytes)?;
    let mut completion: Completion = serde_json::from_value(value).map_err(|_| invalid())?;
    if completion
        .object
        .as_deref()
        .is_some_and(|s| s != "chat.completion")
        || completion.choices.len() != 1
    {
        return Err(invalid());
    }
    let choice = completion.choices.pop().ok_or_else(invalid)?;
    if choice.index != 0
        || choice.finish_reason != "stop"
        || choice.message.role != "assistant"
        || choice.message.refusal.is_some()
        || choice
            .message
            .tool_calls
            .as_ref()
            .is_some_and(|v| !v.is_empty())
    {
        return Err(invalid());
    }
    Ok((completion, choice.message))
}

pub fn decode_speech(bytes: &[u8], task: &SpeechRequest) -> Result<SpeechResult, CodecError> {
    let (completion, message) = decode(bytes)?;
    if message.content.as_ref().is_some_and(|s| !s.is_empty()) {
        return Err(invalid());
    }
    let audio = message.audio.ok_or_else(invalid)?;
    if audio.transcript.is_some() {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Unavailable,
        ));
    }
    let data = STANDARD.decode(audio.data).map_err(|_| invalid())?;
    let artifact =
        AudioArtifact::new(data.into(), Some(task.requested_format())).map_err(|_| invalid())?;
    SpeechResult::new(
        artifact,
        SpeechReport {
            request_id: completion.id,
            reference: AudioReference {
                id: audio.id,
                expires_at: audio.expires_at,
                url: None,
            },
            ..SpeechReport::default()
        },
    )
    .map_err(|_| invalid())
}

pub fn decode_transcription(bytes: &[u8]) -> Result<RecognitionResult, CodecError> {
    let (completion, message) = decode(bytes)?;
    if message.audio.is_some() {
        return Err(invalid());
    }
    RecognitionResult::new(
        message.content.ok_or_else(invalid)?,
        RecognitionReport {
            request_id: completion.id,
            processed_seconds: completion.usage.and_then(|u| u.seconds),
            last_sentence: None,
        },
    )
    .map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn speech_wire_oracle_and_honest_terminal() {
        let mut task = SpeechRequest::new("Hello.", "Chloe").unwrap();
        task.instructions = Some("Speak gently.".into());
        let request = crate::adapter::speech::Request::new("mimo-v2.5-tts", task.clone());
        assert_eq!(
            encode_speech(&request, "mimo-v2.5-tts").unwrap(),
            json!({"model":"mimo-v2.5-tts","messages":[
                {"role":"user","content":"Speak gently."},
                {"role":"assistant","content":"Hello."}],
                "audio":{"format":"mp3","voice":"Chloe"},"stream":false})
        );
        let wire = br#"{"id":"speech-fixture","object":"chat.completion","created":1,"model":"mimo-v2.5-tts","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":null,"audio":{"id":"audio-fixture","data":"YWJj","expires_at":null,"transcript":null}}}],"usage":null,"future":true}"#;
        let result = decode_speech(wire, &task).unwrap();
        assert_eq!(result.audio().data().as_ref(), b"abc");
        assert_eq!(result.audio().encoding(), Some(AudioEncoding::Mp3));
        assert_eq!(
            result.report().request_id.as_deref(),
            Some("speech-fixture")
        );
        assert_eq!(
            result.report().reference.id.as_deref(),
            Some("audio-fixture")
        );
        assert!(result.report().billed_characters.is_none());
        let incomplete = String::from_utf8(wire.to_vec())
            .unwrap()
            .replace("\"stop\"", "\"length\"");
        assert!(decode_speech(incomplete.as_bytes(), &task).is_err());
        let mut unsupported = request;
        unsupported.task.speed = Some(
            crate::semantic::task::speech_synthesis::Speed::new(serde_json::Number::from(2))
                .unwrap(),
        );
        assert!(encode_speech(&unsupported, "mimo-v2.5-tts").is_err());
    }

    #[test]
    fn transcription_wire_oracle_and_unsupported_language() {
        use crate::semantic::task::speech_recognition::{AudioInput, RecognitionRequest};
        let mut task = RecognitionRequest::new(
            AudioInput::new(bytes::Bytes::from_static(b"abc"), InputFormat::Wav).unwrap(),
        );
        task.language = Some("en".into());
        let mut request = crate::adapter::transcription::Request {
            model: "mimo-v2.5-asr".into(),
            task,
        };
        assert_eq!(
            encode_transcription(&request, "mimo-v2.5-asr").unwrap(),
            json!({"model":"mimo-v2.5-asr","messages":[{"role":"user","content":[
                {"type":"input_audio","input_audio":{"data":"data:audio/wav;base64,YWJj","format":"wav"}}]}],
                "asr_options":{"language":"en"},"stream":false})
        );
        let wire = br#"{"id":"asr-fixture","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"Hello.","audio":null,"tool_calls":null}}],"usage":{"seconds":4}}"#;
        let result = decode_transcription(wire).unwrap();
        assert_eq!(result.text(), "Hello.");
        assert_eq!(result.report().processed_seconds, Some(4));
        assert!(result.report().last_sentence.is_none());
        request.task.language = Some("ja".into());
        assert!(encode_transcription(&request, "mimo-v2.5-asr").is_err());
        let malformed = String::from_utf8(wire.to_vec())
            .unwrap()
            .replace("\"seconds\":4", "\"seconds\":\"4\"");
        assert!(decode_transcription(malformed.as_bytes()).is_err());
    }
}
