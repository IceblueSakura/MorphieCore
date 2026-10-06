//! Synthetic router wire shapes; no account, network or model-quality oracle.
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract as Contract,
    protocol::openai::{
        Profile,
        chat_sse::{ChatSseDecoder, ChatSseEncoder},
        events::EventEncoder,
        sse::{Obfuscation, SseLimits},
    },
    semantic::{
        context::StreamOptions,
        task::generation::*,
        value::{Presence, ReplayOrigin},
    },
};
use serde_json::{Value, json};

fn adapter(profile: Profile, dialect: Dialect, scope: &str) -> Adapter {
    Adapter::new(profile, dialect, Some(ReplayOrigin::new(scope).unwrap()))
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
fn router_facts_are_scoped_and_reasoning_and_usage_are_typed() {
    let source = adapter(Profile::Chat, Dialect::OpenRouter, "source");
    let body = chat();
    let mut decoded = source.decode_response(body.to_string().as_bytes()).unwrap();
    assert!(
        matches!(&decoded.semantic.items()[0].1, Item::Reasoning(r) if matches!(&r.parts[0].1, ReasoningContent::Text(t) if t.as_str()=="think"))
    );
    assert_eq!(
        decoded.semantic.usage().unwrap().cached_input_tokens,
        Some(1)
    );
    assert_eq!(
        decoded.semantic.usage().unwrap().input_cache_write_tokens,
        None
    );
    let encoded = source.encode_response(&decoded, &Contract::full()).unwrap();
    assert_eq!(encoded["usage"]["cost"], json!(0.00000123));
    assert_eq!(encoded["provider"], "Synthetic");
    assert_eq!(encoded["choices"][0]["native_finish_reason"], "stop");
    assert_eq!(encoded["choices"][0]["message"]["reasoning"], "think");
    assert!(
        encoded["choices"][0]["message"]
            .get("reasoning_content")
            .is_none()
    );
    for target in [
        adapter(Profile::Chat, Dialect::OpenRouter, "other"),
        adapter(Profile::Chat, Dialect::MorphieCore, "source"),
    ] {
        let out = target.encode_response(&decoded, &Contract::full()).unwrap();
        assert!(out.get("provider").is_none());
        assert!(out["usage"].get("cost").is_none());
        assert!(out["choices"][0].get("native_finish_reason").is_none());
    }
    assert!(
        adapter(Profile::Chat, Dialect::Standard, "source")
            .decode_response(body.to_string().as_bytes())
            .is_err()
    );
    let original = decoded.clone();
    decoded.metadata.id = "another-response".into();
    assert!(
        source
            .encode_response(&decoded, &Contract::full())
            .unwrap()
            .get("provider")
            .is_none()
    );
    decoded = original.clone();
    for replacement in [Some("replaced reasoning"), None] {
        let mut items = original.semantic.items().to_vec();
        items.retain(|(_, item)| !matches!(item, Item::Reasoning(_)));
        if let Some(text) = replacement {
            items.insert(
                0,
                (
                    ItemId::new(99),
                    Item::Reasoning(ReasoningItem {
                        status: ItemLifecycle::Completed,
                        replay: None,
                        parts: vec![(
                            PartId::new(99),
                            ReasoningContent::Text(
                                morphiecore::semantic::value::Text::new(text, "fixture", 128)
                                    .unwrap(),
                            ),
                        )],
                    }),
                ),
            );
        }
        let mut changed = original.clone();
        changed.semantic = GenerationResponse::new(items, Outcome::Completed).unwrap();
        let output = source.encode_response(&changed, &Contract::full()).unwrap();
        assert_eq!(
            output["choices"][0]["message"].get("reasoning"),
            replacement.map(|s| json!(s)).as_ref()
        );
        assert!(output.get("provider").is_none());
        assert!(output["usage"].is_null());
    }
    decoded.semantic = decoded
        .semantic
        .clone()
        .with_usage(Usage {
            scope: UsageScope::Operation,
            basis: UsageBasis::Final,
            output_relation: OutputTokenRelation::IncludesReasoning,
            total_relation: TotalTokenRelation::InputAndOutput,
            input_tokens: Some(4),
            output_tokens: Some(2),
            total_tokens: Some(6),
            cached_input_tokens: None,
            input_cache_write_tokens: None,
            reasoning_tokens: None,
            input_text_tokens: None,
            input_image_tokens: None,
            input_audio_tokens: None,
            output_audio_tokens: None,
            output_text_tokens: None,
            accepted_prediction_tokens: None,
            rejected_prediction_tokens: None,
        })
        .unwrap();
    let out = source.encode_response(&decoded, &Contract::full()).unwrap();
    assert!(out.get("provider").is_none());
    assert!(out["usage"].get("cost").is_none());
    let mut deleted = original;
    deleted.semantic = GenerationResponse::new(vec![], Outcome::Completed).unwrap();
    let responses = adapter(Profile::Responses, Dialect::OpenRouter, "source")
        .encode_response(&deleted, &Contract::full())
        .unwrap();
    assert!(responses["usage"].is_null());
}
#[test]
fn router_rejects_unknown_or_unowned_shapes_and_never_masks_invalid_usage() {
    let source = adapter(Profile::Chat, Dialect::OpenRouter, "source");
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
    body["unknown_router_field"] = json!(true);
    assert!(source.decode_response(body.to_string().as_bytes()).is_err());
    body.as_object_mut().unwrap().remove("unknown_router_field");
    body["choices"][0]["message"]["reasoning_content"] = json!("conflict");
    assert!(source.decode_response(body.to_string().as_bytes()).is_err());
    assert!(
        Adapter::new(Profile::Chat, Dialect::OpenRouter, None)
            .decode_response(chat().to_string().as_bytes())
            .is_err()
    );
}
#[test]
fn router_live_shapes_map_typed_tier_zero_media_and_reasoning_replay() {
    let source = adapter(Profile::Chat, Dialect::OpenRouter, "source");
    let mut body = chat();
    body["service_tier"] = json!("default");
    body["usage"]["prompt_tokens_details"]["audio_tokens"] = json!(0);
    body["usage"]["prompt_tokens_details"]["video_tokens"] = json!(0);
    body["usage"]["completion_tokens_details"]["image_tokens"] = json!(0);
    body["usage"]["completion_tokens_details"]["audio_tokens"] = json!(0);
    body["choices"][0]["message"]["reasoning_details"] = json!([
        {"type":"reasoning.summary","summary":"think","format":"openai-responses-v1","index":0},
        {"type":"reasoning.encrypted","data":"synthetic-replay","id":"rs1","format":"openai-responses-v1","index":1}
    ]);
    let decoded = source.decode_response(body.to_string().as_bytes()).unwrap();
    assert!(
        matches!(&decoded.semantic.items()[0].1, Item::Reasoning(r) if matches!(&r.parts[0].1,ReasoningContent::Summary(t) if t.as_str()=="think"))
    );
    let contract = Contract {
        replay_origin: Some(ReplayOrigin::new("source").unwrap()),
        ..Contract::full()
    };
    let client = adapter(Profile::Chat, Dialect::MorphieCore, "source");
    let output = client.encode_response(&decoded, &contract).unwrap();
    assert_eq!(output["service_tier"], "default");
    assert_eq!(
        output["choices"][0]["message"]["reasoning_details"][1]["data"],
        "synthetic-replay"
    );
    let history = json!({"model":"m","messages":[output["choices"][0]["message"].clone()]});
    let request = client
        .decode_request(history.to_string().as_bytes())
        .unwrap();
    let replay = source.encode_request(&request, "m", &contract).unwrap();
    assert_eq!(
        replay["messages"][0]["reasoning_details"][1]["data"],
        "synthetic-replay"
    );
    assert!(
        source
            .encode_request(&request, "m", &Contract::full())
            .is_err()
    );
    body["usage"]["prompt_tokens_details"]["audio_tokens"] = json!(1);
    let reported = source.decode_response(body.to_string().as_bytes()).unwrap();
    assert_eq!(
        reported.semantic.usage().unwrap().input_audio_tokens,
        Some(1)
    );
    assert_eq!(
        source.encode_response(&reported, &contract).unwrap()["usage"]["prompt_tokens_details"]["audio_tokens"],
        1
    );
    let mut empty = chat();
    empty["choices"][0]["message"]["reasoning"] = json!("");
    empty["choices"][0]["message"]["reasoning_details"] =
        json!([{"type":"reasoning.text","text":"","format":"openai-responses-v1","index":0}]);
    let decoded = source
        .decode_response(empty.to_string().as_bytes())
        .unwrap();
    assert!(
        source.encode_response(&decoded, &contract).is_err(),
        "empty readable reasoning has no lossless Chat carrier, never panic"
    );
}

fn chunk(delta: Value, reason: Value) -> Value {
    json!({"id":"r1","object":"chat.completion.chunk","created":2,"model":"openai/gpt-6-luna",
        "provider":"Synthetic","choices":[{"index":0,"delta":delta,"finish_reason":reason,"native_finish_reason":reason}]})
}
#[test]
fn structured_reasoning_stream_binds_late_identity_and_rejects_duplicate_replay() {
    let source = adapter(Profile::Chat, Dialect::OpenRouter, "source");
    let client = adapter(Profile::Chat, Dialect::MorphieCore, "source");
    let contract = Contract {
        replay_origin: Some(ReplayOrigin::new("source").unwrap()),
        ..Contract::full()
    };
    for summary in [false, true] {
        let encrypted = chunk(
            json!({"reasoning_details":[{"type":"reasoning.encrypted","format":"openai-responses-v1","index":usize::from(summary),"id":"rs-late","data":"synthetic-replay"}]}),
            Value::Null,
        );
        let mut frames = vec![];
        if summary {
            frames.push(chunk(json!({"reasoning":"think","reasoning_details":[{"type":"reasoning.summary","summary":"think","format":"openai-responses-v1","index":0}]}),Value::Null));
        }
        frames.extend([
            encrypted.clone(),
            chunk(json!({"content":"pong"}), Value::Null),
            chunk(json!({}), json!("stop")),
        ]);
        let mut decoder = source.event_decoder();
        let mut encoder = None;
        let mut wire = vec![];
        for frame in &frames {
            let events = decoder.push(frame).unwrap();
            let encoder = encoder.get_or_insert_with(|| {
                EventEncoder::new(Profile::Chat, decoder.metadata().unwrap().clone())
                    .unwrap()
                    .with_contract(client.contract(&contract))
            });
            for event in events {
                wire.extend(encoder.encode(&event, decoder.fidelity()).unwrap());
            }
        }
        for event in decoder.done().unwrap() {
            wire.extend(
                encoder
                    .as_mut()
                    .unwrap()
                    .encode(&event, decoder.fidelity())
                    .unwrap(),
            );
        }
        encoder.unwrap().finish().unwrap();
        let mut consumer = client.event_decoder();
        for frame in &wire {
            consumer.push(frame).unwrap();
        }
        consumer.done().unwrap();
        let decoded = consumer.materialize().unwrap();
        assert_eq!(decoded.semantic, decoder.materialize().unwrap().semantic);
        let id = decoded.semantic.items()[0].0;
        assert_eq!(decoded.fidelity.response_item_id(id), Some("rs-late"));
        let Item::Reasoning(r) = &decoded.semantic.items()[0].1 else {
            panic!("reasoning")
        };
        assert_eq!(
            r.replay.as_ref().and_then(ReplayValue::replay_token),
            Some("synthetic-replay")
        );
        let mut broken = source.event_decoder();
        for frame in &frames[..frames.len() - 2] {
            broken.push(frame).unwrap();
        }
        assert!(broken.push(&encrypted).is_err());
        assert!(broken.done().is_err());
    }
}

#[test]
fn responses_done_trailer_requires_a_real_terminal_and_cannot_repeat() {
    use morphiecore::protocol::openai::sse::ResponsesSseDecoder;
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
            adapter(Profile::Responses, dialect, "source").event_decoder(),
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
    let source = adapter(Profile::Responses, Dialect::OpenRouter, "source");
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
}

#[test]
fn router_repeated_finish_usage_is_content_free_and_matches_static() {
    let source = adapter(Profile::Chat, Dialect::OpenRouter, "source");
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
    let mut encoder = EventEncoder::new(Profile::Chat, decoded.metadata.clone())
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
#[test]
fn router_responses_cost_is_terminal_only_and_survives_only_same_source() {
    let source = adapter(Profile::Responses, Dialect::OpenRouter, "source");
    let mut frames = crate::wire::events(2);
    for frame in &mut frames {
        if let Some(response) = frame.get_mut("response")
            && response["usage"].is_object()
        {
            response["usage"]["cost"] = json!(0.125);
        }
    }
    let mut decoder = source.event_decoder();
    let mut events = vec![];
    for frame in &frames {
        events.extend(decoder.push(frame).unwrap());
    }
    decoder.finish().unwrap();
    let decoded = decoder.materialize().unwrap();
    let static_decoded = source
        .decode_response(frames.last().unwrap()["response"].to_string().as_bytes())
        .unwrap();
    assert_eq!(decoded.semantic, static_decoded.semantic);
    let mut encoder = EventEncoder::new(Profile::Responses, decoded.metadata.clone())
        .unwrap()
        .with_contract(source.contract(&Contract::full()));
    let mut output = vec![];
    for event in &events {
        output.extend(encoder.encode(event, &decoded.fidelity).unwrap());
    }
    encoder.finish().unwrap();
    assert_eq!(
        output.last().unwrap()["response"]["usage"]["cost"],
        json!(0.125)
    );
    assert!(output[0]["response"]["usage"].get("cost").is_none());
}
