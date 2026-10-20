//! Admission regressions for the existing text subset, not the whole SDK union.
use morphiecore::protocol::openai::{Adapter, Dialect, Profile};
use serde_json::json;

#[test]
fn nullable_reasoning_effort_and_optional_function_headers_follow_their_own_schema() {
    let adapter = Adapter::new(Profile::Chat, Dialect::Standard, None);
    let wire =
        json!({"model":"m","messages":[{"role":"user","content":"hi"}],"reasoning_effort":null});
    let decoded = adapter.decode_request(wire.to_string().as_bytes()).unwrap();
    let contract = morphiecore::protocol::openai::TargetCaps::full();
    assert!(
        adapter
            .encode_request(&decoded, "m", &contract)
            .unwrap()
            .get("reasoning_effort")
            .unwrap()
            .is_null()
    );
    let wire = json!({"id":"r","object":"response","created_at":0,"model":"m","status":"completed","output":[{"id":null,"type":"function_call","call_id":"call","name":"lookup","arguments":"{}","status":null}]});
    let decoded = Adapter::new(Profile::Responses, Dialect::Standard, None)
        .decode_response(wire.to_string().as_bytes())
        .unwrap();
    assert!(
        decoded
            .fidelity
            .response_item_id(morphiecore::semantic::task::generation::ItemId::new(1))
            .is_none()
    );
    assert_eq!(
        decoded.semantic.continuation(),
        morphiecore::semantic::task::generation::Continuation::ToolResults(vec![
            morphiecore::semantic::task::generation::CallReference {
                item: morphiecore::semantic::task::generation::ItemId::new(1),
                call_id: "call",
            },
        ])
    );
}

#[test]
fn known_unrepresentable_event_fields_and_changed_bound_metadata_fail_early() {
    use morphiecore::{
        protocol::{
            fidelity::FidelityRecords,
            openai::{ResponseMetadata, events::EventEncoder},
        },
        semantic::{
            context::ServiceClass,
            task::generation::*,
            value::{Presence, Text},
        },
    };
    let metadata = ResponseMetadata {
        storage: None,
        id: Some("c".into()),
        model: "m".into(),
        created: Some(1.into()),
        context: Default::default(),
        instruction_fidelity: Default::default(),
    };
    let fidelity = FidelityRecords::default();
    for kind in [
        ItemKind::Message {
            phase: Some(Phase::FinalAnswer),
        },
        ItemKind::ToolCall {
            format: ArgumentFormat::Raw,
            call_id: Text::new("call", "test", 128).unwrap(),
            name: Text::new("lookup", "test", 128).unwrap(),
            context: CallContext {
                namespace: Some(Text::new("scope", "test", 128).unwrap()),
                ..Default::default()
            },
        },
    ] {
        let mut encoder = EventEncoder::new(Profile::Chat, metadata.clone()).unwrap();
        encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
        assert!(
            encoder
                .encode(
                    &StreamEvent::ItemStarted {
                        item: ItemId::new(1),
                        kind,
                        replay: None
                    },
                    &fidelity
                )
                .is_err()
        );
        assert!(encoder.finish().is_err());
    }
    let mut metadata = metadata;
    metadata
        .context
        .reports
        .latest_mut()
        .expect("report")
        .service_class = Presence::Value(ServiceClass::StandardProcessing);
    let mut encoder = EventEncoder::new(Profile::Chat, metadata.clone()).unwrap();
    encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
    metadata
        .context
        .reports
        .latest_mut()
        .expect("report")
        .service_class = Presence::Value(ServiceClass::PriorityProcessing);
    assert!(encoder.update_metadata(metadata).is_err());
    assert!(encoder.finish().is_err());
    use morphiecore::protocol::{
        openai::{Adapter, Dialect},
        reasoning::ReasoningEffortMapping,
    };
    static MANY_TO_ONE: ReasoningEffortMapping = ReasoningEffortMapping {
        canonical_model: "m",
        rule: "synthetic.reported-effort.v1",
        scope: EffortScope::Reasoning,
        native_levels: &[EffortLevel::High, EffortLevel::XHigh],
        encode: &[(EffortLevel::High, EffortLevel::High)],
        decode: &[
            (EffortLevel::High, EffortLevel::High),
            (EffortLevel::XHigh, EffortLevel::High),
        ],
    };
    let mut adapter = Adapter::new(Profile::Responses, Dialect::Standard, None);
    adapter.adaptation.effort_mapping = Some(&MANY_TO_ONE);
    let mut decoder = adapter.event_decoder();
    decoder
        .push(&json!({"type":"response.created","response":{
            "id":"r","object":"response","created_at":1,"model":"m","status":"in_progress",
            "reasoning":{"effort":"high"},"output":[]}}))
        .unwrap();
    assert!(
        decoder
            .push(&json!({"type":"response.in_progress","response":{
            "id":"r","object":"response","created_at":1,"model":"m","status":"in_progress",
            "reasoning":{"effort":"xhigh"},"output":[]}}))
            .is_err()
    );
    assert!(decoder.materialize().is_err());
    let mut decoder = morphiecore::protocol::openai::events::EventDecoder::new(Profile::Responses);
    let mut created = json!({"type":"response.created","response":{
            "id":"r","object":"response","created_at":1,"model":"m","status":"in_progress",
            "instructions":[{"role":"developer","content":"A"}],"output":[]}});
    decoder.push(&created).unwrap();
    created["type"] = json!("response.in_progress");
    decoder.push(&created).unwrap();
    created["response"]["instructions"][0]["content"] = json!("B");
    assert!(decoder.push(&created).is_err());
    assert!(decoder.materialize().is_err());
}
