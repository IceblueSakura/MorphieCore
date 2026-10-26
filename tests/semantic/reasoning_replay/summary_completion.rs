use crate::events_support::*;
use morphiecore::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            GenerationApi,
            events::{EventDecoder, EventEncoder},
        },
    },
    semantic::task::generation::*,
};
use serde_json::{Value, json};
fn prefix() -> Vec<Value> {
    vec![
        created(),
        json!({"type":"response.output_item.added","output_index":0,"item":{
                "type":"reasoning","id":"reason","status":"in_progress","summary":[]}}),
        json!({"type":"response.reasoning_summary_part.added","output_index":0,"item_id":"reason",
                "summary_index":0,"part":{"type":"summary_text","text":""}}),
        json!({"type":"response.reasoning_summary_text.delta","output_index":0,"item_id":"reason",
                "summary_index":0,"delta":"S"}),
        json!({"type":"response.reasoning_summary_text.done","output_index":0,"item_id":"reason",
                "summary_index":0,"text":"S"}),
    ]
}
#[test]
fn interrupted_summary_close_preserves_event_fact_without_inventing_static_status() {
    let mut decoder = EventDecoder::new(GenerationApi::Responses);
    for frame in prefix() {
        decoder.push(&frame).unwrap();
    }
    let done = json!({"type":"response.reasoning_summary_part.done","output_index":0,"item_id":"reason",
            "summary_index":0,"part":{"type":"summary_text","text":"S"},"status":"incomplete"});
    let event = decoder.push(&done).unwrap();
    assert!(matches!(
        event.as_slice(),
        [StreamEvent::PartInterrupted { .. }]
    ));
    let item = json!({"id":"reason","type":"reasoning","status":"completed","summary":[{"type":"summary_text","text":"S"}]});
    decoder
        .push(&json!({"type":"response.output_item.done","output_index":0,"item":item}))
        .unwrap();
    decoder
        .push(&json!({"type":"response.completed","response":envelope("completed",json!([item]))}))
        .unwrap();
    decoder.finish().unwrap();
    let response = decoder.materialize().unwrap();
    assert_eq!(response.semantic.outcome(), Outcome::Completed);
    let Item::Reasoning(reasoning) = &response.semantic.items()[0].1 else {
        panic!("reasoning")
    };
    assert_eq!(reasoning.status, ItemLifecycle::Completed);
    // Independent event -> wire oracle, not regenerated from decoder output.
    let id = ItemId::new(9);
    let part = PartId::new(8);
    let mut records = FidelityRecords::default();
    records.record_response_item_id(id, "reason").unwrap();
    let mut encoder = EventEncoder::new(GenerationApi::Responses, metadata())
        .unwrap()
        .with_contract(contract());
    for event in [
        StreamEvent::Started,
        StreamEvent::ItemStarted {
            item: id,
            kind: ItemKind::Reasoning,
            replay: None,
        },
        StreamEvent::PartStarted {
            item: id,
            part,
            kind: PartKind::Summary,
        },
        StreamEvent::Delta {
            item: id,
            part,
            fragment: "S".into(),
            logprobs: vec![],
        },
        StreamEvent::ValueFinished { item: id, part },
    ] {
        encoder.encode(&event, &records).unwrap();
    }
    let mut expected = done;
    expected["sequence_number"] = json!(6);
    assert_eq!(
        encoder
            .encode(&StreamEvent::PartInterrupted { item: id, part }, &records)
            .unwrap(),
        vec![expected]
    );
    assert!(
        encoder
            .encode(&StreamEvent::PartInterrupted { item: id, part }, &records)
            .is_err()
    );
}
#[test]
fn unknown_summary_status_is_not_an_ignored_extension() {
    for value in [json!("complete"), json!(false), json!(7)] {
        let mut decoder = EventDecoder::new(GenerationApi::Responses);
        for frame in prefix() {
            decoder.push(&frame).unwrap();
        }
        assert!(decoder.push(&json!({"type":"response.reasoning_summary_part.done","output_index":0,
                "item_id":"reason","summary_index":0,"part":{"type":"summary_text","text":"S"},"status":value})).is_err());
        assert!(decoder.finish().is_err());
    }
}
