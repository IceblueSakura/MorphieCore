//! Reasoning ownership, opaque replay, mixed tool continuation and resource failures.
use crate::events_support::*;
use morphiecore::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            Profile, TargetCaps as Contract,
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
fn typed_disabled_summary_is_valid_semantics_without_a_responses_carrier() {
    let mut reasoning = ReasoningRequest::present(Some(ReasoningSummary::Disabled));
    reasoning.activation =
        morphiecore::semantic::value::Presence::Value(ReasoningActivation::Disabled);
    let settings = GenerationSettings {
        instructions: morphiecore::semantic::value::Presence::Value(text("hello")),
        reasoning,
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
    reported
        .context
        .reports
        .latest_mut()
        .expect("report")
        .settings = Some(settings);
    let response = GenerationResponse::new(vec![], Outcome::Completed).unwrap();
    let target = lower_response(
        &response,
        &fidelity,
        &reported,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    assert!(target.projection().iter().any(|stage| stage.loss.is_some()));
    assert_eq!(
        target
            .metadata()
            .context
            .settings()
            .unwrap()
            .reasoning
            .summary(),
        None
    );
    let mut encoder = EventEncoder::new(Profile::Responses, reported).unwrap();
    let wire = encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
    assert_eq!(wire[0]["response"]["reasoning"]["effort"], "none");
    assert!(wire[0]["response"]["reasoning"].get("summary").is_none());
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
        let mut reasoning = ReasoningRequest::present(None);
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
    assert!(decoded.semantic.settings().effort.is_absent());
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
    update.effort = Some(EffortControl {
        scope: EffortScope::Reasoning,
        level: EffortLevel::High,
    });
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
    assert_eq!(
        update.effort,
        Some(EffortControl {
            scope: EffortScope::Reasoning,
            level: EffortLevel::Max
        })
    );
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
    assert_eq!(
        update.effort,
        Some(EffortControl {
            scope: EffortScope::Reasoning,
            level: EffortLevel::Low
        })
    );
    use morphiecore::protocol::{
        openai::{Adapter, Dialect},
        reasoning::ReasoningEffortMapping,
    };
    static MAPPING: ReasoningEffortMapping = ReasoningEffortMapping {
        canonical_model: "m",
        rule: "synthetic.configuration-effort.v1",
        scope: EffortScope::Reasoning,
        native_levels: &[EffortLevel::Low, EffortLevel::Medium, EffortLevel::High],
        encode: &[
            (EffortLevel::Low, EffortLevel::High),
            (EffortLevel::High, EffortLevel::High),
        ],
        decode: &[
            (EffortLevel::Low, EffortLevel::High),
            (EffortLevel::Medium, EffortLevel::High),
            (EffortLevel::High, EffortLevel::High),
        ],
    };
    let mut adapter = Adapter::new(Profile::Responses, Dialect::Standard, None);
    adapter.adaptation.effort_mapping = Some(&MAPPING);
    let mut source = source;
    source["model"] = json!("m");
    let request = adapter
        .decode_request(source.to_string().as_bytes())
        .unwrap();
    let before = request.clone();
    let (wire, rules) = adapter
        .encode_request_with_rules(&request, "m", &Contract::full())
        .unwrap();
    assert_eq!(wire["input"][0]["reasoning"]["effort"], "high");
    assert_eq!(rules, ["synthetic.configuration-effort.v1"]);
    let mut limited = Contract::full();
    limited.model_limits.allowed_effort = Some(vec![EffortLevel::Low].into());
    assert!(matches!(
        adapter.encode_request(&request, "m", &limited),
        Err(morphiecore::adapter::AdapterError::Representation(
            morphiecore::lowering::generation::RepresentationError::Controls
        ))
    ));
    limited.model_limits.allowed_effort = Some(vec![EffortLevel::High].into());
    assert_eq!(
        adapter.encode_request(&request, "m", &limited).unwrap()["input"][0]["reasoning"]["effort"],
        "high"
    );
    assert_eq!(request, before);
    let scoped = morphiecore::protocol::reasoning::ReasoningRequestPolicy {
        canonical_model: "m".into(),
        rule: "synthetic.configuration-scope.v1",
        conversion: morphiecore::protocol::reasoning::ReasoningConversion::Scope {
            from: EffortScope::Reasoning,
            to: EffortScope::Response,
            levels: &[(EffortLevel::Low, EffortLevel::High)],
        },
    }
    .project_request(&request.task.semantic)
    .unwrap()
    .unwrap();
    assert!(
        matches!(&scoped.items()[0].1, Item::ConfigurationUpdate(update)
            if update.effort.unwrap().scope == EffortScope::Response)
    );
    assert_eq!(request, before);
    let item = json!({"id":"cu","type":"configuration_update","reasoning":{"effort":"low"}});
    let mut calibrated = adapter.event_decoder();
    calibrated.push(&created()).unwrap();
    calibrated
        .push(&json!({"type":"response.output_item.added","output_index":0,"item":item}))
        .unwrap();
    calibrated
        .push(&json!({"type":"response.output_item.done","output_index":0,"item":item}))
        .unwrap();
    calibrated
        .push(&json!({"type":"response.completed","response":envelope("completed",json!([item]))}))
        .unwrap();
    let delivered = calibrated.materialize().unwrap();
    assert!(
        matches!(&delivered.semantic.items()[0].1, Item::ConfigurationUpdate(update)
            if update.effort.unwrap().level == EffortLevel::High)
    );
    let mut conflicting = adapter.event_decoder();
    conflicting.push(&created()).unwrap();
    conflicting
        .push(&json!({"type":"response.output_item.added","output_index":0,"item":item}))
        .unwrap();
    let mut changed = item;
    changed["reasoning"]["effort"] = json!("medium");
    assert!(
        conflicting
            .push(&json!({"type":"response.output_item.done","output_index":0,"item":changed}))
            .is_err()
    );
}
