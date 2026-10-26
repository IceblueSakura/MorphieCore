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
