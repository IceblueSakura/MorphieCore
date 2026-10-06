//! Readable-only and opaque reasoning have separate lifecycle/portability contracts.
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract as Contract,
    protocol::{
        fidelity::FidelityRecords,
        openai::{DecodedResponse, Profile, events::EventEncoder},
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
fn readable_only_output_is_reasoning_not_fabricated_assistant_text_and_replays() {
    let source = Adapter::new(Profile::Chat, Dialect::Xiaomi, None);
    let client = adapter(Dialect::MorphieCore);
    let decoded = source
        .decode_response(
            body(
                json!({"role":"assistant","content":null,"reasoning_content":"检查 alpha"}),
                "stop",
            )
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    assert!(
        matches!(&decoded.semantic.items()[0].1,Item::Reasoning(r) if r.parts[0].1==ReasoningContent::Text(text("检查 alpha")))
    );
    assert!(matches!(&decoded.semantic.items()[1].1,Item::Message(m) if m.parts.is_empty()));
    assert!(matches!(&decoded.semantic.items()[0].1, Item::Reasoning(r) if r.replay.is_none()));
    let output = client.encode_response(&decoded, &Contract::full()).unwrap();
    assert_eq!(
        output["choices"][0]["message"],
        json!({"role":"assistant","content":null,"reasoning_content":"检查 alpha"})
    );
    let request=client.decode_request(json!({"model":"m","messages":[output["choices"][0]["message"].clone(),{"role":"user","content":"next"}]}).to_string().as_bytes()).unwrap();
    let wire = source
        .encode_request(&request, "bound", &Contract::full())
        .unwrap();
    assert_eq!(wire["messages"][0]["reasoning_content"], "检查 alpha");
    let mut changed = request.clone();
    let mut items = changed.task.semantic.items().to_vec();
    let Item::Reasoning(reason) = &mut items[0].1 else {
        panic!("reasoning owner")
    };
    reason.parts[0].1 = ReasoningContent::Text(text("replacement"));
    changed.task.semantic = changed.task.semantic.clone().with_items(items).unwrap();
    let replaced = source
        .encode_request(&changed, "bound", &Contract::full())
        .unwrap();
    assert_eq!(replaced["messages"][0]["reasoning_content"], "replacement");
    changed.task.semantic = changed
        .task
        .semantic
        .clone()
        .retain_items(|_, i| !matches!(i, Item::Reasoning(_)))
        .unwrap();
    let deleted = source
        .encode_request(&changed, "bound", &Contract::full())
        .unwrap();
    assert!(deleted["messages"][0].get("reasoning_content").is_none());
    let mut decoder = source.event_decoder();
    for frame in [
        chunk(
            json!({"role":"assistant","reasoning_content":"检查 "}),
            Value::Null,
        ),
        chunk(json!({"reasoning_content":"alpha"}), Value::Null),
        chunk(json!({}), json!("stop")),
    ] {
        decoder.push(&frame).unwrap();
    }
    decoder.done().unwrap();
    assert_eq!(decoder.materialize().unwrap().semantic, decoded.semantic);
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
fn chat_event_final_value_replaces_or_removes_stale_ciphertext() {
    use crate::events_support::{close, part, start, terminal};
    let client = adapter(Dialect::MorphieCore);
    let source=client.decode_response(body(json!({"role":"assistant","content":"answer","reasoning_details":[{"type":"reasoning.summary","summary":"old summary","format":"openai-responses-v1","index":0},{"type":"reasoning.encrypted","id":"rs","data":"old-synthetic-token","format":"openai-responses-v1","index":1}]}),"stop").to_string().as_bytes()).unwrap();
    for replacement in [Some("new-synthetic-token"), None] {
        let mut events = vec![StreamEvent::Started, start(1, ItemKind::Reasoning)];
        events.extend(part(1, 1, PartKind::Summary, "new summary"));
        events.push(StreamEvent::ItemFinished {
            item: ItemId::new(1),
            status: ItemLifecycle::Completed,
            replay: replacement.map(|v| ReasoningReplay {
                value: ReplayValue::final_value(ReplayFormat::ResponsesEncrypted, text(v)),
                origin: Some(origin()),
            }),
        });
        events.push(start(2, ItemKind::Message { phase: None }));
        events.extend(part(2, 2, PartKind::Text, "answer"));
        events.extend([
            close(2, ItemLifecycle::Completed),
            terminal(StreamTerminal::Completed),
        ]);
        let mut encoder = EventEncoder::new(Profile::Chat, source.metadata.clone())
            .unwrap()
            .with_contract(client.contract(&contract()));
        let mut decoder = client.event_decoder();
        for event in events {
            for frame in encoder.encode(&event, &source.fidelity).unwrap() {
                assert!(!frame.to_string().contains("old-synthetic-token"));
                decoder.push(&frame).unwrap();
            }
        }
        encoder.finish().unwrap();
        decoder.done().unwrap();
        let decoded = decoder.materialize().unwrap();
        let Item::Reasoning(r) = &decoded.semantic.items()[0].1 else {
            panic!("reasoning")
        };
        assert_eq!(
            r.replay.as_ref().and_then(ReplayValue::replay_token),
            replacement
        );
        assert!(
            matches!(&decoded.semantic.items()[0].1,Item::Reasoning(r) if r.parts[0].1==ReasoningContent::Summary(text("new summary")))
        );
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
    assert!(source.encode_request(&request, "m", &other).is_err());
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
