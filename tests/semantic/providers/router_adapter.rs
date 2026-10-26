//! Synthetic router wire shapes; no account, network or model-quality oracle.
use morphiecore::{
    protocol::{
        openai::{
            Dialect, GenerationApi, GenerationCodec, TargetContract as Contract,
            chat::sse::{ChatSseDecoder, ChatSseEncoder},
            events::EventEncoder,
            obfuscation::Obfuscation,
        },
        stream::SseLimits,
    },
    semantic::{
        context::StreamOptions,
        value::{Presence, ReplayOrigin},
    },
};
use serde_json::{Value, json};

fn adapter(profile: GenerationApi, dialect: Dialect, scope: &str) -> GenerationCodec {
    GenerationCodec::new(profile, dialect, Some(ReplayOrigin::new(scope).unwrap()))
}
fn chat() -> Value {
    json!({"id":"r1","object":"chat.completion","created":2,"model":"openai/gpt-6-luna",
        "provider":"Synthetic","choices":[{"index":0,"finish_reason":"stop","native_finish_reason":"stop",
        "message":{"role":"assistant","content":"pong","reasoning":"think","reasoning_details":null}}],
        "usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5,
        "cost":0.00000123,"is_byok":false,"cost_details":{"upstream_inference_cost":null},
        "prompt_tokens_details":{"cached_tokens":1},"completion_tokens_details":{"reasoning_tokens":1}}})
}
#[test]
fn router_rejects_unknown_or_unowned_shapes_and_never_masks_invalid_usage() {
    let source = adapter(GenerationApi::Chat, Dialect::OpenRouter, "source");
    for (pointer, value) in [
        ("/usage/cost", json!("not-money")),
        ("/usage/prompt_tokens", json!(-1)),
        (
            "/choices/0/message/reasoning_details",
            json!([{"type":"reasoning.encrypted","data":"synthetic"}]),
        ),
        ("/provider", json!("x".repeat(4097))),
    ] {
        let mut body = chat();
        *body.pointer_mut(pointer).unwrap() = value;
        assert!(
            source.decode_response(body.to_string().as_bytes()).is_err(),
            "{pointer}"
        );
    }
    let mut body = chat();
    body["choices"][0]["message"]["reasoning_content"] = json!("conflict");
    assert!(source.decode_response(body.to_string().as_bytes()).is_err());
    assert!(
        GenerationCodec::new(GenerationApi::Chat, Dialect::OpenRouter, None)
            .decode_response(chat().to_string().as_bytes())
            .is_err()
    );
}

fn chunk(delta: Value, reason: Value) -> Value {
    json!({"id":"r1","object":"chat.completion.chunk","created":2,"model":"openai/gpt-6-luna",
        "provider":"Synthetic","choices":[{"index":0,"delta":delta,"finish_reason":reason,"native_finish_reason":reason}]})
}

#[test]
fn responses_done_trailer_requires_a_real_terminal_and_cannot_repeat() {
    use morphiecore::protocol::openai::responses::sse::ResponsesSseDecoder;
    let frames = crate::wire::events(2);
    let body = frames
        .iter()
        .map(|v| format!("data: {v}\n\n"))
        .collect::<String>();
    for (dialect, wire, accepted) in [
        (Dialect::OpenRouter, format!("{body}data: [DONE]\n\n"), true),
        (Dialect::Zhipu, format!("{body}data: [DONE]\n\n"), true),
        (Dialect::Nvidia, format!("{body}data: [DONE]\n\n"), true),
        (Dialect::Nvidia, "data: [DONE]\n\n".into(), false),
        (
            Dialect::Nvidia,
            format!("{body}data: [DONE]\n\ndata: [DONE]\n\n"),
            false,
        ),
        (Dialect::Zhipu, "data: [DONE]\n\n".into(), false),
        (
            Dialect::Zhipu,
            format!("{body}data: [DONE]\n\ndata: [DONE]\n\n"),
            false,
        ),
        (Dialect::Standard, format!("{body}data: [DONE]\n\n"), false),
        (Dialect::OpenRouter, "data: [DONE]\n\n".into(), false),
        (
            Dialect::OpenRouter,
            format!("{body}data: [DONE]\n\ndata: [DONE]\n\n"),
            false,
        ),
    ] {
        let mut decoder = ResponsesSseDecoder::with_decoder(
            200,
            "text/event-stream",
            SseLimits::default(),
            adapter(GenerationApi::Responses, dialect, "source").event_decoder(),
        )
        .unwrap();
        let mut remaining = wire.as_bytes();
        let mut ok = true;
        while !remaining.is_empty() {
            match decoder.consume(remaining) {
                Ok((n, _)) if n > 0 => remaining = &remaining[n..],
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        ok = decoder.finish().is_ok() && ok;
        assert_eq!(ok, accepted);
    }
}

#[test]
fn router_reasoning_format_is_a_checked_marker_not_an_opaque_escape() {
    let source = adapter(GenerationApi::Responses, Dialect::OpenRouter, "source");
    let mut body = json!({"id":"r","object":"response","created_at":1,"model":"m","status":"completed","output":[
        {"type":"reasoning","id":"rs","status":"completed","summary":[],"encrypted_content":"synthetic","format":"openai-responses-v1"}
    ],"usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2,"cost_details":{"upstream_inference_input_cost":0.01,"upstream_inference_output_cost":0.02}}});
    let decoded = source.decode_response(body.to_string().as_bytes()).unwrap();
    let c = Contract {
        replay_origin: Some(ReplayOrigin::new("source").unwrap()),
        ..Contract::full()
    };
    let wire = source.encode_response(&decoded, &c).unwrap();
    assert!(wire["output"][0].get("format").is_none());
    assert_eq!(
        wire["usage"]["cost_details"]["upstream_inference_output_cost"],
        0.02
    );
    body["output"][0]["format"] = json!("unrecognized-signature-v1");
    assert!(source.decode_response(body.to_string().as_bytes()).is_err());
    // Unclassified reasoning (observed for z-ai/glm-5.3-flash) is readable text only.
    body["output"][0] = json!({"type":"reasoning","id":"rs","status":"completed","summary":[],
            "content":[{"type":"reasoning_text","text":"think"}],"format":"unknown"});
    let decoded = source.decode_response(body.to_string().as_bytes()).unwrap();
    use morphiecore::semantic::task::generation::{Item, ReasoningContent};
    let Item::Reasoning(reasoning) = &decoded.semantic.items()[0].1 else {
        panic!("reasoning")
    };
    assert!(reasoning.replay.is_none());
    assert!(
        matches!(&reasoning.parts[..], [(_, ReasoningContent::Text(t))] if t.as_str() == "think")
    );
    body["output"][0]["encrypted_content"] = json!("synthetic");
    assert!(source.decode_response(body.to_string().as_bytes()).is_err());
}

#[test]
fn router_repeated_finish_usage_is_content_free_and_matches_static() {
    let source = adapter(GenerationApi::Chat, Dialect::OpenRouter, "source");
    let mut tail = chunk(json!({"role":"assistant","content":""}), json!("stop"));
    tail["usage"] = chat()["usage"].clone();
    let frames = vec![
        chunk(json!({"role":"assistant","reasoning":"think"}), Value::Null),
        chunk(json!({"content":"pong"}), Value::Null),
        chunk(json!({}), json!("stop")),
        tail.clone(),
    ];
    let wire = frames
        .iter()
        .map(|v| format!("data: {v}\n\n"))
        .collect::<String>()
        + "data: [DONE]\n\n";
    let mut stream = ChatSseDecoder::with_decoder(
        200,
        "text/event-stream",
        SseLimits::default(),
        source.event_decoder(),
    )
    .unwrap();
    let mut events = vec![];
    for byte in wire.as_bytes() {
        let (n, batch) = stream.consume(&[*byte]).unwrap();
        assert_eq!(n, 1);
        events.extend(batch);
    }
    stream.finish().unwrap();
    let decoded = stream.materialize().unwrap();
    let expected = source
        .decode_response(chat().to_string().as_bytes())
        .unwrap();
    assert_eq!(decoded.semantic, expected.semantic);
    assert_eq!(
        source.encode_response(&decoded, &Contract::full()).unwrap(),
        source
            .encode_response(&expected, &Contract::full())
            .unwrap()
    );
    let mut encoder = EventEncoder::new(GenerationApi::Chat, decoded.metadata.clone())
        .unwrap()
        .with_contract(source.contract(&Contract::full()));
    let mut encoded = vec![];
    for event in &events {
        encoded.extend(encoder.encode(event, &decoded.fidelity).unwrap());
    }
    encoder.finish().unwrap();
    assert!(encoded[0].get("provider").is_none());
    assert_eq!(encoded.last().unwrap()["usage"]["cost"], json!(0.00000123));
    for include_usage in [false, true] {
        let mut framed = ChatSseEncoder::new(
            decoded.metadata.clone(),
            source.contract(&Contract::full()),
            SseLimits::default(),
            StreamOptions {
                include_usage: Presence::Value(include_usage),
                include_obfuscation: Presence::Value(false),
            },
            Obfuscation::Disabled,
        )
        .unwrap();
        let mut bytes = Vec::new();
        for event in &events {
            for frame in framed.encode(event, &decoded.fidelity).unwrap() {
                bytes.extend_from_slice(&frame);
            }
        }
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(text.contains("\"cost\""), include_usage);
        assert_eq!(text.matches("data: [DONE]").count(), 1);
    }
    for bad in [
        {
            let mut v = tail.clone();
            v["choices"][0]["delta"]["content"] = json!("late");
            v
        },
        {
            let mut v = tail.clone();
            v["choices"][0]["finish_reason"] = json!("length");
            v
        },
        {
            let mut v = tail.clone();
            v["choices"][0]["delta"]["tool_calls"] = json!([]);
            v
        },
    ] {
        let mut decoder = source.event_decoder();
        for frame in &frames[..3] {
            decoder.push(frame).unwrap();
        }
        assert!(decoder.push(&bad).is_err());
        assert!(decoder.done().is_err());
    }
    let mut decoder = source.event_decoder();
    for frame in &frames {
        decoder.push(frame).unwrap();
    }
    assert!(decoder.finish().is_err(), "finish/usage is not DONE");
    assert!(decoder.push(&tail).is_err(), "duplicate usage must fail");
    assert!(decoder.done().is_err());
}
