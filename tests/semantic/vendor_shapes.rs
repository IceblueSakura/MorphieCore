//! Boundary-specific derived views, reported-fact policy and structural admission.
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::{
        GenerationRepresentationContract as Contract, ReportedFactPolicy, RepresentationError,
        lower_response,
    },
    protocol::openai::{Profile, envelope, events::EventDecoder},
    semantic::{
        task::generation::{PartKind, StreamEvent},
        value::ReplayOrigin,
    },
};
use serde_json::{Value, json};
fn body() -> Value {
    json!({"id":"r1","object":"response","created_at":2,"status":"completed","model":"m",
        "output":[{"id":"m1","type":"message","role":"assistant","status":"completed",
            "content":[{"type":"output_text","text":"pong","annotations":[]}]}],
        "usage":{"input_tokens":3,"input_tokens_details":{"cached_tokens":0},
            "output_tokens":2,"output_tokens_details":{"reasoning_tokens":1},"total_tokens":5}})
}
fn xiaomi() -> Adapter {
    Adapter::new(
        Profile::Responses,
        Dialect::Xiaomi,
        Some(ReplayOrigin::new("xiaomi-fixture").unwrap()),
    )
}
#[test]
fn standard_absence_is_not_globally_defaulted_and_settings_are_not_invented() {
    let decoded = envelope::decode_response_bytes(body().to_string().as_bytes()).unwrap();
    assert_eq!(
        decoded.semantic.usage().unwrap().input_cache_write_tokens,
        None
    );
    assert_eq!(
        decoded.semantic.usage().unwrap().cached_input_tokens,
        Some(0)
    );
    assert!(decoded.metadata.context.settings.is_none());
    let target = lower_response(
        &decoded.semantic,
        &decoded.fidelity,
        &decoded.metadata,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    let wire = envelope::encode_response(&target).unwrap();
    assert!(
        wire["usage"]["input_tokens_details"]
            .get("cache_write_tokens")
            .is_none()
    );
    for key in [
        "tools",
        "tool_choice",
        "parallel_tool_calls",
        "instructions",
    ] {
        assert!(wire.get(key).is_none());
    }
    let strict = Contract {
        reported_facts: ReportedFactPolicy::StrictComplete,
        ..Contract::full()
    };
    assert!(matches!(
        lower_response(
            &decoded.semantic,
            &decoded.fidelity,
            &decoded.metadata,
            Profile::Responses,
            strict.clone()
        ),
        Err(RepresentationError::ReportedFacts)
    ));
    let complete =
        envelope::decode_response_bytes(crate::wire::response(2).to_string().as_bytes()).unwrap();
    assert!(
        lower_response(
            &complete.semantic,
            &complete.fidelity,
            &complete.metadata,
            Profile::Responses,
            strict
        )
        .is_ok()
    );
}
#[test]
fn xiaomi_derived_view_is_validated_at_finality_and_never_reencoded() {
    let adapter = xiaomi();
    let mut matching = body();
    matching["output_text"] = json!("pong");
    let decoded = adapter
        .decode_response(matching.to_string().as_bytes())
        .unwrap();
    assert!(
        adapter
            .encode_response(&decoded, &Contract::full())
            .unwrap()
            .get("output_text")
            .is_none()
    );
    assert!(
        envelope::decode_response_bytes(matching.to_string().as_bytes()).is_err(),
        "standard profile does not inherit vendor fields"
    );
    matching["output_text"] = json!("p0ng");
    assert!(
        adapter
            .decode_response(matching.to_string().as_bytes())
            .is_err()
    );
    let created = json!({"type":"response.created","sequence_number":0,"response":{
        "id":"r1","object":"response","created_at":2,"model":"m","status":"in_progress","output":[],"output_text":"po"}});
    assert!(adapter.event_decoder().push(&created).is_ok());
    let mut malformed = created;
    malformed["response"]["output_text"] = json!(5);
    assert!(adapter.event_decoder().push(&malformed).is_err());
}
#[test]
fn adapters_do_not_relax_unknown_fields_or_required_output_structure() {
    let adapter = xiaomi();
    let mut unknown = body();
    unknown["mystery_field"] = json!(1);
    assert!(
        adapter
            .decode_response(unknown.to_string().as_bytes())
            .is_err()
    );
    let mut absent = body();
    absent.as_object_mut().unwrap().remove("output");
    assert!(
        adapter
            .decode_response(absent.to_string().as_bytes())
            .is_err()
    );
}
#[test]
fn reasoning_text_parts_are_admitted_in_content_part_events() {
    let mut decoder = EventDecoder::new(Profile::Responses);
    decoder.push(&json!({"type":"response.created","sequence_number":0,"response":{
        "id":"r1","object":"response","created_at":2,"model":"m","status":"in_progress","output":[]}})).unwrap();
    decoder
        .push(
            &json!({"type":"response.output_item.added","sequence_number":1,"output_index":0,
        "item":{"id":"rs1","type":"reasoning","summary":[],"content":[],"status":"in_progress"}}),
        )
        .unwrap();
    let events = decoder
        .push(
            &json!({"type":"response.content_part.added","sequence_number":2,"output_index":0,
        "item_id":"rs1","content_index":0,"part":{"type":"reasoning_text","text":""}}),
        )
        .unwrap();
    assert!(matches!(
        &events[0],
        StreamEvent::PartStarted {
            kind: PartKind::ReasoningText,
            ..
        }
    ));
    decoder
        .push(
            &json!({"type":"response.reasoning_text.delta","sequence_number":3,"output_index":0,
        "item_id":"rs1","content_index":0,"delta":"think"}),
        )
        .unwrap();
    decoder
        .push(
            &json!({"type":"response.reasoning_text.done","sequence_number":4,"output_index":0,
        "item_id":"rs1","content_index":0,"text":"think"}),
        )
        .unwrap();
    assert!(
        !decoder
            .push(
                &json!({"type":"response.content_part.done","sequence_number":5,"output_index":0,
        "item_id":"rs1","content_index":0,"part":{"type":"reasoning_text","text":"think"}})
            )
            .unwrap()
            .is_empty()
    );
}
