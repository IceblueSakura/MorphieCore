//! Readable-only and opaque reasoning have separate lifecycle/portability contracts.
use morphiecore::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            Adapter, DecodedResponse, Dialect, Profile, TargetCaps as Contract,
            events::EventEncoder,
        },
    },
    semantic::{
        task::generation::*,
        value::{ReplayOrigin, Text},
    },
};
use serde_json::{Value, json};
fn origin() -> ReplayOrigin {
    ReplayOrigin::new("reasoning-test-origin").unwrap()
}
fn adapter(dialect: Dialect) -> Adapter {
    Adapter::new(Profile::Chat, dialect, Some(origin()))
}
fn contract() -> Contract {
    Contract {
        replay_origin: Some(origin()),
        ..Contract::full()
    }
}
fn text(s: &str) -> Text {
    Text::allowing_empty(s, "fixture", MAX_TEXT_BYTES).unwrap()
}
fn body(message: Value, finish: &str) -> Value {
    json!({"id":"r","object":"chat.completion","created":1,"model":"m","choices":[{"index":0,"message":message,"finish_reason":finish}]})
}
fn chunk(delta: Value, finish: Value) -> Value {
    json!({"id":"r","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
}
fn encrypted() -> Value {
    json!({"type":"reasoning.encrypted","id":"rs","data":"synthetic-final","format":"openai-responses-v1","index":0})
}

#[test]
fn completed_encrypted_owner_survives_an_incomplete_carrier_in_json_and_events() {
    let source = adapter(Dialect::OpenRouter);
    let message =
        json!({"role":"assistant","content":"partial answer","reasoning_details":[encrypted()]});
    let decoded = source
        .decode_response(body(message, "length").to_string().as_bytes())
        .unwrap();
    assert!(
        matches!(&decoded.semantic.items()[0].1,Item::Reasoning(r) if r.status==ItemLifecycle::Completed)
    );
    assert!(
        matches!(&decoded.semantic.items()[1].1,Item::Message(m) if m.status==ItemLifecycle::Incomplete)
    );
    let out = source.encode_response(&decoded, &contract()).unwrap();
    assert_eq!(out["choices"][0]["finish_reason"], "length");
    assert_eq!(
        out["choices"][0]["message"]["reasoning_details"][0]["data"],
        "synthetic-final"
    );
    let mut decoder = source.event_decoder();
    let mut events = vec![];
    for frame in [
        chunk(json!({"reasoning_details":[encrypted()]}), Value::Null),
        chunk(json!({"content":"partial answer"}), Value::Null),
        chunk(json!({}), json!("length")),
    ] {
        events.extend(decoder.push(&frame).unwrap());
    }
    events.extend(decoder.done().unwrap());
    let streamed = decoder.materialize().unwrap();
    assert_eq!(streamed.semantic, decoded.semantic);
    let mut encoder = EventEncoder::new(Profile::Chat, decoded.metadata.clone())
        .unwrap()
        .with_contract(source.contract(&contract()));
    let mut wire = vec![];
    for event in events {
        wire.extend(encoder.encode(&event, &streamed.fidelity).unwrap());
    }
    encoder.finish().unwrap();
    let mut consumer = source.event_decoder();
    for frame in wire {
        consumer.push(&frame).unwrap();
    }
    consumer.done().unwrap();
    assert_eq!(consumer.materialize().unwrap().semantic, decoded.semantic);
}

#[test]
fn chat_ciphertext_requires_both_final_value_and_completed_owner() {
    let client = adapter(Dialect::MorphieCore);
    for value in [
        ReplayValue::partial(ReplayFormat::ResponsesEncrypted, text("synthetic-partial")),
        ReplayValue::final_value(ReplayFormat::ResponsesEncrypted, text("synthetic-final")),
    ] {
        let reasoning = ReasoningItem {
            parts: vec![(
                PartId::new(1),
                ReasoningContent::Summary(text("partial summary")),
            )],
            status: ItemLifecycle::Incomplete,
            replay: Some(value.clone()),
        };
        let replay = ReasoningReplay {
            value,
            origin: Some(origin()),
        };
        let mut fidelity = FidelityRecords::default();
        fidelity
            .record_replay(ItemId::new(1), &reasoning, Some(origin()))
            .unwrap();
        let semantic = GenerationResponse::new(
            vec![
                (ItemId::new(1), Item::Reasoning(reasoning)),
                (
                    ItemId::new(2),
                    Item::Message(Message {
                        role: MessageRole::Assistant,
                        parts: vec![],
                        status: ItemLifecycle::Incomplete,
                        phase: None,
                    }),
                ),
            ],
            Outcome::Incomplete,
        )
        .unwrap()
        .with_details(TerminalDetails {
            incomplete: Some(IncompleteReason::MaxOutputTokens),
            error: None,
        })
        .unwrap();
        let decoded = DecodedResponse {
            semantic,
            fidelity,
            metadata: crate::events_support::metadata(),
        };
        assert!(
            client.encode_response(&decoded, &contract()).is_err(),
            "static projection must not drop a token or promote an incomplete owner"
        );
        let mut encoder = EventEncoder::new(Profile::Chat, decoded.metadata.clone())
            .unwrap()
            .with_contract(client.contract(&contract()));
        encoder
            .encode(&StreamEvent::Started, &decoded.fidelity)
            .unwrap();
        encoder
            .encode(
                &StreamEvent::ItemStarted {
                    item: ItemId::new(1),
                    kind: ItemKind::Reasoning,
                    replay: None,
                },
                &decoded.fidelity,
            )
            .unwrap();
        assert!(
            encoder
                .encode(
                    &StreamEvent::ItemFinished {
                        item: ItemId::new(1),
                        status: ItemLifecycle::Incomplete,
                        replay: Some(replay)
                    },
                    &decoded.fidelity
                )
                .is_err(),
            "nonfinal token/owner must fail before emitting any encrypted detail"
        );
        assert!(encoder.finish().is_err());
    }
}

#[test]
fn chat_replay_edits_deletion_and_origin_are_checked_independently_of_plain_text() {
    let source = adapter(Dialect::MorphieCore);
    let history = json!({"model":"m","messages":[{"role":"assistant","content":"answer","reasoning_details":[{"type":"reasoning.summary","summary":"summary","format":"openai-responses-v1","index":0},{"type":"reasoning.encrypted","id":"rs","data":"synthetic-final","format":"openai-responses-v1","index":1}]},{"role":"user","content":"next"}]});
    let mut request = source
        .decode_request(history.to_string().as_bytes())
        .unwrap();
    let mut other = contract();
    other.replay_origin = Some(ReplayOrigin::new("different-issuer").unwrap());
    let foreign = source.encode_request(&request, "m", &other).unwrap();
    assert!(!foreign.to_string().contains("synthetic-final"));
    assert_eq!(
        foreign["messages"][0]["reasoning_details"][0]["summary"],
        "summary"
    );
    let mut items = request.task.semantic.items().to_vec();
    let Item::Reasoning(reason) = &mut items[0].1 else {
        panic!("reasoning owner")
    };
    reason.parts[0].1 = ReasoningContent::Summary(text("changed summary"));
    request.task.semantic = request.task.semantic.clone().with_items(items).unwrap();
    assert!(source.encode_request(&request, "m", &contract()).is_err());
    let mut items = request.task.semantic.items().to_vec();
    let Item::Reasoning(reason) = &mut items[0].1 else {
        panic!("reasoning")
    };
    reason.replay = None;
    request.task.semantic = request.task.semantic.clone().with_items(items).unwrap();
    let edited = source.encode_request(&request, "m", &contract()).unwrap();
    assert_eq!(
        edited["messages"][0]["reasoning_details"],
        json!([{"type":"reasoning.summary","summary":"changed summary","format":"openai-responses-v1","index":0}])
    );
    request.task.semantic = request
        .task
        .semantic
        .clone()
        .retain_items(|_, i| !matches!(i, Item::Reasoning(_)))
        .unwrap();
    let deleted = source.encode_request(&request, "m", &contract()).unwrap();
    assert!(deleted["messages"][0].get("reasoning_details").is_none());
    assert_eq!(deleted["messages"][0]["content"], "answer");
}
