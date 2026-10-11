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
            context::ServiceTier,
            task::generation::*,
            value::{Presence, Text},
        },
    };
    let metadata = ResponseMetadata {
        id: "c".into(),
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
            message: None,
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
    metadata.context.execution.service_tier = Presence::Value(ServiceTier::Default);
    let mut encoder = EventEncoder::new(Profile::Chat, metadata.clone()).unwrap();
    encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
    metadata.context.execution.service_tier = Presence::Value(ServiceTier::Priority);
    assert!(encoder.update_metadata(metadata).is_err());
    assert!(encoder.finish().is_err());
}
