//! Standard Responses omits native container boundaries, never tool identity or outcome.
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
fn attached_calls_project_to_responses_without_changing_source_ir() {
    for content in [Value::Null, json!("Checking.")] {
        for finish in ["tool_calls", "length", "content_filter"] {
            let wire = source(content.clone(), finish);
            let decoded = chat::decode_response(&wire).unwrap();
            let original = decoded.semantic.clone();
            let projected = lower_response(
                &decoded.semantic,
                &decoded.fidelity,
                &decoded.metadata,
                Profile::Responses,
                Contract::full(),
            )
            .unwrap();
            let output = responses::encode_response(&projected).unwrap();
            assert_eq!(output["output"][1]["call_id"], "c");
            assert_eq!(output["output"][1]["arguments"], "{}");
            assert_eq!(
                output["status"],
                if finish == "tool_calls" {
                    "completed"
                } else {
                    "incomplete"
                }
            );
            assert!(projected.semantic().message_envelopes().is_empty());
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
                .unwrap()
                .with_message_owners(
                    decoded
                        .semantic
                        .message_owners()
                        .iter()
                        .map(|(a, b)| (*a, *b))
                        .collect(),
                )
                .unwrap();
                let target = lower_request(
                    &request,
                    &decoded.fidelity,
                    Profile::Responses,
                    Contract::full(),
                )
                .unwrap();
                assert_eq!(
                    responses::encode_generation(&target).unwrap()["input"][1]["call_id"],
                    "c"
                );
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
    let target = lower_request(
        &history.semantic,
        &history.fidelity,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    let projected = responses::encode_generation(&target).unwrap();
    assert_eq!(projected["input"][2]["call_id"], "c");
    assert_eq!(projected["input"][5]["call_id"], "next");
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
                    replay: None,
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
    let target = lower_request(
        &edited,
        &decoded.fidelity,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    let projected = responses::encode_generation(&target).unwrap();
    assert_eq!(projected["input"][2]["call_id"], "b");
    assert_eq!(projected["input"][4]["call_id"], "a");
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
        Item::ToolCall(c) if c.status == ItemLifecycle::Completed));
    assert!(decoded.semantic.message_owners().is_empty());
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
    let meta = metadata();
    let target = lower_response(
        &response,
        &request.fidelity,
        &meta,
        Profile::Chat,
        Contract::full(),
    )
    .unwrap();
    assert_eq!(
        chat::encode_response(&target).unwrap()["choices"][0]["message"],
        json!({"role":"assistant","content":null,"tool_calls":[
            {"id":"c","type":"function","function":{"name":"lookup","arguments":"{}"}}
        ]})
    );
    assert!(response.message_owners().is_empty());
    // An explicit new link changes source data, while standard delivery omits its boundary.
    let items = request.semantic.items().to_vec();
    let owner = items[0].0;
    let member = items[1].0;
    let attached = request
        .semantic
        .with_items(items)
        .unwrap()
        .with_message_owners(vec![(member, owner)])
        .unwrap();
    let target = lower_request(
        &attached,
        &request.fidelity,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    assert!(target.semantic().message_owners().is_empty());
    let response = GenerationResponse::new(attached.items().to_vec(), Outcome::Completed)
        .unwrap()
        .with_message_owners(
            attached
                .message_owners()
                .iter()
                .map(|(a, b)| (*a, *b))
                .collect(),
        )
        .unwrap();
    let meta = metadata();
    let target = lower_response(
        &response,
        &request.fidelity,
        &meta,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    assert_eq!(
        responses::encode_response(&target).unwrap()["output"][1]["call_id"],
        "c"
    );
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
fn chat_stream_projects_attached_calls_and_preserves_the_terminal() {
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
            check_event(&state, event, Profile::Responses, &Contract::full()).unwrap();
            prefix.extend(encoder.encode(event, &decoded.fidelity).unwrap());
            state = reduce(state, event.clone()).unwrap();
        }
        encoder.finish().unwrap();
        assert_eq!(prefix.last().unwrap()["type"], "response.completed");
        assert_eq!(
            prefix.last().unwrap()["response"]["output"][1]["arguments"],
            "{}"
        );
        assert!(
            prefix
                .iter()
                .any(|v| v["type"] == "response.function_call_arguments.delta")
        );
        let mut native = EventEncoder::new(Profile::Chat, decoded.metadata.clone()).unwrap();
        for event in &events {
            native.encode(event, &decoded.fidelity).unwrap();
        }
        native.finish().unwrap();
    }
}
