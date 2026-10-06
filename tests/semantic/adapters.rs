//! Independent adapter contracts: normalized semantics never fork by vendor.
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract as Contract,
    protocol::openai::{Profile, events::EventEncoder},
    semantic::{task::generation::StreamEvent, value::ReplayOrigin},
};
use serde_json::{Value, json};

fn adapter(profile: Profile, dialect: Dialect, scope: &str) -> Adapter {
    Adapter::new(profile, dialect, Some(ReplayOrigin::new(scope).unwrap()))
}
fn response() -> Value {
    json!({"id":"r1","object":"response","created_at":2,"status":"completed","model":"m",
        "output":[{"id":"m1","type":"message","role":"assistant","status":"completed",
            "content":[{"type":"output_text","text":"pong","annotations":[]}]}],
        "usage":{"input_tokens":3,"output_tokens":2,"total_tokens":5}})
}
#[test]
fn forced_upstream_stream_preserves_client_delivery_and_final_controls() {
    let client = adapter(Profile::Responses, Dialect::MorphieCore, "a");
    let provider = adapter(Profile::Responses, Dialect::Codex, "a");
    for streaming in [false, true] {
        let mut request = client.decode_request(
            json!({"model":"client-label","input":"hello","stream":streaming,"max_output_tokens":37})
                .to_string().as_bytes(),
        ).unwrap();
        let wire = provider
            .encode_request(&request, "upstream-label", &Contract::full())
            .unwrap();
        assert_eq!(wire["model"], "upstream-label");
        assert_eq!(wire["stream"], true);
        assert_eq!(wire["store"], false);
        assert_eq!(wire["instructions"], "");
        assert_eq!(wire["max_output_tokens"], 37);
        assert_eq!(request.delivery.streaming(), streaming);
        assert_eq!(
            client
                .encode_request(&request, "client-label", &Contract::full())
                .unwrap()["stream"],
            streaming
        );
        for cap in [Some(11), None] {
            let mut settings = request.task.semantic.settings().clone();
            settings.controls.max_output_tokens = cap;
            request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
            let wire = provider
                .encode_request(&request, "upstream-label", &Contract::full())
                .unwrap();
            match cap {
                Some(cap) => assert_eq!(wire["max_output_tokens"], cap),
                None => assert!(wire.get("max_output_tokens").is_none()),
            }
        }
    }
}

#[test]
fn deepseek_default_is_scoped_and_never_overwrites_reported_or_invalid_values() {
    let deepseek = adapter(Profile::Responses, Dialect::DeepSeek, "a");
    let standard = adapter(Profile::Responses, Dialect::Standard, "a");
    for detail in [None, Some(Value::Null), Some(json!(0)), Some(json!(2))] {
        let mut body = response();
        if let Some(value) = &detail {
            body["usage"]["input_tokens_details"] = json!({"cache_write_tokens":value});
        }
        let decoded = deepseek
            .decode_response(body.to_string().as_bytes())
            .unwrap();
        let expected = detail.as_ref().and_then(Value::as_u64).unwrap_or(0);
        assert_eq!(
            decoded.semantic.usage().unwrap().input_cache_write_tokens,
            Some(expected)
        );
        assert_eq!(
            decoded.fidelity.normalizations().len(),
            usize::from(detail.as_ref().is_none_or(Value::is_null))
        );
        let output = standard
            .encode_response(&decoded, &Contract::full())
            .unwrap();
        assert_eq!(
            output["usage"]["input_tokens_details"]["cache_write_tokens"],
            expected
        );
        let chat = adapter(Profile::Chat, Dialect::Standard, "b")
            .encode_response(&decoded, &Contract::full())
            .unwrap();
        assert_eq!(
            chat["usage"]["prompt_tokens_details"]["cache_write_tokens"],
            expected
        );
        let plain = standard
            .decode_response(body.to_string().as_bytes())
            .unwrap();
        assert_eq!(
            plain.semantic.usage().unwrap().input_cache_write_tokens,
            detail.as_ref().and_then(Value::as_u64)
        );
    }
    for invalid in [json!(-1), json!("0"), json!(false), json!(1.5)] {
        let mut body = response();
        body["usage"]["input_tokens_details"] = json!({"cache_write_tokens":invalid});
        assert!(
            deepseek
                .decode_response(body.to_string().as_bytes())
                .is_err()
        );
    }
    for usage in [None, Some(Value::Null)] {
        let mut body = response();
        body.as_object_mut().unwrap().remove("usage");
        if let Some(value) = usage {
            body["usage"] = value;
        }
        let decoded = deepseek
            .decode_response(body.to_string().as_bytes())
            .unwrap();
        assert!(decoded.semantic.usage().is_none());
        assert!(decoded.fidelity.normalizations().is_empty());
    }
}
#[test]
fn extras_require_matching_source_and_surviving_response_dependencies() {
    let source = adapter(Profile::Responses, Dialect::DeepSeek, "a");
    let other = adapter(Profile::Responses, Dialect::DeepSeek, "b");
    let mut body = response();
    body["content_filters"] = json!({"result":"final"});
    let mut decoded = source.decode_response(body.to_string().as_bytes()).unwrap();
    assert_eq!(
        source.encode_response(&decoded, &Contract::full()).unwrap()["content_filters"],
        body["content_filters"]
    );
    assert!(
        other
            .encode_response(&decoded, &Contract::full())
            .unwrap()
            .get("content_filters")
            .is_none()
    );
    let original_id = decoded.metadata.id.clone();
    decoded.metadata.id = "different-response".into();
    assert!(
        source
            .encode_response(&decoded, &Contract::full())
            .unwrap()
            .get("content_filters")
            .is_none(),
        "response-wide records cannot attach to another response identity"
    );
    decoded.metadata.id = original_id;
    let standard = adapter(Profile::Responses, Dialect::Standard, "a");
    assert!(
        standard
            .decode_response(body.to_string().as_bytes())
            .is_err()
    );
    assert!(
        standard
            .encode_response(&decoded, &Contract::full())
            .unwrap()
            .get("content_filters")
            .is_none()
    );
    decoded.semantic = morphiecore::semantic::task::generation::GenerationResponse::new(
        vec![],
        morphiecore::semantic::task::generation::Outcome::Completed,
    )
    .unwrap();
    let output = source.encode_response(&decoded, &Contract::full()).unwrap();
    assert!(output.get("content_filters").is_none());
    assert!(
        output["usage"].is_null(),
        "normalization records never restore deleted usage"
    );
}
#[test]
fn stream_terminal_extras_and_defaults_match_static_without_early_binding() {
    let source = adapter(Profile::Responses, Dialect::DeepSeek, "a");
    let mut frames = crate::wire::events(2);
    for frame in &mut frames {
        if let Some(snapshot) = frame.get_mut("response") {
            snapshot["content_filters"] = Value::Null;
            if let Some(details) = snapshot
                .pointer_mut("/usage/input_tokens_details")
                .and_then(Value::as_object_mut)
            {
                details.remove("cache_write_tokens");
            }
        }
    }
    frames.last_mut().unwrap()["response"]["content_filters"] = json!({"result":"final"});
    let mut decoder = source.event_decoder();
    let mut events = vec![];
    for frame in &frames {
        events.extend(decoder.push(frame).unwrap());
    }
    decoder.finish().unwrap();
    let decoded = decoder.materialize().unwrap();
    assert_eq!(
        decoded.semantic.usage().unwrap().input_cache_write_tokens,
        Some(0)
    );
    let expected = source.encode_response(&decoded, &Contract::full()).unwrap();
    let mut encoder = EventEncoder::new(Profile::Responses, decoded.metadata.clone())
        .unwrap()
        .with_contract(source.contract(&Contract::full()));
    let mut output = vec![];
    for event in &events {
        output.extend(encoder.encode(event, &decoded.fidelity).unwrap());
    }
    encoder.finish().unwrap();
    assert!(
        output[0]["response"].get("content_filters").is_none(),
        "terminal facts must not leak into early snapshots"
    );
    assert_eq!(output.last().unwrap()["response"], expected);
    assert!(matches!(events.last(), Some(StreamEvent::Terminal { .. })));
}
#[test]
fn chat_cache_write_projection_and_null_fingerprint_are_preserved() {
    let source = adapter(Profile::Chat, Dialect::DeepSeek, "a");
    let target = adapter(Profile::Chat, Dialect::Standard, "b");
    let body = json!({"id":"c1","object":"chat.completion","created":2,"model":"m","system_fingerprint":null,
        "choices":[{"index":0,"message":{"role":"assistant","content":"pong"},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}});
    let decoded = source.decode_response(body.to_string().as_bytes()).unwrap();
    let encoded = target.encode_response(&decoded, &Contract::full()).unwrap();
    assert!(
        encoded
            .as_object()
            .unwrap()
            .contains_key("system_fingerprint")
    );
    assert_eq!(encoded["system_fingerprint"], Value::Null);
    assert_eq!(
        encoded["usage"]["prompt_tokens_details"]["cache_write_tokens"],
        0
    );
}
#[test]
fn chat_usage_events_normalize_only_absent_details_and_poison_invalid_reports() {
    let source = adapter(Profile::Chat, Dialect::DeepSeek, "a");
    for detail in [None, Some(Value::Null), Some(json!(2)), Some(json!("bad"))] {
        let mut decoder = source.event_decoder();
        let opening = json!({"id":"c1","object":"chat.completion.chunk","created":2,"model":"m",
            "choices":[{"index":0,"delta":{"role":"assistant","content":"pong"},"finish_reason":null}]});
        assert!(
            decoder
                .push(&opening)
                .unwrap()
                .iter()
                .all(|event| !matches!(event, StreamEvent::Usage(_)))
        );
        let mut finish = json!({"id":"c1","object":"chat.completion.chunk","created":2,"model":"m",
            "choices":[{"index":0,"delta":{},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}});
        if let Some(value) = &detail {
            finish["usage"]["prompt_tokens_details"] = json!({"cache_write_tokens":value});
        }
        let events = decoder.push(&finish);
        if detail == Some(json!("bad")) {
            assert!(events.is_err());
            assert!(decoder.done().is_err());
            continue;
        }
        let expected = detail.as_ref().and_then(Value::as_u64).unwrap_or(0);
        assert!(events.unwrap().iter().any(|event| matches!(event, StreamEvent::Usage(u) if u.input_cache_write_tokens==Some(expected))));
        decoder.done().unwrap();
        let decoded = decoder.materialize().unwrap();
        assert_eq!(
            decoded.semantic.usage().unwrap().input_cache_write_tokens,
            Some(expected)
        );
    }
}

#[test]
fn truncated_reasoning_usage_cannot_exceed_reported_output_or_recover_a_stream() {
    let source = adapter(Profile::Chat, Dialect::Xiaomi, "a");
    for reasoning in [8, 12] {
        let usage = json!({"prompt_tokens":22,"completion_tokens":8,"total_tokens":30,
            "completion_tokens_details":{"reasoning_tokens":reasoning},
            "prompt_tokens_details":{"cached_tokens":0}});
        let body = json!({"id":"c1","object":"chat.completion","created":2,"model":"m",
            "choices":[{"index":0,"message":{"role":"assistant","content":"","reasoning_content":"synthetic partial thought","tool_calls":null},"finish_reason":"length"}],
            "usage":usage});
        let decoded = source.decode_response(body.to_string().as_bytes());
        assert_eq!(decoded.is_ok(), reasoning == 8);
        let mut decoder = source.event_decoder();
        decoder.push(&json!({"id":"c1","object":"chat.completion.chunk","created":2,"model":"m",
            "choices":[{"index":0,"delta":{"role":"assistant","reasoning_content":"synthetic partial thought"},"finish_reason":null}]})).unwrap();
        let terminal = json!({"id":"c1","object":"chat.completion.chunk","created":2,"model":"m",
            "choices":[{"index":0,"delta":{},"finish_reason":"length"}],"usage":usage});
        let result = decoder.push(&terminal);
        if reasoning == 8 {
            result.unwrap();
            decoder.done().unwrap();
            assert_eq!(
                decoder
                    .materialize()
                    .unwrap()
                    .semantic
                    .usage()
                    .unwrap()
                    .reasoning_tokens,
                Some(8)
            );
        } else {
            assert!(result.is_err());
            assert!(decoder.done().is_err());
            assert!(decoder.materialize().is_err());
        }
    }
}

#[test]
fn chat_projection_cannot_attach_reasoning_to_a_user_message() {
    let responses = adapter(Profile::Responses, Dialect::Standard, "a");
    let request = responses.decode_request(json!({"model":"m","input":[
        {"type":"reasoning","id":"rs1","summary":[],"content":[{"type":"reasoning_text","text":"think"}]},
        {"role":"user","content":"hello"}
    ]}).to_string().as_bytes()).unwrap();
    let chat = adapter(Profile::Chat, Dialect::MorphieCore, "b");
    assert!(
        chat.encode_request(&request, "m", &Contract::full())
            .is_err()
    );
}

#[test]
fn extras_cannot_be_captured_unbound_or_over_budget() {
    let mut body = response();
    body["content_filters"] = Value::Null;
    let unbound = Adapter::new(Profile::Responses, Dialect::DeepSeek, None);
    assert!(
        unbound
            .decode_response(body.to_string().as_bytes())
            .is_err()
    );
    let source = adapter(Profile::Responses, Dialect::DeepSeek, "a");
    let decoded = source.decode_response(body.to_string().as_bytes()).unwrap();
    assert!(
        unbound
            .encode_response(&decoded, &Contract::full())
            .unwrap()
            .get("content_filters")
            .is_none()
    );
    body["content_filters"] = json!("x".repeat(4097));
    assert!(source.decode_response(body.to_string().as_bytes()).is_err());
}

#[test]
fn standard_chat_does_not_inherit_vendor_reasoning_or_usage_aliases() {
    let standard = adapter(Profile::Chat, Dialect::Standard, "a");
    let vendor = adapter(Profile::Chat, Dialect::DeepSeek, "a");
    let mut body = json!({"id":"c1","object":"chat.completion","created":2,"model":"m",
        "choices":[{"index":0,"message":{"role":"assistant","content":"pong","reasoning_content":"think"},"finish_reason":"stop"}]});
    assert!(
        standard
            .decode_response(body.to_string().as_bytes())
            .is_err()
    );
    assert!(vendor.decode_response(body.to_string().as_bytes()).is_ok());
    body["choices"][0]["message"]
        .as_object_mut()
        .unwrap()
        .remove("reasoning_content");
    body["usage"] = json!({"prompt_tokens":3,"completion_tokens":2,"total_tokens":5,"prompt_cache_hit_tokens":1,"prompt_cache_miss_tokens":2});
    assert!(
        standard
            .decode_response(body.to_string().as_bytes())
            .is_err()
    );
    assert_eq!(
        vendor
            .decode_response(body.to_string().as_bytes())
            .unwrap()
            .semantic
            .usage()
            .unwrap()
            .cached_input_tokens,
        Some(1)
    );
}
