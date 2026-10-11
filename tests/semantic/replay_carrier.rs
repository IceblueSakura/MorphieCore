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
