//! Independent timestamp admission and typed edits, without Provider or Gateway bindings.
use morphiecore::{
    lowering::generation::RepresentationError,
    protocol::openai::{
        Profile, TargetCaps as Contract, chat_envelope, envelope, events::EventEncoder,
        lowering::lower_response,
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
fn chat_response(created: &str) -> String {
    format!(
        r#"{{"id":"r","object":"chat.completion","created":{created},"model":"synthetic","choices":[{{"index":0,"message":{{"role":"assistant","content":"ok"}},"finish_reason":"stop"}}]}}"#
    )
}

#[test]
fn timestamp_unreported_creation_is_not_representable_on_standard_static_or_event_targets() {
    for profile in [Profile::Chat, Profile::Responses] {
        let mut decoded = match profile {
            Profile::Chat => {
                chat_envelope::decode_response_bytes(chat_response("0").as_bytes()).unwrap()
            }
            Profile::Responses => {
                envelope::decode_response_bytes(response("0", None).as_bytes()).unwrap()
            }
        };
        decoded.metadata.created = None;
        decoded.metadata.validate().unwrap();
        assert!(matches!(
            lower_response(
                &decoded.semantic,
                &decoded.fidelity,
                &decoded.metadata,
                profile,
                Contract::full()
            ),
            Err(RepresentationError::Metadata)
        ));
        assert!(EventEncoder::new(profile, decoded.metadata).is_err());
    }
}

#[test]
fn timestamp_raw_decode_rejects_negative_values_even_when_they_underflow() {
    for bad in ["-0.1", "-1e-9999", "-9.99e-9999", "-1E-9999", "-1e9999"] {
        assert!(
            envelope::decode_response_bytes(response(bad, None).as_bytes()).is_err(),
            "created_at {bad}"
        );
        assert!(
            envelope::decode_response_bytes(response("0", Some(bad)).as_bytes()).is_err(),
            "completed_at {bad}"
        );
        assert!(
            chat_envelope::decode_response_bytes(chat_response(bad).as_bytes()).is_err(),
            "Chat created {bad}"
        );
    }
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
