//! Synthetic offline event builders, not wire or semantic oracles.
use morphiecore::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            GenerationApi, ResponseMetadata, TargetContract as Contract, events::EventEncoder,
        },
    },
    semantic::{
        task::generation::*,
        value::{ReplayOrigin, Text},
    },
};
use serde_json::{Value, json};
pub fn text(s: &str) -> Text {
    Text::new(s, "synthetic", MAX_TEXT_BYTES).unwrap()
}
pub fn metadata() -> ResponseMetadata {
    ResponseMetadata {
        delivery: None,
        storage: None,
        id: Some("r".into()),
        model: "synthetic".into(),
        created: Some(0.into()),
        context: Default::default(),
        instruction_fidelity: Default::default(),
    }
}
pub fn origin() -> ReplayOrigin {
    ReplayOrigin::new("synthetic-owner").unwrap()
}
pub fn contract() -> Contract {
    let mut c = Contract::full();
    c.replay_origin = Some(origin());
    c
}
pub fn token(s: &str, final_value: bool) -> ItemReplay {
    ItemReplay {
        origin: Some(origin()),
        value: if final_value {
            ReplayValue::final_value(ReplayFormat::ResponsesEncrypted, text(s))
        } else {
            ReplayValue::partial(ReplayFormat::ResponsesEncrypted, text(s))
        },
    }
}
pub fn start(item: u64, kind: ItemKind) -> StreamEvent {
    StreamEvent::ItemStarted {
        item: ItemId::new(item),
        kind,
        replay: None,
    }
}
pub fn close(item: u64, status: ItemLifecycle) -> StreamEvent {
    StreamEvent::ItemFinished {
        item: ItemId::new(item),
        status,
        replay: None,
    }
}
pub fn terminal(t: StreamTerminal) -> StreamEvent {
    StreamEvent::Terminal {
        terminal: t,
        details: TerminalDetails::default(),
    }
}
pub fn part(item: u64, part: u64, kind: PartKind, s: &str) -> Vec<StreamEvent> {
    let item = ItemId::new(item);
    let part = PartId::new(part);
    vec![
        StreamEvent::PartStarted { item, part, kind },
        StreamEvent::Delta {
            item,
            part,
            fragment: s.into(),
            logprobs: vec![],
        },
        StreamEvent::ValueFinished { item, part },
        StreamEvent::PartFinished { item, part },
    ]
}
pub fn apply(events: &[StreamEvent]) -> Result<StreamState, EventError> {
    events.iter().cloned().try_fold(StreamState::new(), reduce)
}
pub fn encode(events: &[StreamEvent], profile: GenerationApi, f: &FidelityRecords) -> Vec<Value> {
    let mut encoder = EventEncoder::new(profile, metadata())
        .unwrap()
        .with_contract(contract());
    let values = events
        .iter()
        .flat_map(|e| encoder.encode(e, f).unwrap())
        .collect();
    encoder.finish().unwrap();
    values
}
pub fn envelope(status: &str, output: Value) -> Value {
    json!({"id":"r","object":"response","created_at":0,"model":"synthetic","status":status,"output":output})
}
pub fn created() -> Value {
    json!({"type":"response.created","response":envelope("in_progress",json!([]))})
}
pub fn call_item(id: &str, call_id: &str, args: &str, status: &str) -> Value {
    json!({"id":id,"type":"function_call","call_id":call_id,"name":"lookup","arguments":args,"status":status})
}

/// Later owners and parts finish first; the first owner's signature arrives last.
pub fn interleaved_messages(replay: ItemReplay) -> Vec<StreamEvent> {
    let begin = |item, part, kind| StreamEvent::PartStarted {
        item: ItemId::new(item),
        part: PartId::new(part),
        kind,
    };
    let delta = |item, part, s: &str| StreamEvent::Delta {
        item: ItemId::new(item),
        part: PartId::new(part),
        fragment: s.into(),
        logprobs: vec![],
    };
    let end = |item, part| {
        [
            StreamEvent::ValueFinished {
                item: ItemId::new(item),
                part: PartId::new(part),
            },
            StreamEvent::PartFinished {
                item: ItemId::new(item),
                part: PartId::new(part),
            },
        ]
    };
    let mut usage = Usage::operation(1, 2, 3);
    usage.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
    let mut events = vec![
        StreamEvent::Started,
        StreamEvent::Usage(usage),
        start(10, ItemKind::Reasoning),
        start(3, ItemKind::Message { phase: None }),
    ];
    // Deliberately non-monotonic IDs: declaration order, not ID sorting, is authority.
    for (item, id) in [(8, "first"), (2, "second")] {
        events.push(start(
            item,
            ItemKind::ToolCall {
                format: ArgumentFormat::Raw,
                call_id: text(id),
                name: text("lookup"),
                context: Default::default(),
            },
        ));
    }
    events.extend([
        begin(10, 1, PartKind::Summary),
        delta(10, 1, "Plan"),
        begin(3, 9, PartKind::Text),
        delta(3, 9, "A"),
        begin(3, 4, PartKind::Text),
        delta(3, 4, "B"),
        delta(3, 9, "C"),
        begin(8, 8, PartKind::Arguments),
        delta(8, 8, " \n"),
        delta(8, 8, "{\"n\":"),
        begin(2, 2, PartKind::Arguments),
        delta(2, 2, "{\"n\":2}"),
    ]);
    events.extend(end(2, 2));
    events.push(close(2, ItemLifecycle::Completed));
    events.push(delta(8, 8, "1}"));
    events.extend(end(8, 8));
    events.push(close(8, ItemLifecycle::Completed));
    events.extend(end(3, 4));
    events.extend(end(3, 9));
    events.push(close(3, ItemLifecycle::Completed));
    events.extend(end(10, 1));
    events.push(StreamEvent::ItemFinished {
        item: ItemId::new(10),
        status: ItemLifecycle::Completed,
        replay: Some(replay),
    });
    events.push(StreamEvent::Progress(
        InteractionProgress::AwaitingToolResults,
    ));
    events.push(terminal(StreamTerminal::Completed));
    events
}
