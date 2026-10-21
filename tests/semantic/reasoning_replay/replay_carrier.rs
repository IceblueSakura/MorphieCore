//! Opaque reasoning crosses the client boundary in a tagged envelope and returns
//! only to the Provider/model that issued it.
use crate::wire;
use morphiecore::{
    execution::replay::{CLIENT_ORIGIN, open_from_client, seal_event, seal_for_client},
    protocol::{
        openai::{Adapter, Dialect, Profile, TargetCaps as Contract},
        replay_envelope,
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
fn upstream(label: &str) -> Adapter {
    Adapter::new(Profile::Responses, Dialect::Standard, Some(origin(label)))
}
fn contract(label: &str) -> Contract {
    Contract {
        replay_origin: Some(origin(label)),
        ..Contract::full()
    }
}
fn client() -> Adapter {
    Adapter::new(
        Profile::Responses,
        Dialect::Standard,
        Some(CLIENT_ORIGIN.clone()),
    )
}
fn reasoning(items: &[(morphiecore::semantic::task::generation::ItemId, Item)]) -> Vec<&Item> {
    items
        .iter()
        .map(|(_, item)| item)
        .filter(|item| matches!(item, Item::Reasoning(_)))
        .collect()
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
    let native = morphiecore::protocol::anthropic::messages::TargetCaps::new(
        morphiecore::protocol::anthropic::messages::Profile::AdaptiveTextTools,
        morphiecore::protocol::anthropic::messages::Dialect::Standard,
    );
    native.encode_request(&request, &target).unwrap();
    let mut partial = body;
    partial["input"][1]["status"] = json!("incomplete");
    let mut partial = client()
        .decode_request(partial.to_string().as_bytes())
        .unwrap();
    open_from_client(&mut partial).unwrap();
    assert!(
        upstream("other/model-b")
            .encode_request_with_rules(&partial, "model-b", &allowed)
            .is_err()
    );
    assert!(native.encode_request(&partial, &target).is_err());
}

#[test]
fn tool_turn_transfer_keeps_real_results_and_forgets_paired_wire_ids_only() {
    let mut body = client_round_trip("openai/model-a");
    body["input"].as_array_mut().unwrap().pop();
    body["input"].as_array_mut().unwrap().extend([
            json!({"role":"assistant","content":[{"type":"output_text","text":"Checking.","annotations":[]}]}),
            json!({"type":"function_call","id":"fc_paired","call_id":"c","name":"read","arguments":"{}"}),
            json!({"type":"function_call_output","call_id":"c","output":"42"}),
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
            {"type":"message","role":"assistant","content":[{"type":"output_text","text":"Checking.","annotations":[]}]},
            {"type":"function_call","call_id":"c","name":"read","arguments":"{}"},
            {"type":"function_call_output","call_id":"c","output":"42"},
        ])
    );
    assert_eq!(rules, ["responses.transfer-visible-history.v1"]);
    assert_eq!(request, before);
    let native = upstream("openai/model-a")
        .encode_request(&request, "model-a", &contract("openai/model-a"))
        .unwrap();
    assert_eq!(native["input"][3]["id"], "fc_paired");
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
    assert!(
        upstream("other/model-b")
            .encode_request(&mixed, "model-b", &contract("other/model-b"))
            .unwrap_err()
            .to_string()
            .contains("mixed replay call dependencies")
    );
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
fn values_without_a_known_origin_are_dropped() {
    let mut body = client_round_trip("openai/model-a");
    body["input"][1]["encrypted_content"] = json!("raw-token-from-elsewhere");
    let mut request = client()
        .decode_request(body.to_string().as_bytes())
        .unwrap();
    open_from_client(&mut request).unwrap();
    assert!(
        reasoning(request.task.semantic.items())
            .iter()
            .all(|item| matches!(item, Item::Reasoning(r) if r.replay.is_none()))
    );
    let wire = upstream("openai/model-a")
        .encode_request(&request, "model-a", &contract("openai/model-a"))
        .unwrap();
    assert!(!wire.to_string().contains("raw-token-from-elsewhere"));
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
fn foreign_and_malformed_opaque_only_items_are_removed() {
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
        open_from_client(&mut request).unwrap();
        let saved = request.clone();
        let wire = upstream("openai/model-a")
            .encode_request(&request, "model-a", &contract("openai/model-a"))
            .unwrap();
        assert_eq!(wire["input"].as_array().unwrap().len(), 1);
        assert_eq!(wire["input"][0]["role"], "user");
        assert_eq!(request, saved);
    }
}

fn chat_client() -> Adapter {
    Adapter::new(
        Profile::Chat,
        Dialect::MorphieCore,
        Some(CLIENT_ORIGIN.clone()),
    )
}
/// Two reasoning items, each closed by its own encrypted value, ahead of one message.
fn two_reasoning_items() -> (Value, Vec<Value>) {
    let r1 = json!({"id":"r1","type":"reasoning","status":"completed","summary":[{"type":"summary_text","text":"Aa"}],"encrypted_content":"tok-1"});
    let r2 = json!({"id":"r2","type":"reasoning","status":"completed","summary":[],"content":[{"type":"reasoning_text","text":"B"}],"encrypted_content":"tok-2"});
    let m = json!({"id":"m","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"ok","annotations":[]}]});
    let response = json!({"id":"resp","object":"response","created_at":1,"status":"completed","model":"model-a",
            "error":null,"incomplete_details":null,"output":[r1,r2,m],
            "usage":{"input_tokens":3,"output_tokens":5,"total_tokens":8,"input_tokens_details":{"cached_tokens":0},"output_tokens_details":{"reasoning_tokens":2}}});
    let mut created = response.clone();
    created["status"] = json!("in_progress");
    created["output"] = json!([]);
    created["usage"] = Value::Null;
    let added = |n: usize, item: &Value| {
        let mut item = item.clone();
        item["status"] = json!("in_progress");
        for key in ["summary", "content"] {
            if item.get(key).is_some() {
                item[key] = json!([]);
            }
        }
        item.as_object_mut().unwrap().remove("encrypted_content");
        json!({"type":"response.output_item.added","output_index":n,"item":item})
    };
    let done = |n: usize, item: &Value| json!({"type":"response.output_item.done","output_index":n,"item":item});
    let at = |n: usize, id: &str, mut event: Value| {
        event["output_index"] = json!(n);
        event["item_id"] = json!(id);
        event
    };
    let mut events = vec![
        json!({"type":"response.created","response":created}),
        added(0, &r1),
        at(
            0,
            "r1",
            json!({"type":"response.reasoning_summary_part.added","summary_index":0,"part":{"type":"summary_text","text":""}}),
        ),
        at(
            0,
            "r1",
            json!({"type":"response.reasoning_summary_text.delta","summary_index":0,"delta":"A"}),
        ),
        at(
            0,
            "r1",
            json!({"type":"response.reasoning_summary_text.delta","summary_index":0,"delta":"a"}),
        ),
        at(
            0,
            "r1",
            json!({"type":"response.reasoning_summary_text.done","summary_index":0,"text":"Aa"}),
        ),
        at(
            0,
            "r1",
            json!({"type":"response.reasoning_summary_part.done","summary_index":0,"part":{"type":"summary_text","text":"Aa"}}),
        ),
        done(0, &r1),
        added(1, &r2),
        at(
            1,
            "r2",
            json!({"type":"response.reasoning_text.delta","content_index":0,"delta":"B"}),
        ),
        at(
            1,
            "r2",
            json!({"type":"response.reasoning_text.done","content_index":0,"text":"B"}),
        ),
        done(1, &r2),
        added(2, &m),
        at(
            2,
            "m",
            json!({"type":"response.content_part.added","content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
        ),
        at(
            2,
            "m",
            json!({"type":"response.output_text.delta","content_index":0,"delta":"ok","logprobs":[]}),
        ),
        at(
            2,
            "m",
            json!({"type":"response.output_text.done","content_index":0,"text":"ok","logprobs":[]}),
        ),
        at(
            2,
            "m",
            json!({"type":"response.content_part.done","content_index":0,"part":{"type":"output_text","text":"ok","annotations":[]}}),
        ),
        done(2, &m),
        json!({"type":"response.completed","response":response}),
    ];
    for (n, event) in events.iter_mut().enumerate() {
        event["sequence_number"] = json!(n);
    }
    (response, events)
}
/// Replaces each sealed value with the payload it carries.
fn opened(mut details: Value) -> Value {
    for detail in details.as_array_mut().unwrap() {
        if let Some(data) = detail.get_mut("data") {
            let (o, _, payload) = replay_envelope::open(data.as_str().unwrap()).unwrap();
            assert_eq!(o, origin("openai/model-a"));
            *data = json!(payload.as_str());
        }
    }
    details
}

#[test]
fn chat_details_keep_item_boundaries_and_survive_pi_and_opencode_replays() {
    use morphiecore::protocol::openai::events::EventEncoder;
    let issuer = upstream("openai/model-a");
    let client_contract = chat_client().contract(&contract(CLIENT_ORIGIN.as_str()));
    let (response, events) = two_reasoning_items();
    let details = json!([
        {"type":"reasoning.summary","summary":"Aa","format":"openai-responses-v1","index":0},
        {"type":"reasoning.encrypted","data":"tok-1","format":"openai-responses-v1","index":1,"id":"r1"},
        {"type":"reasoning.text","text":"B","format":"openai-responses-v1","index":2},
        {"type":"reasoning.encrypted","data":"tok-2","format":"openai-responses-v1","index":3,"id":"r2"},
    ]);

    let mut decoded = issuer
        .decode_response(response.to_string().as_bytes())
        .unwrap();
    seal_for_client(&mut decoded, &CLIENT_ORIGIN).unwrap();
    let mut message = chat_client()
        .encode_response(&decoded, &contract(CLIENT_ORIGIN.as_str()))
        .unwrap()["choices"][0]["message"]
        .take();
    message["reasoning_details"] = opened(message["reasoning_details"].take());
    assert_eq!(
        message,
        json!({"role":"assistant","content":"ok","reasoning_content":"AaB","reasoning_details":details})
    );

    let mut decoder = issuer.event_decoder();
    let mut encoder = EventEncoder::new(Profile::Chat, decoded.metadata.clone())
        .unwrap()
        .with_contract(client_contract);
    let mut deltas = vec![];
    for frame in &events {
        for event in decoder.push(frame).unwrap() {
            let event = seal_event(event, &CLIENT_ORIGIN, decoder.fidelity()).unwrap();
            for chunk in encoder.encode(&event, &decoded.fidelity).unwrap() {
                deltas.push(chunk["choices"][0]["delta"].clone());
            }
        }
    }
    let summary = |s: &str, first: bool| {
        let mut detail = json!({"type":"reasoning.summary","summary":s,"index":0});
        if first {
            detail["format"] = json!("openai-responses-v1");
        }
        json!({"reasoning_content":s,"reasoning_details":[detail]})
    };
    let sealed = |n: usize| json!({"reasoning_details":[{"type":"reasoning.encrypted","format":"openai-responses-v1","index":n,"id":format!("r{}", n / 2 + 1)}]});
    let without_data: Vec<_> = deltas
        .iter()
        .cloned()
        .map(|mut delta| {
            if let Some(detail) = delta.pointer_mut("/reasoning_details/0")
                && detail.get("data").is_some()
            {
                detail.as_object_mut().unwrap().remove("data");
            }
            delta
        })
        .collect();
    // Both views persist; constant format occurs once, avoiding SDK string concatenation.
    assert_eq!(
        without_data[..8],
        [
            json!({"role":"assistant"}),
            summary("A", true),
            summary("a", false),
            sealed(1),
            json!({"reasoning_content":"B","reasoning_details":[{"type":"reasoning.text","text":"B","format":"openai-responses-v1","index":2}]}),
            sealed(3),
            json!({"content":""}),
            json!({"content":"ok"}),
        ]
    );

    // pi merges adjacent readable deltas of one type, keeps the first index,
    // and replays only the details.
    let mut merged: Vec<Value> = vec![];
    for detail in deltas
        .iter()
        .filter_map(|d| d.get("reasoning_details"))
        .flat_map(|d| d.as_array().unwrap().clone())
    {
        match merged.last_mut() {
            Some(last)
                if last["type"] == "reasoning.summary" && detail["type"] == "reasoning.summary" =>
            {
                last["summary"] = json!(format!(
                    "{}{}",
                    last["summary"].as_str().unwrap(),
                    detail["summary"].as_str().unwrap()
                ));
            }
            _ => merged.push(detail),
        }
    }
    // opencode (e5a557e) merges alike and also returns the concatenated view.
    for view in [None, Some("AaB")] {
        let mut assistant = json!({"role":"assistant","content":"ok","reasoning_details":merged});
        if let Some(view) = view {
            assistant["reasoning_content"] = json!(view);
        }
        let history = json!({"model":"m","messages":[
            {"role":"user","content":"q"},
            assistant,
            {"role":"user","content":"next"},
        ]});
        let mut request = chat_client()
            .decode_client_request(history.to_string().as_bytes())
            .unwrap();
        open_from_client(&mut request).unwrap();
        let wire = issuer
            .encode_request(&request, "model-a", &contract("openai/model-a"))
            .unwrap();
        assert_eq!(
            wire["input"].as_array().unwrap()[1..3],
            [
                json!({"type":"reasoning","id":"r1","summary":[{"type":"summary_text","text":"Aa"}],"encrypted_content":"tok-1"}),
                json!({"type":"reasoning","id":"r2","summary":[],"content":[{"type":"reasoning_text","text":"B"}],"encrypted_content":"tok-2"}),
            ]
        );
    }
}

#[test]
fn openrouter_signed_text_details_become_items_and_views_must_agree() {
    use morphiecore::semantic::{
        task::generation::{ItemLifecycle, PartId, ReasoningContent, ReasoningItem, ReplayValue},
        value::Text,
    };
    let history = |view: &str| {
        json!({"model":"m","messages":[
            {"role":"user","content":"q"},
            {"role":"assistant","content":"ok","reasoning":view,"reasoning_details":[
                {"type":"reasoning.text","text":"A","signature":"sig-1","format":"anthropic-claude-v1","index":0},
                {"type":"reasoning.summary","summary":"B","format":"openai-responses-v1","index":0},
                {"type":"reasoning.encrypted","id":"r2","data":"tok-2","format":"openai-responses-v1","index":0},
            ]},
        ]})
    };
    let request = chat_client()
        .decode_client_request(history("AB").to_string().as_bytes())
        .unwrap();
    let text = |s: &str| Text::new(s, "synthetic", 64).unwrap();
    let item = |content, token: &str| {
        Item::Reasoning(ReasoningItem {
            parts: vec![(PartId::new(0), content)],
            status: ItemLifecycle::Completed,
            replay: Some(ReplayValue::final_value(
                ReplayFormat::ResponsesEncrypted,
                text(token),
            )),
        })
    };
    let items: Vec<_> = request.task.semantic.items()[1..3]
        .iter()
        .map(|(_, item)| {
            let mut item = item.clone();
            if let Item::Reasoning(r) = &mut item {
                r.parts[0].0 = PartId::new(0);
            }
            item
        })
        .collect();
    assert_eq!(
        items,
        [
            item(ReasoningContent::Text(text("A")), "sig-1"),
            item(ReasoningContent::Summary(text("B")), "tok-2"),
        ]
    );
    assert!(
        chat_client()
            .decode_client_request(history("AX").to_string().as_bytes())
            .is_err()
    );
    // pi's DeepSeek compat adds an empty `reasoning_content` beside the details.
    assert!(
        chat_client()
            .decode_client_request(history("").to_string().as_bytes())
            .is_ok()
    );
    let mut empty = history("");
    empty["messages"][1]["reasoning_details"][0]["text"] = json!("");
    empty["messages"][1]["reasoning_details"][1]["summary"] = json!("");
    let decoded = chat_client()
        .decode_client_request(empty.to_string().as_bytes())
        .unwrap();
    assert!(
        matches!(&decoded.task.semantic.items()[1].1, Item::Reasoning(r)
            if matches!(&r.parts[..], [(_, ReasoningContent::Text(t))] if t.as_str().is_empty())
                && r.replay.as_ref().unwrap().replay_token() == Some("sig-1"))
    );
    assert!(
        matches!(&decoded.task.semantic.items()[2].1, Item::Reasoning(r)
            if matches!(&r.parts[..], [(_, ReasoningContent::Summary(t))] if t.as_str().is_empty())
                && r.replay.as_ref().unwrap().replay_token() == Some("tok-2"))
    );
}
