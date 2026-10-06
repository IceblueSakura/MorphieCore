//! Refusal and non-success terminals for the text and function-call path.
use morphiecore::{
    lowering::generation::{
        GenerationRepresentationContract as Contract, RepresentationError, lower_request,
        lower_response,
    },
    protocol::openai::{Profile, ResponseMetadata, chat, responses},
    semantic::task::generation::*,
};
use serde_json::{Value, json};

fn metadata() -> ResponseMetadata {
    ResponseMetadata {
        id: "response_1".into(),
        model: "fixture-model".into(),
        created: 10.into(),
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
fn refusal_is_not_text_and_user_refusal_fails() {
    let decoded = chat::decode_response(&chat_refusal()).unwrap();
    let Item::Message(message) = &decoded.semantic.items()[0].1 else {
        panic!("message");
    };
    assert!(matches!(message.parts[0].content, ContentPart::Refusal(_)));
    let fidelity = morphiecore::protocol::fidelity::FidelityRecords::default();
    let metadata = metadata();
    let chat = lower_response(
        &decoded.semantic,
        &fidelity,
        &metadata,
        Profile::Chat,
        Contract::full(),
    )
    .unwrap();
    assert_eq!(
        chat::encode_response(&chat).unwrap()["choices"][0]["message"]["refusal"],
        json!("Cannot do that.")
    );
    let mut responses_fidelity = morphiecore::protocol::fidelity::FidelityRecords::default();
    responses_fidelity
        .record_response_item_id(ItemId::new(1), "item_1")
        .unwrap();
    let responses = lower_response(
        &decoded.semantic,
        &responses_fidelity,
        &metadata,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    assert_eq!(
        responses::encode_response(&responses).unwrap()["output"][0]["content"][0]["type"],
        json!("refusal")
    );
    let other = responses::decode_response(&responses_refusal()).unwrap();
    let Item::Message(message) = &other.semantic.items()[0].1 else {
        panic!("message");
    };
    let ContentPart::Refusal(text) = &message.parts[0].content else {
        panic!("refusal");
    };
    assert_eq!(text.as_str(), "Cannot do that.");
    let mut history = json!({"messages":[{"role":"user","content":null,"refusal":"no"}]});
    assert!(chat::decode_generation(&history).is_err());
    history["messages"][0]["role"] = json!("assistant");
    history["messages"][0]["content"] = json!("visible");
    assert!(chat::decode_generation(&history).is_err());
}

#[test]
fn incomplete_and_failed_responses_do_not_encode_as_success() {
    let mut chat_wire = chat_refusal();
    chat_wire["choices"][0]["message"] = json!({"role":"assistant","content":"Partial"});
    chat_wire["choices"][0]["finish_reason"] = json!("length");
    let decoded = chat::decode_response(&chat_wire).unwrap();
    assert_eq!(decoded.semantic.outcome(), Outcome::Incomplete);
    assert_eq!(decoded.semantic.continuation(), Continuation::Unreported);
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
fn chat_length_has_the_same_partial_semantics_in_json_and_events() {
    let mut wire = chat_refusal();
    wire["choices"][0]["message"] = json!({"role":"assistant","content":"Partial"});
    wire["choices"][0]["finish_reason"] = json!("length");
    let expected = chat::decode_response(&wire).unwrap().semantic;
    let mut decoder = morphiecore::protocol::openai::events::EventDecoder::new(Profile::Chat);
    decoder.push(&json!({"id":"response_1","object":"chat.completion.chunk","created":10,"model":"fixture-model","choices":[{"index":0,"delta":{"role":"assistant","content":"Partial"},"finish_reason":"length"}]})).unwrap();
    assert!(decoder.finish().is_err());
    decoder.done().unwrap();
    assert_eq!(decoder.materialize().unwrap().semantic, expected);
    assert_eq!(
        expected.details().incomplete,
        Some(morphiecore::semantic::task::generation::IncompleteReason::MaxOutputTokens)
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
            if profile == Profile::Responses && message.get("tool_calls").is_some() {
                assert!(matches!(target, Err(RepresentationError::MessageGrouping)));
                continue;
            }
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

#[test]
fn partial_history_cannot_lose_item_status_on_chat() {
    let d = responses::decode_generation(
        &json!({"input":[crate::events_support::call_item("fc","c","{","incomplete")]}),
    )
    .unwrap();
    assert!(lower_request(&d.semantic, &d.fidelity, Profile::Chat, Contract::full()).is_err());
}

#[test]
fn chat_finish_cannot_replace_mixed_message_and_call_status() {
    use crate::events_support::{call_item, envelope};
    let d=responses::decode_response(&envelope("incomplete",json!([{"id":"m","type":"message","role":"assistant","status":"completed","content":[]},call_item("fc","c","{","incomplete")]))).unwrap();
    let m = metadata();
    let mut items = d.semantic.items().to_vec();
    let owner = items[0].0;
    let Item::ToolCall(call) = &mut items[1].1 else {
        panic!("call");
    };
    call.message = Some(owner);
    let response = GenerationResponse::new(items, Outcome::Incomplete).unwrap();
    assert!(matches!(
        lower_response(&response, &d.fidelity, &m, Profile::Chat, Contract::full()),
        Err(RepresentationError::Terminal)
    ));
}

#[test]
fn empty_message_wire_identity_cannot_collide_with_another_item() {
    let items = vec![
        (
            ItemId::new(1),
            Item::Message(Message {
                phase: None,
                role: MessageRole::Assistant,
                status: ItemLifecycle::Completed,
                parts: vec![],
            }),
        ),
        (
            ItemId::new(2),
            Item::Message(Message {
                phase: None,
                role: MessageRole::Assistant,
                status: ItemLifecycle::Completed,
                parts: vec![Part {
                    id: PartId::new(1),
                    content: ContentPart::Text(crate::events_support::text("hello").into()),
                }],
            }),
        ),
    ];
    let response = GenerationResponse::new(items, Outcome::Completed).unwrap();
    let mut f = morphiecore::protocol::fidelity::FidelityRecords::default();
    f.record_response_item_id(ItemId::new(2), "item_1").unwrap();
    assert!(
        lower_response(
            &response,
            &f,
            &metadata(),
            Profile::Responses,
            Contract::full()
        )
        .is_err()
    );
}

#[test]
fn empty_responses_output_is_not_an_invented_chat_message() {
    let mut wire = responses_refusal();
    wire["output"] = json!([]);
    let decoded = responses::decode_response(&wire).unwrap();
    assert!(decoded.semantic.items().is_empty());
    assert!(
        lower_response(
            &decoded.semantic,
            &decoded.fidelity,
            &metadata(),
            Profile::Responses,
            Contract::full()
        )
        .is_ok()
    );
    assert!(
        lower_response(
            &decoded.semantic,
            &decoded.fidelity,
            &metadata(),
            Profile::Chat,
            Contract::full()
        )
        .is_err()
    );
}
