//! A standard Responses target cannot carry explicit message-call ownership.
use crate::events_support::{metadata, text};
use morphiecore::{
    lowering::{
        events::check_event,
        generation::{
            GenerationRepresentationContract as Contract, RepresentationError, lower_request,
            lower_response,
        },
    },
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            Profile, chat,
            events::{EventDecoder, EventEncoder},
            responses,
        },
    },
    semantic::task::generation::*,
};
use serde_json::{Value, json};

fn source(content: Value, finish: &str) -> Value {
    json!({"id":"r","object":"chat.completion","created":0,"model":"synthetic","choices":[{"index":0,"message":{"role":"assistant","content":content,"tool_calls":[{"id":"c","type":"function","function":{"name":"lookup","arguments":"{}"}}]},"finish_reason":finish}],"usage":null})
}

#[test]
fn attached_calls_fail_responses_request_and_static_projection_without_changing_ir() {
    for content in [Value::Null, json!("Checking.")] {
        for finish in ["tool_calls", "length", "content_filter"] {
            let wire = source(content.clone(), finish);
            let decoded = chat::decode_response(&wire).unwrap();
            let original = decoded.semantic.clone();
            assert!(matches!(
                lower_response(
                    &decoded.semantic,
                    &decoded.fidelity,
                    &decoded.metadata,
                    Profile::Responses,
                    Contract::full()
                ),
                Err(RepresentationError::MessageGrouping)
            ));
            assert_eq!(decoded.semantic, original);
            let native = lower_response(
                &decoded.semantic,
                &decoded.fidelity,
                &decoded.metadata,
                Profile::Chat,
                Contract::full(),
            )
            .unwrap();
            assert_eq!(chat::encode_response(&native).unwrap(), wire);
            if finish == "tool_calls" {
                let request = GenerationRequest::new(
                    decoded.semantic.items().to_vec(),
                    GenerationControls::default(),
                )
                .unwrap();
                assert!(matches!(
                    lower_request(
                        &request,
                        &decoded.fidelity,
                        Profile::Responses,
                        Contract::full()
                    ),
                    Err(RepresentationError::MessageGrouping)
                ));
            }
        }
    }
}

#[test]
fn native_chat_delivery_and_history_replay_keep_explicit_groups() {
    let decoded = chat::decode_response(&source(json!("Checking."), "tool_calls")).unwrap();
    let target = lower_response(
        &decoded.semantic,
        &decoded.fidelity,
        &decoded.metadata,
        Profile::Chat,
        Contract::full(),
    )
    .unwrap();
    let delivered = chat::encode_response(&target).unwrap();
    let expected = json!({"messages":[
        {"role":"user","content":"lookup"},
        {"role":"assistant","content":"Checking.","tool_calls":[{"id":"c","type":"function","function":{"name":"lookup","arguments":"{}"}}]},
        {"role":"tool","tool_call_id":"c","content":"done"},
        {"role":"assistant","content":null,"tool_calls":[{"id":"next","type":"function","function":{"name":"lookup","arguments":"{\"n\":2}"}}]},
        {"role":"tool","tool_call_id":"next","content":"second"}
    ]});
    let mut replay = expected.clone();
    replay["messages"][1] = delivered["choices"][0]["message"].clone();
    let history = chat::decode_generation(&replay).unwrap();
    let groups: Vec<_> = history.semantic.message_groups().collect();
    assert_eq!(groups.len(), 2);
    assert_eq!(
        groups[0].calls().map(|c| c.call_id).collect::<Vec<_>>(),
        ["c"]
    );
    assert_eq!(
        groups[1].calls().map(|c| c.call_id).collect::<Vec<_>>(),
        ["next"]
    );
    assert_ne!(
        groups[0].owner(),
        decoded.semantic.message_groups().next().unwrap().owner()
    );
    assert_eq!(history.semantic.continuation(), Continuation::Unreported);
    let native = lower_request(
        &history.semantic,
        &history.fidelity,
        Profile::Chat,
        Contract::full(),
    )
    .unwrap();
    assert_eq!(chat::encode_generation(&native).unwrap(), expected);
    assert_eq!(
        lower_request(
            &history.semantic,
            &history.fidelity,
            Profile::Responses,
            Contract::full()
        )
        .err(),
        Some(RepresentationError::MessageGrouping),
    );
}

#[test]
fn grouping_checks_follow_final_membership_after_insert_replace_reorder_and_delete() {
    let decoded = chat::decode_generation(&json!({"messages":[
        {"role":"assistant","content":null,"tool_calls":[{"id":"a","type":"function","function":{"name":"lookup","arguments":"{}"}}]},
        {"role":"assistant","content":"next","tool_calls":[{"id":"b","type":"function","function":{"name":"lookup","arguments":"{}"}}]}
    ]})).unwrap();
    let mut reordered = decoded.semantic.items()[2..].to_vec();
    reordered.extend_from_slice(&decoded.semantic.items()[..2]);
    reordered.insert(
        0,
        (
            ItemId::new(90),
            Item::Message(Message {
                role: MessageRole::User,
                phase: None,
                status: ItemLifecycle::Completed,
                parts: vec![Part {
                    id: PartId::new(90),
                    content: ContentPart::Text(text("inserted").into()),
                }],
            }),
        ),
    );
    let Item::Message(owner) = &mut reordered[1].1 else {
        panic!("owner")
    };
    owner.parts[0].content = ContentPart::Text(text("replacement").into());
    let edited = decoded.semantic.clone().with_items(reordered).unwrap();
    assert!(matches!(
        lower_request(
            &edited,
            &decoded.fidelity,
            Profile::Responses,
            Contract::full()
        ),
        Err(RepresentationError::MessageGrouping)
    ));
    let deleted = edited
        .retain_items(|_, item| !matches!(item, Item::ToolCall(_)))
        .unwrap();
    let target = lower_request(
        &deleted,
        &decoded.fidelity,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    assert_eq!(
        responses::encode_generation(&target).unwrap()["input"],
        json!([
            {"type":"message","role":"user","content":[{"type":"input_text","text":"inserted"}]},
            {"type":"message","role":"assistant","content":[{"type":"output_text","text":"replacement","annotations":[]}]},
            {"type":"message","role":"assistant","content":[]}
        ])
    );
}

#[test]
fn independent_empty_owner_and_calls_preserve_identity_phase_and_status() {
    // Independent wire expectations, not Chat output with silently cleared links.
    let wire = json!({"id":"r","object":"response","created_at":0,"model":"synthetic","status":"incomplete","usage":null,"output":[
        {"id":"owner","type":"message","role":"assistant","phase":"commentary","content":[],"status":"incomplete"},
        {"id":"call","type":"function_call","call_id":"c","name":"lookup","arguments":"{}","status":"completed"}
    ]});
    let decoded = responses::decode_response(&wire).unwrap();
    assert!(matches!(&decoded.semantic.items()[0].1,
        Item::Message(m) if m.parts.is_empty() && m.phase == Some(Phase::Commentary)
            && m.status == ItemLifecycle::Incomplete));
    assert!(matches!(&decoded.semantic.items()[1].1,
        Item::ToolCall(c) if c.message.is_none() && c.status == ItemLifecycle::Completed));
    assert_eq!(
        decoded
            .semantic
            .message_groups()
            .next()
            .unwrap()
            .calls()
            .len(),
        0
    );
    let target = lower_response(
        &decoded.semantic,
        &decoded.fidelity,
        &decoded.metadata,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    assert_eq!(responses::encode_response(&target).unwrap(), wire);
    let request = responses::decode_generation(&json!({"input":[
        {"type":"message","role":"assistant","content":[]},
        {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"}
    ]}))
    .unwrap();
    let target = lower_request(
        &request.semantic,
        &request.fidelity,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    assert_eq!(
        responses::encode_generation(&target).unwrap()["input"],
        json!([
            {"type":"message","role":"assistant","content":[]},
            {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"}
        ])
    );
    let response =
        GenerationResponse::new(request.semantic.items().to_vec(), Outcome::Completed).unwrap();
    assert!(matches!(
        lower_response(
            &response,
            &request.fidelity,
            &metadata(),
            Profile::Chat,
            Contract::full()
        ),
        Err(RepresentationError::MessageGrouping)
    ));
    // A new typed link must be rejected even when intake declared independent items.
    let mut items = request.semantic.items().to_vec();
    let owner = items[0].0;
    let Item::ToolCall(call) = &mut items[1].1 else {
        panic!("call")
    };
    call.message = Some(owner);
    let attached = request.semantic.with_items(items).unwrap();
    assert!(matches!(
        lower_request(
            &attached,
            &request.fidelity,
            Profile::Responses,
            Contract::full()
        ),
        Err(RepresentationError::MessageGrouping)
    ));
    let response = GenerationResponse::new(attached.items().to_vec(), Outcome::Completed).unwrap();
    assert!(matches!(
        lower_response(
            &response,
            &request.fidelity,
            &metadata(),
            Profile::Responses,
            Contract::full()
        ),
        Err(RepresentationError::MessageGrouping)
    ));
}

#[test]
fn empty_owner_still_participates_in_wire_identity_checks() {
    let decoded = responses::decode_response(&json!({"id":"r","object":"response","created_at":0,"model":"synthetic","status":"completed","output":[
        {"id":"owner","type":"message","role":"assistant","content":[],"status":"completed"},
        {"id":"call","type":"function_call","call_id":"c","name":"lookup","arguments":"{}","status":"completed"}
    ]})).unwrap();
    let mut collision = FidelityRecords::default();
    collision
        .record_response_item_id(ItemId::new(2), "item_1")
        .unwrap();
    assert!(matches!(
        lower_response(
            &decoded.semantic,
            &collision,
            &decoded.metadata,
            Profile::Responses,
            Contract::full()
        ),
        Err(RepresentationError::Metadata)
    ));
}

#[test]
fn chat_stream_rejects_at_the_first_attached_call_and_cannot_emit_a_terminal() {
    for content in [Value::Null, json!("Checking.")] {
        let chunk = |delta: Value, finish: Value| json!({"id":"r","object":"chat.completion.chunk","created":0,"model":"synthetic","choices":[{"index":0,"delta":delta,"finish_reason":finish}]});
        let mut decoder = EventDecoder::new(Profile::Chat);
        let mut events = vec![];
        for wire in [
            chunk(
                json!({"role":"assistant","content":content,"tool_calls":[{"index":0,"id":"c","type":"function","function":{"name":"lookup","arguments":"{"}}]}),
                Value::Null,
            ),
            chunk(
                json!({"tool_calls":[{"index":0,"function":{"arguments":"}"}}]}),
                Value::Null,
            ),
            chunk(json!({}), json!("tool_calls")),
        ] {
            events.extend(decoder.push(&wire).unwrap());
        }
        events.extend(decoder.done().unwrap());
        let decoded = decoder.materialize().unwrap();
        assert_eq!(
            decoded
                .semantic
                .message_groups()
                .next()
                .unwrap()
                .calls()
                .len(),
            1
        );
        let mut encoder = EventEncoder::new(Profile::Responses, decoded.metadata.clone()).unwrap();
        let mut state = StreamState::new();
        let mut prefix = vec![];
        for event in &events {
            if matches!(
                event,
                StreamEvent::ItemStarted {
                    kind: ItemKind::ToolCall {
                        message: Some(_),
                        ..
                    },
                    ..
                }
            ) {
                assert_eq!(
                    check_event(&state, event, Profile::Responses, &Contract::full()),
                    Err(RepresentationError::MessageGrouping)
                );
                assert!(encoder.encode(event, &decoded.fidelity).is_err());
                assert!(
                    encoder
                        .encode(events.last().unwrap(), &decoded.fidelity)
                        .is_err()
                );
                assert!(encoder.finish().is_err());
                break;
            }
            prefix.extend(encoder.encode(event, &decoded.fidelity).unwrap());
            state = reduce(state, event.clone()).unwrap();
        }
        assert!(
            !prefix
                .iter()
                .any(|v| v["type"] == "response.completed" || v["item"]["type"] == "function_call")
        );
        let mut native = EventEncoder::new(Profile::Chat, decoded.metadata.clone()).unwrap();
        for event in &events {
            native.encode(event, &decoded.fidelity).unwrap();
        }
        native.finish().unwrap();
    }
}
