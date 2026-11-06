//! Reasoning ownership, opaque replay, mixed tool continuation and resource failures.
use crate::events_support::*;
use morphiecore::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            GenerationApi, TargetContract as Contract,
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
    let mut reasoning = ReasoningRequest::present(None);
    reasoning.presentation.visibility = Some(ReasoningDisplay::Omitted);
    reasoning.activation = Some(ReasoningActivation::Disabled);
    let settings = GenerationSettings {
        instructions: Some(text("hello").into()),
        reasoning,
        ..Default::default()
    };
    let request = GenerationRequest::from_settings(vec![], settings.clone()).unwrap();
    assert_eq!(
        request.reasoning().presentation.visibility.as_ref(),
        Some(&ReasoningDisplay::Omitted)
    );
    let fidelity = FidelityRecords::default();
    for profile in [GenerationApi::Responses, GenerationApi::Chat] {
        assert_eq!(
            lower_request(&request, &fidelity, profile, Contract::full()).err(),
            Some(
                morphiecore::protocol::conversion::generation::RepresentationError::Admission(
                    Feature::ReasoningDisplay,
                )
                .into()
            )
        );
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
        GenerationApi::Responses,
        Contract::full(),
    )
    .unwrap();
    assert!(
        target
            .projection()
            .iter()
            .any(|stage| stage.change().is_loss())
    );
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
    let mut encoder = EventEncoder::new(GenerationApi::Responses, reported).unwrap();
    let wire = encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
    assert_eq!(wire[0]["response"]["reasoning"]["effort"], "none");
    assert!(wire[0]["response"]["reasoning"].get("summary").is_none());
}

#[test]
fn standard_summary_presence_and_deprecated_alias_have_independent_wire_expectations() {
    for (summary, wire) in [
        (None, json!({})),
        (None, json!({"summary":null})),
        (Some(ReasoningSummary::Auto), json!({"summary":"auto"})),
        (
            Some(ReasoningSummary::Concise),
            json!({"summary":"concise"}),
        ),
        (
            Some(ReasoningSummary::Detailed),
            json!({"summary":"detailed"}),
        ),
    ] {
        let decoded =
            responses::decode_generation(&json!({"input":"hello","reasoning":wire})).unwrap();
        assert_eq!(decoded.semantic.reasoning().presentation.summary, summary);
        let mut reasoning = ReasoningRequest::present(None);
        reasoning.presentation.visibility = summary.map(|_| ReasoningDisplay::Summary);
        reasoning.presentation.summary = summary;
        let request = GenerationRequest::from_settings(
            vec![],
            GenerationSettings {
                instructions: Some(text("hello").into()),
                reasoning,
                ..Default::default()
            },
        )
        .unwrap();
        let fidelity = FidelityRecords::default();
        let target = lower_request(
            &request,
            &fidelity,
            GenerationApi::Responses,
            Contract::full(),
        )
        .unwrap();
        assert_eq!(
            responses::encode_generation(&target)
                .unwrap()
                .get("reasoning"),
            summary.map(|_| wire).as_ref()
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
            GenerationApi::Responses,
            Contract::full(),
        )
        .unwrap();
        let wire = responses::encode_generation(&target).unwrap();
        if value.is_null() {
            assert!(wire.get("reasoning").is_none());
        } else {
            assert_eq!(wire["reasoning"], json!({"summary":value}));
        }
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
    let mut d = EventDecoder::new(GenerationApi::Responses);
    d.push(&created()).unwrap();
    d.push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"rs","type":"reasoning","status":"in_progress","summary":[]}})).unwrap();
    assert!(d.push(&json!({"type":"response.output_item.done","output_index":0,"item":reasoning("completed")})).is_err());
}
#[test]
fn encoder_rejects_unbound_replay_before_emitting_it() {
    let mut encoder = EventEncoder::new(GenerationApi::Responses, metadata()).unwrap();
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
    assert!(decoded.semantic.settings().effort.is_none());
    let encoded = responses::encode_generation(
        &lower_request(
            &decoded.semantic,
            &decoded.fidelity,
            GenerationApi::Responses,
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
        &lower_request(
            &replaced,
            &decoded.fidelity,
            GenerationApi::Responses,
            contract(),
        )
        .unwrap(),
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
        &lower_request(
            &deleted,
            &decoded.fidelity,
            GenerationApi::Responses,
            contract(),
        )
        .unwrap(),
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
    let chat = lower_request(
        &decoded.semantic,
        &decoded.fidelity,
        GenerationApi::Chat,
        contract(),
    )
    .unwrap();
    assert_eq!(
        morphiecore::protocol::openai::chat::encode_generation(&chat).unwrap()["reasoning_effort"],
        "low"
    );
    let mut idless = json!({"type":"configuration_update","reasoning":{"effort":"max"}});
    assert!(responses::decode_response(&envelope("completed", json!([idless.clone()]))).is_err());
    idless["id"] = json!("cu");
    assert!(responses::decode_response(&envelope("completed", json!([idless]))).is_err());
    let response = GenerationResponse::new(
        vec![decoded.semantic.items()[0].clone()],
        Outcome::Completed,
    )
    .unwrap();
    assert!(
        lower_response(
            &response,
            &decoded.fidelity,
            &metadata(),
            GenerationApi::Responses,
            contract()
        )
        .is_err()
    );
    let mut d = EventDecoder::new(GenerationApi::Responses);
    d.push(&created()).unwrap();
    let item = json!({"id":"cu","type":"configuration_update","reasoning":{"effort":"low"}});
    assert!(
        d.push(&json!({"type":"response.output_item.added","output_index":0,"item":item}))
            .is_err()
    );
    use morphiecore::protocol::{
        openai::{Dialect, GenerationCodec},
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
    let mut adapter = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
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
    assert_eq!(
        adapter.encode_request(&request, "m", &limited).unwrap()["input"][0]["reasoning"]["effort"],
        "low"
    );
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
    .project_request(&request.task.semantic, |_| false)
    .unwrap()
    .unwrap();
    assert!(
        matches!(&scoped.items()[0].1, Item::ConfigurationUpdate(update)
            if update.effort.unwrap().scope == EffortScope::Response)
    );
    assert_eq!(request, before);
}

#[test]
fn subsequent_response_effort_replaces_off_and_applies_to_tool_continuation() {
    use morphiecore::protocol::openai::{Dialect, GenerationCodec};
    let codec = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    let chat = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None);
    let mut target = Contract::full();
    // Exercise the ordinary catalog's activation defaults, not the all-native table.
    target.dispositions = morphiecore::lowering::disposition::Dispositions::text_images();
    for (initial, updated, expected) in [("low", "none", "none"), ("none", "high", "high")] {
        let wire = json!({"model":"m","reasoning":{"effort":initial},"input":[
            {"role":"user","content":"go"},
            {"type":"configuration_update","id":null,"reasoning":{"effort":updated}}
        ]});
        let source = codec
            .decode_client_request(wire.to_string().as_bytes())
            .unwrap();
        let before = source.clone();
        assert_eq!(
            chat.encode_request(&source, "m", &target).unwrap()["reasoning_effort"],
            expected
        );
        assert_eq!(source, before);
    }
    let wire = json!({"model":"m","reasoning":{"effort":"low"},"input":[
        {"role":"user","content":"go"},
        {"type":"configuration_update","reasoning":{"effort":"medium"}},
        {"type":"function_call","call_id":"call","name":"read","arguments":"{}"},
        {"type":"function_call_output","call_id":"call","output":"result"},
        {"type":"configuration_update","reasoning":{"effort":"high"}}
    ]});
    let source = codec
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    let resolved = source.task.semantic.resolved_configuration();
    assert_eq!(resolved.effort.unwrap().level, EffortLevel::High);
    assert!(resolved.pending.is_none());
    let native = codec
        .encode_request(&source, "m", &Contract::full())
        .unwrap();
    assert_eq!(native["reasoning"]["effort"], "low");
    assert_eq!(native["input"][4]["reasoning"]["effort"], "high");
    for invalid in [
        json!([{"type":"configuration_update","reasoning":{"effort":"low"}},
            {"type":"configuration_update","reasoning":{"effort":"high"}}]),
        json!([{"type":"configuration_update","reasoning":{"effort":null}}]),
    ] {
        assert!(
            codec
                .decode_client_request(json!({"model":"m","input":invalid}).to_string().as_bytes())
                .is_err()
        );
    }
}
