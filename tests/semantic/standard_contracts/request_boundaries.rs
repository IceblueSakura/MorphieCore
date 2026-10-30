//! Independent request expectations from the pinned OpenAI/Anthropic parameter types.
//! See docs/refs/2026-10-26-standard-request-boundaries.md for source revisions.
use super::*;
use morphiecore::{
    execution::replay::open_from_client,
    protocol::{
        DecodedRequest, GenerationRequestEnvelope,
        anthropic::messages::{self as native, Profile},
        native::ReplayTarget,
    },
    semantic::value::Presence,
};

#[test]
fn chat_storage_and_audio_history_keep_request_meaning() {
    let codec = adapter(GenerationApi::Chat);
    for audio in [
        json!({"id":"a","unknown":0}),
        json!({"id":"a","data":"AQID","expires_at":1,"transcript":"x"}),
    ] {
        let body = json!({"model":"m","store":false,"messages":[{"role":"assistant","content":"A","audio":audio,"annotations":false,"parsed":42}]});
        let request = codec
            .decode_client_request(body.to_string().as_bytes())
            .unwrap();
        assert_eq!(request.context.store, Presence::Value(false));
        assert_eq!(request.task.semantic.resources().iter().count(), 0);
        let Item::Message(message) = &request.task.semantic.items()[0].1 else {
            panic!("message")
        };
        assert!(matches!(
            message.parts[1].content,
            ContentPart::AudioReference(_)
        ));
    }
    let mut request = codec
        .decode_request(br#"{"model":"m","messages":[{"role":"user","content":"x"}]}"#)
        .unwrap();
    request.context.store = Presence::Value(false);
    let before = request.clone();
    assert_eq!(
        codec
            .encode_request(&request, "m", &TargetContract::full())
            .unwrap(),
        json!({"model":"m","store":false,"messages":[{"role":"user","content":"x"}]})
    );
    assert_eq!(request, before);
}

#[test]
fn chat_null_logit_bias_is_the_documented_default_not_active_bias() {
    let chat = adapter(GenerationApi::Chat);
    let body = json!({"model":"m","messages":[{"role":"user","content":"x"}]});
    let expected = chat
        .decode_client_request(body.to_string().as_bytes())
        .unwrap();
    let mut nullable = body.clone();
    nullable["logit_bias"] = Value::Null;
    let actual = chat
        .decode_client_request(nullable.to_string().as_bytes())
        .unwrap();
    assert_eq!(actual.task, expected.task);
    let (wire, rules) = chat
        .encode_request_with_rules(&actual, "m", &TargetContract::full())
        .unwrap();
    assert_eq!(wire, body);
    assert!(rules.is_empty());
    for value in [json!({"1":1}), json!({}), json!([]), json!(false), json!(0)] {
        let mut active = body.clone();
        active["logit_bias"] = value.clone();
        assert!(
            chat.decode_client_request(active.to_string().as_bytes())
                .is_err(),
            "{value}"
        );
    }
}

#[test]
fn standard_known_fields_are_strict_and_sdk_views_are_not_authority() {
    let chat = adapter(GenerationApi::Chat);
    for message in [
        json!({"role":"assistant"}),
        json!({"role":"assistant","content":"A","tool_calls":null}),
    ] {
        assert!(
            chat.decode_client_request(
                json!({"model":"m","messages":[message]})
                    .to_string()
                    .as_bytes()
            )
            .is_err()
        );
    }
    let call = json!({"id":"c","type":"function","function":{"name":"f","arguments":"not-json"}});
    let base = json!({"model":"m","messages":[{"role":"assistant","tool_calls":[call]}]});
    let expected = chat
        .decode_client_request(base.to_string().as_bytes())
        .unwrap();
    let mut extra = base.clone();
    extra["messages"][0]["tool_calls"][0]["index"] = json!("not a chunk");
    extra["session_id"] = json!({"not":"affinity"});
    extra["enable_thinking"] = json!(false);
    let actual = chat
        .decode_client_request(extra.to_string().as_bytes())
        .unwrap();
    assert_eq!(actual.task, expected.task);
    assert!(actual.cache_session.is_none());
    assert!(actual.conversation.is_none());
    assert!(
        chat.decode_client_request(
            br#"{"model":"m","messages":[{"role":"assistant","content":null}]}"#
        )
        .is_ok()
    );
    for (key, value) in [
        ("functions", json!("")),
        ("functions", Value::Null),
        ("function_call", json!([])),
        ("web_search_options", json!([])),
        ("web_search_options", json!({})),
    ] {
        let mut body = json!({"model":"m","messages":[{"role":"user","content":"x"}]});
        body[key] = value;
        assert!(
            chat.decode_client_request(body.to_string().as_bytes())
                .is_err(),
            "{key}"
        );
    }
    let responses = adapter(GenerationApi::Responses);
    for fields in [
        json!({"access_programs":null}),
        json!({"input":[{"type":"reasoning","id":"r","summary":[],"status":null}]}),
        json!({"input":[{"type":"reasoning","id":"r","summary":[],"content":null}]}),
        json!({"tools":[{"type":"custom","name":"f","description":null}]}),
    ] {
        let mut body = json!({"model":"m","input":"x"});
        body.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        assert!(
            responses
                .decode_client_request(body.to_string().as_bytes())
                .is_err(),
            "{fields}"
        );
    }
    let body = json!({"model":"m","input":[{"type":"function_call","call_id":"c","name":"f","arguments":"{bad json","parsed_arguments":{"false":"view"}}]});
    let request = responses
        .decode_client_request(body.to_string().as_bytes())
        .unwrap();
    let Item::ToolCall(call) = &request.task.semantic.items()[0].1 else {
        panic!("call")
    };
    assert_eq!(call.arguments.as_raw(), Some("{bad json"));
    for key in ["id", "status", "namespace", "async"] {
        let mut bad = body.clone();
        bad["input"][0][key] = Value::Null;
        assert!(
            responses
                .decode_client_request(bad.to_string().as_bytes())
                .is_err(),
            "{key}"
        );
    }
    let mut caller = body;
    caller["input"][0]["caller"] = Value::Null;
    assert!(
        responses
            .decode_client_request(caller.to_string().as_bytes())
            .is_ok()
    );
}

#[test]
fn standard_content_discriminators_do_not_admit_provider_video() {
    for (api, key, part) in [
        (
            GenerationApi::Chat,
            "messages",
            json!({"type":"video_url","video_url":{"url":"https://example.invalid/a.mp4"}}),
        ),
        (
            GenerationApi::Responses,
            "input",
            json!({"type":"input_video","video_url":"https://example.invalid/a.mp4"}),
        ),
    ] {
        let body = json!({"model":"m",key:[{"role":"user","content":[part]}]});
        assert!(
            adapter(api)
                .decode_client_request(body.to_string().as_bytes())
                .is_err()
        );
        assert!(
            adapter(api)
                .decode_request(body.to_string().as_bytes())
                .is_err()
        );
    }
}

#[test]
fn damaged_or_unbound_continuation_never_becomes_successful_deletion() {
    for summary in [json!([]), json!([{"type":"summary_text","text":"plan"}])] {
        for token in ["bare-token", "mc1.broken", "mc2.future"] {
            let body = json!({"model":"m","input":[{"type":"reasoning","id":"r","summary":summary,"encrypted_content":token},{"role":"user","content":"x"}]});
            let mut request = adapter(GenerationApi::Responses)
                .decode_client_request(body.to_string().as_bytes())
                .unwrap();
            let before = request.clone();
            assert!(open_from_client(&mut request).is_err());
            assert_eq!(request, before);
        }
    }
    for block in [
        json!({"type":"thinking","thinking":"plan","signature":"bare"}),
        json!({"type":"redacted_thinking","data":"mc1.broken"}),
    ] {
        let body = json!({"model":"m","max_tokens":8,"messages":[{"role":"user","content":"x"},{"role":"assistant","content":[block]},{"role":"user","content":"next"}]});
        let mut request = decode_client_request(body.to_string().as_bytes()).unwrap();
        let before = request.clone();
        assert!(open_from_client(&mut request).is_err());
        assert_eq!(request, before);
    }
}

#[test]
fn responses_history_uses_complete_standard_union_shapes() {
    let codec = adapter(GenerationApi::Responses);
    let complete = json!({"type":"message","id":"msg_1","role":"assistant","status":"completed","content":[{"type":"output_text","text":"A","annotations":[]}]});
    let mut mixed = complete.clone();
    mixed["content"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"input_text","text":"B"}));
    assert!(
        codec
            .decode_client_request(json!({"model":"m","input":[mixed]}).to_string().as_bytes())
            .is_err()
    );
    for probs in [Value::Null, json!([{"token":"A","logprob":-1.0}])] {
        let mut bad = complete.clone();
        bad["content"][0]["logprobs"] = probs;
        assert!(
            codec
                .decode_client_request(json!({"model":"m","input":[bad]}).to_string().as_bytes())
                .is_err()
        );
    }
    let mut valid = complete.clone();
    valid["content"][0]["logprobs"] =
        json!([{"token":"A","logprob":-1.0,"bytes":[65],"top_logprobs":[]}]);
    assert!(
        codec
            .decode_client_request(json!({"model":"m","input":[valid]}).to_string().as_bytes())
            .is_ok()
    );
    let request = codec
        .decode_client_request(
            json!({"model":"m","input":[complete]})
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    assert_eq!(
        codec
            .encode_request(&request, "m", &TargetContract::full())
            .unwrap()["input"][0],
        complete
    );
    for field in ["id", "status"] {
        let mut bad = complete.clone();
        bad.as_object_mut().unwrap().shift_remove(field);
        assert!(
            codec
                .decode_client_request(json!({"model":"m","input":[bad]}).to_string().as_bytes())
                .is_err()
        );
    }
    for status in ["completed", "incomplete", "in_progress"] {
        let item = json!({"type":"message","role":"developer","status":status,"content":[{"type":"input_text","text":"P"}]});
        let request = codec
            .decode_client_request(json!({"model":"m","input":[item]}).to_string().as_bytes())
            .unwrap();
        assert_eq!(
            codec
                .encode_request(&request, "m", &TargetContract::full())
                .unwrap()["input"][0],
            item
        );
    }
    let request = codec
        .decode_client_request(br#"{"model":"m","input":[{"role":"assistant","content":"A"}]}"#)
        .unwrap();
    let item = &codec
        .encode_request(&request, "m", &TargetContract::full())
        .unwrap()["input"][0];
    assert_eq!(item["status"], "completed");
    assert_eq!(item["id"], "item_1");
    assert_eq!(
        item["content"],
        json!([{"type":"output_text","text":"A","annotations":[]}])
    );
}

#[test]
fn stable_messages_filters_extras_without_enabling_beta_semantics() {
    let base = json!({"model":"m","max_tokens":32,"messages":[{"role":"user","content":"x"}],"tool_choice":{"type":"auto"}});
    let expected = decode_client_request(base.to_string().as_bytes()).unwrap();
    let mut extra = base.clone();
    extra["tool_choice"]["x_extra"] = json!(7);
    extra["session_id"] = json!("ignored");
    assert_eq!(
        decode_client_request(extra.to_string().as_bytes())
            .unwrap()
            .task,
        expected.task
    );
    for fields in [
        json!({"thinking":{"type":"adaptive","block_binding":{"prefix_mismatch_behavior":"error"}}}),
        json!({"messages":[{"role":"user","content":"x"},{"role":"system","content":[],"output_config":{"effort":"low"}}]}),
        json!({"messages":[{"role":"user","content":"x"},{"role":"system","content":[{"type":"tool_removal","tool":{"type":"tool_reference","name":"f"}}]}]}),
    ] {
        let mut body = base.clone();
        body.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        assert!(
            decode_client_request(body.to_string().as_bytes()).is_err(),
            "{fields}"
        );
    }
}

fn message(role: MessageRole, name: Option<&str>, value: &str) -> Item {
    Item::Message(Message {
        participant: name.map(|n| Text::new(n, "test", 32).unwrap()),
        role,
        status: ItemLifecycle::Completed,
        phase: None,
        parts: vec![Part::new(
            PartId::new(1),
            ContentPart::Text(Text::new(value, "test", 32).unwrap().into()),
        )],
    })
}
fn typed(items: Vec<Item>, settings: GenerationSettings) -> GenerationRequestEnvelope {
    GenerationRequestEnvelope::from_generation(
        DecodedRequest {
            semantic: GenerationRequest::from_settings(
                items
                    .into_iter()
                    .enumerate()
                    .map(|(i, item)| (ItemId::new(i as u64 + 1), item))
                    .collect(),
                settings,
            )
            .unwrap(),
            fidelity: Default::default(),
            evidence: Default::default(),
        },
        "m",
    )
    .unwrap()
}
#[test]
fn independent_calls_do_not_acquire_the_previous_named_participant() {
    let request = typed(
        vec![
            message(MessageRole::Assistant, Some("alice"), "A"),
            Item::ToolCall(ToolCall {
                call_id: Text::new("c", "test", 32).unwrap(),
                name: Text::new("f", "test", 32).unwrap(),
                arguments: ToolArguments::Raw("{}".into()),
                status: ItemLifecycle::Completed,
                context: Default::default(),
                cache: None,
                replay: None,
            }),
            Item::ToolResult(ToolResult {
                call_id: Some(Text::new("c", "test", 32).unwrap()),
                name: None,
                output: ToolOutput::Text("ok".into()),
                is_error: None,
                execution: None,
                status: None,
                context: Default::default(),
                cache: None,
            }),
        ],
        Default::default(),
    );
    let before = request.clone();
    assert_eq!(
        adapter(GenerationApi::Chat)
            .encode_request(&request, "m", &TargetContract::full())
            .unwrap()["messages"],
        json!([
            {"role":"assistant","name":"alice","content":"A"},
            {"role":"assistant","content":null,"tool_calls":[{"id":"c","type":"function","function":{"name":"f","arguments":"{}"}}]},
            {"role":"tool","tool_call_id":"c","content":"ok"}
        ])
    );
    assert_eq!(request, before);
    let mut request = typed(
        vec![message(MessageRole::User, None, "x")],
        Default::default(),
    );
    let mut settings = request.task.semantic.settings().clone();
    settings.reasoning = ReasoningRequest::present(None);
    settings.reasoning.replay_mismatch = Some(ReplayMismatch::Reject);
    settings.effort = Presence::Value(EffortControl {
        scope: EffortScope::Reasoning,
        level: EffortLevel::Low,
    });
    request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
    assert!(
        adapter(GenerationApi::Chat)
            .encode_request(&request, "m", &TargetContract::full())
            .is_err()
    );
}

#[test]
fn messages_native_grammar_and_manual_thinking_are_source_and_target_rules() {
    let call =
        json!({"role":"assistant","content":[{"type":"tool_use","id":"c","name":"f","input":{}}]});
    let result = json!({"type":"tool_result","tool_use_id":"c","content":"ok"});
    let mut body = json!({"model":"m","max_tokens":2048,"messages":[{"role":"user","content":"x"},call,{"role":"user","content":[result]}]});
    assert!(decode_client_request(body.to_string().as_bytes()).is_ok());
    for content in [
        json!([{"type":"text","text":"late"},result]),
        json!([{"type":"text","text":"missing"}]),
    ] {
        let mut bad = body.clone();
        bad["messages"][2]["content"] = content;
        assert!(decode_client_request(bad.to_string().as_bytes()).is_err());
    }
    let target = native::TargetContract::new(Profile::AdaptiveTextTools, native::Dialect::Standard);
    let mut request = decode_client_request(body.to_string().as_bytes()).unwrap();
    let mut settings = request.task.semantic.settings().clone();
    settings.reasoning = ReasoningRequest::present(None);
    settings.reasoning.activation = Presence::Value(ReasoningActivation::Enabled);
    settings.reasoning.mode = Presence::Value(ReasoningMode::Budgeted);
    settings.reasoning.budget_tokens = Some(1024);
    request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
    assert!(
        target
            .encode_request(&request, &ReplayTarget::new("synthetic", "m").unwrap())
            .is_err()
    );
    body["thinking"] = json!({"type":"enabled","budget_tokens":1024});
    assert!(decode_client_request(body.to_string().as_bytes()).is_err());
    body["messages"][1]["content"]
        .as_array_mut()
        .unwrap()
        .insert(
            0,
            json!({"type":"thinking","thinking":"plan","signature":"synthetic-native"}),
        );
    assert!(decode_client_request(body.to_string().as_bytes()).is_ok());
    body["tools"] = json!([{"name":"f","input_schema":{"type":"object"}}]);
    body["tool_choice"] = json!({"type":"any"});
    assert!(decode_client_request(body.to_string().as_bytes()).is_err());
    body["thinking"] = json!({"type":"adaptive"});
    assert!(decode_client_request(body.to_string().as_bytes()).is_ok());
    // Manual thinking spans successive tool steps, not each assistant message.
    let mut multi = body.clone();
    multi["thinking"] = json!({"type":"enabled","budget_tokens":1024});
    multi["tool_choice"] = json!({"type":"auto"});
    multi["messages"].as_array_mut().unwrap().extend([
        json!({"role":"assistant","content":[{"type":"tool_use","id":"c2","name":"f","input":{}}]}),
        json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"c2","content":"ok"}]}),
    ]);
    assert!(decode_client_request(multi.to_string().as_bytes()).is_ok());
    let fresh = json!({"model":"m","max_tokens":2048,
        "thinking":{"type":"enabled","budget_tokens":1024},
        "tools":[{"name":"f","input_schema":{"type":"object"}}],
        "messages":[{"role":"user","content":"x"}]});
    let mut fresh = decode_client_request(fresh.to_string().as_bytes()).unwrap();
    let replay_target = ReplayTarget::new("synthetic", "m").unwrap();
    assert!(target.encode_request(&fresh, &replay_target).is_ok());
    // Enabling manual thinking on a fresh user turn does not rewrite older turns.
    let mut archive = json!({"model":"m","max_tokens":2048,
        "thinking":{"type":"enabled","budget_tokens":1024},
        "messages":[{"role":"user","content":"old"},call,
            {"role":"user","content":[result]},
            {"role":"assistant","content":"completed"},
            {"role":"user","content":"new task"}]});
    let archived = decode_client_request(archive.to_string().as_bytes()).unwrap();
    assert!(target.encode_request(&archived, &replay_target).is_ok());
    archive["messages"].as_array_mut().unwrap().truncate(3);
    assert!(decode_client_request(archive.to_string().as_bytes()).is_err());
    for choice in [
        ToolChoice::Required,
        ToolChoice::Specific(Text::new("f", "test", 32).unwrap()),
    ] {
        let mut settings = fresh.task.semantic.settings().clone();
        settings.tool_choice = Some(choice);
        fresh.task.semantic = fresh.task.semantic.clone().with_settings(settings).unwrap();
        assert!(target.encode_request(&fresh, &replay_target).is_err());
    }
    for (key, value) in [
        ("stop_sequences", Value::Null),
        ("diagnostics", json!({"previous_message_id":null})),
    ] {
        let mut bad = messages();
        bad[key] = value;
        assert!(decode_client_request(bad.to_string().as_bytes()).is_err());
    }
    let mut bad = body;
    bad["messages"][1]["content"][1]["caller"] = Value::Null;
    assert!(decode_client_request(bad.to_string().as_bytes()).is_err());
}
