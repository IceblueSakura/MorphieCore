//! A named unreported-probability rule must not weaken complete SSE or fabricate facts.
use morphiecore::protocol::openai::{Adapter, Dialect, Profile, sse::ResponsesSseDecoder};
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
