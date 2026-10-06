//! Qwen ASR Flash native synchronous HTTP mapping, not Chat-compatible ASR.
//! Source: https://help.aliyun.com/en/model-studio/fun-asr-flash-recorded-speech-recognition-http-api
use crate::{
    adapter::transcription::Request,
    protocol::CodecError,
    semantic::task::speech_recognition::{
        InputFormat, RecognitionReport, RecognitionResult, RecognizedSentence, RecognizedWord,
    },
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Map, Value, json};
fn invalid() -> CodecError {
    CodecError::Invalid("native transcription")
}
fn object<'a>(v: &'a Value, keys: &[&str]) -> Result<&'a Map<String, Value>, CodecError> {
    let m = v.as_object().ok_or_else(invalid)?;
    if m.keys().any(|k| !keys.contains(&k.as_str())) {
        return Err(invalid());
    }
    Ok(m)
}
fn required<'a>(m: &'a Map<String, Value>, key: &str) -> Result<&'a Value, CodecError> {
    m.get(key).ok_or_else(invalid)
}
fn number(m: &Map<String, Value>, key: &str) -> Result<u64, CodecError> {
    required(m, key)?.as_u64().ok_or_else(invalid)
}
fn text(m: &Map<String, Value>, key: &str) -> Result<String, CodecError> {
    required(m, key)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(invalid)
}
pub fn encode_request(request: &Request, model: &str) -> Result<Value, CodecError> {
    request.task.validate().map_err(|_| invalid())?;
    if !super::openai::speech::valid_model(&request.model)
        || !super::openai::speech::valid_model(model)
    {
        return Err(invalid());
    }
    if request.task.language.as_ref().is_some_and(|s| {
        ![
            "zh", "en", "ja", "ko", "vi", "th", "id", "ms", "tl", "hi", "ar", "fr", "de", "es",
            "pt", "ru", "it", "nl", "sv", "da", "fi", "no", "el", "pl", "cs", "hu", "ro", "bg",
            "hr", "sk",
        ]
        .contains(&s.as_str())
    }) {
        return Err(CodecError::Unsupported("transcription language".into()));
    }
    let (format, mime) = match request.task.audio.format() {
        InputFormat::Wav => ("wav", "audio/wav"),
        InputFormat::Mp3 => ("mp3", "audio/mpeg"),
    };
    let data = format!(
        "data:{mime};base64,{}",
        STANDARD.encode(request.task.audio.data())
    );
    let mut result = json!({"model":model,"input":{"messages":[{"role":"user","content":[
        {"type":"input_audio","input_audio":{"data":data}}
    ]}]},"parameters":{"format":format}});
    if let Some(language) = &request.task.language {
        result["parameters"]["language_hints"] = json!([language]);
    }
    Ok(result)
}
pub fn decode_response(bytes: &[u8]) -> Result<RecognitionResult, CodecError> {
    let value = super::openai::json::decode(bytes)?;
    let root = object(&value, &["request_id", "output", "usage"])?;
    let output = object(required(root, "output")?, &["text", "sentence"])?;
    let body = text(output, "text")?;
    let last_sentence = output
        .get("sentence")
        .map(|s| {
            let s = object(
                s,
                &[
                    "sentence_id",
                    "sentence_end",
                    "text",
                    "begin_time",
                    "end_time",
                    "channel_id",
                    "words",
                ],
            )?;
            if required(s, "sentence_end")?.as_bool() != Some(true) {
                return Err(invalid());
            }
            let words = required(s, "words")?.as_array().ok_or_else(invalid)?;
            if words.len() > 65536 {
                return Err(CodecError::Limit);
            }
            let words = words
                .iter()
                .map(|w| {
                    let w = object(
                        w,
                        &["text", "punctuation", "begin_time", "end_time", "fixed"],
                    )?;
                    if required(w, "fixed")?.as_bool() != Some(true) {
                        return Err(invalid());
                    }
                    Ok(RecognizedWord {
                        text: text(w, "text")?,
                        punctuation: text(w, "punctuation")?,
                        start_ms: number(w, "begin_time")?,
                        end_ms: number(w, "end_time")?,
                    })
                })
                .collect::<Result<Vec<_>, CodecError>>()?;
            Ok(RecognizedSentence {
                id: number(s, "sentence_id")?,
                channel: number(s, "channel_id")?,
                text: text(s, "text")?,
                start_ms: number(s, "begin_time")?,
                end_ms: number(s, "end_time")?,
                words,
            })
        })
        .transpose()?;
    let processed_seconds = root
        .get("usage")
        .map(|u| {
            let u = object(u, &["duration"])?;
            number(u, "duration")
        })
        .transpose()?;
    if processed_seconds.is_some_and(|s| s > 300)
        || last_sentence.as_ref().is_some_and(|s| {
            s.end_ms > 300_000 || processed_seconds.is_some_and(|n| s.end_ms > n * 1000)
        })
    {
        return Err(invalid());
    }
    let request_id = root
        .get("request_id")
        .map(|v| v.as_str().map(str::to_owned).ok_or_else(invalid))
        .transpose()?;
    RecognitionResult::new(
        body,
        RecognitionReport {
            request_id,
            processed_seconds,
            last_sentence,
        },
    )
    .map_err(|_| invalid())
}
