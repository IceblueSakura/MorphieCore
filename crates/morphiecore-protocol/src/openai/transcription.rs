//! Standard synchronous transcription multipart/JSON/text codec and projection.
//! Source: https://developers.openai.com/api/reference/resources/audio/subresources/transcriptions/methods/create
use crate::{
    CodecError,
    multipart::{EncodedMultipart, Part},
    semantic::task::speech_recognition::{
        MAX_TEXT_BYTES, OutputFormat, RecognitionEvent, RecognitionReport, RecognitionResult,
        RecognitionUsage,
    },
    transcription::Request,
};
use bytes::Bytes;
use serde_json::{Map, Value, json};

mod multipart;
pub use multipart::{MAX_UPLOAD_BYTES, decode_request};

pub enum ProjectedOutput {
    Json(Value),
    Text(Vec<u8>),
}

pub struct Projection {
    pub output: ProjectedOutput,
    pub omitted_alignment: bool,
    pub omitted_request_id: bool,
    pub omitted_ancillary: bool,
}

fn required<'a>(map: &'a Map<String, Value>, key: &str) -> Result<&'a Value, CodecError> {
    map.get(key).ok_or_else(CodecError::invalid)
}

fn u64_field(map: &Map<String, Value>, key: &str) -> Result<u64, CodecError> {
    required(map, key)?.as_u64().ok_or_else(CodecError::invalid)
}

fn optional_u64(map: &Map<String, Value>, key: &str) -> Result<Option<u64>, CodecError> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value.as_u64().map(Some).ok_or_else(CodecError::invalid),
    }
}

pub fn encode_request(
    request: &Request,
    model: &str,
    limit: usize,
) -> Result<EncodedMultipart, CodecError> {
    request.task.validate().map_err(|_| CodecError::invalid())?;
    if !super::speech::valid_model(&request.model)
        || !super::speech::valid_model(model)
        || (request.stream && request.task.response_format != OutputFormat::Json)
    {
        return Err(CodecError::invalid());
    }
    let format = request.task.audio.format();
    let filename = format!("audio.{}", format.extension());
    let temp = request
        .task
        .temperature
        .as_ref()
        .map(|t| t.number().to_string());
    let response_format = match request.task.response_format {
        OutputFormat::Json => "json",
        OutputFormat::Text => "text",
    };

    let mut parts =
        Vec::with_capacity(5 + request.task.languages.len() + request.task.keywords.len());
    parts.push(Part::Text {
        name: "model",
        value: model,
    });
    if let Some(language) = &request.task.language {
        parts.push(Part::Text {
            name: "language",
            value: language,
        });
    }
    for language in &request.task.languages {
        parts.push(Part::Text {
            name: "languages[]",
            value: language,
        });
    }
    for keyword in &request.task.keywords {
        parts.push(Part::Text {
            name: "keywords[]",
            value: keyword,
        });
    }
    if let Some(prompt) = &request.task.prompt {
        parts.push(Part::Text {
            name: "prompt",
            value: prompt,
        });
    }
    if let Some(temp) = &temp {
        parts.push(Part::Text {
            name: "temperature",
            value: temp,
        });
    }
    parts.push(Part::Text {
        name: "response_format",
        value: response_format,
    });
    if request.stream {
        parts.push(Part::Text {
            name: "stream",
            value: "true",
        });
    }
    parts.push(Part::File {
        name: "file",
        filename: &filename,
        content_type: format.mime(),
        data: request.task.audio.data(),
    });
    crate::multipart::encode(&parts, limit)
}

fn parse_languages(map: &Map<String, Value>) -> Result<Option<Vec<String>>, CodecError> {
    match map.get("languages") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let items = value.as_array().ok_or_else(CodecError::invalid)?;
            let mut codes = Vec::with_capacity(items.len());
            for item in items {
                let obj = item.as_object().ok_or_else(CodecError::invalid)?;
                let code = required(obj, "code")?
                    .as_str()
                    .ok_or_else(CodecError::invalid)?
                    .to_owned();
                codes.push(code);
            }
            Ok(Some(codes))
        }
    }
}

fn parse_usage(map: &Map<String, Value>) -> Result<Option<RecognitionUsage>, CodecError> {
    match map.get("usage") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let usage = value.as_object().ok_or_else(CodecError::invalid)?;
            let kind = required(usage, "type")?
                .as_str()
                .ok_or_else(CodecError::invalid)?;
            Ok(Some(match kind {
                "duration" => {
                    let seconds = required(usage, "seconds")?
                        .as_number()
                        .ok_or_else(CodecError::invalid)?
                        .clone();
                    RecognitionUsage::duration(seconds).map_err(|_| CodecError::invalid())?
                }
                "tokens" => {
                    let (audio_tokens, text_tokens) = match usage.get("input_token_details") {
                        None | Some(Value::Null) => (None, None),
                        Some(details) => {
                            let details = details.as_object().ok_or_else(CodecError::invalid)?;
                            (
                                optional_u64(details, "audio_tokens")?,
                                optional_u64(details, "text_tokens")?,
                            )
                        }
                    };
                    RecognitionUsage::Tokens {
                        input_tokens: u64_field(usage, "input_tokens")?,
                        output_tokens: u64_field(usage, "output_tokens")?,
                        total_tokens: u64_field(usage, "total_tokens")?,
                        audio_tokens,
                        text_tokens,
                    }
                }
                _ => return Err(CodecError::invalid()),
            }))
        }
    }
}

pub fn decode_response_json(bytes: &[u8]) -> Result<RecognitionResult, CodecError> {
    let value = crate::json::decode(bytes)?;
    let map = value.as_object().ok_or_else(CodecError::invalid)?;
    let text = required(map, "text")?
        .as_str()
        .ok_or_else(CodecError::invalid)?
        .to_owned();
    let detected_languages = parse_languages(map)?;
    let usage = parse_usage(map)?;
    RecognitionResult::new(
        text,
        RecognitionReport {
            request_id: None,
            usage,
            detected_languages,
            last_sentence: None,
        },
    )
    .map_err(|_| CodecError::invalid())
}

pub fn decode_response_text(bytes: &[u8]) -> Result<RecognitionResult, CodecError> {
    if bytes.len() > MAX_TEXT_BYTES {
        return Err(CodecError::Limit);
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| CodecError::invalid())?
        .to_owned();
    RecognitionResult::new(text, RecognitionReport::default()).map_err(|_| CodecError::invalid())
}

pub fn decode_response(
    bytes: &[u8],
    format: OutputFormat,
) -> Result<RecognitionResult, CodecError> {
    match format {
        OutputFormat::Json => decode_response_json(bytes),
        OutputFormat::Text => decode_response_text(bytes),
    }
}

pub fn project_result(
    result: &RecognitionResult,
    format: OutputFormat,
) -> Result<Projection, CodecError> {
    let report = result.report();
    report
        .validate(result.text())
        .map_err(|_| CodecError::invalid())?;
    let omitted_alignment = report.last_sentence.is_some();
    let omitted_request_id = report.request_id.is_some();
    match format {
        OutputFormat::Text => Ok(Projection {
            output: ProjectedOutput::Text(result.text().as_bytes().to_vec()),
            omitted_alignment,
            omitted_request_id,
            omitted_ancillary: omitted_alignment
                || omitted_request_id
                || report.usage.is_some()
                || report.detected_languages.is_some(),
        }),
        OutputFormat::Json => {
            let mut body = json!({"text": result.text()});
            if let Some(languages) = &report.detected_languages {
                body["languages"] =
                    Value::Array(languages.iter().map(|code| json!({"code": code})).collect());
            }
            if let Some(usage) = &report.usage {
                body["usage"] = match usage {
                    RecognitionUsage::Duration(seconds) => {
                        json!({"type": "duration", "seconds": seconds})
                    }
                    RecognitionUsage::Tokens {
                        input_tokens,
                        output_tokens,
                        total_tokens,
                        audio_tokens,
                        text_tokens,
                    } => {
                        let mut obj = json!({
                            "type": "tokens",
                            "input_tokens": input_tokens,
                            "output_tokens": output_tokens,
                            "total_tokens": total_tokens,
                        });
                        if audio_tokens.is_some() || text_tokens.is_some() {
                            let mut details = Map::new();
                            if let Some(audio) = audio_tokens {
                                details.insert("audio_tokens".into(), json!(audio));
                            }
                            if let Some(text) = text_tokens {
                                details.insert("text_tokens".into(), json!(text));
                            }
                            obj["input_token_details"] = Value::Object(details);
                        }
                        obj
                    }
                };
            }
            Ok(Projection {
                output: ProjectedOutput::Json(body),
                omitted_alignment,
                omitted_request_id,
                omitted_ancillary: omitted_alignment || omitted_request_id,
            })
        }
    }
}

pub struct StreamDecoder {
    max_text_bytes: usize,
    delta_bytes: usize,
    remaining_events: usize,
    done: bool,
    poisoned: bool,
}
impl StreamDecoder {
    pub fn new(max_text_bytes: usize, max_events: usize) -> Self {
        Self {
            max_text_bytes: max_text_bytes.min(MAX_TEXT_BYTES),
            delta_bytes: 0,
            remaining_events: max_events.min(65536),
            done: false,
            poisoned: false,
        }
    }
    pub fn push(&mut self, value: &Value) -> Result<RecognitionEvent, CodecError> {
        if self.poisoned {
            return Err(CodecError::invalid());
        }
        let result = self.push_inner(value);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn push_inner(&mut self, value: &Value) -> Result<RecognitionEvent, CodecError> {
        if self.done || self.remaining_events == 0 {
            return Err(CodecError::invalid());
        }
        self.remaining_events -= 1;
        let map = value.as_object().ok_or_else(CodecError::invalid)?;
        let kind = required(map, "type")?
            .as_str()
            .ok_or_else(CodecError::invalid)?;
        match kind {
            "transcript.text.delta" => {
                let delta = required(map, "delta")?
                    .as_str()
                    .ok_or_else(CodecError::invalid)?
                    .to_owned();
                self.delta_bytes = self
                    .delta_bytes
                    .checked_add(delta.len())
                    .filter(|n| *n <= self.max_text_bytes)
                    .ok_or(CodecError::Limit)?;
                Ok(RecognitionEvent::TextDelta { delta })
            }
            "transcript.text.done" => {
                let text = required(map, "text")?
                    .as_str()
                    .ok_or_else(CodecError::invalid)?
                    .to_owned();
                if text.len() > self.max_text_bytes {
                    return Err(CodecError::Limit);
                }
                let detected_languages = parse_languages(map)?;
                let usage = parse_usage(map)?;
                let result = RecognitionResult::new(
                    text,
                    RecognitionReport {
                        request_id: None,
                        usage,
                        detected_languages,
                        last_sentence: None,
                    },
                )
                .map_err(|_| CodecError::invalid())?;
                self.done = true;
                Ok(RecognitionEvent::Done(result))
            }
            _ => Err(CodecError::invalid()),
        }
    }
    pub fn finish(&self) -> Result<(), CodecError> {
        if self.poisoned || !self.done {
            return Err(CodecError::invalid());
        }
        Ok(())
    }
}

pub fn encode_sse_event(event: &RecognitionEvent) -> Result<Bytes, CodecError> {
    let (name, payload) = match event {
        RecognitionEvent::TextDelta { delta } => (
            "transcript.text.delta",
            json!({"type": "transcript.text.delta", "delta": delta}),
        ),
        RecognitionEvent::Done(result) => {
            let ProjectedOutput::Json(mut obj) = project_result(result, OutputFormat::Json)?.output
            else {
                unreachable!("json projection");
            };
            obj["type"] = json!("transcript.text.done");
            ("transcript.text.done", obj)
        }
    };
    let encoded = serde_json::to_string(&payload).map_err(|_| CodecError::invalid())?;
    Ok(Bytes::from(format!("event: {name}\ndata: {encoded}\n\n")))
}
