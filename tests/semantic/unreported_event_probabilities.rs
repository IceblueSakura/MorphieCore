//! A named unreported-probability rule must not weaken complete SSE or fabricate facts.
use morphiecore::{
    adapter::{Adapter, Dialect},
    protocol::openai::{Profile, events::EventEncoder, sse::ResponsesSseDecoder},
    semantic::{task::generation::*, value::Presence},
};
use serde_json::{Value, json};
fn fixture() -> (Value, Vec<Value>) {
    let message = json!({"id":"msg","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"x","annotations":[]}]});
    let response = json!({"id":"r","object":"response","model":"m","created_at":1,"status":"completed","output":[message],"usage":null,"error":null,"incomplete_details":null});
    let mut initial = response.clone();
    initial["status"] = json!("in_progress");
    initial["output"] = json!([]);
    let mut added = message.clone();
    added["status"] = json!("in_progress");
    added["content"] = json!([]);
    let mut events = vec![
        json!({"type":"response.created","response":initial}),
        json!({"type":"response.output_item.added","output_index":0,"item":added}),
        json!({"type":"response.content_part.added","output_index":0,"item_id":"msg","content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
        json!({"type":"response.output_text.delta","output_index":0,"item_id":"msg","content_index":0,"delta":"x"}),
        json!({"type":"response.output_text.done","output_index":0,"item_id":"msg","content_index":0,"text":"x"}),
        json!({"type":"response.content_part.done","output_index":0,"item_id":"msg","content_index":0,"part":message["content"][0]}),
        json!({"type":"response.output_item.done","output_index":0,"item":message}),
        json!({"type":"response.completed","response":response}),
    ];
    for (n, event) in events.iter_mut().enumerate() {
        event["sequence_number"] = json!(n + 1);
    }
    (response, events)
}
fn sse(events: &[Value]) -> Vec<u8> {
    events
        .iter()
        .map(|event| {
            format!(
                "event: {}\ndata: {event}\n\n",
                event["type"].as_str().unwrap()
            )
        })
        .collect::<String>()
        .into_bytes()
}
fn decoder(dialect: Dialect) -> ResponsesSseDecoder {
    ResponsesSseDecoder::with_decoder(
        200,
        "text/event-stream",
        Default::default(),
        Adapter::new(Profile::Responses, dialect, None).event_decoder(),
    )
    .unwrap()
}
#[test]
fn missing_event_probabilities_are_unreported_only_under_a_named_rule() {
    let (response, source) = fixture();
    let provider = Adapter::new(Profile::Responses, Dialect::Xiaomi, None);
    let expected = provider
        .decode_response(&serde_json::to_vec(&response).unwrap())
        .unwrap();
    let mut decoded = decoder(Dialect::Xiaomi);
    let mut semantic = vec![];
    for byte in sse(&source) {
        let (n, events) = decoded.consume(&[byte]).unwrap();
        assert_eq!(n, 1);
        semantic.extend(events);
    }
    decoded.finish().unwrap();
    assert_eq!(decoded.materialize().unwrap().semantic, expected.semantic);
    let Item::Message(message) = &expected.semantic.items()[0].1 else {
        panic!()
    };
    let ContentPart::Text(text) = &message.parts[0].content else {
        panic!()
    };
    assert_eq!(text.logprobs(), &Presence::Absent);
    let mut encoder = EventEncoder::new(Profile::Responses, expected.metadata.clone()).unwrap();
    let mut projected = vec![];
    for event in &semantic {
        projected.extend(encoder.encode(event, &expected.fidelity).unwrap());
    }
    encoder.finish().unwrap();
    for event in &projected {
        if matches!(
            event["type"].as_str(),
            Some("response.output_text.delta" | "response.output_text.done")
        ) {
            assert_eq!(event["logprobs"], json!([]));
        }
    }
    assert!(
        projected.last().unwrap()["response"]["output"][0]["content"][0]
            .get("logprobs")
            .is_none()
    );
    let mut standard = decoder(Dialect::Standard);
    let bytes = sse(&source);
    let mut rest = bytes.as_slice();
    let mut rejected = false;
    while !rest.is_empty() {
        match standard.consume(rest) {
            Ok((used, _)) => rest = &rest[used..],
            Err(_) => {
                rejected = true;
                break;
            }
        }
    }
    assert!(rejected);
    assert!(standard.finish().is_err());
}

#[test]
fn empty_delta_carrier_does_not_invent_a_fact_when_done_omits_probabilities() {
    let (_, mut events) = fixture();
    events[3]["logprobs"] = json!([]);
    let mut source = decoder(Dialect::Zhipu);
    let bytes = sse(&events);
    let mut rest = bytes.as_slice();
    while !rest.is_empty() {
        let (used, _) = source.consume(rest).unwrap();
        rest = &rest[used..];
    }
    source.finish().unwrap();
    let materialized = source.materialize().unwrap();
    let Item::Message(message) = &materialized.semantic.items()[0].1 else {
        panic!()
    };
    let ContentPart::Text(text) = &message.parts[0].content else {
        panic!()
    };
    // An empty delta carrier is not a final static probability report.
    assert_eq!(text.logprobs(), &Presence::Absent);
}

#[test]
fn named_omission_does_not_hide_malformed_probabilities_or_missing_complete_fields() {
    let (_, source) = fixture();
    let mut cases = vec![];
    for index in [3, 4] {
        for value in [
            Value::Null,
            json!(false),
            json!([{"token":"x","logprob":1}]),
        ] {
            let mut events = source.clone();
            events[index]["logprobs"] = value;
            cases.push(events);
        }
    }
    for (index, key) in [(0, "sequence_number"), (2, "part"), (6, "item_id")] {
        let mut events = source.clone();
        if key == "item_id" {
            events[6]["item"]
                .as_object_mut()
                .unwrap()
                .shift_remove("id");
        } else {
            events[index].as_object_mut().unwrap().shift_remove(key);
        }
        cases.push(events);
    }
    let mut events = source.clone();
    events[3]["logprobs"] = json!([{"token":"x","logprob":-0.5,"bytes":[120],"top_logprobs":[]}]);
    cases.push(events);
    for events in cases {
        let mut decoded = decoder(Dialect::Xiaomi);
        let bytes = sse(&events);
        let mut rest = bytes.as_slice();
        let mut rejected = false;
        while !rest.is_empty() {
            match decoded.consume(rest) {
                Ok((used, _)) => rest = &rest[used..],
                Err(_) => {
                    rejected = true;
                    break;
                }
            }
        }
        assert!(rejected);
        assert!(decoded.consume(&sse(&source[source.len() - 1..])).is_err());
        assert!(decoded.finish().is_err());
    }
}
