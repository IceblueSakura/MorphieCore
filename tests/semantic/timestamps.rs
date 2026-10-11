//! Independent timestamp admission and typed edits, without Provider or Gateway bindings.
use morphiecore::{
    lowering::generation::RepresentationError,
    protocol::openai::{
        Profile, TargetCaps as Contract, envelope, events::EventEncoder, lowering::lower_response,
    },
    semantic::value::Presence,
};
use serde_json::Number;

fn number(source: &str) -> Number {
    source.parse().unwrap()
}
fn response(created: &str, completed: Option<&str>) -> String {
    let completed = completed.map_or(String::new(), |v| format!(",\"completed_at\":{v}"));
    format!(
        r#"{{"id":"r","object":"response","created_at":{created}{completed},"model":"synthetic","status":"completed","output":[]}}"#
    )
}

#[test]
fn timestamp_typed_edits_cannot_bypass_context_lowering_or_event_encoding() {
    let decoded = envelope::decode_response_bytes(response("0", None).as_bytes()).unwrap();
    for completion in [false, true] {
        let mut metadata = decoded.metadata.clone();
        if completion {
            metadata.context.completed_at = Presence::Value(number("-1e-9999"));
            assert!(metadata.context.validate().is_err());
        } else {
            metadata.created = Some(number("-1e-9999"));
        }
        assert!(matches!(
            lower_response(
                &decoded.semantic,
                &decoded.fidelity,
                &metadata,
                Profile::Responses,
                Contract::full()
            ),
            Err(RepresentationError::Metadata)
        ));
        assert!(EventEncoder::new(Profile::Responses, metadata).is_err());
    }
}
