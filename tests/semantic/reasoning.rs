//! Reasoning ownership, opaque replay, mixed tool continuation and resource failures.
use crate::events_support::*;
use morphiecore::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            Profile, TargetCaps as Contract, chat,
            events::{EventDecoder, EventEncoder},
            lowering::{lower_request, lower_response},
            responses,
        },
    },
    semantic::task::generation::*,
};
use serde_json::{Value, json};
fn reasoning(status: &str) -> Value {
    json!({"id":"rs","type":"reasoning","status":status,"summary":[{"type":"summary_text","text":"summary"}],"content":[{"type":"reasoning_text","text":"reasoning"}],"encrypted_content":"final-synthetic"})
}

#[test]
fn standard_summary_rejects_nonstandard_values_on_requests_echoes_and_events() {
    use morphiecore::protocol::openai::{Adapter, Dialect};
    for name in ["summary", "generate_summary"] {
        for value in [
            json!(false),
            json!(true),
            json!(0),
            json!("none"),
            json!({}),
        ] {
            let mut controls = json!({});
            controls[name] = value;
            let request = json!({"model":"fixture-model","input":"hello","reasoning":controls});
            for dialect in [Dialect::Standard, Dialect::MorphieCore, Dialect::Nvidia] {
                assert!(
                    Adapter::new(Profile::Responses, dialect, None)
                        .decode_request(request.to_string().as_bytes())
                        .is_err(),
                    "{name} on {dialect:?}"
                );
            }
            let mut response = envelope("completed", json!([]));
            response["reasoning"] = controls.clone();
            assert!(
                responses::decode_response(&response).is_err(),
                "{name} echo"
            );
            let mut event = created();
            event["response"]["reasoning"] = controls;
            let mut decoder = EventDecoder::new(Profile::Responses);
            assert!(decoder.push(&event).is_err(), "{name} event");
            assert!(decoder.finish().is_err());
        }
    }
}

#[test]
fn typed_disabled_summary_is_valid_semantics_without_a_responses_carrier() {
    let settings = GenerationSettings {
        instructions: morphiecore::semantic::value::Presence::Value(text("hello")),
        reasoning: ReasoningRequest::present(
            Some(ReasoningEffort::None),
            Some(ReasoningSummary::Disabled),
        ),
        ..Default::default()
    };
    let request = GenerationRequest::from_settings(vec![], settings.clone()).unwrap();
    assert_eq!(
        request.reasoning().summary(),
        Some(ReasoningSummary::Disabled)
    );
    let fidelity = FidelityRecords::default();
    for profile in [Profile::Responses, Profile::Chat] {
        assert!(matches!(
            lower_request(&request, &fidelity, profile, Contract::full()),
            Err(morphiecore::lowering::generation::RepresentationError::Reasoning)
        ));
    }
    let mut reported = metadata();
    reported.context.settings = Some(settings);
    let response = GenerationResponse::new(vec![], Outcome::Completed).unwrap();
    assert!(
        lower_response(
            &response,
            &fidelity,
            &reported,
            Profile::Responses,
            Contract::full()
        )
        .is_err()
    );
    let mut encoder = EventEncoder::new(Profile::Responses, reported).unwrap();
    assert!(encoder.encode(&StreamEvent::Started, &fidelity).is_err());
    assert!(encoder.encode(&StreamEvent::Started, &fidelity).is_err());
}

#[test]
fn standard_summary_presence_and_deprecated_alias_have_independent_wire_expectations() {
    use morphiecore::semantic::value::Presence;
    for (summary, wire) in [
        (Presence::Absent, json!({})),
        (Presence::Null, json!({"summary":null})),
        (
            Presence::Value(ReasoningSummary::Auto),
            json!({"summary":"auto"}),
        ),
        (
            Presence::Value(ReasoningSummary::Concise),
            json!({"summary":"concise"}),
        ),
        (
            Presence::Value(ReasoningSummary::Detailed),
            json!({"summary":"detailed"}),
        ),
    ] {
        let decoded =
            responses::decode_generation(&json!({"input":"hello","reasoning":wire})).unwrap();
        assert_eq!(decoded.semantic.reasoning().summary, summary);
        let mut reasoning = ReasoningRequest::present(None, None);
        reasoning.summary = summary;
        let request = GenerationRequest::from_settings(
            vec![],
            GenerationSettings {
                instructions: Presence::Value(text("hello")),
                reasoning,
                ..Default::default()
            },
        )
        .unwrap();
        let fidelity = FidelityRecords::default();
        let target =
            lower_request(&request, &fidelity, Profile::Responses, Contract::full()).unwrap();
        assert_eq!(
            responses::encode_generation(&target).unwrap()["reasoning"],
            wire
        );
    }
    for value in [
        Value::Null,
        json!("auto"),
        json!("concise"),
        json!("detailed"),
    ] {
        let decoded = responses::decode_generation(&json!({
            "input":"hello","reasoning":{"generate_summary":value}
        }))
        .unwrap();
        let target = lower_request(
            &decoded.semantic,
            &decoded.fidelity,
            Profile::Responses,
            Contract::full(),
        )
        .unwrap();
        let wire = responses::encode_generation(&target).unwrap();
        assert_eq!(wire["reasoning"], json!({"summary":value}));
    }
    assert!(
        responses::decode_generation(&json!({
            "input":"hello","reasoning":{"summary":"auto","generate_summary":"detailed"}
        }))
        .is_err()
    );
}

#[test]
fn controls_keep_absence_empty_none_and_encrypted_output_distinct() {
    let input = json!([{"role":"user","content":"hello"}]);
    let absent = responses::decode_generation(&json!({"input":input})).unwrap();
    let empty = responses::decode_generation(&json!({"input":input,"reasoning":{}})).unwrap();
    let none=responses::decode_generation(&json!({"input":input,"reasoning":{"effort":"none"},"include":["reasoning.encrypted_content"]})).unwrap();
    assert_ne!(absent.semantic.reasoning(), empty.semantic.reasoning());
    assert_eq!(
        none.semantic.reasoning().effort(),
        Some(ReasoningEffort::None)
    );
    assert!(none.semantic.reasoning().summary.is_absent());
    let t = lower_request(
        &none.semantic,
        &none.fidelity,
        Profile::Responses,
        contract(),
    )
    .unwrap();
    let v = responses::encode_generation(&t).unwrap();
    assert_eq!(v["reasoning"], json!({"effort":"none"}));
    assert_eq!(v["include"], json!(["reasoning.encrypted_content"]));
    assert!(lower_request(&none.semantic, &none.fidelity, Profile::Chat, contract()).is_err());
    for bad in [
        json!({"effort":"none","summary":"auto"}),
        json!({"summary":[]}),
        json!({"summary":true}),
    ] {
        assert!(responses::decode_generation(&json!({"input":input,"reasoning":bad})).is_err());
    }
    let chat =
        chat::decode_generation(&json!({"messages":input,"reasoning_effort":"low"})).unwrap();
    assert_eq!(
        chat.semantic.reasoning().effort(),
        Some(ReasoningEffort::Low)
    );
}

#[test]
fn partial_final_phases_and_resource_limits_fail_without_panics() {
    let state = apply(&[
        StreamEvent::Started,
        start(1, ItemKind::Reasoning),
        StreamEvent::PartStarted {
            item: ItemId::new(1),
            part: PartId::new(1),
            kind: PartKind::ReasoningText,
        },
    ])
    .unwrap();
    assert!(matches!(
        reduce(
            state,
            StreamEvent::Delta {
                item: ItemId::new(1),
                part: PartId::new(1),
                fragment: "x".repeat(MAX_TEXT_BYTES + 1),
                logprobs: vec![]
            }
        ),
        Err(EventError::Limit)
    ));
    let state = apply(&[StreamEvent::Started, start(1, ItemKind::Reasoning)]).unwrap();
    assert!(
        reduce(
            state,
            StreamEvent::ItemFinished {
                item: ItemId::new(1),
                status: ItemLifecycle::Completed,
                replay: Some(token("partial", false))
            }
        )
        .is_err()
    );
    let mut f = FidelityRecords::default();
    let mut owner = ReasoningItem {
        parts: vec![],
        status: ItemLifecycle::Completed,
        replay: Some(ReplayValue::final_value(
            ReplayFormat::ResponsesEncrypted,
            text("x"),
        )),
    };
    for i in 0..MAX_ITEMS {
        f.record_replay(ItemId::new(i as u64), &owner, Some(origin()))
            .unwrap();
    }
    assert!(
        f.record_replay(ItemId::new(MAX_ITEMS as u64), &owner, Some(origin()))
            .is_err()
    );
    owner.replay = Some(ReplayValue::partial(
        ReplayFormat::ResponsesEncrypted,
        text("partial"),
    ));
    assert!(
        f.record_replay(ItemId::new(0), &owner, Some(origin()))
            .is_err()
    );
    owner.replay = Some(ReplayValue::final_value(
        ReplayFormat::ResponsesEncrypted,
        text("final"),
    ));
    assert!(f.record_replay(ItemId::new(0), &owner, None).is_err());
}
#[test]
fn unsupported_snapshot_content_is_not_silently_dropped() {
    let mut d = EventDecoder::new(Profile::Responses);
    d.push(&created()).unwrap();
    d.push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"rs","type":"reasoning","status":"in_progress","summary":[]}})).unwrap();
    assert!(d.push(&json!({"type":"response.output_item.done","output_index":0,"item":reasoning("completed")})).is_err());
}
#[test]
fn encoder_rejects_unbound_replay_before_emitting_it() {
    let mut encoder = EventEncoder::new(Profile::Responses, metadata()).unwrap();
    let f = FidelityRecords::default();
    encoder.encode(&StreamEvent::Started, &f).unwrap();
    assert!(
        encoder
            .encode(
                &StreamEvent::ItemStarted {
                    item: ItemId::new(1),
                    kind: ItemKind::Reasoning,
                    replay: Some(token("partial", false))
                },
                &f
            )
            .is_err()
    );
}
#[test]
fn configuration_update_is_an_ordered_effort_item_not_a_settings_patch() {
    let source = json!({"input":[
        {"type":"configuration_update","reasoning":{"effort":"low"}},
        {"role":"user","content":"next"}
    ]});
    let decoded = responses::decode_generation(&source).unwrap();
    assert!(decoded.semantic.reasoning().effort().is_none());
    let encoded = responses::encode_generation(
        &lower_request(
            &decoded.semantic,
            &decoded.fidelity,
            Profile::Responses,
            contract(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        encoded["input"][0],
        json!({"type":"configuration_update","reasoning":{"effort":"low"}})
    );
    let mut items = decoded.semantic.items().to_vec();
    let Item::ConfigurationUpdate(update) = &mut items[0].1 else {
        panic!("configuration update")
    };
    update.effort = Some(ReasoningEffort::High);
    let replaced = decoded.semantic.clone().with_items(items).unwrap();
    let replaced_wire = responses::encode_generation(
        &lower_request(&replaced, &decoded.fidelity, Profile::Responses, contract()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        replaced_wire["input"][0]["reasoning"]["effort"],
        json!("high")
    );
    let deleted = replaced
        .retain_items(|_, item| !matches!(item, Item::ConfigurationUpdate(_)))
        .unwrap();
    let deleted_wire = responses::encode_generation(
        &lower_request(&deleted, &decoded.fidelity, Profile::Responses, contract()).unwrap(),
    )
    .unwrap();
    assert_eq!(deleted_wire["input"].as_array().unwrap().len(), 1);
    for bad in [
        json!({"type":"configuration_update","reasoning":null}),
        json!({"type":"configuration_update","summary":"auto"}),
        json!({"type":"configuration_update","reasoning":{"effort":"unregistered"}}),
        json!({"type":"configuration_update","reasoning":{"mode":"pro"}}),
    ] {
        assert!(
            responses::decode_generation(&json!({"input":[bad,{"role":"user","content":"next"}]}))
                .is_err()
        );
    }
    assert!(
        lower_request(
            &decoded.semantic,
            &decoded.fidelity,
            Profile::Chat,
            contract()
        )
        .is_err()
    );
    let mut idless = json!({"type":"configuration_update","reasoning":{"effort":"max"}});
    assert!(responses::decode_response(&envelope("completed", json!([idless.clone()]))).is_err());
    idless["id"] = json!("cu");
    let response = responses::decode_response(&envelope("completed", json!([idless]))).unwrap();
    let Item::ConfigurationUpdate(update) = &response.semantic.items()[0].1 else {
        panic!("response update")
    };
    assert_eq!(update.effort, Some(ReasoningEffort::Max));
    let mut d = EventDecoder::new(Profile::Responses);
    d.push(&created()).unwrap();
    let item = json!({"id":"cu","type":"configuration_update","reasoning":{"effort":"low"}});
    d.push(&json!({"type":"response.output_item.added","output_index":0,"item":item}))
        .unwrap();
    assert!(d.push(&json!({"type":"response.output_item.done","output_index":0,"item":{"id":"cu","type":"configuration_update","reasoning":{"effort":"high"}}})).is_err());
    let mut ok = EventDecoder::new(Profile::Responses);
    ok.push(&created()).unwrap();
    ok.push(&json!({"type":"response.output_item.added","output_index":0,"item":item.clone()}))
        .unwrap();
    ok.push(&json!({"type":"response.output_item.done","output_index":0,"item":item.clone()}))
        .unwrap();
    ok.push(&json!({"type":"response.completed","response":envelope("completed", json!([item]))}))
        .unwrap();
    let materialized = ok.materialize().unwrap();
    let Item::ConfigurationUpdate(update) = &materialized.semantic.items()[0].1 else {
        panic!("stream update")
    };
    assert_eq!(update.effort, Some(ReasoningEffort::Low));
}
