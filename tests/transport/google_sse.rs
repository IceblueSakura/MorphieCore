//! Gemini SSE through attempt intake and client delivery; independent wire frames.
use morphiecore::{
    adapter::UpstreamAdapter,
    execution::{Attempt, ResponseDelivery},
    protocol::{
        native::ReplayTarget,
        openai::{Adapter, Dialect, Profile, TargetCaps, stream::Obfuscation},
        stream::SseLimits,
    },
    semantic::{
        context::StreamOptions,
        task::generation::{ContentPart, Item},
        value::{Presence, Text},
    },
};
use serde_json::{Value, json};

fn incomplete_frames(parts: Value) -> Vec<u8> {
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
            frame["candidates"][0]["finishReason"] = json!("MAX_TOKENS");
            frame["usageMetadata"] = json!({"promptTokenCount":10,"candidatesTokenCount":5,
                "thoughtsTokenCount":3,"totalTokenCount":18});
        }
        out.push_str(&format!("data: {frame}\r\n\r\n"));
    }
    out.into_bytes()
}
fn deliver(bytes: &[u8], profile: Profile, stream: bool) -> Result<Value, String> {
    let target = ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap();
    let mut attempt = Attempt::new(
        UpstreamAdapter::GoogleGenerateContent { target },
        2 << 20,
        SseLimits::default(),
    );
    let mut delivery = ResponseDelivery::new(
        morphiecore::adapter::ClientAdapter::openai(
            Adapter::new(profile, Dialect::MorphieCore, None),
            TargetCaps::full(),
        ),
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
fn signed_text_and_partial_outputs_keep_replay_and_terminal_contracts() {
    let empty = incomplete_frames(json!([{"text":"","thoughtSignature":"U0lH"}]));
    let response = deliver(&empty, Profile::Responses, false).unwrap();
    assert_eq!(response["output"][0]["content"][0]["text"], "");
    let client = Adapter::new(
        Profile::Responses,
        Dialect::Standard,
        Some(morphiecore::execution::replay::CLIENT_ORIGIN.clone()),
    );
    let mut returned = client
        .decode_request(
            json!({"model":"m","input":response["output"]})
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    morphiecore::execution::replay::open_from_client(&mut returned).unwrap();
    let (encoded, _) = morphiecore::protocol::google::encode_generation_request(
        &returned.task.semantic,
        &returned.task.fidelity,
        &ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(
        encoded["contents"][0]["parts"],
        json!([{"text":"","thoughtSignature":"U0lH"}])
    );
    let mut items = returned.task.semantic.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        panic!("signed text")
    };
    message.parts[0].content =
        ContentPart::Text(Text::new("edited", "synthetic", 64).unwrap().into());
    returned.task.semantic = returned.task.semantic.with_items(items).unwrap();
    assert!(
        morphiecore::protocol::google::encode_generation_request(
            &returned.task.semantic,
            &returned.task.fidelity,
            &ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap(),
            None,
        )
        .is_err()
    );
    let stream = deliver(&empty, Profile::Responses, true).unwrap();
    assert!(
        stream
            .as_str()
            .unwrap()
            .contains("event: response.incomplete\n")
    );
    assert!(
        !stream
            .as_str()
            .unwrap()
            .contains("event: response.completed\n")
    );

    let partial = b"data: {\"responseId\":\"resp\",\"modelVersion\":\"gemini-synthetic\",\"candidates\":[{\"index\":0,\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"partial\"}]},\"finishReason\":\"MAX_TOKENS\"}],\"usageMetadata\":{\"promptTokenCount\":1,\"candidatesTokenCount\":1,\"totalTokenCount\":2}}\n\n";
    let json = deliver(partial, Profile::Responses, false).unwrap();
    assert_eq!(json["status"], "incomplete");
    assert_eq!(json["incomplete_details"]["reason"], "max_output_tokens");
    assert_eq!(json["output"][0]["content"][0]["text"], "partial");
    let sse = deliver(partial, Profile::Responses, true).unwrap();
    let sse = sse.as_str().unwrap();
    assert!(sse.contains("event: response.incomplete\n"));
    assert!(!sse.contains("event: response.completed\n"));
    let chat = deliver(partial, Profile::Chat, false).unwrap();
    assert_eq!(chat["choices"][0]["finish_reason"], "length");
    assert_eq!(chat["choices"][0]["message"]["content"], "partial");
}

#[test]
fn readable_reasoning_after_content_still_cannot_reach_chat() {
    let late_thought =
        incomplete_frames(json!([{"text":"Reading."},{"text":"Plan","thought":true}]));
    for stream in [false, true] {
        assert!(deliver(&late_thought, Profile::Chat, stream).is_err());
    }
    assert!(deliver(&late_thought, Profile::Responses, false).is_ok());
}
