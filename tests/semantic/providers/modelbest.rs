//! Independent wire fixtures for the selected hosted Chat adapter, not model inventory.
use morphiecore::{
    protocol::openai::{Dialect, GenerationApi, GenerationCodec, TargetContract as Contract},
    semantic::value::ReplayOrigin,
};
use serde_json::{Value, json};

fn provider() -> GenerationCodec {
    GenerationCodec::new(
        GenerationApi::Chat,
        Dialect::ModelBest,
        Some(ReplayOrigin::new("synthetic-source").unwrap()),
    )
}
fn chunk(delta: Value, finish: Value) -> Value {
    json!({"id":"r","object":"chat.completion.chunk","model":"synthetic-model","created":1,"choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
}
#[test]
fn modelbest_thinking_switch_preserves_standard_controls_and_absence() {
    let client = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None);
    for (effort, enabled) in [
        (Some("high"), Some(true)),
        (Some("none"), Some(false)),
        (None, None),
    ] {
        let mut wire = json!({"model":"m","messages":[{"role":"user","content":"hello"}]});
        if let Some(effort) = effort {
            wire["reasoning_effort"] = json!(effort);
        }
        let source = client
            .decode_request(&serde_json::to_vec(&wire).unwrap())
            .unwrap();
        let before = source.clone();
        let encoded = provider()
            .encode_request(&source, "m", &Contract::full())
            .unwrap();
        assert_eq!(
            encoded.get("chat_template_kwargs"),
            enabled
                .map(|value| json!({"enable_thinking":value}))
                .as_ref()
        );
        assert_eq!(
            encoded.get("reasoning_effort"),
            if effort == Some("none") {
                None
            } else {
                wire.get("reasoning_effort")
            }
        );
        assert!(encoded.get("enable_thinking").is_none());
        assert_eq!(source, before);
    }
    // Public extension fields cannot choose the trusted native carrier.
    let wire = json!({"model":"m","messages":[{"role":"user","content":"hello"}],
        "chat_template_kwargs":{"enable_thinking":true}});
    let source = client
        .decode_client_request(&serde_json::to_vec(&wire).unwrap())
        .unwrap();
    assert!(
        provider()
            .encode_request(&source, "m", &Contract::full())
            .unwrap()
            .get("chat_template_kwargs")
            .is_none()
    );
}

#[test]
fn modelbest_native_template_thinking_has_a_typed_owner() {
    // Hosted guide: OpenBMB/MiniCPM-V@4dbbb6c58b6f8a004d6689028417296719f0831a/docs/api.md.
    use morphiecore::semantic::task::generation::ReasoningActivation;
    for (enabled, activation) in [
        (true, ReasoningActivation::Enabled),
        (false, ReasoningActivation::Disabled),
    ] {
        let wire = json!({"model":"m","messages":[{"role":"user","content":"hello"}],
            "chat_template_kwargs":{"enable_thinking":enabled}});
        let source = provider()
            .decode_request(&serde_json::to_vec(&wire).unwrap())
            .unwrap();
        assert_eq!(
            source.task.semantic.reasoning().activation,
            Some(activation)
        );
        assert_eq!(
            provider()
                .encode_request(&source, "m", &Contract::full())
                .unwrap(),
            wire
        );
    }
    for value in [Value::Null, json!("true"), json!(1), json!({})] {
        let wire = json!({"model":"m","messages":[{"role":"user","content":"hello"}],
            "chat_template_kwargs":{"enable_thinking":value}});
        assert!(
            provider()
                .decode_request(&serde_json::to_vec(&wire).unwrap())
                .is_err()
        );
    }
    for (effort, enabled) in [("none", true), ("high", false)] {
        let wire = json!({"model":"m","messages":[{"role":"user","content":"hello"}],
            "reasoning_effort":effort,"chat_template_kwargs":{"enable_thinking":enabled}});
        assert!(
            provider()
                .decode_request(&serde_json::to_vec(&wire).unwrap())
                .is_err()
        );
    }
}

#[test]
fn modelbest_tool_history_uses_the_hosted_models_reasoning_field() {
    let client = GenerationCodec::new(GenerationApi::Chat, Dialect::MorphieCore, None);
    let source = client.decode_request(&serde_json::to_vec(&json!({
        "model":"m","reasoning_effort":"medium","messages":[
            {"role":"user","content":"look up cedar"},
            {"role":"assistant","reasoning_content":"thought","content":null,
             "tool_calls":[{"id":"call","type":"function","function":{"name":"lookup","arguments":"{}"}}]},
            {"role":"tool","tool_call_id":"call","content":"Cedar-73"}]
    })).unwrap()).unwrap();
    for (id, field) in [
        ("minicpm5-1b", "reasoning_content"),
        ("MiniCPM-V-4.7-1B", "reasoning"),
    ] {
        let binding = morphiecore::topology::catalog::API_KEY_BINDINGS
            .iter()
            .find(|binding| binding.model == id)
            .unwrap();
        let endpoint = binding
            .endpoint(morphiecore::topology::GenerationProtocol::OpenAiChat)
            .unwrap();
        let (wire, _) = endpoint.encode_request_with_rules(&source).unwrap();
        assert_eq!(wire["messages"][1][field], "thought");
        assert_eq!(wire["messages"][1]["tool_calls"][0]["id"], "call");
        assert_eq!(wire["messages"][2]["tool_call_id"], "call");
        assert_eq!(wire["chat_template_kwargs"]["enable_thinking"], true);
    }
}

#[test]
fn modelbest_reasoning_aliases_have_independent_json_and_stream_delivery() {
    let client = GenerationCodec::new(GenerationApi::Chat, Dialect::MorphieCore, None);
    for field in ["reasoning", "reasoning_content"] {
        let mut message = json!({"role":"assistant","content":"answer"});
        message[field] = json!("thought");
        let wire = json!({"id":"r","object":"chat.completion","model":"synthetic-model","created":1,
            "choices":[{"index":0,"message":message,"finish_reason":"stop"}]});
        let decoded = provider()
            .decode_response(&serde_json::to_vec(&wire).unwrap())
            .unwrap();
        let delivered = client.encode_response(&decoded, &Contract::full()).unwrap();
        assert_eq!(
            delivered["choices"][0]["message"]["reasoning_content"],
            "thought"
        );
        assert_eq!(delivered["choices"][0]["message"]["content"], "answer");
        let mut stream = provider().event_decoder();
        for delta in ["th", "ought"] {
            let mut body = json!({});
            body[field] = json!(delta);
            stream.push(&chunk(body, Value::Null)).unwrap();
        }
        stream
            .push(&chunk(json!({"content":"answer"}), json!("stop")))
            .unwrap();
        stream.done().unwrap();
        assert_eq!(stream.materialize().unwrap().semantic, decoded.semantic);
    }
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
    let client = GenerationCodec::new(GenerationApi::Chat, Dialect::MorphieCore, None);
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
        edited.semantic = edited.semantic.with_usage(Some(usage)).unwrap();
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
    let client = GenerationCodec::new(GenerationApi::Chat, Dialect::MorphieCore, None);
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
        edited.semantic = edited.semantic.with_usage(Some(report)).unwrap();
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
