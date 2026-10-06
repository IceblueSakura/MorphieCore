//! Independent timestamp admission and typed edits, without Provider or Gateway bindings.
use morphiecore::{
    lowering::generation::{
        GenerationRepresentationContract as Contract, RepresentationError, lower_response,
    },
    protocol::openai::{Profile, chat_envelope, envelope, events::EventEncoder},
    semantic::{
        task::generation::{StreamEvent, StreamTerminal, TerminalDetails},
        value::Presence,
    },
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
fn timestamp_chat_target_keeps_its_integer_requirement_and_exact_integer_value() {
    let decoded = chat_envelope::decode_response_bytes(chat_response("0").as_bytes()).unwrap();
    for source in ["-0.0", "1.25", "1e-9999"] {
        let mut metadata = decoded.metadata.clone();
        metadata.created = number(source);
        assert!(matches!(
            lower_response(
                &decoded.semantic,
                &decoded.fidelity,
                &metadata,
                Profile::Chat,
                Contract::full()
            ),
            Err(RepresentationError::Metadata)
        ));
        assert!(EventEncoder::new(Profile::Chat, metadata).is_err());
    }
    for source in ["0", "9007199254740993"] {
        let mut metadata = decoded.metadata.clone();
        metadata.created = number(source);
        let target = lower_response(
            &decoded.semantic,
            &decoded.fidelity,
            &metadata,
            Profile::Chat,
            Contract::full(),
        )
        .unwrap();
        let wire = chat_envelope::encode_response(&target).unwrap();
        assert_eq!(wire["created"].as_number().unwrap().as_str(), source);
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
            metadata.created = number("-1e-9999");
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

#[test]
fn timestamp_invalid_completion_update_poisoned_encoder_cannot_emit_success() {
    let decoded = envelope::decode_response_bytes(response("0", None).as_bytes()).unwrap();
    let mut encoder = EventEncoder::new(Profile::Responses, decoded.metadata.clone()).unwrap();
    assert!(
        !encoder
            .encode(&StreamEvent::Started, &decoded.fidelity)
            .unwrap()
            .is_empty()
    );
    let mut changed = decoded.metadata;
    changed.context.completed_at = Presence::Value(number("-1e-9999"));
    assert!(encoder.update_metadata(changed).is_err());
    assert!(
        encoder
            .encode(
                &StreamEvent::Terminal {
                    terminal: StreamTerminal::Completed,
                    details: TerminalDetails::default(),
                },
                &decoded.fidelity
            )
            .is_err()
    );
    assert!(encoder.finish().is_err());
}

#[test]
fn timestamp_nonnegative_values_keep_exact_numbers_and_presence() {
    for source in [
        "0",
        "-0.0",
        "0.000",
        "-0.000",
        "0e-9999",
        "-0e-9999",
        "1.25",
        "1e-9999",
        "9007199254740993.125",
    ] {
        let expected = number(source);
        let decoded =
            envelope::decode_response_bytes(response(source, Some(source)).as_bytes()).unwrap();
        assert_eq!(decoded.metadata.created.as_str(), expected.as_str());
        assert_eq!(
            decoded.metadata.context.completed_at,
            Presence::Value(expected.clone())
        );
        let lowered = lower_response(
            &decoded.semantic,
            &decoded.fidelity,
            &decoded.metadata,
            Profile::Responses,
            Contract::full(),
        )
        .unwrap();
        let wire = envelope::encode_response(&lowered).unwrap();
        assert_eq!(
            wire["created_at"].as_number().unwrap().as_str(),
            expected.as_str()
        );
        assert_eq!(
            wire["completed_at"].as_number().unwrap().as_str(),
            expected.as_str()
        );
        let mut encoder = EventEncoder::new(Profile::Responses, decoded.metadata).unwrap();
        for event in [
            StreamEvent::Started,
            StreamEvent::Terminal {
                terminal: StreamTerminal::Completed,
                details: TerminalDetails::default(),
            },
        ] {
            let frames = encoder.encode(&event, &decoded.fidelity).unwrap();
            assert!(!frames.is_empty());
            for frame in frames {
                assert_eq!(
                    frame["response"]["created_at"]
                        .as_number()
                        .unwrap()
                        .as_str(),
                    expected.as_str()
                );
                if frame["type"] == "response.completed" {
                    assert_eq!(
                        frame["response"]["completed_at"]
                            .as_number()
                            .unwrap()
                            .as_str(),
                        expected.as_str()
                    );
                }
            }
        }
        encoder.finish().unwrap();
    }
    for completed in [None, Some("null"), Some("0")] {
        let decoded = envelope::decode_response_bytes(response("0", completed).as_bytes()).unwrap();
        let expected = match completed {
            None => Presence::Absent,
            Some("null") => Presence::Null,
            _ => Presence::Value(number("0")),
        };
        assert_eq!(decoded.metadata.context.completed_at, expected);
        let wire = envelope::encode_response(
            &lower_response(
                &decoded.semantic,
                &decoded.fidelity,
                &decoded.metadata,
                Profile::Responses,
                Contract::full(),
            )
            .unwrap(),
        )
        .unwrap();
        match completed {
            None => assert!(wire.get("completed_at").is_none()),
            Some("null") => assert!(wire["completed_at"].is_null()),
            _ => assert_eq!(wire["completed_at"], 0),
        }
    }
    for invalid in ["null", "false", "\"1\"", "1e9999"] {
        assert!(envelope::decode_response_bytes(response(invalid, None).as_bytes()).is_err());
    }
}
