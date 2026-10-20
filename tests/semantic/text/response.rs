//! Refusal and non-success terminals for the text and function-call path.
use morphiecore::{
    lowering::generation::RepresentationError,
    protocol::openai::{
        Profile, ResponseMetadata, TargetCaps as Contract, chat, lowering::lower_response,
        responses,
    },
    semantic::task::generation::*,
};
use serde_json::{Value, json};

fn metadata() -> ResponseMetadata {
    ResponseMetadata {
        storage: None,
        id: Some("response_1".into()),
        model: "fixture-model".into(),
        created: Some(10.into()),
        context: Default::default(),
        instruction_fidelity: Default::default(),
    }
}

fn chat_refusal() -> Value {
    json!({"id":"response_1","object":"chat.completion","created":10,"model":"fixture-model",
        "choices":[{"index":0,"message":{"role":"assistant","content":null,"refusal":"Cannot do that."},"finish_reason":"stop"}],
        "usage":null})
}
fn responses_refusal() -> Value {
    json!({"id":"response_1","object":"response","created_at":10,"model":"fixture-model","status":"completed",
        "output":[{"id":"item_1","type":"message","role":"assistant","status":"completed","content":[{"type":"refusal","refusal":"Cannot do that."}]}],
        "usage":null})
}

#[test]
fn incomplete_and_failed_responses_do_not_encode_as_success() {
    let mut chat_wire = chat_refusal();
    chat_wire["choices"][0]["message"] = json!({"role":"assistant","content":"Partial"});
    chat_wire["choices"][0]["finish_reason"] = json!("length");
    let decoded = chat::decode_response(&chat_wire).unwrap();
    assert_eq!(decoded.semantic.outcome(), Outcome::Incomplete);
    let fidelity = morphiecore::protocol::fidelity::FidelityRecords::default();
    let metadata = metadata();
    let encoded = lower_response(
        &decoded.semantic,
        &fidelity,
        &metadata,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    assert_eq!(
        responses::encode_response(&encoded).unwrap()["status"],
        json!("incomplete")
    );
    let mut failed = responses_refusal();
    failed["status"] = json!("failed");
    failed["output"][0]["status"] = json!("incomplete");
    let decoded = responses::decode_response(&failed).unwrap();
    assert_eq!(decoded.semantic.outcome(), Outcome::Failed);
    let mut fidelity = morphiecore::protocol::fidelity::FidelityRecords::default();
    fidelity
        .record_response_item_id(decoded.semantic.items()[0].0, "item_1")
        .unwrap();
    assert!(matches!(
        lower_response(
            &decoded.semantic,
            &fidelity,
            &metadata,
            Profile::Chat,
            Contract::full()
        ),
        Err(RepresentationError::Terminal)
    ));
    assert_eq!(
        responses::encode_response(
            &lower_response(
                &decoded.semantic,
                &fidelity,
                &metadata,
                Profile::Responses,
                Contract::full()
            )
            .unwrap()
        )
        .unwrap()["status"],
        json!("failed")
    );
}

#[test]
fn content_filter_is_incomplete_not_length_or_refusal_success() {
    for message in [
        json!({"role":"assistant","content":null}),
        json!({"role":"assistant","content":"Partial"}),
        json!({"role":"assistant","content":null,"refusal":"Cannot do that."}),
        json!({"role":"assistant","content":null,"tool_calls":[{"id":"c","type":"function","function":{"name":"lookup","arguments":"{"}}]}),
    ] {
        let mut wire = chat_refusal();
        wire["choices"][0]["message"] = message.clone();
        wire["choices"][0]["finish_reason"] = json!("content_filter");
        let decoded = chat::decode_response(&wire).unwrap();
        assert_eq!(decoded.semantic.outcome(), Outcome::Incomplete);
        assert_eq!(
            decoded.semantic.details().incomplete,
            Some(IncompleteReason::ContentFilter)
        );
        assert!(
            decoded
                .semantic
                .items()
                .iter()
                .all(|(_, i)| i.lifecycle() == Some(ItemLifecycle::Incomplete))
        );
        for profile in [Profile::Chat, Profile::Responses] {
            let target = lower_response(
                &decoded.semantic,
                &decoded.fidelity,
                &decoded.metadata,
                profile,
                Contract::full(),
            );
            let target = target.unwrap();
            if profile == Profile::Chat {
                let encoded = chat::encode_response(&target).unwrap();
                assert_eq!(encoded["choices"][0]["finish_reason"], "content_filter");
                assert_eq!(encoded["choices"][0]["message"], message);
            } else {
                let encoded = responses::encode_response(&target).unwrap();
                assert_eq!(encoded["status"], "incomplete");
                assert_eq!(encoded["incomplete_details"]["reason"], "content_filter");
                let other = responses::decode_response(&encoded).unwrap();
                let back = lower_response(
                    &other.semantic,
                    &other.fidelity,
                    &other.metadata,
                    Profile::Chat,
                    Contract::full(),
                )
                .unwrap();
                assert_eq!(
                    chat::encode_response(&back).unwrap()["choices"][0]["finish_reason"],
                    "content_filter"
                );
            }
        }
    }
}
