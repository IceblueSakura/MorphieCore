//! Standard Speech binary and SSE delivery branch. No custom voice resources, sniffing or transcoding.
//! Sources: https://developers.openai.com/api/reference/resources/audio/subresources/speech/methods/create
//! https://developers.openai.com/api/docs/guides/text-to-speech
use crate::{
    CodecError,
    semantic::{
        task::speech_synthesis::{SpeechEvent, SpeechRequest, SpeechTokenUsage, Speed},
        value::{AudioArtifact, AudioEncoding, MAX_AUDIO_BYTES},
    },
    speech::{Request, StreamFormat},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use bytes::Bytes;
use serde_json::{Value, json};

pub const VOICES: &[&str] = &[
    "alloy", "ash", "ballad", "coral", "echo", "fable", "onyx", "nova", "sage", "shimmer", "verse",
    "marin", "cedar",
];
pub const FORMATS: &[AudioEncoding] = &[
    AudioEncoding::Mp3,
    AudioEncoding::Opus,
    AudioEncoding::Aac,
    AudioEncoding::Flac,
    AudioEncoding::Wav,
    AudioEncoding::PcmS16Le24KhzMono,
];

pub fn valid_model(model: &str) -> bool {
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
        _ => return Err(CodecError::invalid()),
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
    let value = crate::json::decode(bytes)?;
    let map = value.as_object().ok_or_else(CodecError::invalid)?;
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
        return Err(CodecError::invalid());
    }
    let required = |key| {
        map.get(key)
            .and_then(Value::as_str)
            .ok_or_else(CodecError::invalid)
    };
    let mut task = SpeechRequest::new(required("input")?, required("voice")?)
        .map_err(|_| CodecError::invalid())?;
    task.instructions = map
        .get("instructions")
        .map(|v| v.as_str().map(String::from).ok_or_else(CodecError::invalid))
        .transpose()?;
    task.speed = map
        .get("speed")
        .map(|v| {
            Speed::new(v.as_number().ok_or_else(CodecError::invalid)?.clone())
                .map_err(|_| CodecError::invalid())
        })
        .transpose()?;
    task.format = map
        .get("response_format")
        .map(|v| format(v.as_str().ok_or_else(CodecError::invalid)?))
        .transpose()?;
    let mut request = Request::new(required("model")?, task);
    if let Some(delivery) = map.get("stream_format") {
        request.stream_format = Some(match delivery.as_str() {
            Some("audio") => StreamFormat::Audio,
            Some("sse") => StreamFormat::Sse,
            _ => return Err(CodecError::invalid()),
        });
    }
    validate_request(&request)?;
    Ok(request)
}
pub fn validate_request(request: &Request) -> Result<(), CodecError> {
    request.task.validate().map_err(|_| CodecError::invalid())?;
    if !valid_model(&request.model) {
        return Err(CodecError::invalid());
    }
    Ok(())
}
pub fn encode_request(request: &Request, model: &str) -> Result<Value, CodecError> {
    validate_request(request)?;
    if !valid_model(model) {
        return Err(CodecError::invalid());
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
    if let Some(stream_format) = request.stream_format {
        value["stream_format"] = json!(match stream_format {
            StreamFormat::Audio => "audio",
            StreamFormat::Sse => "sse",
        });
    }
    Ok(value)
}
/// Only a format-specific report establishes encoding. Generic bytes remain unreported.
/// PCM layout follows this selected standard profile, not a universal MIME assumption.
pub fn reported_encoding(
    content_type: &str,
    task: &SpeechRequest,
) -> Result<Option<AudioEncoding>, CodecError> {
    task.validate().map_err(|_| CodecError::invalid())?;
    let mime: mime::Mime = content_type.parse().map_err(|_| CodecError::invalid())?;
    if mime.params().next().is_some() {
        return Err(CodecError::invalid());
    }
    let encoding = match mime.essence_str() {
        "application/octet-stream" => None,
        "audio/mpeg" => Some(AudioEncoding::Mp3),
        "audio/opus" => Some(AudioEncoding::Opus),
        "audio/aac" => Some(AudioEncoding::Aac),
        "audio/flac" => Some(AudioEncoding::Flac),
        "audio/wav" | "audio/x-wav" => Some(AudioEncoding::Wav),
        "audio/pcm" => Some(AudioEncoding::PcmS16Le24KhzMono),
        _ => return Err(CodecError::invalid()),
    };
    if encoding.is_some_and(|e| e != task.requested_format()) {
        return Err(CodecError::invalid());
    }
    Ok(encoding)
}
pub fn decode_response(
    bytes: Bytes,
    content_type: &str,
    task: &SpeechRequest,
) -> Result<AudioArtifact, CodecError> {
    AudioArtifact::new(bytes, reported_encoding(content_type, task)?)
        .map_err(|_| CodecError::invalid())
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
) -> Result<crate::speech::projection::BinaryProjection, CodecError> {
    crate::speech::projection::project_binary(result).map_err(|_| CodecError::invalid())
}

pub struct StreamDecoder {
    audio_limit: usize,
    decoded_bytes: usize,
    remaining_events: usize,
    saw_delta: bool,
    done: bool,
    poisoned: bool,
}
impl StreamDecoder {
    pub fn new(audio_limit: usize, max_events: usize) -> Self {
        Self {
            audio_limit: audio_limit.min(MAX_AUDIO_BYTES),
            decoded_bytes: 0,
            remaining_events: max_events.min(65536),
            saw_delta: false,
            done: false,
            poisoned: false,
        }
    }
    pub fn push(&mut self, value: &Value) -> Result<SpeechEvent, CodecError> {
        if self.poisoned {
            return Err(CodecError::invalid());
        }
        let result = self.push_inner(value);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn push_inner(&mut self, value: &Value) -> Result<SpeechEvent, CodecError> {
        if self.done || self.remaining_events == 0 {
            return Err(CodecError::invalid());
        }
        self.remaining_events -= 1;
        let map = value.as_object().ok_or_else(CodecError::invalid)?;
        let kind = map
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(CodecError::invalid)?;
        match kind {
            "speech.audio.delta" => {
                let encoded = map
                    .get("audio")
                    .and_then(Value::as_str)
                    .ok_or_else(CodecError::invalid)?;
                if encoded.is_empty() {
                    return Err(CodecError::invalid());
                }
                let remaining = self.audio_limit.saturating_sub(self.decoded_bytes);
                if encoded.len() > remaining.div_ceil(3) * 4 {
                    return Err(CodecError::Limit);
                }
                let chunk = STANDARD
                    .decode(encoded)
                    .map_err(|_| CodecError::invalid())?;
                if chunk.is_empty() {
                    return Err(CodecError::invalid());
                }
                self.decoded_bytes = self
                    .decoded_bytes
                    .checked_add(chunk.len())
                    .filter(|n| *n <= self.audio_limit)
                    .ok_or(CodecError::Limit)?;
                self.saw_delta = true;
                Ok(SpeechEvent::AudioDelta {
                    audio: Bytes::from(chunk),
                })
            }
            "speech.audio.done" => {
                if !self.saw_delta {
                    return Err(CodecError::invalid());
                }
                let usage = match map.get("usage") {
                    None | Some(Value::Null) => None,
                    Some(raw) => {
                        let obj = raw.as_object().ok_or_else(CodecError::invalid)?;
                        let field = |k| {
                            obj.get(k)
                                .and_then(Value::as_u64)
                                .ok_or_else(CodecError::invalid)
                        };
                        Some(SpeechTokenUsage {
                            input_tokens: field("input_tokens")?,
                            output_tokens: field("output_tokens")?,
                            total_tokens: field("total_tokens")?,
                        })
                    }
                };
                self.done = true;
                Ok(SpeechEvent::Done { usage })
            }
            _ => Err(CodecError::invalid()),
        }
    }
    pub fn finish(&self) -> Result<(), CodecError> {
        if self.poisoned || !self.done || !self.saw_delta {
            return Err(CodecError::invalid());
        }
        Ok(())
    }
}

pub fn encode_sse_event(event: &SpeechEvent) -> Result<Bytes, CodecError> {
    let (name, payload) = match event {
        SpeechEvent::AudioDelta { audio } => {
            if audio.is_empty() || audio.len() > MAX_AUDIO_BYTES {
                return Err(CodecError::invalid());
            }
            (
                "speech.audio.delta",
                json!({
                    "type": "speech.audio.delta",
                    "audio": STANDARD.encode(audio),
                }),
            )
        }
        SpeechEvent::Done { usage } => {
            let mut obj = json!({"type": "speech.audio.done"});
            if let Some(usage) = usage {
                obj["usage"] = json!({
                    "input_tokens": usage.input_tokens,
                    "output_tokens": usage.output_tokens,
                    "total_tokens": usage.total_tokens,
                });
            }
            ("speech.audio.done", obj)
        }
    };
    let encoded = serde_json::to_string(&payload).map_err(|_| CodecError::invalid())?;
    Ok(Bytes::from(format!("event: {name}\ndata: {encoded}\n\n")))
}
