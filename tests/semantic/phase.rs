//! Assistant `phase` is typed, status-independent and preserved on every projection.
use crate::events_support::*;
use morphiecore::{
    protocol::openai::{CodecError, DecodedRequest, Profile, envelope, events::EventDecoder},
    semantic::task::generation::*,
};
use serde_json::{Value, json};

fn decode_input(items: Value) -> Result<DecodedRequest, CodecError> {
    let source = json!({"model":"fixture-model","input":items});
    Ok(envelope::decode_request_bytes(&serde_json::to_vec(&source).unwrap())?.task)
}
fn labelled(label: Value) -> Value {
    json!({"type":"message","id":"m","role":"assistant","status":"completed","content":[{"type":"output_text","text":"hi","annotations":[]}],"phase":label})
}
fn message(d: &DecodedRequest) -> &Message {
    let Item::Message(m) = &d.semantic.items()[0].1 else {
        panic!()
    };
    m
}

#[test]
fn phase_keys_are_assistant_only_and_fully_typed() {
    for bad in [
        json!("final"),
        json!(5),
        json!({}),
        json!(true),
        json!(["final_answer"]),
    ] {
        assert!(decode_input(json!([labelled(bad)])).is_err());
    }
    // Only assistant messages are labelled.
    let mut user = json!({"type":"message","id":"m","role":"user","status":"completed","content":[{"type":"input_text","text":"hi"}]});
    user["phase"] = Value::Null;
    assert!(decode_input(json!([user])).is_err());
    for role in ["system", "developer"] {
        let mut item = json!({"type":"message","id":"m","role":role,"content":[{"type":"input_text","text":"rules"}]});
        item["phase"] = json!("final_answer");
        assert!(decode_input(json!([item])).is_err());
    }
    // The shorthand message form carries the same typed label.
    let d =
        decode_input(json!([{"role":"assistant","content":"hi","phase":"commentary"}])).unwrap();
    assert_eq!(message(&d).phase, Some(Phase::Commentary));
}

#[test]
fn commentary_text_never_carries_a_parsed_view() {
    let item = |label: Value, parsed: Value| json!({"type":"message","id":"m","role":"assistant","status":"completed","content":[{"type":"output_text","text":"{\"ok\":true}","annotations":[],"parsed":parsed}],"phase":label});
    // The pinned SDK never parses commentary text; contradictory dumps cannot replay.
    assert!(decode_input(json!([item(json!("commentary"), json!({"ok":true}))])).is_err());
    assert!(decode_input(json!([item(json!("commentary"), Value::Null)])).is_ok());
    assert!(decode_input(json!([item(json!("final_answer"), json!({"ok":true}))])).is_ok());
    assert!(decode_input(json!([item(Value::Null, json!({"ok":true}))])).is_ok());
}

fn stream(label: Option<Value>, done_label: Option<Value>) -> Vec<Value> {
    let mut started =
        json!({"id":"m","type":"message","role":"assistant","status":"in_progress","content":[]});
    let mut done = json!({"id":"m","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"hi","annotations":[]}]});
    if let Some(v) = &label {
        started["phase"] = v.clone();
    }
    if let Some(v) = &done_label {
        done["phase"] = v.clone();
    }
    vec![
        created(),
        json!({"type":"response.output_item.added","output_index":0,"item":started}),
        json!({"type":"response.content_part.added","output_index":0,"item_id":"m","content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
        json!({"type":"response.output_text.delta","output_index":0,"item_id":"m","content_index":0,"delta":"hi","logprobs":[]}),
        json!({"type":"response.output_text.done","output_index":0,"item_id":"m","content_index":0,"text":"hi","logprobs":[]}),
        json!({"type":"response.content_part.done","output_index":0,"item_id":"m","content_index":0,"part":{"type":"output_text","text":"hi","annotations":[]}}),
        json!({"type":"response.output_item.done","output_index":0,"item":done.clone()}),
        json!({"type":"response.completed","response":envelope("completed",json!([done]))}),
    ]
}

#[test]
fn event_snapshots_preserve_phase_and_conflicts_fail() {
    let mut d = EventDecoder::new(Profile::Responses);
    for v in stream(Some(json!("commentary")), Some(json!("commentary"))) {
        d.push(&v).unwrap();
    }
    let r = d.materialize().unwrap().semantic;
    let Item::Message(m) = &r.items()[0].1 else {
        panic!()
    };
    assert_eq!(m.phase, Some(Phase::Commentary));
    // A final snapshot that contradicts the started label cannot close the item.
    for (started, done) in [
        (Some(json!("commentary")), Some(Value::Null)),
        (Some(json!("commentary")), Some(json!("final_answer"))),
        (None, Some(json!("final_answer"))),
    ] {
        let mut d = EventDecoder::new(Profile::Responses);
        let mut values = stream(started, done);
        let failed = values.split_off(6);
        for v in values {
            d.push(&v).unwrap();
        }
        assert!(d.push(&failed[0]).is_err());
    }
}
