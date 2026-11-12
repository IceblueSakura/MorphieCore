//! Opaque reasoning crosses the client boundary in a tagged envelope and returns
//! only to the Provider/model that issued it.
use crate::wire;
use morphiecore::{
    execution::replay::{open_from_client, seal_event, seal_for_client},
    protocol::{
        openai::{Dialect, GenerationApi, GenerationCodec, TargetContract as Contract},
        replay_envelope::{self, CLIENT_ORIGIN},
    },
    semantic::{
        task::generation::{Item, ReplayFormat, StreamEvent},
        value::ReplayOrigin,
    },
};
use serde_json::{Value, json};

fn origin(label: &str) -> ReplayOrigin {
    ReplayOrigin::new(label).unwrap()
}
fn upstream(label: &str) -> GenerationCodec {
    GenerationCodec::new(
        GenerationApi::Responses,
        Dialect::Standard,
        Some(origin(label)),
    )
}
fn contract(label: &str) -> Contract {
    Contract {
        replay_origin: Some(origin(label)),
        ..Contract::full()
    }
}
fn client() -> GenerationCodec {
    GenerationCodec::new(
        GenerationApi::Responses,
        Dialect::Standard,
        Some(CLIENT_ORIGIN.clone()),
    )
}
/// Upstream response → sealed client JSON → the client's next request.
fn client_round_trip(issuer: &str) -> Value {
    let mut decoded = upstream(issuer)
        .decode_response(wire::response(1).to_string().as_bytes())
        .unwrap();
    seal_for_client(&mut decoded, &CLIENT_ORIGIN).unwrap();
    let output = client()
        .encode_response(&decoded, &contract(CLIENT_ORIGIN.as_str()))
        .unwrap();
    let sealed = output["output"][0]["encrypted_content"].as_str().unwrap();
    let (o, format, payload) = replay_envelope::open(sealed).unwrap();
    assert_eq!(o, origin(issuer));
    assert_eq!(format, ReplayFormat::ResponsesEncrypted);
    assert_eq!(payload.as_str(), "synthetic-final-token");
    let mut item = output["output"][0].clone();
    item.as_object_mut().unwrap().remove("content");
    json!({"model":"m","input":[
        {"role":"user","content":"first"},
        item,
        {"role":"user","content":"next"},
    ]})
}

#[test]
fn same_issuer_receives_its_original_value() {
    let mut edited = upstream("openai/model-a")
        .decode_response(wire::response(1).to_string().as_bytes())
        .unwrap();
    let mut items = edited.semantic.items().to_vec();
    let Item::Reasoning(reasoning) = &mut items[0].1 else {
        panic!("reasoning")
    };
    let (_, content) = &mut reasoning.parts[0];
    *content = morphiecore::semantic::task::generation::ReasoningContent::Summary(
        morphiecore::semantic::value::Text::new("edited", "synthetic", 64).unwrap(),
    );
    edited.semantic = edited.semantic.with_items(items).unwrap();
    let before = edited.clone();
    assert!(seal_for_client(&mut edited, &CLIENT_ORIGIN).is_err());
    assert_eq!(edited, before);

    let body = client_round_trip("openai/model-a");
    let mut request = client()
        .decode_request(body.to_string().as_bytes())
        .unwrap();
    open_from_client(&mut request).unwrap();
    assert!(
        upstream("openai/model-a")
            .encode_request(&request, "model-a", &Contract::full())
            .is_err()
    );
    let wire = upstream("openai/model-a")
        .encode_request(&request, "model-a", &contract("openai/model-a"))
        .unwrap();
    assert_eq!(
        wire["input"][1]["encrypted_content"],
        "synthetic-final-token"
    );
}

#[test]
fn another_target_gets_visible_history_without_foreign_thinking() {
    let mut body = client_round_trip("openai/model-a");
    body["max_output_tokens"] = json!(4096);
    let mut request = client()
        .decode_request(body.to_string().as_bytes())
        .unwrap();
    open_from_client(&mut request).unwrap();
    let before = request.clone();
    let allowed = contract("other/model-b");
    let (wire, rules) = upstream("other/model-b")
        .encode_request_with_rules(&request, "model-b", &allowed)
        .unwrap();
    // Foreign thinking is not presented as an assistant answer.
    assert_eq!(
        wire["input"],
        json!([
            {"type":"message","role":"user","content":[{"type":"input_text","text":"first"}]},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"next"}]},
        ])
    );
    assert_eq!(rules, ["responses.transfer-visible-history.v1"]);
    assert_eq!(request, before);
    let target = morphiecore::protocol::native::ReplayTarget::new("other", "model-b").unwrap();
    let native = morphiecore::protocol::anthropic::messages::TargetContract::new(
        morphiecore::protocol::anthropic::messages::Profile::AdaptiveTextTools,
        morphiecore::protocol::anthropic::messages::Dialect::Standard,
    );
    native.encode_request(&request, &target).unwrap();
    for status in ["incomplete", "in_progress"] {
        let mut body = body.clone();
        body["input"][1]["status"] = json!(status);
        let mut request = client()
            .decode_request(body.to_string().as_bytes())
            .unwrap();
        if status == "in_progress" {
            assert!(
                open_from_client(&mut request).is_err(),
                "partial client tokens cannot open"
            );
            continue;
        }
        open_from_client(&mut request).unwrap();
        let original = request.clone();
        let (transferred, rules) = upstream("other/model-b")
            .encode_request_with_rules(&request, "model-b", &allowed)
            .unwrap();
        assert_eq!(
            transferred, wire,
            "foreign {status} reasoning is not replayed"
        );
        assert_eq!(rules, ["responses.transfer-visible-history.v1"]);
        native.encode_request(&request, &target).unwrap();
        assert_eq!(request, original);
    }
}

#[test]
fn tool_turn_transfer_keeps_real_results_and_forgets_paired_wire_ids_only() {
    let mut body = client_round_trip("openai/model-a");
    body["input"].as_array_mut().unwrap().pop();
    body["input"].as_array_mut().unwrap().extend([
            json!({"type":"message","id":"msg_check","status":"completed","role":"assistant","content":[{"type":"output_text","text":"Checking.","annotations":[]}]}),
            json!({"type":"function_call","id":"fc_paired","call_id":"c","name":"read","arguments":"{}"}),
            json!({"type":"function_call","id":"fc_second","call_id":"d","name":"read","arguments":"{\"key\":\"second\"}"}),
            json!({"type":"function_call_output","call_id":"c","output":"42"}),
            json!({"type":"function_call_output","call_id":"d","output":"84"}),
        ]);
    let mut request = client()
        .decode_request(body.to_string().as_bytes())
        .unwrap();
    open_from_client(&mut request).unwrap();
    let before = request.clone();
    let (wire, rules) = upstream("other/model-b")
        .encode_request_with_rules(&request, "model-b", &contract("other/model-b"))
        .unwrap();
    assert_eq!(
        wire["input"],
        json!([
            {"type":"message","role":"user","content":[{"type":"input_text","text":"first"}]},
            {"id":"msg_check","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"Checking.","annotations":[]}]},
            {"type":"function_call","call_id":"c","name":"read","arguments":"{}"},
            {"type":"function_call","call_id":"d","name":"read","arguments":"{\"key\":\"second\"}"},
            {"type":"function_call_output","call_id":"c","output":"42"},
            {"type":"function_call_output","call_id":"d","output":"84"},
        ])
    );
    assert_eq!(rules, ["responses.transfer-visible-history.v1"]);
    assert_eq!(request, before);
    let native = upstream("openai/model-a")
        .encode_request(&request, "model-a", &contract("openai/model-a"))
        .unwrap();
    assert_eq!(native["input"][3]["id"], "fc_paired");
    let summary_target = GenerationCodec::new(
        GenerationApi::Responses,
        Dialect::Bailian,
        Some(origin("openai/model-a")),
    );
    let assessed = Contract {
        reasoning_history: Some(morphiecore::protocol::reasoning::ReasoningHistory::Accepted),
        ..Contract::full()
    };
    let delegated = summary_target
        .encode_request_with_rules(&request, "qwen3.8-flash", &Contract::full())
        .unwrap();
    assert_eq!(delegated.0["input"], wire["input"]);
    assert_eq!(delegated.1, ["responses.transfer-visible-history.v1"]);
    let (transferred, losses) = summary_target
        .encode_request_with_rules(&request, "qwen3.8-flash", &assessed)
        .unwrap();
    assert_eq!(transferred["input"], wire["input"]);
    assert_eq!(losses, ["responses.transfer-visible-history.v1"]);
    assert_eq!(request, before);
    for status in ["incomplete", "in_progress"] {
        let mut native_body = body.clone();
        native_body["input"][1]["status"] = json!(status);
        native_body["input"][1]["encrypted_content"] = json!("synthetic-final-token");
        // Decode a trusted native value, bypassing client-envelope opening.
        let phased = upstream("openai/model-a")
            .decode_request(native_body.to_string().as_bytes())
            .unwrap();
        let original = phased.clone();
        let result = summary_target.encode_request(&phased, "qwen3.8-flash", &assessed);
        if status == "incomplete" {
            assert_eq!(result.unwrap()["input"], wire["input"]);
        } else {
            assert!(matches!(
                result,
                Err(morphiecore::protocol::ConversionFailure {
                    cause: morphiecore::protocol::ConversionCause::Codec(
                        morphiecore::protocol::CodecError::Unsupported(
                            morphiecore::protocol::failure::CarrierReason::Reasoning
                        )
                    ),
                    ..
                })
            ));
        }
        assert_eq!(phased, original);
    }
    let summary = json!({"type":"reasoning","id":"reason-kept",
        "summary":[{"type":"summary_text","text":"kept summary"}]});
    let mut mixed_body = body.clone();
    mixed_body["input"]
        .as_array_mut()
        .unwrap()
        .insert(2, summary.clone());
    let mut supported = client()
        .decode_request(mixed_body.to_string().as_bytes())
        .unwrap();
    open_from_client(&mut supported).unwrap();
    let original = supported.clone();
    let mut expected = wire["input"].as_array().unwrap().clone();
    expected.insert(1, summary);
    let (projected, rules) = summary_target
        .encode_request_with_rules(&supported, "qwen3.8-flash", &assessed)
        .unwrap();
    assert_eq!(projected["input"], json!(expected));
    assert_eq!(rules, ["responses.transfer-visible-history.v1"]);
    assert_eq!(supported, original);
    let mut mixed = before.clone();
    let mut items = mixed.task.semantic.items().to_vec();
    let Item::Reasoning(mut compatible) = items[1].1.clone() else {
        panic!("reasoning")
    };
    for (index, (part, _)) in compatible.parts.iter_mut().enumerate() {
        *part = morphiecore::semantic::task::generation::PartId::scoped(
            morphiecore::semantic::task::generation::LocalScope::new(77),
            index as u64,
        );
    }
    let owner = morphiecore::semantic::task::generation::ItemId::new(999);
    mixed
        .task
        .fidelity
        .record_replay(owner, &compatible, Some(origin("other/model-b")))
        .unwrap();
    items.insert(2, (owner, Item::Reasoning(compatible)));
    mixed.task.semantic = mixed.task.semantic.with_items(items).unwrap();
    assert!(matches!(
        upstream("other/model-b")
            .encode_request(&mixed, "model-b", &contract("other/model-b"))
            .unwrap_err(),
        morphiecore::protocol::ConversionFailure {
            boundary: Some(
                morphiecore::protocol::conversion::generation::ConversionBoundary::RequestEncode
            ),
            cause: morphiecore::protocol::ConversionCause::Codec(
                morphiecore::protocol::CodecError::Unsupported(
                    morphiecore::protocol::failure::CarrierReason::Replay
                )
            ),
            ..
        }
    ));
    assert_eq!(
        native["input"][1]["encrypted_content"],
        "synthetic-final-token"
    );
    let Item::Reasoning(value) = &request.task.semantic.items()[1].1 else {
        panic!("reasoning")
    };
    request
        .task
        .fidelity
        .record_replay(
            morphiecore::semantic::task::generation::ItemId::new(999),
            value,
            Some(origin("other/model-b")),
        )
        .unwrap();
    assert!(
        upstream("other/model-b")
            .encode_request(&request, "model-b", &contract("other/model-b"))
            .is_err()
    );
}

#[test]
fn unsealed_values_transfer_visible_history_without_issuer_authority_or_mutation() {
    let mut body = client_round_trip("openai/model-a");
    body["input"][1]["encrypted_content"] = json!("raw-token-from-elsewhere");
    let mut request = client()
        .decode_request(body.to_string().as_bytes())
        .unwrap();
    let saved = request.clone();
    open_from_client(&mut request).unwrap();
    assert_eq!(request, saved);
    let (wire, rules) = upstream("openai/model-a")
        .encode_request_with_rules(&request, "model-a", &contract("openai/model-a"))
        .unwrap();
    assert_eq!(
        wire["input"],
        json!([
            {"type":"message","role":"user","content":[{"type":"input_text","text":"first"}]},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"next"}]}
        ])
    );
    assert_eq!(rules, ["responses.transfer-visible-history.v1"]);
    assert_eq!(request, saved);
}

#[test]
fn streamed_replay_is_sealed_before_client_encoding() {
    let issuer = upstream("openai/model-a");
    let mut decoder = issuer.event_decoder();
    let mut sealed_tokens = vec![];
    for frame in wire::events(1) {
        for event in decoder.push(&frame).unwrap() {
            let event = seal_event(event, &CLIENT_ORIGIN, decoder.fidelity()).unwrap();
            if let StreamEvent::ItemStarted {
                replay: Some(replay),
                ..
            }
            | StreamEvent::ItemFinished {
                replay: Some(replay),
                ..
            } = &event
            {
                assert_eq!(replay.origin.as_ref(), Some(&*CLIENT_ORIGIN));
                sealed_tokens.push(replay.value.as_str().to_owned());
            }
        }
    }
    decoder.finish().unwrap();
    assert!(!sealed_tokens.is_empty());
    for token in sealed_tokens {
        let (o, _, _) = replay_envelope::open(&token).unwrap();
        assert_eq!(o, origin("openai/model-a"));
    }
}

#[test]
fn foreign_replay_remains_bound_and_malformed_replay_rejects() {
    for token in [
        replay_envelope::seal(
            &origin("foreign/model"),
            ReplayFormat::ResponsesEncrypted,
            "foreign-token",
        )
        .unwrap()
        .as_str()
        .to_owned(),
        "bare-token".into(),
        "mc1.invalid".into(),
        "mc2.future".into(),
    ] {
        let body = json!({"model":"m","input":[
            {"type":"reasoning","id":"rs","summary":[],"encrypted_content":token},
            {"role":"user","content":"next"}
        ]});
        let mut request = client()
            .decode_request(body.to_string().as_bytes())
            .unwrap();
        let original = request.clone();
        if token.starts_with("mc1.") && replay_envelope::open(&token).is_none() {
            assert!(open_from_client(&mut request).is_err());
            assert_eq!(request, original);
            continue;
        }
        open_from_client(&mut request).unwrap();
        assert_eq!(request.task.semantic.items().len(), 2);
        let saved = request.clone();
        let wire = upstream("openai/model-a")
            .encode_request(&request, "model-a", &contract("openai/model-a"))
            .unwrap();
        assert_eq!(wire["input"].as_array().unwrap().len(), 1);
        assert_eq!(wire["input"][0]["role"], "user");
        assert_eq!(request, saved);
    }
}
