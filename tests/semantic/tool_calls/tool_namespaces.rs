//! Namespace expectations are independent of the encoder and provider catalog.
use morphiecore::{
    protocol::openai::{
        GenerationApi, TargetContract as Contract, lowering::lower_request, responses,
    },
    semantic::{task::generation::*, value::Text},
};
use serde_json::json;

#[test]
fn grouped_definitions_and_qualified_selection_are_not_flattened() {
    let wire = json!({"input":[{"role":"user","content":"check"}],
        "tools":[
            {"type":"namespace","name":"left","description":"first","tools":[
                {"type":"function","name":"status","strict":false}]},
            {"type":"namespace","name":"right","description":"second","tools":[
                {"type":"function","name":"status","strict":false}]}],
        "tool_choice":"required"});
    let decoded = responses::decode_generation(&wire).expect("standard namespace");
    let before = decoded.clone();
    let representation = lower_request(
        &decoded.semantic,
        &decoded.fidelity,
        GenerationApi::Responses,
        Contract::full(),
    )
    .unwrap();
    let encoded = responses::encode_generation(&representation).unwrap();
    assert_eq!(encoded["tools"], wire["tools"]);
    assert_eq!(encoded["tool_choice"], wire["tool_choice"]);
    // Direct unprojected Chat lowering still rejects raw ToolDeclaration::Namespace.
    assert!(
        morphiecore::protocol::openai::lowering::render_request(
            std::borrow::Cow::Borrowed(&decoded.semantic),
            &decoded.fidelity,
            GenerationApi::Chat,
            Contract::full(),
        )
        .is_err()
    );
    // Target projection aliases both namespaces distinctly onto Chat without mutating source IR.
    let chat = lower_request(
        &decoded.semantic,
        &decoded.fidelity,
        GenerationApi::Chat,
        Contract::full(),
    )
    .unwrap();
    assert_eq!(decoded, before);
    let chat_wire = morphiecore::protocol::openai::chat::encode_generation(&chat).unwrap();
    let left_name = chat_wire["tools"][0]["function"]["name"].as_str().unwrap();
    let right_name = chat_wire["tools"][1]["function"]["name"].as_str().unwrap();
    assert_ne!(left_name, right_name);
    assert!(left_name.starts_with("mcns_"));
    assert!(right_name.starts_with("mcns_"));
    assert_eq!(chat_wire["tool_choice"], "required");
    assert!(chat.projection.iter().any(|effect| {
        effect.rule() == morphiecore::protocol::lowering::disposition::ALIAS_TOOL_NAMESPACES.id
    }));
    let mut settings = decoded.semantic.settings().clone();
    settings.tool_choice = Some(ToolChoice::Qualified(ToolReference {
        kind: ToolKind::Function,
        name: Text::new("status", "name", 128).unwrap(),
        namespace: Some(Text::new("right", "namespace", 128).unwrap()),
    }));
    let selected = decoded
        .semantic
        .clone()
        .with_settings(settings.clone())
        .unwrap();
    assert!(
        lower_request(
            &selected,
            &decoded.fidelity,
            GenerationApi::Responses,
            Contract::full()
        )
        .is_err()
    );
    let chat_selected = lower_request(
        &selected,
        &decoded.fidelity,
        GenerationApi::Chat,
        Contract::full(),
    )
    .unwrap();
    let chat_selected_wire =
        morphiecore::protocol::openai::chat::encode_generation(&chat_selected).unwrap();
    assert_eq!(
        chat_selected_wire["tool_choice"]["function"]["name"],
        right_name
    );
    settings.tools.as_mut().unwrap().pop();
    assert!(decoded.semantic.with_settings(settings).is_err());
}

#[test]
fn namespace_aliases_project_across_protocols_and_restore_on_json_sse_and_cache_successor() {
    use morphiecore::{
        adapter::ClientAdapter,
        execution::{Attempt, ResponseDelivery},
        protocol::openai::{Dialect, GenerationCodec},
        topology::{GenerationProtocol, catalog},
    };

    let wire = json!({
        "model": "public",
        "stream": false,
        "prompt_cache_key": "codex-session-1",
        "input": [
            {"role": "user", "content": "check status"},
            {
                "type": "additional_tools",
                "role": "developer",
                "tools": [
                    {
                        "type": "namespace",
                        "name": "clock",
                        "description": "time tools",
                        "tools": [{
                            "type": "function",
                            "name": "now",
                            "description": "current time",
                            "parameters": {"type": "object", "properties": {}, "required": [], "additionalProperties": false},
                            "strict": false
                        }]
                    }
                ]
            }
        ],
        "tools": [
            {
                "type": "namespace",
                "name": "ops",
                "description": "operations",
                "tools": [{
                    "type": "function",
                    "name": "status",
                    "description": "service status",
                    "parameters": {"type": "object", "properties": {}, "required": [], "additionalProperties": false},
                    "strict": false
                }]
            }
        ],
        "tool_choice": "auto"
    });
    let client = ClientAdapter::openai(
        GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None),
        Contract::full(),
    );
    let mut env_req = client
        .decode_client_request(&serde_json::to_vec(&wire).unwrap())
        .unwrap();
    env_req.conversation = Some(
        morphiecore::semantic::context::ConversationContext::conversation("codex-conv-1").unwrap(),
    );
    let req_snapshot = env_req.clone();

    let messages_ep = catalog::messages_bindings()[0].endpoint();
    let interactions_ep = catalog::google_bindings()
        .iter()
        .find(|b| b.canonical_model().id.as_str().starts_with("gemini"))
        .unwrap()
        .endpoint();
    let chat_ep = catalog::api_key_bindings()
        .iter()
        .find_map(|b| {
            (b.provider()
                .generation_entry(GenerationProtocol::OpenAiChat.operation())
                .is_some())
            .then(|| b.endpoint(GenerationProtocol::OpenAiChat).ok())
            .flatten()
        })
        .unwrap();
    let bailian_ep = catalog::api_key_bindings()
        .iter()
        .find_map(|b| {
            (b.dialect == Dialect::Bailian
                && b.provider()
                    .generation_entry(GenerationProtocol::OpenAiResponses.operation())
                    .is_some())
            .then(|| b.endpoint(GenerationProtocol::OpenAiResponses).ok())
            .flatten()
        })
        .unwrap();

    let mut expected_alias = String::new();
    for (label, ep) in [
        ("messages", &messages_ep),
        ("interactions", &interactions_ep),
        ("chat", &chat_ep),
        ("bailian", &bailian_ep),
    ] {
        let (conv, map) = ep
            .encode_request_with_delivery(&env_req)
            .unwrap_or_else(|err| panic!("{label} encode failed: {err:?}"));
        assert_eq!(env_req, req_snapshot, "{label} mutated source request");
        assert!(
            conv.evidence.iter().any(|e| {
                e.rule() == morphiecore::protocol::lowering::disposition::ALIAS_TOOL_NAMESPACES.id
            }),
            "{label} missing namespace alias evidence"
        );
        let entry = map
            .entries
            .iter()
            .find(|(_, orig)| {
                orig.namespace
                    .as_ref()
                    .map(morphiecore::semantic::value::Text::as_str)
                    == Some("ops")
                    && orig.name.as_str() == "status"
                    && !orig.wrapped_custom
            })
            .unwrap_or_else(|| panic!("{label} missing ops::status delivery mapping"));
        if expected_alias.is_empty() {
            expected_alias = entry.0.clone();
        } else {
            assert_eq!(
                entry.0, &expected_alias,
                "{label} generated divergent alias"
            );
        }
        let serialized = serde_json::to_string(&conv.value).unwrap();
        assert!(
            !serialized.contains("\"type\":\"namespace\""),
            "{label} leaked raw namespace definition on wire: {serialized}"
        );
        assert!(
            serialized.contains(&expected_alias),
            "{label} missing aliased wire name"
        );
    }

    // Verify independent Messages SSE upstream response restores original (namespace, name)
    // across JSON snapshot delivery, cache_successor, and SSE delivery.
    let (_, tool_delivery) = messages_ep.encode_request_with_delivery(&env_req).unwrap();
    let sse_wire = format!(
        "event: message_start\ndata: {}\n\n\
         event: content_block_start\ndata: {}\n\n\
         event: content_block_delta\ndata: {{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"input_json_delta\",\"partial_json\":\"{{}}\"}}}}\n\n\
         event: content_block_stop\ndata: {{\"type\":\"content_block_stop\",\"index\":0}}\n\n\
         event: message_delta\ndata: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"tool_use\",\"stop_sequence\":null}},\"usage\":{{\"output_tokens\":5}}}}\n\n\
         event: message_stop\ndata: {{\"type\":\"message_stop\"}}\n\n",
        json!({
            "type": "message_start",
            "message": {
                "id": "msg_02",
                "type": "message",
                "role": "assistant",
                "model": messages_ep.upstream_model,
                "content": [],
                "stop_reason": null,
                "usage": {"input_tokens": 10, "output_tokens": 0}
            }
        }),
        json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": {
                "type": "tool_use",
                "id": "toolu_02",
                "name": expected_alias,
                "input": {}
            }
        }),
    );
    fn push_all_sse(
        attempt: &mut Attempt,
        mut bytes: &[u8],
    ) -> Vec<morphiecore::semantic::task::generation::StreamEvent> {
        let mut out = Vec::new();
        while !bytes.is_empty() {
            let (consumed, events) = attempt.push(bytes).unwrap();
            assert!(consumed > 0, "stalled SSE push");
            bytes = &bytes[consumed..];
            out.extend(events);
        }
        out
    }

    let mut attempt_json =
        Attempt::new(messages_ep.adapter().unwrap(), 1 << 20, Default::default());
    attempt_json.begin(200, "text/event-stream").unwrap();
    let json_events = push_all_sse(&mut attempt_json, sse_wire.as_bytes());
    attempt_json.finish().unwrap();

    let mut delivery_json = ResponseDelivery::new(
        client.clone(),
        "public",
        Default::default(),
        Default::default(),
        morphiecore::protocol::openai::obfuscation::Obfuscation::Disabled,
        false,
    )
    .with_tool_delivery(tool_delivery.clone());
    delivery_json
        .observe_events(&attempt_json, &json_events)
        .unwrap();
    let (decoded_json, _) = delivery_json.encode_json_snapshot(&attempt_json).unwrap();
    let rendered_json = client.render_projected(decoded_json).unwrap().1;
    assert_eq!(rendered_json["output"][0]["name"], "status");
    assert_eq!(rendered_json["output"][0]["namespace"], "ops");
    let rendered_json_str = serde_json::to_string(&rendered_json).unwrap();
    assert!(!rendered_json_str.contains(&expected_alias));

    delivery_json.commit().unwrap();
    delivery_json.complete(&attempt_json).unwrap();
    // `prefixes()` excludes requests containing `additional_tools`, so also verify
    // `cache_successor` on a root-namespace request with the same restored delivery.
    let root_only_wire = json!({
        "model": "public",
        "stream": false,
        "prompt_cache_key": "codex-session-1",
        "input": [{"role": "user", "content": "check status"}],
        "tools": wire["tools"],
        "tool_choice": "auto"
    });
    let mut root_req = client
        .decode_client_request(&serde_json::to_vec(&root_only_wire).unwrap())
        .unwrap();
    root_req.conversation = env_req.conversation.clone();
    let (_, root_delivery) = messages_ep.encode_request_with_delivery(&root_req).unwrap();
    let mut attempt_root =
        Attempt::new(messages_ep.adapter().unwrap(), 1 << 20, Default::default());
    attempt_root.begin(200, "text/event-stream").unwrap();
    let root_events = push_all_sse(&mut attempt_root, sse_wire.as_bytes());
    attempt_root.finish().unwrap();
    let mut delivery_root = ResponseDelivery::new(
        client.clone(),
        "public",
        Default::default(),
        Default::default(),
        morphiecore::protocol::openai::obfuscation::Obfuscation::Disabled,
        false,
    )
    .with_tool_delivery(root_delivery);
    delivery_root
        .observe_events(&attempt_root, &root_events)
        .unwrap();
    delivery_root.encode_json_snapshot(&attempt_root).unwrap();
    delivery_root.commit().unwrap();
    delivery_root.complete(&attempt_root).unwrap();
    let successor = delivery_root
        .cache_successor(&attempt_root, &root_req)
        .expect("cache successor should retain restored call");
    assert!(successor.task.semantic.items().iter().any(|(_, item)| {
        matches!(
            item,
            Item::ToolCall(call)
                if call.name.as_str() == "status"
                    && call.context.namespace.as_ref().map(Text::as_str) == Some("ops")
        )
    }));

    let mut attempt_sse = Attempt::new(messages_ep.adapter().unwrap(), 1 << 20, Default::default());
    attempt_sse.begin(200, "text/event-stream").unwrap();
    let events = push_all_sse(&mut attempt_sse, sse_wire.as_bytes());
    attempt_sse.finish().unwrap();

    let mut delivery_sse = ResponseDelivery::new(
        client,
        "public",
        Default::default(),
        Default::default(),
        morphiecore::protocol::openai::obfuscation::Obfuscation::Disabled,
        true,
    )
    .with_tool_delivery(tool_delivery);
    let sse_first = delivery_sse.encode_events(&attempt_sse, &events).unwrap();
    let sse_final = delivery_sse.finish_stream(&attempt_sse).unwrap();
    let sse_bytes: Vec<u8> = [sse_first, sse_final]
        .into_iter()
        .flat_map(|batch| batch.into_bytes())
        .flat_map(|chunk| chunk.to_vec())
        .collect();
    let sse_text = String::from_utf8(sse_bytes).unwrap();
    assert!(sse_text.contains("\"name\":\"status\""));
    assert!(sse_text.contains("\"namespace\":\"ops\""));
    assert!(!sse_text.contains(&expected_alias));
}

#[test]
fn namespace_rejects_duplicates_nested_groups_and_unqualified_selection() {
    let group = json!({"type":"namespace","name":"ops","description":"operations",
        "tools":[{"type":"function","name":"status","strict":false}]});
    let wire = json!({"input":"check","tools":[group.clone()], "tool_choice":"auto"});
    assert!(responses::decode_generation(&wire).is_ok());
    let mut duplicate = wire.clone();
    duplicate["tools"][0]["tools"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"function","name":"status","strict":false}));
    assert!(responses::decode_generation(&duplicate).is_err());
    let mut nested = wire.clone();
    nested["tools"][0]["tools"] = json!([group]);
    assert!(responses::decode_generation(&nested).is_err());
    let mut missing = wire;
    missing["tool_choice"] = json!({"type":"function","name":"status"});
    assert!(responses::decode_generation(&missing).is_err());
    missing["tool_choice"]["namespace"] = json!("ops");
    assert!(responses::decode_generation(&missing).is_err());
}

#[test]
fn plain_and_namespaced_custom_tools_wrap_across_protocols_and_unwrap_on_delivery() {
    use morphiecore::{
        adapter::ClientAdapter,
        execution::{Attempt, ResponseDelivery},
        protocol::openai::{Dialect, GenerationCodec},
        topology::{GenerationProtocol, catalog},
    };

    let wire = json!({
        "model": "public",
        "stream": true,
        "prompt_cache_key": "codex-custom-session",
        "input": [{"role": "user", "content": "run code mode"}],
        "tools": [
            {
                "type": "namespace",
                "name": "functions",
                "description": "",
                "tools": [{
                    "type": "custom",
                    "name": "exec",
                    "description": "execute code",
                    "format": {"type": "text"}
                }]
            },
            {
                "type": "custom",
                "name": "scratch"
            }
        ],
        "tool_choice": "auto"
    });
    let client = ClientAdapter::openai(
        GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None),
        Contract::full(),
    );
    let mut env_req = client
        .decode_client_request(&serde_json::to_vec(&wire).unwrap())
        .unwrap();
    env_req.conversation = Some(
        morphiecore::semantic::context::ConversationContext::conversation("codex-custom-conv-1")
            .unwrap(),
    );
    let snapshot = env_req.clone();

    let messages_ep = catalog::messages_bindings()[0].endpoint();
    let interactions_ep = catalog::google_bindings()
        .iter()
        .find(|b| b.canonical_model().id.as_str().starts_with("gemini"))
        .unwrap()
        .endpoint();
    let chat_ep = catalog::api_key_bindings()
        .iter()
        .find_map(|b| {
            b.provider()
                .generation_entry(GenerationProtocol::OpenAiChat.operation())
                .is_some()
                .then(|| b.endpoint(GenerationProtocol::OpenAiChat).ok())
                .flatten()
        })
        .unwrap();
    let bailian_ep = catalog::api_key_bindings()
        .iter()
        .find_map(|b| {
            (b.dialect == Dialect::Bailian
                && b.provider()
                    .generation_entry(GenerationProtocol::OpenAiResponses.operation())
                    .is_some())
            .then(|| b.endpoint(GenerationProtocol::OpenAiResponses).ok())
            .flatten()
        })
        .unwrap();

    let mut exec_alias = String::new();
    for (label, ep) in [
        ("messages", &messages_ep),
        ("interactions", &interactions_ep),
        ("chat", &chat_ep),
        ("bailian", &bailian_ep),
    ] {
        let (conv, map) = ep
            .encode_request_with_delivery(&env_req)
            .unwrap_or_else(|err| panic!("{label} wrap failed: {err:?}"));
        assert_eq!(env_req, snapshot, "{label} mutated source request");
        assert!(
            conv.evidence.iter().any(|e| {
                e.rule() == morphiecore::protocol::lowering::disposition::WRAP_PLAIN_CUSTOM_TOOLS.id
            }),
            "{label} missing WRAP_PLAIN_CUSTOM_TOOLS evidence"
        );
        let entry = map
            .entries
            .iter()
            .find(|(_, orig)| {
                orig.namespace
                    .as_ref()
                    .map(morphiecore::semantic::value::Text::as_str)
                    == Some("functions")
                    && orig.name.as_str() == "exec"
                    && orig.wrapped_custom
            })
            .unwrap_or_else(|| panic!("{label} missing wrapped functions::exec mapping"));
        if exec_alias.is_empty() {
            exec_alias = entry.0.clone();
        } else {
            assert_eq!(entry.0, &exec_alias);
        }
        assert!(
            map.entries
                .get("scratch")
                .is_some_and(|orig| orig.wrapped_custom && orig.namespace.is_none()),
            "{label} missing plain scratch wrapper mapping"
        );
        let wire_text = serde_json::to_string(&conv.value).unwrap();
        assert!(!wire_text.contains("\"type\":\"custom\""));
        assert!(wire_text.contains(&exec_alias));
        assert!(wire_text.contains("\"input\":{\"type\":\"string\"}"));
    }

    // Deliver wrapped tool call from Messages SSE with quotes, newlines, and Unicode ->
    // verify both JSON snapshot and SSE emit restored custom_tool_call with namespace="functions".
    let payload_code = "const msg = \"hi\\n世界\"; // \\\"quoted\\\"";
    let payload_json = serde_json::to_string(&json!({ "input": payload_code })).unwrap();
    let sse_wire = format!(
        "event: message_start\ndata: {}\n\n\
         event: content_block_start\ndata: {}\n\n\
         event: content_block_delta\ndata: {}\n\n\
         event: content_block_stop\ndata: {{\"type\":\"content_block_stop\",\"index\":0}}\n\n\
         event: message_delta\ndata: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"tool_use\",\"stop_sequence\":null}},\"usage\":{{\"output_tokens\":8}}}}\n\n\
         event: message_stop\ndata: {{\"type\":\"message_stop\"}}\n\n",
        json!({
            "type": "message_start",
            "message": {
                "id": "msg_custom_01",
                "type": "message",
                "role": "assistant",
                "model": messages_ep.upstream_model,
                "content": [],
                "stop_reason": null,
                "usage": {"input_tokens": 12, "output_tokens": 0}
            }
        }),
        json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": {
                "type": "tool_use",
                "id": "toolu_exec_1",
                "name": exec_alias,
                "input": {}
            }
        }),
        json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {
                "type": "input_json_delta",
                "partial_json": payload_json
            }
        }),
    );

    fn push_all(
        attempt: &mut Attempt,
        mut bytes: &[u8],
    ) -> Vec<morphiecore::semantic::task::generation::StreamEvent> {
        let mut out = Vec::new();
        while !bytes.is_empty() {
            let (consumed, events) = attempt.push(bytes).unwrap();
            assert!(consumed > 0);
            bytes = &bytes[consumed..];
            out.extend(events);
        }
        out
    }

    let (_, delivery_map) = messages_ep.encode_request_with_delivery(&env_req).unwrap();
    let mut attempt_json =
        Attempt::new(messages_ep.adapter().unwrap(), 1 << 20, Default::default());
    attempt_json.begin(200, "text/event-stream").unwrap();
    let json_events = push_all(&mut attempt_json, sse_wire.as_bytes());
    attempt_json.finish().unwrap();

    let mut delivery_json = ResponseDelivery::new(
        client.clone(),
        "public",
        Default::default(),
        Default::default(),
        morphiecore::protocol::openai::obfuscation::Obfuscation::Disabled,
        false,
    )
    .with_tool_delivery(delivery_map.clone());
    delivery_json
        .observe_events(&attempt_json, &json_events)
        .unwrap();
    let (decoded_json, _) = delivery_json.encode_json_snapshot(&attempt_json).unwrap();
    let rendered_json = client.render_projected(decoded_json).unwrap().1;
    assert_eq!(rendered_json["output"][0]["type"], "custom_tool_call");
    assert_eq!(rendered_json["output"][0]["name"], "exec");
    assert_eq!(rendered_json["output"][0]["namespace"], "functions");
    assert_eq!(rendered_json["output"][0]["input"], payload_code);
    assert!(
        !serde_json::to_string(&rendered_json)
            .unwrap()
            .contains(&exec_alias)
    );

    let mut attempt_sse = Attempt::new(messages_ep.adapter().unwrap(), 1 << 20, Default::default());
    attempt_sse.begin(200, "text/event-stream").unwrap();
    let sse_events = push_all(&mut attempt_sse, sse_wire.as_bytes());
    attempt_sse.finish().unwrap();

    let mut delivery_sse = ResponseDelivery::new(
        client.clone(),
        "public",
        Default::default(),
        Default::default(),
        morphiecore::protocol::openai::obfuscation::Obfuscation::Disabled,
        true,
    )
    .with_tool_delivery(delivery_map);
    let b1 = delivery_sse
        .encode_events(&attempt_sse, &sse_events)
        .unwrap();
    let b2 = delivery_sse.finish_stream(&attempt_sse).unwrap();
    let sse_text = String::from_utf8(
        [b1, b2]
            .into_iter()
            .flat_map(|b| b.into_bytes())
            .flat_map(|c| c.to_vec())
            .collect(),
    )
    .unwrap();
    assert!(sse_text.contains("\"type\":\"custom_tool_call\""));
    assert!(sse_text.contains("\"name\":\"exec\""));
    assert!(sse_text.contains("\"namespace\":\"functions\""));
    assert!(!sse_text.contains(&exec_alias));

    // Multi-turn replay: turn 2 sends back custom_tool_call + custom_tool_call_output.
    let turn2_wire = json!({
        "model": "public",
        "stream": true,
        "prompt_cache_key": "codex-custom-session",
        "input": [
            {"role": "user", "content": "run code mode"},
            {
                "type": "custom_tool_call",
                "call_id": "toolu_exec_1",
                "name": "exec",
                "namespace": "functions",
                "input": payload_code
            },
            {
                "type": "custom_tool_call_output",
                "call_id": "toolu_exec_1",
                "output": "done"
            }
        ],
        "tools": wire["tools"],
        "tool_choice": "auto"
    });
    let mut turn2_req = client
        .decode_client_request(&serde_json::to_vec(&turn2_wire).unwrap())
        .unwrap();
    turn2_req.conversation = env_req.conversation.clone();
    for (label, ep) in [
        ("messages-t2", &messages_ep),
        ("interactions-t2", &interactions_ep),
        ("chat-t2", &chat_ep),
        ("bailian-t2", &bailian_ep),
    ] {
        let (conv, _) = ep
            .encode_request_with_delivery(&turn2_req)
            .unwrap_or_else(|err| panic!("{label} turn2 wrap failed: {err:?}"));
        let serialized = serde_json::to_string(&conv.value).unwrap();
        assert!(!serialized.contains("custom_tool_call"));
        assert!(serialized.contains(&exec_alias));
    }
}

#[test]
fn grammar_custom_tools_remain_rejected_on_function_only_and_streaming_chat_targets() {
    use morphiecore::{
        adapter::ClientAdapter,
        protocol::openai::{Dialect, GenerationCodec},
        topology::{GenerationProtocol, catalog},
    };

    let wire = json!({
        "model": "public",
        "stream": true,
        "prompt_cache_key": "codex-grammar-session",
        "input": [{"role": "user", "content": "query"}],
        "tools": [{
            "type": "custom",
            "name": "sql",
            "format": {
                "type": "grammar",
                "syntax": "lark",
                "definition": "start: \"SELECT 1\""
            }
        }],
        "tool_choice": "auto"
    });
    let client = ClientAdapter::openai(
        GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None),
        Contract::full(),
    );
    let mut env_req = client
        .decode_client_request(&serde_json::to_vec(&wire).unwrap())
        .unwrap();
    env_req.conversation = Some(
        morphiecore::semantic::context::ConversationContext::conversation("codex-grammar-conv")
            .unwrap(),
    );
    let messages_ep = catalog::messages_bindings()[0].endpoint();
    let interactions_ep = catalog::google_bindings()
        .iter()
        .find(|b| b.canonical_model().id.as_str().starts_with("gemini"))
        .unwrap()
        .endpoint();
    let chat_ep = catalog::api_key_bindings()
        .iter()
        .find_map(|b| {
            b.provider()
                .generation_entry(GenerationProtocol::OpenAiChat.operation())
                .is_some()
                .then(|| b.endpoint(GenerationProtocol::OpenAiChat).ok())
                .flatten()
        })
        .unwrap();
    let bailian_ep = catalog::api_key_bindings()
        .iter()
        .find_map(|b| {
            (b.dialect == Dialect::Bailian
                && b.provider()
                    .generation_entry(GenerationProtocol::OpenAiResponses.operation())
                    .is_some())
            .then(|| b.endpoint(GenerationProtocol::OpenAiResponses).ok())
            .flatten()
        })
        .unwrap();
    for (label, ep) in [
        ("messages", &messages_ep),
        ("interactions", &interactions_ep),
        ("chat-streaming", &chat_ep),
        ("bailian", &bailian_ep),
    ] {
        assert!(
            ep.encode_request_with_delivery(&env_req).is_err(),
            "{label} must continue rejecting grammar custom tool"
        );
    }
}
