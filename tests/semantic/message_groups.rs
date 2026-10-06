//! Explicit assistant ownership is not adjacency, a turn ID, or tool execution.
use crate::events_support::{metadata, start, terminal, text};
use morphiecore::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{Profile, chat, events::EventEncoder},
    },
    semantic::task::generation::*,
};
use serde_json::json;

fn message(role: MessageRole) -> Item {
    Item::Message(Message {
        role,
        phase: None,
        status: ItemLifecycle::Completed,
        parts: vec![],
    })
}
fn call(id: &str, owner: Option<ItemId>) -> Item {
    Item::ToolCall(ToolCall {
        call_id: text(id),
        name: text("lookup"),
        arguments: "{}".into(),
        message: owner,
        status: ItemLifecycle::Completed,
        context: CallContext::default(),
    })
}
fn items() -> Vec<(ItemId, Item)> {
    vec![
        (ItemId::new(10), message(MessageRole::Assistant)),
        (ItemId::new(11), call("a", Some(ItemId::new(10)))),
        (ItemId::new(12), call("b", Some(ItemId::new(10)))),
        (ItemId::new(20), message(MessageRole::Assistant)),
        (ItemId::new(21), call("c", Some(ItemId::new(20)))),
        (ItemId::new(30), call("independent", None)),
    ]
}

#[test]
fn groups_follow_declared_membership_and_final_order_without_copying_values() {
    let request = GenerationRequest::new(items(), GenerationControls::default()).unwrap();
    let groups: Vec<_> = request.message_groups().collect();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].owner(), ItemId::new(10));
    assert!(groups[0].message().parts.is_empty());
    assert_eq!(groups[0].items().len(), 3);
    assert_eq!(
        groups[0].calls().collect::<Vec<_>>(),
        vec![
            CallReference {
                item: ItemId::new(11),
                call_id: "a"
            },
            CallReference {
                item: ItemId::new(12),
                call_id: "b"
            },
        ]
    );
    assert_eq!(
        groups[1].calls().map(|r| r.call_id).collect::<Vec<_>>(),
        ["c"]
    );
    assert!(std::ptr::eq(
        groups[0].items().as_ptr(),
        request.items().as_ptr()
    ));

    let mut reordered = groups[1].items().to_vec();
    reordered.extend_from_slice(groups[0].items());
    reordered.push(request.items()[5].clone());
    let changed = request.clone().with_items(reordered).unwrap();
    assert_eq!(
        changed
            .message_groups()
            .map(|g| g.owner())
            .collect::<Vec<_>>(),
        [ItemId::new(20), ItemId::new(10)]
    );
    let response = GenerationResponse::new(changed.items().to_vec(), Outcome::Completed).unwrap();
    assert_eq!(
        response
            .message_groups()
            .flat_map(|g| g.calls())
            .map(|r| r.call_id)
            .collect::<Vec<_>>(),
        ["c", "a", "b"]
    );
    let edited = response
        .with_items(vec![(ItemId::new(10), message(MessageRole::Assistant))])
        .unwrap();
    assert_eq!(edited.message_groups().next().unwrap().calls().len(), 0);
}

#[test]
fn removing_or_crossing_an_owner_fails_instead_of_reattaching_calls() {
    for edit in 0..4 {
        let mut values = items();
        match edit {
            0 => {
                values.remove(0);
            }
            1 => values.swap(1, 3),
            2 => {
                values[0].1 = Item::Message(Message {
                    role: MessageRole::User,
                    phase: None,
                    status: ItemLifecycle::Completed,
                    parts: vec![Part {
                        id: PartId::new(90),
                        content: ContentPart::Text(text("user").into()),
                    }],
                });
            }
            _ => {
                let Item::ToolCall(call) = &mut values[1].1 else {
                    unreachable!()
                };
                call.message = Some(ItemId::new(999));
            }
        }
        assert!(GenerationRequest::new(values.clone(), GenerationControls::default()).is_err());
        assert!(GenerationResponse::new(values, Outcome::Completed).is_err());
    }
    let request = GenerationRequest::new(items(), GenerationControls::default()).unwrap();
    let edited = request.retain_items(|id, _| id != ItemId::new(11)).unwrap();
    assert_eq!(
        edited
            .message_groups()
            .next()
            .unwrap()
            .calls()
            .map(|r| r.call_id)
            .collect::<Vec<_>>(),
        ["b"]
    );
}

#[test]
fn chat_history_declares_separate_groups_even_without_body_text() {
    let decoded = chat::decode_generation(&json!({"messages":[
        {"role":"assistant","content":null,"tool_calls":[{"id":"a","type":"function","function":{"name":"lookup","arguments":"{}"}}]},
        {"role":"assistant","content":"next","tool_calls":[{"id":"b","type":"function","function":{"name":"lookup","arguments":"{}"}}]}
    ]})).unwrap();
    let groups: Vec<_> = decoded.semantic.message_groups().collect();
    assert_eq!(groups.len(), 2);
    assert_ne!(groups[0].owner(), groups[1].owner());
    assert_eq!(
        groups[0].calls().map(|r| r.call_id).collect::<Vec<_>>(),
        ["a"]
    );
    assert_eq!(
        groups[1].calls().map(|r| r.call_id).collect::<Vec<_>>(),
        ["b"]
    );
}

#[test]
fn responses_adjacency_does_not_declare_a_message_group() {
    let decoded = morphiecore::protocol::openai::responses::decode_generation(&json!({"input":[
        {"role":"user","content":"lookup"},
        {"type":"reasoning","id":"r","summary":[]},
        {"type":"message","id":"m","role":"assistant","status":"completed","content":[]},
        {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"},
        {"type":"function_call_output","call_id":"c","output":"ok"}
    ]}))
    .unwrap();
    let groups: Vec<_> = decoded.semantic.message_groups().collect();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].items().len(), 1);
    assert_eq!(groups[0].calls().len(), 0);
}

#[test]
fn refusal_and_attached_calls_conflict_at_the_first_known_event_in_either_order() {
    let mut values = items();
    let Item::Message(message) = &mut values[0].1 else {
        unreachable!()
    };
    message.parts.push(Part {
        id: PartId::new(1),
        content: ContentPart::Refusal(text("no").into()),
    });
    assert_eq!(
        GenerationResponse::new(values, Outcome::Completed).unwrap_err(),
        GenerationError::InvalidResponse
    );

    for call_first in [true, false] {
        let call = start(
            2,
            ItemKind::ToolCall {
                format: ArgumentFormat::Raw,
                call_id: text("a"),
                name: text("lookup"),
                message: Some(ItemId::new(1)),
                context: CallContext::default(),
            },
        );
        let refusal = StreamEvent::PartStarted {
            item: ItemId::new(1),
            part: PartId::new(1),
            kind: PartKind::Refusal,
        };
        let (first, conflicting) = if call_first {
            (call, refusal)
        } else {
            (refusal, call)
        };
        let prefix = [
            StreamEvent::Started,
            start(1, ItemKind::Message { phase: None }),
            first,
        ];
        let state = prefix
            .iter()
            .cloned()
            .try_fold(StreamState::new(), reduce)
            .unwrap();
        assert_eq!(
            reduce(state, conflicting.clone()).unwrap_err(),
            EventError::Semantic(GenerationError::InvalidResponse)
        );
        // Responses rejects any attached call before this semantic conflict;
        // Chat owns the representable group and must reject the conflicting part.
        let mut encoder = EventEncoder::new(Profile::Chat, metadata()).unwrap();
        let fidelity = FidelityRecords::default();
        for event in &prefix {
            encoder.encode(event, &fidelity).unwrap();
        }
        assert!(encoder.encode(&conflicting, &fidelity).is_err());
        assert!(
            encoder
                .encode(&terminal(StreamTerminal::Completed), &fidelity)
                .is_err()
        );
        assert!(encoder.finish().is_err());
    }
}
