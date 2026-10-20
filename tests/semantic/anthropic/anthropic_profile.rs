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
/// Strict tools follow the native structured-output subset and request-wide grammar
/// budget; nothing is simplified to fit.
#[test]
fn strict_tool_schemas_admit_only_the_native_subset_and_budget() {
    let p = Profile::AdaptiveTextTools;
    let encode = |tools: Vec<Value>, output: Option<Value>| {
        let mut body = request();
        body["tools"] = json!(tools);
        if let Some(schema) = output {
            body["output_config"] = json!({"format":{"type":"json_schema","schema":schema}});
        }
        let decoded = p.decode_request(body.to_string().as_bytes()).unwrap();
        p.encode_request(&decoded)
    };
    let tool = |schema: Value| json!({"name":"lookup","input_schema":schema,"strict":true});
    let object = |properties: Value, required: Vec<String>| json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
    let admitted = |result: Result<Value, CodecError>| result.is_ok();
    let rejected = |result: Result<Value, CodecError>| matches!(result, Err(CodecError::Unsupported(r)) if r == "Anthropic strict tool schema");
    let rich = object(
        json!({
            "email":{"type":["string","null"],"format":"email"},
            "count":{"anyOf":[{"type":"integer"},{"type":"null"}]},
            "tags":{"type":"array","items":{"type":"number"},"minItems":1}
        }),
        vec!["email".into(), "count".into(), "tags".into()],
    );
    assert!(admitted(encode(vec![tool(rich.clone())], None)));
    for (path, value) in [
        ("/properties/email/format", json!("binary")),
        ("/properties/email/type", json!([])),
        ("/properties/email/pattern", json!("^a")),
        ("/properties/email/enum", json!(["a"])),
        ("/properties/count/anyOf", json!([])),
        ("/properties/tags/minItems", json!(2)),
        ("/properties/tags/items", json!({"$ref":"#/$defs/n"})),
        ("/additionalProperties", json!(true)),
    ] {
        let mut schema = rich.clone();
        let (parent, key) = path.rsplit_once('/').unwrap();
        let target = if parent.is_empty() {
            &mut schema
        } else {
            schema.pointer_mut(parent).unwrap()
        };
        target[key] = value;
        assert!(rejected(encode(vec![tool(schema)], None)), "{path}");
    }

    let optional = |n: usize| {
        object(
            (0..n)
                .map(|i| (format!("p{i}"), json!({"type":"string"})))
                .collect::<serde_json::Map<_, _>>()
                .into(),
            vec![],
        )
    };
    assert!(admitted(encode(vec![tool(optional(24))], None)));
    assert!(rejected(encode(vec![tool(optional(25))], None)));
    // The budget is request-wide: output schemas count beside strict tools.
    assert!(rejected(encode(
        vec![tool(optional(20))],
        Some(optional(5))
    )));
    let unions = |n: usize| {
        let names: Vec<String> = (0..n).map(|i| format!("u{i}")).collect();
        object(
            names
                .iter()
                .map(|name| (name.clone(), json!({"type":["string","null"]})))
                .collect::<serde_json::Map<_, _>>()
                .into(),
            names,
        )
    };
    assert!(admitted(encode(vec![tool(unions(16))], None)));
    assert!(rejected(encode(vec![tool(unions(17))], None)));
    let many = |n: usize| {
        (0..n)
                .map(|i| {
                    json!({"name":format!("t{i}"),"input_schema":object(json!({}), vec![]),"strict":true})
                })
                .collect::<Vec<_>>()
    };
    assert!(admitted(encode(many(20), None)));
    assert!(rejected(encode(many(21), None)));
}
#[test]
fn audit_consecutive_messages_share_tool_turn_validation_without_reordering() {
    let p = Profile::AdaptiveTextTools;
    let mut body = request();
    body["messages"] = json!([
        {"role":"user","content":"go"},
        {"role":"assistant","content":[{"type":"tool_use","id":"c1","name":"lookup","input":{}}]},
        {"role":"assistant","content":[{"type":"tool_use","id":"c2","name":"lookup","input":{}}]},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":"c1","content":"one"}]},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":"c2","content":"two"},{"type":"text","text":"Continue."}]}
    ]);
    let decoded = p.decode_request(body.to_string().as_bytes()).unwrap();
    assert_eq!(
        p.encode_request(&decoded).unwrap()["messages"],
        json!([
            {"role":"user","content":"go"},
            {"role":"assistant","content":[{"type":"tool_use","id":"c1","name":"lookup","input":{}}]},
            {"role":"assistant","content":[{"type":"tool_use","id":"c2","name":"lookup","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"c1","content":"one"}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"c2","content":"two"},{"type":"text","text":"Continue."}]}
        ])
    );
    let mut missing = body.clone();
    missing["messages"].as_array_mut().unwrap().remove(4);
    assert!(matches!(
        p.decode_request(missing.to_string().as_bytes()),
        Err(CodecError::Invalid("Anthropic pending results"))
    ));
    body["messages"][3]["content"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"text","text":"Too early."}));
    assert!(matches!(
        p.decode_request(body.to_string().as_bytes()),
        Err(CodecError::Invalid("Anthropic result reference"))
    ));
}
#[test]
fn audit_protected_client_controls_are_not_discarded() {
    use morphiecore::protocol::anthropic::client::decode_client_request;
    let base = json!({"model":"m","max_tokens":8,"messages":[{"role":"user","content":"go"}]});
    assert!(decode_client_request(base.to_string().as_bytes()).is_ok());
    for (key, value) in [
        ("inference_geo", json!("us")),
        ("speed", json!("fast")),
        ("fallbacks", json!([{"model":"other"}])),
        ("fallback_credit_token", json!("synthetic")),
    ] {
        let mut body = base.clone();
        body[key] = value;
        assert!(matches!(decode_client_request(body.to_string().as_bytes()),
                Err(CodecError::Unsupported(field)) if field == key));
    }
    let mut body = base;
    body["thinking"] = json!({"type":"adaptive"});
    let plain = decode_client_request(body.to_string().as_bytes()).unwrap();
    // Block binding has a typed owner; `{}` and a null behavior keep the default.
    use morphiecore::semantic::task::generation::ReplayMismatch;
    for (binding, expected) in [
        (
            json!({"prefix_mismatch_behavior":"drop_block"}),
            Some(ReplayMismatch::Drop),
        ),
        (
            json!({"prefix_mismatch_behavior":"error"}),
            Some(ReplayMismatch::Reject),
        ),
        (json!({}), None),
        (json!({"prefix_mismatch_behavior":null}), None),
    ] {
        body["thinking"]["block_binding"] = binding;
        let decoded = decode_client_request(body.to_string().as_bytes()).unwrap();
        assert_eq!(decoded.task.semantic.reasoning().replay_mismatch, expected);
        assert!(decoded.task.fidelity.normalizations().is_empty());
    }
    assert_eq!(plain.task.semantic.reasoning().replay_mismatch, None);
    body["thinking"]["block_binding"] = json!({"prefix_mismatch_behavior":"ignore"});
    assert!(matches!(
        decode_client_request(body.to_string().as_bytes()),
        Err(CodecError::Invalid("thinking.block_binding"))
    ));
    // The check exists only for adaptive thinking.
    body["max_tokens"] = json!(4096);
    body["thinking"] = json!({"type":"enabled","budget_tokens":1024,
            "block_binding":{"prefix_mismatch_behavior":"drop_block"}});
    assert!(matches!(
        decode_client_request(body.to_string().as_bytes()),
        Err(CodecError::Invalid("thinking.block_binding"))
    ));
}
fn message() -> Value {
    json!({"id":"msg_synthetic","type":"message","role":"assistant","model":"synthetic-model",
        "content":[{"type":"thinking","thinking":"","signature":"synthetic-signature"},
            {"type":"redacted_thinking","data":"synthetic-redacted"},
            {"type":"tool_use","id":"toolu_1","name":"lookup","input":{"key":"A"}}],
        "stop_reason":"tool_use","stop_sequence":null,
        "usage":{"input_tokens":11,"output_tokens":7}})
}
#[test]
fn typed_native_shapes_preserve_schema_presence_and_signature_only() {
    let p = Profile::AdaptiveTextTools;
    let decoded = p
        .decode_request(&request().to_string().into_bytes())
        .unwrap();
    assert_eq!(p.encode_request(&decoded).unwrap(), request());
    let mut selected = request();
    selected["service_tier"] = json!("standard_only");
    let decoded = p.decode_request(selected.to_string().as_bytes()).unwrap();
    assert_eq!(p.encode_request(&decoded).unwrap(), selected);
    selected["service_tier"] = json!("priority");
    assert!(p.decode_request(selected.to_string().as_bytes()).is_err());
    let decoded = p
        .decode_response(&message().to_string().into_bytes())
        .unwrap();
    assert_eq!(p.encode_response(&decoded).unwrap(), message());
    assert!(!format!("{decoded:?}").contains("synthetic-signature"));
    assert!(!format!("{decoded:?}").contains("synthetic-redacted"));
    for data in [json!(""), json!(7), Value::Null] {
        let mut bad = message();
        bad["content"][1]["data"] = data;
        assert!(p.decode_response(bad.to_string().as_bytes()).is_err());
    }
    // Unknown upstream fields are ignored; known ones are re-encoded unchanged.
    let mut unknown = message();
    unknown["created_at"] = json!(0);
    unknown["usage"]["service_tier"] = json!("standard");
    unknown["usage"]["synthetic_future_usage"] = json!({"count":0});
    unknown["content"][2]["extra"] = json!(null);
    let decoded = p.decode_response(unknown.to_string().as_bytes()).unwrap();
    let mut expected = message();
    expected["usage"]["service_tier"] = json!("standard");
    assert_eq!(p.encode_response(&decoded).unwrap(), expected);
    unknown["usage"]["service_tier"] = json!("auto");
    assert!(p.decode_response(unknown.to_string().as_bytes()).is_err());
    let mut edited = p.decode_request(request().to_string().as_bytes()).unwrap();
    edited.max_tokens = 0;
    assert!(p.encode_request(&edited).is_err());
}
#[test]
fn native_non_strict_tools_do_not_rewrite_or_drop_constraints() {
    let p = Profile::AdaptiveTextTools;
    for (key, value) in [
        ("temperature", json!(1.1)),
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
    let mut body = request();
    body["tools"][0]["strict"] = Value::Null;
    assert!(p.decode_request(body.to_string().as_bytes()).is_err());
    body["tools"][0]["strict"] = json!(true);
    let decoded = p.decode_request(body.to_string().as_bytes()).unwrap();
    // Native strict string enums have a documented capitalization loss.
    assert!(p.encode_request(&decoded).is_err());
    body["tools"][0]["input_schema"]["properties"]["key"] = json!({"type":"integer","enum":[1,2]});
    let decoded = p.decode_request(body.to_string().as_bytes()).unwrap();
    assert_eq!(p.encode_request(&decoded).unwrap(), body);
    let mut too_many = body.clone();
    too_many["tools"] = json!(
        (0..21)
            .map(|n| {
                let mut tool = body["tools"][0].clone();
                tool["name"] = json!(format!("tool_{n}"));
                tool
            })
            .collect::<Vec<_>>()
    );
    let decoded = p.decode_request(too_many.to_string().as_bytes()).unwrap();
    assert!(p.encode_request(&decoded).is_err());
    let mut complex = body.clone();
    let properties = (0..25)
        .map(|n| (format!("p{n}"), json!({"type":"boolean"})))
        .collect::<serde_json::Map<_, _>>();
    complex["output_config"] = json!({"format":{"type":"json_schema","schema":{
        "type":"object","properties":properties,"additionalProperties":false
    }}});
    let decoded = p.decode_request(complex.to_string().as_bytes()).unwrap();
    assert!(p.encode_request(&decoded).is_err());
    let mut output_only = complex.clone();
    output_only.as_object_mut().unwrap().remove("tools");
    let decoded = p
        .decode_request(output_only.to_string().as_bytes())
        .unwrap();
    assert!(p.encode_request(&decoded).is_err());
    let properties = (0..17)
        .map(|n| (format!("p{n}"), json!({"type":["boolean","null"]})))
        .collect::<serde_json::Map<_, _>>();
    complex["output_config"]["format"]["schema"]["properties"] = json!(properties);
    let decoded = p.decode_request(complex.to_string().as_bytes()).unwrap();
    assert!(p.encode_request(&decoded).is_err());
    output_only["output_config"] = complex["output_config"].clone();
    let decoded = p
        .decode_request(output_only.to_string().as_bytes())
        .unwrap();
    assert!(p.encode_request(&decoded).is_err());
    output_only["output_config"]["format"]["schema"] = json!({
        "type":"object","properties":{"ok":{"type":"boolean"}},
        "required":["ok"],"additionalProperties":false
    });
    let decoded = p
        .decode_request(output_only.to_string().as_bytes())
        .unwrap();
    assert_eq!(p.encode_request(&decoded).unwrap(), output_only);
    body["tools"][0]["input_schema"]["properties"]["key"]["minimum"] = json!(2);
    let decoded = p.decode_request(body.to_string().as_bytes()).unwrap();
    assert!(p.encode_request(&decoded).is_err());
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
        json!({"type":"message_delta","delta":{"stop_reason":"stop_sequence","stop_sequence":"END"},"usage":{"output_tokens":9}}),
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
    let extra = json!({"type":"message_stop","extra":false});
    let event = p
        .decode_event("message_stop", extra.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        p.encode_event(&event).unwrap(),
        json!({"type":"message_stop"})
    );
    let billing = json!({"type":"error","error":{"type":"billing_error","message":"synthetic"}});
    p.decode_event("error", billing.to_string().as_bytes())
        .unwrap();
    for wire in [
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":"END"},"usage":{"output_tokens":9}}),
        json!({"type":"content_block_delta","index":morphiecore::semantic::task::generation::MAX_ITEMS,"delta":{"type":"text_delta","text":"x"}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"citations_delta","citation":{}}}),
    ] {
        assert!(
            p.decode_event(wire["type"].as_str().unwrap(), wire.to_string().as_bytes())
                .is_err()
        );
    }
    let mut bad = message();
    bad["stop_reason"] = json!("pause_turn");
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
    req.messages =
        vec![req.messages[0].clone(); morphiecore::semantic::task::generation::MAX_ITEMS + 1];
    assert!(matches!(p.encode_request(&req), Err(CodecError::Limit)));
    let mut req = p.decode_request(request().to_string().as_bytes()).unwrap();
    req.system = morphiecore::semantic::value::Presence::Value(
        morphiecore::protocol::anthropic::TextContent::Text(
            "x".repeat(morphiecore::semantic::task::generation::MAX_TEXT_BYTES + 1),
        ),
    );
    assert!(matches!(p.encode_request(&req), Err(CodecError::Limit)));
    let mut response = p.decode_response(message().to_string().as_bytes()).unwrap();
    let input = StructuredValue::new(json!({"data":vec![Value::Null;4096]})).unwrap();
    let blocks = morphiecore::semantic::value::JsonLimits::ENVELOPE.nodes / 4096 + 1;
    response.content = (0..blocks)
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
