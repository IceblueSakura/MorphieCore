//! Gemini SSE through attempt intake and client delivery; independent wire frames.
use morphiecore::{
    adapter::UpstreamAdapter,
    execution::{Attempt, ResponseDelivery},
    protocol::{
        native::ReplayTarget,
        openai::{
            Adapter, Dialect, Profile, TargetCaps,
            sse::{Obfuscation, SseLimits},
        },
    },
    semantic::{context::StreamOptions, value::Presence},
};
use serde_json::{Value, json};

fn frames(parts: Value) -> Vec<u8> {
    let meta = json!({"responseId":"resp","modelVersion":"gemini-synthetic"});
    let mut out = String::new();
    let parts = parts.as_array().unwrap();
    for (n, part) in parts.iter().enumerate() {
        let mut frame = json!({"candidates":[{"content":{"role":"model","parts":[part]}}]});
        frame
            .as_object_mut()
            .unwrap()
            .extend(meta.as_object().unwrap().clone());
        if n + 1 == parts.len() {
            frame["candidates"][0]["finishReason"] = json!("STOP");
            frame["usageMetadata"] = json!({"promptTokenCount":10,"candidatesTokenCount":5,
                "thoughtsTokenCount":3,"totalTokenCount":18});
        }
        out.push_str(&format!("data: {frame}\r\n\r\n"));
    }
    out.into_bytes()
}
/// Gemini's usual tool turn: thoughts, a lead-in, then the signed first call.
fn tool_turn() -> Vec<u8> {
    frames(json!([
        {"text":"Plan","thought":true},
        {"text":"Reading."},
        {"functionCall":{"id":"c1","name":"read","args":{"path":"a.txt"}},"thoughtSignature":"U0lH"}
    ]))
}
fn deliver(bytes: &[u8], profile: Profile, stream: bool) -> Result<Value, String> {
    let target = ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap();
    let mut attempt = Attempt::new(
        UpstreamAdapter::GoogleGenerateContent { target },
        2 << 20,
        SseLimits::default(),
    );
    let mut delivery = ResponseDelivery::new(
        Adapter::new(profile, Dialect::MorphieCore, None),
        TargetCaps::full(),
        "public",
        SseLimits::default(),
        StreamOptions {
            include_usage: Presence::Value(true),
            include_obfuscation: Presence::Value(false),
        },
        Obfuscation::Disabled,
    );
    attempt.begin(200, "text/event-stream").unwrap();
    let mut output = vec![];
    let mut rest = bytes;
    while !rest.is_empty() {
        let (used, events) = attempt.push(rest).map_err(|e| e.to_string())?;
        rest = &rest[used..];
        if stream {
            let produced = delivery
                .encode_events(&attempt, &events)
                .map_err(|e| e.to_string())?;
            if !produced.is_empty() && delivery.lifecycle().may_retry_or_fallback() {
                delivery.commit().unwrap();
            }
            output.extend(produced.into_iter().flatten());
        }
    }
    attempt.finish().map_err(|e| e.to_string())?;
    if !stream {
        let body = delivery.encode_json(&attempt).map_err(|e| e.to_string())?;
        return Ok(serde_json::from_slice(&body).unwrap());
    }
    output.extend(
        delivery
            .finish_stream(&attempt)
            .map_err(|e| e.to_string())?
            .into_iter()
            .flatten(),
    );
    Ok(json!(String::from_utf8(output).unwrap()))
}

#[test]
fn signed_calls_after_content_reach_chat_and_responses_clients() {
    for profile in [Profile::Chat, Profile::Responses] {
        for stream in [false, true] {
            let body = deliver(&tool_turn(), profile, stream)
                .unwrap_or_else(|e| panic!("{profile:?} stream={stream}: {e}"));
            let text = body.to_string();
            // Gemini input already includes cache reads; its total passes through.
            assert!(
                text.contains("\"total_tokens\":18") || text.contains("\\\"total_tokens\\\":18"),
                "{profile:?} stream={stream}: usage"
            );
            match (profile, stream) {
                (Profile::Chat, false) => {
                    let message = &body["choices"][0]["message"];
                    assert_eq!(message["content"], "Reading.");
                    assert_eq!(message["tool_calls"][0]["id"], "c1");
                    let types: Vec<_> = message["reasoning_details"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|d| d["type"].as_str().unwrap())
                        .collect();
                    assert_eq!(
                        types,
                        [
                            "reasoning.summary",
                            "reasoning.encrypted",
                            "reasoning.encrypted"
                        ]
                    );
                }
                (Profile::Responses, false) => {
                    let types: Vec<_> = body["output"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|o| o["type"].as_str().unwrap())
                        .collect();
                    assert_eq!(
                        types,
                        ["reasoning", "message", "function_call", "reasoning"]
                    );
                }
                _ => {
                    assert!(
                        text.contains("reasoning.encrypted") || text.contains("encrypted_content")
                    );
                    assert!(text.contains("c1") && !text.contains("U0lH"));
                }
            }
        }
    }
}

#[test]
fn readable_reasoning_after_content_still_cannot_reach_chat() {
    let late_thought = frames(json!([{"text":"Reading."},{"text":"Plan","thought":true}]));
    for stream in [false, true] {
        assert!(deliver(&late_thought, Profile::Chat, stream).is_err());
    }
    assert!(deliver(&late_thought, Profile::Responses, false).is_ok());
}
