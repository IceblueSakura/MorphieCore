//! Independent adapter contracts: normalized semantics never fork by vendor.
use morphiecore::{
    protocol::openai::{Adapter, Dialect, Profile, TargetCaps as Contract},
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
