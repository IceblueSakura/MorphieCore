//! Independent wire fixtures for the selected hosted Chat adapter, not model inventory.
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract as Contract,
    protocol::openai::Profile,
    semantic::value::ReplayOrigin,
};
use serde_json::{Value, json};

fn provider() -> Adapter {
    Adapter::new(
        Profile::Chat,
        Dialect::ModelBest,
        Some(ReplayOrigin::new("synthetic-source").unwrap()),
    )
}
fn chunk(delta: Value, finish: Value) -> Value {
    json!({"id":"r","object":"chat.completion.chunk","model":"synthetic-model","created":1,"choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
}
#[test]
fn modelbest_limits_and_user_images_follow_final_ir_without_declaring_controls() {
    let client = Adapter::new(Profile::Chat, Dialect::MorphieCore, None);
    let request=client.decode_request(br#"{"model":"public","max_completion_tokens":37,"messages":[{"role":"user","content":[{"type":"text","text":"describe"},{"type":"image_url","image_url":{"url":"data:image/png;base64,AQID"}}]}]}"#).unwrap();
    let before = request.clone();
    let wire = provider()
        .encode_request(&request, "bound-model", &Contract::full())
        .unwrap();
    assert_eq!(
        wire,
        json!({"model":"bound-model","max_tokens":37,"messages":[{"role":"user","content":[{"type":"text","text":"describe"},{"type":"image_url","image_url":{"url":"data:image/png;base64,AQID"}}]}]})
    );
    assert_eq!(request, before);
    let decoded = provider()
        .decode_request(
            br#"{"model":"m","max_tokens":7,"messages":[{"role":"user","content":"hello"}]}"#,
        )
        .unwrap();
    assert_eq!(decoded.task.semantic.controls().max_output_tokens, Some(7));
    assert!(provider().decode_request(br#"{"model":"m","max_tokens":7,"max_completion_tokens":7,"messages":[{"role":"user","content":"hello"}]}"#).is_err());
    let controlled=client.decode_request(br#"{"model":"public","messages":[{"role":"user","content":"hello"}],"reasoning_effort":"low"}"#).unwrap();
    assert!(
        provider()
            .encode_request(&controlled, "bound-model", &Contract::full())
            .is_err()
    );
}
#[test]
fn modelbest_flat_reasoning_counts_and_stop_diagnostics_are_scoped_and_editable() {
    let wire = json!({"id":"r","object":"chat.completion","model":"synthetic-model","created":1,
        "choices":[{"index":0,"message":{"role":"assistant","content":"pong","reasoning_content":null,"tool_calls":null},"finish_reason":"stop","logprobs":null,"matched_stop":777}],
        "metadata":{"serving_tag":"synthetic"},
        "usage":{"prompt_tokens":3,"completion_tokens":4,"total_tokens":7,"reasoning_tokens":2,"prompt_tokens_details":{"cached_tokens":0}}});
    let decoded = provider()
        .decode_response(&serde_json::to_vec(&wire).unwrap())
        .unwrap();
    assert_eq!(decoded.semantic.usage().unwrap().reasoning_tokens, Some(2));
    let client = Adapter::new(Profile::Chat, Dialect::MorphieCore, None);
    let delivered = client.encode_response(&decoded, &Contract::full()).unwrap();
    assert_eq!(
        delivered["usage"]["completion_tokens_details"]["reasoning_tokens"],
        2
    );
    assert!(delivered["usage"].get("reasoning_tokens").is_none());
    assert!(delivered["choices"][0].get("matched_stop").is_none());
    let replayed = provider()
        .encode_response(&decoded, &Contract::full())
        .unwrap();
    assert_eq!(replayed["usage"]["reasoning_tokens"], 2);
    assert!(replayed["usage"].get("completion_tokens_details").is_none());
    for count in [Some(1), None] {
        let mut edited = decoded.clone();
        let mut usage = edited.semantic.usage().unwrap();
        usage.reasoning_tokens = count;
        edited.semantic = edited.semantic.with_usage(usage).unwrap();
        let encoded = provider()
            .encode_response(&edited, &Contract::full())
            .unwrap();
        assert_eq!(
            encoded["usage"].get("reasoning_tokens"),
            count.map(|value| json!(value)).as_ref()
        );
    }
    for count in [json!(-1), json!(1.5), json!("2"), json!(true)] {
        let mut invalid = wire.clone();
        invalid["usage"]["reasoning_tokens"] = count;
        assert!(
            provider()
                .decode_response(&serde_json::to_vec(&invalid).unwrap())
                .is_err()
        );
    }
    let mut conflicting = wire.clone();
    conflicting["usage"]["completion_tokens_details"] = json!({"reasoning_tokens":1});
    assert!(
        provider()
            .decode_response(&serde_json::to_vec(&conflicting).unwrap())
            .is_err()
    );
    conflicting["usage"]["completion_tokens_details"] = json!({"reasoning_tokens":2});
    assert_eq!(
        provider()
            .decode_response(&serde_json::to_vec(&conflicting).unwrap())
            .unwrap()
            .semantic,
        decoded.semantic
    );
    let mut unknown = wire.clone();
    unknown["usage"]["reasoning_tokens"] = Value::Null;
    assert_eq!(
        provider()
            .decode_response(&serde_json::to_vec(&unknown).unwrap())
            .unwrap()
            .semantic
            .usage()
            .unwrap()
            .reasoning_tokens,
        None
    );
    let standard = Adapter::new(Profile::Chat, Dialect::Standard, None);
    assert!(
        standard
            .decode_response(&serde_json::to_vec(&wire).unwrap())
            .is_err()
    );
    let mut stream = provider().event_decoder();
    for event in [
        chunk(json!({"role":"assistant","content":"pong"}), Value::Null),
        chunk(json!({}), json!("stop")),
        json!({"id":"r","object":"chat.completion.chunk","model":"synthetic-model","created":1,"choices":[],"usage":{"prompt_tokens":3,"completion_tokens":4,"total_tokens":7,"reasoning_tokens":2,"prompt_tokens_details":{"cached_tokens":0}}}),
    ] {
        stream.push(&event).unwrap();
    }
    stream.done().unwrap();
    assert_eq!(stream.materialize().unwrap().semantic, decoded.semantic);
}

#[test]
fn modelbest_inactive_server_shapes_do_not_erase_active_data_or_reported_counters() {
    let mut wire = json!({"id":"r","object":"chat.completion","model":"synthetic-model","created":1,
        "choices":[{"index":0,"message":{"role":"assistant","content":"pong","refusal":null,"reasoning":null,"annotations":null,"audio":null,"function_call":null},"finish_reason":"stop","logprobs":null,"stop_reason":null,"token_ids":null,"routed_experts":null}],
        "usage":{"prompt_tokens":8,"completion_tokens":2,"total_tokens":10,"prompt_tokens_details":{"cached_tokens":2,"created_cache_tokens":3,"multimodal_tokens":{"image":1}}},
        "prompt_logprobs":null,"prompt_token_ids":null,"prompt_text":null,"kv_transfer_params":null,"ec_transfer_params":null,"metrics":null});
    let decoded = provider()
        .decode_response(&serde_json::to_vec(&wire).unwrap())
        .unwrap();
    let usage = decoded.semantic.usage().unwrap();
    assert_eq!(usage.input_cache_write_tokens, Some(3));
    assert_eq!(usage.input_image_tokens, Some(1));
    let client = Adapter::new(Profile::Chat, Dialect::MorphieCore, None);
    let delivered = client.encode_response(&decoded, &Contract::full()).unwrap();
    assert_eq!(
        delivered["usage"]["prompt_tokens_details"],
        json!({"cached_tokens":2,"cache_write_tokens":3,"image_tokens":1})
    );
    let encoded = provider()
        .encode_response(&decoded, &Contract::full())
        .unwrap();
    assert_eq!(
        encoded["usage"]["prompt_tokens_details"],
        json!({"cached_tokens":2,"created_cache_tokens":3,"multimodal_tokens":{"image":1}})
    );
    for (write, image) in [(Some(0), None), (None, Some(0)), (None, None)] {
        let mut edited = decoded.clone();
        let mut report = edited.semantic.usage().unwrap();
        report.input_cache_write_tokens = write;
        report.input_image_tokens = image;
        edited.semantic = edited.semantic.with_usage(report).unwrap();
        let encoded = provider()
            .encode_response(&edited, &Contract::full())
            .unwrap();
        assert_eq!(
            encoded["usage"]["prompt_tokens_details"].get("created_cache_tokens"),
            write.map(|v| json!(v)).as_ref()
        );
        assert_eq!(
            encoded["usage"]["prompt_tokens_details"]
                .get("multimodal_tokens")
                .and_then(|v| v.get("image")),
            image.map(|v| json!(v)).as_ref()
        );
    }
    let standard = Adapter::new(Profile::Chat, Dialect::Standard, None);
    assert!(
        standard
            .decode_response(&serde_json::to_vec(&wire).unwrap())
            .is_err()
    );
    for path in [
        "/prompt_logprobs",
        "/prompt_token_ids",
        "/prompt_text",
        "/kv_transfer_params",
        "/ec_transfer_params",
        "/metrics",
        "/choices/0/token_ids",
        "/choices/0/stop_reason",
        "/choices/0/routed_experts",
        "/choices/0/message/annotations",
        "/choices/0/message/audio",
        "/choices/0/message/function_call",
    ] {
        let mut active = wire.clone();
        *active.pointer_mut(path).unwrap() = json!("active");
        assert!(
            provider()
                .decode_response(&serde_json::to_vec(&active).unwrap())
                .is_err(),
            "{path}"
        );
    }
    wire["usage"]["prompt_tokens_details"]["cache_write_tokens"] = json!(2);
    assert!(
        provider()
            .decode_response(&serde_json::to_vec(&wire).unwrap())
            .is_err()
    );
    wire["usage"]["prompt_tokens_details"]
        .as_object_mut()
        .unwrap()
        .remove("cache_write_tokens");
    wire["usage"]["prompt_tokens_details"]["multimodal_tokens"] = Value::Null;
    assert_eq!(
        provider()
            .decode_response(&serde_json::to_vec(&wire).unwrap())
            .unwrap()
            .semantic
            .usage()
            .unwrap()
            .input_image_tokens,
        None
    );
}
#[test]
fn modelbest_created_drift_is_not_identity_or_fingerprint_drift() {
    let first = chunk(json!({"role":"assistant","content":"po"}), Value::Null);
    let mut next = chunk(json!({"content":"ng"}), Value::Null);
    next["created"] = json!(2);
    let mut stream = provider().event_decoder();
    stream.push(&first).unwrap();
    stream.push(&next).unwrap();
    stream.push(&chunk(json!({}), json!("stop"))).unwrap();
    stream.done().unwrap();
    assert_eq!(
        stream.metadata().unwrap().created,
        json!(1).as_number().unwrap().clone()
    );
    for field in ["id", "model", "system_fingerprint"] {
        let mut before = first.clone();
        before["system_fingerprint"] = json!("fixed");
        let mut after = next.clone();
        after["system_fingerprint"] = json!("fixed");
        after[field] = json!("different");
        let mut rejected = provider().event_decoder();
        rejected.push(&before).unwrap();
        assert!(rejected.push(&after).is_err());
    }
}
#[test]
fn modelbest_readable_reasoning_static_and_events_agree_without_terminal_repair() {
    let wire = json!({"id":"r","object":"chat.completion","model":"synthetic-model","created":1,
        "choices":[{"index":0,"message":{"role":"assistant","content":"pong","reasoning":"think"},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":3,"completion_tokens":4,"total_tokens":7}});
    let decoded = provider()
        .decode_response(&serde_json::to_vec(&wire).unwrap())
        .unwrap();
    let client = Adapter::new(
        Profile::Chat,
        Dialect::MorphieCore,
        Some(ReplayOrigin::new("synthetic-source").unwrap()),
    );
    let delivered = client.encode_response(&decoded, &Contract::full()).unwrap();
    assert_eq!(
        delivered["choices"][0]["message"]["reasoning_content"],
        "think"
    );
    assert_eq!(delivered["choices"][0]["message"]["content"], "pong");
    let mut stream = provider().event_decoder();
    for event in [
        chunk(json!({"role":"assistant","reasoning":"think"}), Value::Null),
        chunk(json!({"content":"pong"}), Value::Null),
        chunk(json!({}), json!("stop")),
        json!({"id":"r","object":"chat.completion.chunk","model":"synthetic-model","created":1,"choices":[],"usage":{"prompt_tokens":3,"completion_tokens":4,"total_tokens":7}}),
    ] {
        stream.push(&event).unwrap();
    }
    stream.done().unwrap();
    assert_eq!(stream.materialize().unwrap().semantic, decoded.semantic);
    let mut truncated = provider().event_decoder();
    truncated
        .push(&chunk(
            json!({"role":"assistant","content":"pong"}),
            Value::Null,
        ))
        .unwrap();
    assert!(truncated.finish().is_err());
    let mut conflicting = wire.clone();
    conflicting["choices"][0]["message"]["reasoning_content"] = json!("different");
    assert!(
        provider()
            .decode_response(&serde_json::to_vec(&conflicting).unwrap())
            .is_err()
    );
    let standard = Adapter::new(Profile::Chat, Dialect::Standard, None);
    assert!(
        standard
            .decode_response(&serde_json::to_vec(&wire).unwrap())
            .is_err()
    );
}
