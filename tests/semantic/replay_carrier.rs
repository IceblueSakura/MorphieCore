//! Opaque reasoning crosses the client boundary in a tagged envelope and returns
//! only to the Provider/model that issued it.
use crate::wire;
use morphiecore::{
    adapter::{Adapter, Dialect},
    execution::replay::{CLIENT_ORIGIN, open_from_client, seal_event, seal_for_client},
    lowering::generation::GenerationRepresentationContract as Contract,
    protocol::{openai::Profile, replay_envelope},
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
fn another_target_gets_a_copy_without_the_opaque_value() {
    let body = client_round_trip("openai/model-a");
    let mut request = client()
        .decode_request(body.to_string().as_bytes())
        .unwrap();
    open_from_client(&mut request).unwrap();
    let before = request.clone();
    let wire = upstream("other/model-b")
        .encode_request(&request, "model-b", &contract("other/model-b"))
        .unwrap();
    assert!(!wire.to_string().contains("encrypted_content"));
    assert!(!wire.to_string().contains("synthetic-final-token"));
    // The readable summary still reaches the new target; the shared request is unchanged.
    assert!(wire.to_string().contains("Plan"));
    assert_eq!(request, before);
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
            let event = seal_event(event, &CLIENT_ORIGIN).unwrap();
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

#[test]
fn chat_carrier_round_trip_preserves_opaque_without_reinterpreting_it() {
    let issuer = "provider/model";
    let body = json!({"object":"response","id":"r","model":"m","created_at":1,"status":"completed","output":[
        {"type":"reasoning","id":"rs","status":"completed","summary":[{"type":"summary_text","text":"Plan"}],"encrypted_content":"signature"},
        {"type":"message","id":"msg","role":"assistant","status":"completed","content":[{"type":"output_text","text":"answer","annotations":[]}]}
    ]});
    let mut decoded = upstream(issuer)
        .decode_response(body.to_string().as_bytes())
        .unwrap();
    seal_for_client(&mut decoded, &CLIENT_ORIGIN).unwrap();
    let chat = Adapter::new(
        Profile::Chat,
        Dialect::MorphieCore,
        Some(CLIENT_ORIGIN.clone()),
    );
    let wire = chat
        .encode_response(&decoded, &contract(CLIENT_ORIGIN.as_str()))
        .unwrap();
    let message = &wire["choices"][0]["message"];
    let token = message["reasoning_details"][1]["data"].as_str().unwrap();
    assert!(token.starts_with("mc1."));
    assert_eq!(
        replay_envelope::open(token).unwrap().2.as_str(),
        "signature"
    );
    let input = json!({"model":"m","messages":[message,{"role":"user","content":"next"}]});
    let mut request = chat.decode_request(input.to_string().as_bytes()).unwrap();
    open_from_client(&mut request).unwrap();
    let prepared = upstream(issuer)
        .encode_request(&request, "m", &contract(issuer))
        .unwrap();
    assert_eq!(prepared["input"][0]["encrypted_content"], "signature");
    assert_eq!(prepared["input"][0]["summary"][0]["text"], "Plan");
}

#[test]
fn native_reasoning_format_survives_the_responses_carrier() {
    use morphiecore::semantic::task::generation::ReplayValue;
    let issuer = replay_envelope::issuer("anthropic", "model");
    let mut decoded = upstream(issuer.as_str())
        .decode_response(wire::response(1).to_string().as_bytes())
        .unwrap();
    let mut items = decoded.semantic.items().to_vec();
    let (id, Item::Reasoning(reason)) = &mut items[0] else {
        panic!("reasoning")
    };
    reason.replay = Some(ReplayValue::final_value(
        ReplayFormat::AnthropicMessagesThinking,
        morphiecore::semantic::value::Text::new("native-signature", "test", 100).unwrap(),
    ));
    decoded.fidelity.remove_replay(*id);
    decoded
        .fidelity
        .record_replay(*id, reason, Some(issuer.clone()))
        .unwrap();
    decoded.semantic = decoded.semantic.with_items(items).unwrap();
    seal_for_client(&mut decoded, &CLIENT_ORIGIN).unwrap();
    let output = client()
        .encode_response(&decoded, &contract(CLIENT_ORIGIN.as_str()))
        .unwrap();
    let body = json!({"model":"m","input":[output["output"][0],{"role":"user","content":"next"}]});
    let mut request = client()
        .decode_request(body.to_string().as_bytes())
        .unwrap();
    open_from_client(&mut request).unwrap();
    let Item::Reasoning(reason) = &request.task.semantic.items()[0].1 else {
        panic!("reasoning")
    };
    let replay = reason.replay.as_ref().unwrap();
    assert_eq!(replay.format(), ReplayFormat::AnthropicMessagesThinking);
    assert_eq!(replay.as_str(), "native-signature");
    assert_eq!(
        request
            .task
            .fidelity
            .replay_origin(request.task.semantic.items()[0].0),
        Some(&issuer)
    );
}
