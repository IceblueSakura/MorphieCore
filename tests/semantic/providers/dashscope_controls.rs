use crate::events_support::{metadata, text};
use morphiecore::{
    protocol::{
        DecodedRequest, GenerationRequestEnvelope as Request,
        fidelity::FidelityRecords,
        openai::{Dialect, GenerationApi, GenerationCodec, TargetContract},
    },
    semantic::{task::generation::*, value::Presence},
};
use serde_json::{Value, json};

fn message(role: MessageRole, part: u64, value: &str) -> Item {
    Item::Message(Message {
        participant: None,
        role,
        status: ItemLifecycle::Completed,
        phase: None,
        parts: vec![Part::new(
            PartId::new(part),
            ContentPart::Text(TextContent::new(text(value), vec![], Presence::Absent).unwrap()),
        )],
    })
}
#[test]
fn native_history_reading_has_independent_input_and_output_oracles() {
    let adapter = GenerationCodec::new(GenerationApi::Chat, Dialect::Bailian, None);
    for enabled in [false, true] {
        let wire = json!({"model":"m","preserve_thinking":enabled,"messages":[
                {"role":"assistant","reasoning_content":"old thought","content":"old answer"},
                {"role":"user","content":"again"}]});
        let input = adapter
            .decode_client_request(wire.to_string().as_bytes())
            .unwrap();
        let public = GenerationCodec::new(GenerationApi::Chat, Dialect::MorphieCore, None)
            .decode_client_request(wire.to_string().as_bytes())
            .unwrap();
        assert_eq!(
            public.task.semantic.reasoning().read_history,
            Presence::Value(enabled)
        );
        let endpoint = morphiecore::topology::catalog::API_KEY_BINDINGS
            .iter()
            .find(|binding| binding.endpoint_prefix == "dashscope-flash")
            .unwrap()
            .endpoint(morphiecore::topology::GenerationProtocol::OpenAiChat)
            .unwrap();
        let (prepared, losses) = endpoint.encode_request_with_rules(&public).unwrap();
        assert_eq!(prepared["preserve_thinking"], enabled);
        assert!(losses.is_empty());
        assert_eq!(
            input.task.semantic.reasoning().read_history,
            Presence::Value(enabled)
        );
        assert!(
            matches!(&input.task.semantic.items()[0].1, Item::Reasoning(r)
                if r.replay.is_none() && r.parts == vec![(PartId::new(1), ReasoningContent::Text(text("old thought")))])
        );
        let mut settings = GenerationSettings {
            reasoning: ReasoningRequest::present(None),
            ..Default::default()
        };
        settings.reasoning.read_history = Presence::Value(enabled);
        let semantic = GenerationRequest::from_settings(
            vec![
                (
                    ItemId::new(1),
                    Item::Reasoning(ReasoningItem {
                        parts: vec![(PartId::new(1), ReasoningContent::Text(text("old thought")))],
                        replay: None,
                        status: ItemLifecycle::Completed,
                    }),
                ),
                (
                    ItemId::new(2),
                    message(MessageRole::Assistant, 2, "old answer"),
                ),
                (ItemId::new(3), message(MessageRole::User, 3, "again")),
            ],
            settings,
        )
        .unwrap();
        let request = Request::from_generation(
            DecodedRequest {
                evidence: Default::default(),
                semantic,
                fidelity: FidelityRecords::default(),
            },
            "m",
        )
        .unwrap();
        let before = request.clone();
        let (encoded, rules) = adapter
            .encode_request_with_rules(&request, "m", &TargetContract::full())
            .unwrap();
        assert_eq!(encoded, wire);
        assert!(rules.is_empty());
        assert_eq!(request, before);
        assert!(
            GenerationCodec::new(GenerationApi::Responses, Dialect::OpenRouter, None)
                .encode_request(&request, "m", &TargetContract::full())
                .is_err()
        );
    }
    for value in [Value::Null, json!("true"), json!(1), json!({})] {
        assert!(
            adapter
                .decode_client_request(
                    json!({"model":"m",
                "messages":[{"role":"user","content":"go"}],"preserve_thinking":value})
                    .to_string()
                    .as_bytes()
                )
                .is_err()
        );
    }
}
#[test]
fn native_static_reasoning_and_usage_have_independent_delivery_expectations() {
    let adapter = GenerationCodec::new(GenerationApi::Chat, Dialect::Bailian, None);
    let wire = json!({"id":"r","object":"chat.completion","created":0,"model":"synthetic",
            "choices":[{"index":0,"message":{"role":"assistant","content":"answer","reasoning_content":"thought"},
                "finish_reason":"stop"}],
            "usage":{"prompt_tokens":10,"completion_tokens":6,"total_tokens":16,
                "prompt_tokens_details":{"cached_tokens":2,"text_tokens":10},
                "completion_tokens_details":{"reasoning_tokens":4,"text_tokens":6}}});
    let input = adapter
        .decode_response(wire.to_string().as_bytes())
        .unwrap();
    let usage = input.semantic.usage().unwrap();
    assert_eq!(usage.reasoning_tokens, Some(4));
    assert_eq!(usage.output_text_tokens, Some(6));
    let semantic = GenerationResponse::new(
        vec![
            (
                ItemId::new(1),
                Item::Reasoning(ReasoningItem {
                    status: ItemLifecycle::Completed,
                    replay: None,
                    parts: vec![(PartId::new(1), ReasoningContent::Text(text("thought")))],
                }),
            ),
            (ItemId::new(2), message(MessageRole::Assistant, 2, "answer")),
        ],
        Outcome::Completed,
    )
    .unwrap()
    .with_details(TerminalDetails {
        completion: Some(CompletionCause::NaturalOrStopSequence),
        ..Default::default()
    })
    .unwrap()
    .with_usage(Some({
        let mut usage = Usage::operation(10, 6, 16);
        usage.cached_input_tokens = Some(2);
        usage.input_text_tokens = Some(10);
        usage.reasoning_tokens = Some(4);
        usage.output_text_tokens = Some(6);
        usage
    }))
    .unwrap();
    let output = morphiecore::protocol::DecodedResponse {
        evidence: Default::default(),
        semantic,
        fidelity: FidelityRecords::default(),
        metadata: metadata(),
    };
    let encoded = adapter
        .encode_response(&output, &TargetContract::full())
        .unwrap();
    assert_eq!(encoded, wire);
}
