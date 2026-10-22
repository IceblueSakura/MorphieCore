//! Small native diagnostic contracts, not gateway codecs or model capability claims.
use super::{Error, PROMPT, Protocol};
use morphiecore::{
    semantic::value::{JsonLimits, parse_json},
    transport::sse::SseDecoder,
};
use serde_json::{Value, json};

pub(super) fn request(protocol: Protocol, model: &str, siwc: bool) -> Value {
    match protocol {
        Protocol::OpenaiChat => {
            json!({"model":model,"messages":[{"role":"user","content":PROMPT}],"stream":false})
        }
        Protocol::OpenaiResponses => {
            json!({"model":model,"input":[{"role":"user","content":PROMPT}],"store":false,"stream":siwc})
        }
        Protocol::Anthropic => {
            // Messages requires max_tokens; other protocols retain their upstream defaults.
            json!({"model":model,"messages":[{"role":"user","content":PROMPT}],"max_tokens":128,"stream":false})
        }
        Protocol::Google => {
            json!({"contents":[{"role":"user","parts":[{"text":PROMPT}]}]})
        }
    }
}
pub(super) fn decode(bytes: &[u8]) -> Result<Value, Error> {
    parse_json(bytes, JsonLimits::ENVELOPE).map_err(|_| Error::Response)
}
fn string(value: &Value) -> Result<&str, Error> {
    value
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control))
        .ok_or(Error::Response)
}
fn array(value: &Value) -> Result<&Vec<Value>, Error> {
    value.as_array().ok_or(Error::Response)
}
fn no_error(value: &Value) -> Result<(), Error> {
    if !value.is_object() || value.get("error").is_some_and(|v| !v.is_null()) {
        return Err(Error::Response);
    }
    Ok(())
}
fn text_parts(parts: &[Value], kind: &str) -> Result<usize, Error> {
    let mut bytes = 0;
    for part in parts {
        if part["type"].as_str() == Some(kind) {
            bytes += part["text"].as_str().ok_or(Error::Response)?.len();
        }
    }
    Ok(bytes)
}
fn completed_response(value: &Value) -> Result<usize, Error> {
    no_error(value)?;
    if value["status"] != "completed" {
        return Err(Error::Incomplete);
    }
    let mut bytes = 0;
    for item in array(&value["output"])? {
        if item["type"] == "message" {
            if item["role"] != "assistant" || item.get("status").is_some_and(|v| v != "completed") {
                return Err(Error::Incomplete);
            }
            bytes += text_parts(array(&item["content"])?, "output_text")?;
        }
    }
    Ok(bytes)
}
fn completion(bytes: usize) -> Result<Value, Error> {
    if bytes == 0 {
        return Err(Error::Incomplete);
    }
    Ok(json!({"completed":true,"text_bytes":bytes}))
}
pub(super) fn probe(bytes: &[u8], protocol: Protocol) -> Result<Value, Error> {
    let value = decode(bytes)?;
    no_error(&value)?;
    let text_bytes = match protocol {
        Protocol::OpenaiChat => {
            let choices = array(&value["choices"])?;
            if choices.len() != 1 || choices[0]["finish_reason"] != "stop" {
                return Err(Error::Incomplete);
            }
            let message = &choices[0]["message"];
            if message["role"] != "assistant" {
                return Err(Error::Response);
            }
            message["content"].as_str().ok_or(Error::Incomplete)?.len()
        }
        Protocol::OpenaiResponses => completed_response(&value)?,
        Protocol::Anthropic => {
            if value["type"] != "message" || value["role"] != "assistant" {
                return Err(Error::Response);
            }
            if value["stop_reason"] != "end_turn" {
                return Err(Error::Incomplete);
            }
            text_parts(array(&value["content"])?, "text")?
        }
        Protocol::Google => {
            let candidates = array(&value["candidates"])?;
            if candidates.len() != 1 || candidates[0]["finishReason"] != "STOP" {
                return Err(Error::Incomplete);
            }
            let mut bytes = 0;
            for part in array(&candidates[0]["content"]["parts"])? {
                if let Some(thought) = part.get("thought")
                    && thought.as_bool().ok_or(Error::Response)?
                {
                    continue;
                }
                if let Some(text) = part.get("text") {
                    bytes += text.as_str().ok_or(Error::Response)?.len();
                }
            }
            bytes
        }
    };
    completion(text_bytes)
}
pub(super) fn responses_stream(bytes: &[u8]) -> Result<Value, Error> {
    let mut decoder = SseDecoder::new(1 << 20);
    let mut completed = None;
    let mut closed_text = std::collections::BTreeMap::new();
    let mut done = false;
    let mut count = 0;
    // Bound the number of framed events retained at once, even for one large HTTP chunk.
    for chunk in bytes.chunks(8192) {
        for event in decoder.push(chunk).map_err(|_| Error::Response)? {
            count += 1;
            if count > 10000 {
                return Err(Error::Limit);
            }
            if event.data().is_empty() && event.event().is_none() {
                continue;
            }
            if event.data() == "[DONE]" && completed.is_some() && !done {
                done = true;
                continue;
            }
            if completed.is_some() || done {
                return Err(Error::Response);
            }
            let value = decode(event.data().as_bytes())?;
            let kind = value["type"].as_str().ok_or(Error::Response)?;
            if event.event().is_some_and(|name| name != kind) {
                return Err(Error::Response);
            }
            match kind {
                "error" | "response.failed" | "response.incomplete" => {
                    return Err(Error::Incomplete);
                }
                "response.output_item.done" if value["item"]["type"] == "message" => {
                    let item = &value["item"];
                    let id = string(&item["id"])?;
                    let bytes = completed_response(&json!({"status":"completed","output":[item]}))?;
                    if closed_text.insert(id.to_owned(), bytes).is_some() {
                        return Err(Error::Response);
                    }
                }
                "response.completed" => {
                    let text = completed_response(&value["response"])?;
                    // SIWC may summarize an event-owned output with an empty final array.
                    completed = Some(
                        if value["response"]["output"]
                            .as_array()
                            .is_some_and(Vec::is_empty)
                        {
                            closed_text.values().sum()
                        } else {
                            text
                        },
                    );
                }
                _ if kind.starts_with("response.") => {}
                _ => return Err(Error::Response),
            }
        }
    }
    decoder.finish_strict().map_err(|_| Error::Response)?;
    completion(completed.ok_or(Error::Incomplete)?)
}
