//! Native HTTP SpeechSynthesizer MP3 mapping and bounded SSE-data reduction.
//! Source: https://help.aliyun.com/en/model-studio/qwen-audio-tts-http-api
use crate::{
    adapter::speech::Request,
    protocol::{CodecError, openai::speech},
    semantic::{
        task::speech_synthesis::{
            AudioReference, SpeechReport, SpeechResult, SpeechSentence, SpeechWord,
        },
        value::{AudioArtifact, MAX_AUDIO_BYTES},
    },
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Map, Value, json};

fn invalid() -> CodecError {
    CodecError::Invalid("native speech")
}
fn object<'a>(v: &'a Value, keys: &[&str]) -> Result<&'a Map<String, Value>, CodecError> {
    let m = v.as_object().ok_or_else(invalid)?;
    if m.keys().any(|k| !keys.contains(&k.as_str())) {
        return Err(invalid());
    }
    Ok(m)
}
fn text(v: &Value) -> Result<&str, CodecError> {
    v.as_str().ok_or_else(invalid)
}
fn number(v: &Value) -> Result<u64, CodecError> {
    v.as_u64().ok_or_else(invalid)
}
fn required<'a>(m: &'a Map<String, Value>, k: &str) -> Result<&'a Value, CodecError> {
    m.get(k).ok_or_else(invalid)
}
fn string(m: &Map<String, Value>, k: &str) -> Result<Option<String>, CodecError> {
    m.get(k).map(|v| text(v).map(str::to_owned)).transpose()
}
fn merge<T: Eq>(old: &mut Option<T>, next: Option<T>) -> Result<(), CodecError> {
    if let Some(next) = next {
        if old.as_ref().is_some_and(|old| *old != next) {
            return Err(invalid());
        }
        *old = Some(next);
    }
    Ok(())
}
fn merge_reference(old: &mut AudioReference, next: AudioReference) -> Result<(), CodecError> {
    merge(&mut old.id, next.id)?;
    merge(&mut old.url, next.url)?;
    merge(&mut old.expires_at, next.expires_at)
}
pub fn encode_request(request: &Request, model: &str) -> Result<Value, CodecError> {
    speech::validate_request(request)?;
    if request.task.requested_format() != crate::semantic::value::AudioEncoding::Mp3
        || request.task.instructions.is_some()
        || request.task.speed.is_some()
    {
        return Err(CodecError::Unsupported("native speech control".into()));
    }
    if !speech::valid_model(model) {
        return Err(invalid());
    }
    Ok(json!({"model":model,"input":{
        "text":request.task.input(),"voice":request.task.voice(),"format":"mp3"
    }}))
}

pub struct Decoder {
    bytes: Vec<u8>,
    report: SpeechReport,
    active: Option<(SpeechSentence, usize)>,
    complete: bool,
    poisoned: bool,
    byte_limit: usize,
    remaining_report: usize,
    remaining_events: usize,
}
impl Decoder {
    pub fn new(byte_limit: usize, report_budget: usize, max_events: usize) -> Self {
        Self {
            bytes: vec![],
            report: SpeechReport::default(),
            active: None,
            complete: false,
            poisoned: false,
            byte_limit: byte_limit.min(MAX_AUDIO_BYTES),
            remaining_report: report_budget.min(64 << 20),
            remaining_events: max_events.min(65536),
        }
    }
    /// Every failure poisons the reduction; a later stop cannot repair missing data.
    pub fn push(&mut self, value: &Value) -> Result<(), CodecError> {
        if self.poisoned {
            return Err(invalid());
        }
        let result = self.push_inner(value);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn push_inner(&mut self, value: &Value) -> Result<(), CodecError> {
        if self.complete || self.remaining_events == 0 {
            return Err(invalid());
        }
        self.remaining_events -= 1;
        let size = crate::semantic::value::json_size(value, self.remaining_report)
            .map_err(|_| CodecError::Limit)?;
        self.remaining_report -= size;
        let root = object(value, &["request_id", "output", "usage"])?;
        let id = text(required(root, "request_id")?)?;
        if id.is_empty() || id.len() > 256 {
            return Err(invalid());
        }
        merge(&mut self.report.request_id, Some(id.into()))?;
        if let Some(usage) = root.get("usage") {
            let usage = object(usage, &["characters"])?;
            if let Some(count) = usage.get("characters") {
                let count = number(count)?;
                if self.report.billed_characters.is_some_and(|n| count < n) {
                    return Err(invalid());
                }
                self.report.billed_characters = Some(count);
            }
        }
        let output = object(
            required(root, "output")?,
            &[
                "finish_reason",
                "type",
                "original_text",
                "sentence",
                "audio",
            ],
        )?;
        let reason = required(output, "finish_reason")?;
        let stop = reason.as_str() == Some("stop");
        if !stop && !reason.is_null() && reason.as_str() != Some("null") {
            return Err(invalid());
        }
        let audio = object(
            required(output, "audio")?,
            &["data", "id", "url", "expires_at"],
        )?;
        let data = text(required(audio, "data")?)?;
        let reference = AudioReference {
            id: string(audio, "id")?,
            url: string(audio, "url")?,
            expires_at: audio.get("expires_at").map(number).transpose()?,
        };
        reference.validate().map_err(|_| invalid())?;
        if stop {
            if self.active.is_some()
                || self.bytes.is_empty()
                || !data.is_empty()
                || ["type", "original_text", "sentence"]
                    .iter()
                    .any(|k| output.contains_key(*k))
            {
                return Err(invalid());
            }
            self.report.reference = reference;
            self.report.billed_characters_final = root
                .get("usage")
                .and_then(|v| v.get("characters"))
                .is_some();
            self.complete = true;
            return Ok(());
        }
        let kind = text(required(output, "type")?)?;
        if kind == "sentence-begin" {
            if self.active.is_some() || self.report.sentences.len() >= 4096 || !data.is_empty() {
                return Err(invalid());
            }
            let sentence = object(required(output, "sentence")?, &["index", "words"])?;
            let index = number(required(sentence, "index")?)?;
            if index != self.report.sentences.len() as u64 {
                return Err(invalid());
            }
            self.active = Some((
                SpeechSentence {
                    index,
                    text: None,
                    words: vec![],
                    reference: AudioReference::default(),
                },
                self.bytes.len(),
            ));
        } else if !matches!(kind, "sentence-synthesis" | "sentence-end") {
            return Err(invalid());
        }
        let (active, start) = self.active.as_mut().ok_or_else(invalid)?;
        merge(&mut active.text, string(output, "original_text")?)?;
        merge_reference(&mut active.reference, reference)?;
        if let Some(sentence) = output.get("sentence") {
            let sentence = object(sentence, &["index", "words"])?;
            if number(required(sentence, "index")?)? != active.index {
                return Err(invalid());
            }
            if let Some(words) = sentence.get("words") {
                let words = words.as_array().ok_or_else(invalid)?;
                if words.len() > 65536 {
                    return Err(CodecError::Limit);
                }
                let parsed = words
                    .iter()
                    .map(|w| {
                        let w = object(
                            w,
                            &["text", "begin_index", "end_index", "begin_time", "end_time"],
                        )?;
                        Ok(SpeechWord {
                            text: text(required(w, "text")?)?.into(),
                            start_character: number(required(w, "begin_index")?)?,
                            end_character: number(required(w, "end_index")?)?,
                            start_ms: number(required(w, "begin_time")?)?,
                            end_ms: number(required(w, "end_time")?)?,
                        })
                    })
                    .collect::<Result<Vec<_>, CodecError>>()?;
                if !parsed.is_empty() {
                    if !parsed.starts_with(&active.words) {
                        return Err(invalid());
                    }
                    active.words = parsed;
                }
            }
        }
        if kind == "sentence-synthesis" {
            if data.is_empty()
                || data.len() > self.byte_limit.saturating_sub(self.bytes.len()).div_ceil(3) * 4
            {
                return Err(CodecError::Limit);
            }
            let chunk = STANDARD.decode(data).map_err(|_| invalid())?;
            if self.bytes.len().saturating_add(chunk.len()) > self.byte_limit {
                return Err(CodecError::Limit);
            }
            self.bytes.extend_from_slice(&chunk);
        } else if !data.is_empty() {
            return Err(invalid());
        }
        if kind == "sentence-end" {
            if self.bytes.len() == *start {
                return Err(invalid());
            }
            let (sentence, _) = self.active.take().ok_or_else(invalid)?;
            self.report.sentences.push(sentence);
            self.report.validate().map_err(|_| invalid())?;
        }
        Ok(())
    }
    /// The caller must independently establish strict SSE framing and transport EOF.
    pub fn finish(self) -> Result<SpeechResult, CodecError> {
        if self.poisoned || !self.complete || self.active.is_some() {
            return Err(invalid());
        }
        let audio = AudioArtifact::new(self.bytes.into(), None).map_err(|_| invalid())?;
        SpeechResult::new(audio, self.report).map_err(|_| invalid())
    }
}
