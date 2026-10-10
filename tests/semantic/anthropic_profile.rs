//! Independent native wire shapes; no registry, credentials or semantic round-trip oracle.
use morphiecore::protocol::{CodecError, anthropic::Profile};
use serde_json::{Value, json};

fn request() -> Value {
    json!({"model":"synthetic-model","max_tokens":512,"system":"Use lookup.",
        "tools":[{"name":"lookup","input_schema":{"type":"object",
            "properties":{"key":{"type":"string","enum":["A","B"]}},
            "required":["key"],"additionalProperties":false},"strict":false}],
        "tool_choice":{"type":"auto"},"messages":[{"role":"user","content":"Look up A."}]})
}
fn message() -> Value {
    json!({"id":"msg_synthetic","type":"message","role":"assistant","model":"synthetic-model",
        "content":[{"type":"thinking","thinking":"","signature":"synthetic-signature"},
            {"type":"tool_use","id":"toolu_1","name":"lookup","input":{"key":"A"}}],
        "stop_reason":"tool_use","stop_sequence":null,
        "usage":{"input_tokens":11,"output_tokens":7}})
}
#[test]
fn independently_constructed_native_values_encode_without_an_openai_dto() {
    use morphiecore::{
        protocol::anthropic::{Block, Message, StopReason, Usage},
        semantic::{task::generation::StructuredValue, value::Presence},
    };
    let p = Profile::AdaptiveTextTools;
    let typed = Message {
        id: "msg_synthetic".into(),
        model: "synthetic-model".into(),
        content: vec![
            Block::Thinking {
                thinking: String::new(),
                signature: "synthetic-signature".into(),
            },
            Block::ToolUse {
                id: "toolu_1".into(),
                name: "lookup".into(),
                input: StructuredValue::new(json!({"key":"A"})).unwrap(),
                caller: Presence::Absent,
            },
        ],
        stop_reason: Presence::Value(StopReason::ToolUse),
        stop_sequence: Presence::Null,
        usage: Usage {
            input_tokens: Presence::Value(11),
            output_tokens: 7,
            cache_read_input_tokens: Presence::Absent,
            cache_creation_input_tokens: Presence::Absent,
        },
    };
    assert_eq!(p.encode_response(&typed).unwrap(), message());
    let metadata = typed.reported_metadata().unwrap();
    metadata.validate().unwrap();
    assert_eq!(metadata.created, None);
    let decoded = p.decode_response(message().to_string().as_bytes()).unwrap();
    assert_eq!(decoded.stop_reason, Presence::Value(StopReason::ToolUse));
    assert_eq!(decoded.usage.input_tokens, Presence::Value(11));
    assert!(
        matches!(&decoded.content[0], Block::Thinking { thinking, signature }
        if thinking.is_empty() && signature == "synthetic-signature")
    );
}
#[test]
fn typed_native_shapes_preserve_schema_presence_and_signature_only() {
    let p = Profile::AdaptiveTextTools;
    let decoded = p
        .decode_request(&request().to_string().into_bytes())
        .unwrap();
    assert_eq!(p.encode_request(&decoded).unwrap(), request());
    let decoded = p
        .decode_response(&message().to_string().into_bytes())
        .unwrap();
    assert_eq!(p.encode_response(&decoded).unwrap(), message());
    assert!(!format!("{decoded:?}").contains("synthetic-signature"));
    let mut unknown = message();
    unknown["created_at"] = json!(0);
    assert!(p.decode_response(unknown.to_string().as_bytes()).is_err());
    let mut edited = p.decode_request(request().to_string().as_bytes()).unwrap();
    edited.max_tokens = 0;
    assert!(p.encode_request(&edited).is_err());
}
#[test]
fn native_non_strict_tools_do_not_rewrite_or_drop_constraints() {
    let p = Profile::AdaptiveTextTools;
    for (key, value) in [
        ("temperature", json!(0.5)),
        ("thinking", json!({"type":"enabled","budget_tokens":1024})),
        ("unknown", json!(false)),
        ("max_tokens", Value::Null),
        ("stream", Value::Null),
    ] {
        let mut body = request();
        body[key] = value;
        assert!(
            p.decode_request(body.to_string().as_bytes()).is_err(),
            "{key}"
        );
    }
    for strict in [Value::Null, json!(true)] {
        let mut body = request();
        body["tools"][0]["strict"] = strict;
        assert!(p.decode_request(body.to_string().as_bytes()).is_err());
    }
    let mut body = request();
    body["tools"][0]
        .as_object_mut()
        .unwrap()
        .remove("input_schema");
    assert!(p.decode_request(body.to_string().as_bytes()).is_err());
    let mut body = request();
    body["thinking"] = json!({"type":"adaptive","display":"omitted"});
    body["output_config"] = json!({"effort":"medium"});
    body["stream"] = json!(false);
    assert_eq!(
        p.encode_request(&p.decode_request(body.to_string().as_bytes()).unwrap())
            .unwrap(),
        body
    );
    for content in [json!(""), json!([]), json!([{"type":"text","text":""}])] {
        let mut body = request();
        body["messages"][0]["content"] = content;
        assert!(p.decode_request(body.to_string().as_bytes()).is_err());
    }
}
#[test]
fn native_wire_history_preserves_error_presence_and_rejects_bad_arrangements() {
    let p = Profile::AdaptiveTextTools;
    for marker in [None, Some(json!(false)), Some(json!(true))] {
        let mut body = request();
        let mut result = json!({"type":"tool_result","tool_use_id":"toolu_1","content":""});
        if let Some(marker) = marker {
            result["is_error"] = marker;
        }
        body["messages"] = json!([
            {"role":"user","content":"Look up A."},
            {"role":"assistant","content":message()["content"]},
            {"role":"user","content":[result]}]);
        assert_eq!(
            p.encode_request(&p.decode_request(body.to_string().as_bytes()).unwrap())
                .unwrap(),
            body
        );
        body["messages"][2]["content"][0]["tool_use_id"] = json!("other");
        assert!(p.decode_request(body.to_string().as_bytes()).is_err());
    }
    let mut body = request();
    body["messages"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"assistant","content":message()["content"]}));
    assert!(p.decode_request(body.to_string().as_bytes()).is_err());
}
#[test]
fn raw_native_json_rejects_duplicates_and_retains_exact_object_values() {
    let p = Profile::AdaptiveTextTools;
    assert_eq!(
        p.decode_request(br#"{"model":"a","model":"b"}"#).err(),
        Some(CodecError::Invalid("JSON"))
    );
    let raw = br#"{"id":"msg","type":"message","role":"assistant","model":"synthetic","content":[{"type":"tool_use","id":"c","name":"lookup","input":{"n":9007199254740993.125,"nested":{"$serde_json::private::Number":"1"}}}],"stop_reason":"tool_use","stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":2}}"#;
    let wire = p.encode_response(&p.decode_response(raw).unwrap()).unwrap();
    assert_eq!(
        wire["content"][0]["input"]["n"].to_string(),
        "9007199254740993.125"
    );
    assert!(wire["content"][0]["input"]["nested"].is_object());
    let malformed = String::from_utf8(raw.to_vec())
        .unwrap()
        .replace("\"n\":9007199254740993.125", "\"n\":0,\"n\":1");
    assert_eq!(
        p.decode_response(malformed.as_bytes()).err(),
        Some(CodecError::Invalid("JSON"))
    );
}
#[test]
fn individual_native_event_shapes_do_not_claim_cross_event_closure() {
    let p = Profile::AdaptiveTextTools;
    let mut opening = message();
    opening["content"] = json!([]);
    opening["stop_reason"] = Value::Null;
    let events = [
        json!({"type":"message_start","message":opening}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"c","name":"lookup","input":{}}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"key\":"}}),
        json!({"type":"content_block_delta","index":1,"delta":{"type":"signature_delta","signature":"synthetic-fragment"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{"output_tokens":9}}),
        json!({"type":"message_stop"}),
        json!({"type":"ping"}),
        json!({"type":"error","error":{"type":"overloaded_error","message":"synthetic failure"}}),
    ];
    for wire in events {
        let name = wire["type"].as_str().unwrap();
        let event = p.decode_event(name, wire.to_string().as_bytes()).unwrap();
        assert_eq!(p.encode_event(&event).unwrap(), wire);
        assert!(
            p.decode_event("not-the-event", wire.to_string().as_bytes())
                .is_err()
        );
    }
    for wire in [
        json!({"type":"content_block_delta","index":1024,"delta":{"type":"text_delta","text":"x"}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"citations_delta","citation":{}}}),
        json!({"type":"message_stop","extra":false}),
    ] {
        assert!(
            p.decode_event(wire["type"].as_str().unwrap(), wire.to_string().as_bytes())
                .is_err()
        );
    }
    let mut bad = message();
    bad["stop_reason"] = json!("refusal");
    assert!(p.decode_response(bad.to_string().as_bytes()).is_err());
}
#[test]
fn native_typed_edits_cannot_escape_local_resource_limits() {
    use morphiecore::{
        protocol::anthropic::Block,
        semantic::{task::generation::StructuredValue, value::Presence},
    };
    let p = Profile::AdaptiveTextTools;
    let mut req = p.decode_request(request().to_string().as_bytes()).unwrap();
    req.messages = vec![req.messages[0].clone(); 1025];
    assert!(matches!(p.encode_request(&req), Err(CodecError::Limit)));
    let mut req = p.decode_request(request().to_string().as_bytes()).unwrap();
    req.system = morphiecore::semantic::value::Presence::Value(
        morphiecore::protocol::anthropic::TextContent::Text("x".repeat((1 << 20) + 1)),
    );
    assert!(matches!(p.encode_request(&req), Err(CodecError::Limit)));
    let mut response = p.decode_response(message().to_string().as_bytes()).unwrap();
    let input = StructuredValue::new(json!({"data":vec![Value::Null;4096]})).unwrap();
    response.content = (0..16)
        .map(|index| Block::ToolUse {
            id: format!("c_{index}"),
            name: "lookup".into(),
            input: input.clone(),
            caller: Presence::Absent,
        })
        .collect();
    assert!(matches!(
        p.encode_response(&response),
        Err(CodecError::Limit)
    ));
}
