//! Message envelopes own ordered membership, not tool-result association.
use morphiecore::{protocol::openai::responses, semantic::task::generation::*};
use serde_json::json;

fn history() -> GenerationRequest {
    responses::decode_generation(&json!({"input":[
        {"role":"assistant","content":"before"},
        {"type":"function_call","call_id":"c","name":"lookup","arguments":"{ \"x\": 1 }"},
        {"role":"assistant","content":"after"},
        {"type":"function_call_output","call_id":"c","output":"result"},
        {"role":"user","content":"continue"}
    ]}))
    .unwrap()
    .semantic
}

#[test]
fn heterogeneous_envelopes_keep_order_and_result_identity() {
    let request = history();
    let ids: Vec<_> = request.items().iter().map(|(id, _)| *id).collect();
    let assistant = MessageEnvelope::new(
        GroupId::new(LocalScope::ROOT, 1),
        MessageEnvelopeRole::Assistant,
        ids[..3].to_vec(),
    )
    .unwrap();
    let user = MessageEnvelope::new(
        GroupId::new(LocalScope::ROOT, 2),
        MessageEnvelopeRole::User,
        ids[3..].to_vec(),
    )
    .unwrap();
    let grouped = request
        .clone()
        .with_message_envelopes(vec![assistant, user])
        .unwrap();
    assert_eq!(grouped.items(), request.items());
    assert_eq!(grouped.message_envelopes()[0].members(), &ids[..3]);
    assert_eq!(grouped.continuation(), Continuation::Unreported);
    // Removing the later text must not silently attach it to another message.
    let edited = grouped.clone().retain_items(|id, _| id != ids[2]).unwrap();
    assert_eq!(edited.message_envelopes()[0].members(), &ids[..2]);
    let mut reversed = grouped.items().to_vec();
    reversed.swap(0, 2);
    assert!(grouped.with_items(reversed).is_err());
}

#[test]
fn duplicate_members_wrong_roles_and_dangling_empty_boundaries_fail() {
    let request = history();
    let ids: Vec<_> = request.items().iter().map(|(id, _)| *id).collect();
    let group = |n, role, members| {
        MessageEnvelope::new(GroupId::new(LocalScope::ROOT, n), role, members).unwrap()
    };
    assert!(
        request
            .clone()
            .with_message_envelopes(vec![
                group(1, MessageEnvelopeRole::Assistant, vec![ids[0], ids[1]]),
                group(2, MessageEnvelopeRole::Assistant, vec![ids[1]]),
            ])
            .is_err()
    );
    assert!(
        request
            .clone()
            .with_message_envelopes(vec![group(1, MessageEnvelopeRole::User, vec![ids[1]]),])
            .is_err()
    );
    let empty = MessageEnvelope::empty(
        GroupId::new(LocalScope::ROOT, 3),
        MessageEnvelopeRole::Assistant,
        Some(ids[0]),
    );
    let grouped = request.with_message_envelopes(vec![empty]).unwrap();
    assert!(grouped.retain_items(|id, _| id != ids[0]).is_err());
}

#[test]
fn chat_sse_delivers_calls_and_a_real_responses_terminal() {
    use morphiecore::protocol::openai::{
        Profile,
        events::{EventDecoder, EventEncoder},
    };
    let chunk = |delta, finish| {
        json!({"id":"r","object":"chat.completion.chunk",
        "created":0,"model":"synthetic","choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
    };
    let mut decoder = EventDecoder::new(Profile::Chat);
    let mut events = vec![];
    for wire in [
        chunk(
            json!({"role":"assistant","content":"checking","tool_calls":[
                {"index":0,"id":"c","type":"function","function":{"name":"lookup","arguments":"{ \"x\""}}
            ]}),
            json!(null),
        ),
        chunk(
            json!({"tool_calls":[{"index":0,"function":{"arguments":": 1 }"}}]}),
            json!(null),
        ),
        chunk(json!({}), json!("tool_calls")),
    ] {
        events.extend(decoder.push(&wire).unwrap());
    }
    events.extend(decoder.done().unwrap());
    let source = decoder.materialize().unwrap();
    let mut encoder = EventEncoder::new(Profile::Responses, source.metadata.clone()).unwrap();
    let mut wire = vec![];
    for event in &events {
        wire.extend(encoder.encode(event, &source.fidelity).unwrap());
    }
    encoder.finish().unwrap();
    assert_eq!(wire.last().unwrap()["type"], "response.completed");
    assert_eq!(
        wire.last().unwrap()["response"]["output"][1]["call_id"],
        "c"
    );
    assert_eq!(
        wire.last().unwrap()["response"]["output"][1]["arguments"],
        "{ \"x\": 1 }"
    );
    let mut consumer = EventDecoder::new(Profile::Responses);
    for value in wire {
        consumer.push(&value).unwrap();
    }
    assert_eq!(consumer.materialize().unwrap().semantic.items().len(), 2);
    assert!(
        encoder
            .projection()
            .iter()
            .any(|stage| stage.loss.is_some())
    );
    // The complete explicit-envelope event and the native attached-call event
    // are two construction paths to the same Chat-representable shape.
    for event in &mut events {
        if let StreamEvent::ItemStarted {
            kind: ItemKind::ToolCall { message, .. },
            ..
        } = event
        {
            *message = None;
        }
    }
    events.insert(
        events.len() - 1,
        StreamEvent::MessageEnvelope(
            MessageEnvelope::new(
                GroupId::new(LocalScope::ROOT, 42),
                MessageEnvelopeRole::Assistant,
                source.semantic.items().iter().map(|(id, _)| *id).collect(),
            )
            .unwrap(),
        ),
    );
    for profile in [Profile::Chat, Profile::Responses] {
        let mut encoder = EventEncoder::new(profile, source.metadata.clone()).unwrap();
        let mut consumer = EventDecoder::new(profile);
        for event in &events {
            for wire in encoder.encode(event, &source.fidelity).unwrap() {
                consumer.push(&wire).unwrap();
            }
        }
        encoder.finish().unwrap();
        if profile == Profile::Chat {
            consumer.done().unwrap();
        }
        let response = consumer.materialize().unwrap();
        let Item::ToolCall(call) = &response.semantic.items()[1].1 else {
            panic!("call");
        };
        assert_eq!(call.call_id.as_str(), "c");
        assert_eq!(call.arguments, ToolArguments::Raw("{ \"x\": 1 }".into()));
    }
}

#[test]
fn envelope_event_is_closed_unique_and_materializes_the_same_container() {
    use crate::events_support::{start, terminal};
    let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
    state = reduce(state, start(1, ItemKind::Reasoning)).unwrap();
    let group = MessageEnvelope::new(
        GroupId::new(LocalScope::ROOT, 1),
        MessageEnvelopeRole::Assistant,
        vec![ItemId::new(1)],
    )
    .unwrap();
    let declaration = StreamEvent::MessageEnvelope(group.clone());
    assert!(reduce(state.clone(), declaration.clone()).is_err());
    state = reduce(
        state,
        StreamEvent::ItemFinished {
            item: ItemId::new(1),
            status: ItemLifecycle::Completed,
            replay: None,
        },
    )
    .unwrap();
    state = reduce(state, declaration.clone()).unwrap();
    assert!(reduce(state.clone(), declaration.clone()).is_err());
    state = reduce(state, terminal(StreamTerminal::Completed)).unwrap();
    let response = materialize(&state).unwrap();
    assert_eq!(response.message_envelopes(), &[group]);
    assert!(reduce(state, declaration).is_err());
}

#[test]
fn envelope_dependencies_and_selection_do_not_reduce_to_call_ids() {
    let source = history();
    let ids: Vec<_> = source.items().iter().map(|(id, _)| *id).collect();
    let group = MessageEnvelope::new(
        GroupId::new(LocalScope::ROOT, 1),
        MessageEnvelopeRole::Assistant,
        ids[..3].to_vec(),
    )
    .unwrap();
    let source = source.with_message_envelopes(vec![group.clone()]).unwrap();
    let proof = RequestDependencyProof::capture(
        &source,
        HistoryDependency::MessageEnvelope(group.id()),
        SettingsDependency::None,
    )
    .unwrap();
    assert!(proof.check(&source).is_ok());
    let dropped = source.clone().with_message_envelopes(vec![]).unwrap();
    assert!(proof.check(&dropped).is_err());
    let edited = source.clone().retain_items(|id, _| id != ids[2]).unwrap();
    assert!(proof.check(&edited).is_err());
    let rebuilt = ClientManaged::new(GenerationSettings::default())
        .unwrap()
        .select_request_items(&source, &ids[..3])
        .unwrap()
        .build(&[])
        .unwrap();
    assert_eq!(rebuilt.message_envelopes(), &[group]);
    assert!(proof.check(&rebuilt).is_ok());
}
