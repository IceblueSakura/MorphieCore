//! Public protocol boundaries reject private attachments without weakening typed IR.
use crate::{events_support as event, wire};
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract as Contract,
    protocol::openai::{Profile, events::EventEncoder},
    semantic::{task::generation::*, value::ReplayOrigin},
};
use serde_json::{Value, json};
fn client(dialect: Dialect) -> Adapter {
    Adapter::new(
        Profile::Responses,
        dialect,
        Some(ReplayOrigin::new("synthetic-client").unwrap()),
    )
}
fn contract() -> Contract {
    let mut contract = Contract::full();
    contract.replay_origin = Some(ReplayOrigin::new("synthetic-client").unwrap());
    contract
}
fn no_attachment(value: &Value) {
    match value {
        Value::Object(object) => {
            assert!(!object.contains_key("_openbridge"));
            object.values().for_each(no_attachment);
        }
        Value::Array(values) => values.iter().for_each(no_attachment),
        _ => {}
    }
}
#[test]
fn public_client_rejects_independent_attachments() {
    let call = json!({"type":"function_call","id":"call","call_id":"c","name":"lookup","arguments":"{}","status":"completed"});
    for dialect in [Dialect::Standard, Dialect::MorphieCore] {
        let adapter = client(dialect);
        for attachment in [
            Value::Null,
            json!({}),
            json!({"version":1,"arguments":"json"}),
        ] {
            let mut item = call.clone();
            item["_openbridge"] = attachment.clone();
            let request = json!({"model":"fixture-model","input":[item]});
            assert!(
                adapter
                    .decode_request(&serde_json::to_vec(&request).unwrap())
                    .is_err()
            );
            let request = json!({"model":"fixture-model","input":"hi","_openbridge":attachment});
            assert!(
                adapter
                    .decode_request(&serde_json::to_vec(&request).unwrap())
                    .is_err()
            );
        }
        let histories = [
            json!([{"type":"reasoning","id":"thought","summary":[],"status":"completed","_openbridge":{"version":1,"replay":{"format":"google-interactions-v1-thought","phase":"final","value":"synthetic"}}}]),
            json!([{"type":"message","id":"owner","role":"assistant","content":[],"status":"completed"},
                {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}","_openbridge":{"version":1,"message":"owner"}}]),
            json!([call, {"type":"function_call_output","call_id":"c","output":"{}","_openbridge":{"version":1,"output":"json","execution":{"status":"succeeded"}}}]),
        ];
        for history in histories {
            let request = json!({"model":"fixture-model","input":history});
            assert!(
                adapter
                    .decode_request(&serde_json::to_vec(&request).unwrap())
                    .is_err()
            );
        }
        for attachment in [
            Value::Null,
            json!({}),
            json!({"version":1,"progress":"turn_finished"}),
            json!({"version":1,"usage":[{"scope":"session","basis":"cumulative","input_tokens":3}]}),
        ] {
            let mut response = wire::response(2);
            response["_openbridge"] = attachment;
            assert!(
                adapter
                    .decode_response(&serde_json::to_vec(&response).unwrap())
                    .is_err()
            );
        }
        let mut response = wire::response(1);
        response["output"][2]["_openbridge"] = json!({"version":1,"arguments":"json"});
        assert!(
            adapter
                .decode_response(&serde_json::to_vec(&response).unwrap())
                .is_err()
        );
    }
}
#[test]
fn attachment_at_any_snapshot_boundary_poisons_the_stream() {
    let original = wire::events(1);
    let opening = original
        .iter()
        .position(|v| v["type"] == "response.output_item.added")
        .unwrap();
    let done = original
        .iter()
        .position(|v| v["type"] == "response.output_item.done")
        .unwrap();
    for dialect in [Dialect::Standard, Dialect::MorphieCore] {
        for (index, pointer, attachment) in [
            (
                0,
                "/response",
                json!({"version":1,"progress":"needs_continuation"}),
            ),
            (
                opening,
                "/item",
                json!({"version":1,"replay":{"format":"google-interactions-v1-thought","phase":"partial","value":"synthetic"}}),
            ),
            (done, "/item", Value::Null),
            (
                original.len() - 1,
                "/response",
                json!({"version":1,"progress":"turn_finished"}),
            ),
            (
                original.len() - 1,
                "/response/output/2",
                json!({"version":1,"arguments":"json"}),
            ),
        ] {
            let mut decoder = client(dialect).event_decoder();
            for event in &original[..index] {
                decoder.push(event).unwrap();
            }
            let mut value = original[index].clone();
            value.pointer_mut(pointer).unwrap()["_openbridge"] = attachment;
            assert!(decoder.push(&value).is_err(), "boundary {index}");
            assert!(decoder.push(&original[index]).is_err());
            assert!(decoder.finish().is_err());
        }
    }
}
#[test]
fn standard_values_and_native_replay_do_not_need_an_attachment() {
    for dialect in [Dialect::Standard, Dialect::MorphieCore] {
        let adapter = client(dialect);
        let response = adapter
            .decode_response(&serde_json::to_vec(&wire::response(1)).unwrap())
            .unwrap();
        let encoded = adapter.encode_response(&response, &contract()).unwrap();
        no_attachment(&encoded);
        assert_eq!(encoded["output"], wire::response(1)["output"]);
        assert_eq!(encoded["usage"], wire::response(1)["usage"]);
        let mut decoder = adapter.event_decoder();
        let mut events = vec![];
        for value in wire::events(1) {
            events.extend(decoder.push(&value).unwrap());
        }
        decoder.finish().unwrap();
        // Static calls have no part IDs; stream builders allocate argument parts.
        let streamed = decoder.materialize().unwrap();
        let streamed_wire = adapter.encode_response(&streamed, &contract()).unwrap();
        assert_eq!(streamed_wire["output"], wire::response(1)["output"]);
        assert_eq!(
            streamed.semantic.usage_reports(),
            response.semantic.usage_reports()
        );
        let mut encoder = EventEncoder::new(Profile::Responses, response.metadata.clone())
            .unwrap()
            .with_contract(adapter.contract(&contract()));
        let mut output = vec![];
        for value in events {
            output.extend(encoder.encode(&value, decoder.fidelity()).unwrap());
        }
        encoder.finish().unwrap();
        for value in &output {
            no_attachment(value);
        }
        assert_eq!(
            output.last().unwrap()["response"]["output"],
            wire::response(1)["output"]
        );

        // A protocol-key rejection is not a ban on arbitrary user strings or JSON keys.
        let raw = "{\"_openbridge\":\"business data\",\"n\":18446744073709551616001}";
        let value = json!({"model":"fixture-model","metadata":{"_openbridge":"user metadata"},"input":[
            {"type":"function_call","call_id":"c","name":"lookup","arguments":raw},
            {"type":"function_call_output","call_id":"c","output":raw}
        ]});
        let request = adapter
            .decode_request(&serde_json::to_vec(&value).unwrap())
            .unwrap();
        let encoded = adapter
            .encode_request(&request, "fixture-model", &contract())
            .unwrap();
        assert_eq!(encoded["input"][0]["arguments"], raw);
        assert_eq!(encoded["input"][1]["output"], raw);
        assert_eq!(encoded["metadata"]["_openbridge"], "user metadata");
    }
}
#[test]
fn unsupported_typed_values_remain_rejected_instead_of_becoming_text_or_extras() {
    let adapter = client(Dialect::MorphieCore);
    let base = adapter
        .decode_response(&serde_json::to_vec(&wire::response(2)).unwrap())
        .unwrap();
    let call = |arguments, message| {
        Item::ToolCall(ToolCall {
            call_id: event::text("c"),
            name: event::text("lookup"),
            arguments,
            message,
            status: ItemLifecycle::Completed,
            context: Default::default(),
        })
    };
    let mut usage = Usage::operation(3, 5, 8);
    usage.scope = UsageScope::Session;
    let mut prediction = Usage::operation(3, 5, 8);
    prediction.accepted_prediction_tokens = Some(1);
    let response =
        |item| GenerationResponse::new(vec![(ItemId::new(1), item)], Outcome::Completed).unwrap();
    let mut grouped = base.semantic.items().to_vec();
    grouped.push((
        ItemId::new(2),
        call(ToolArguments::Raw("{}".into()), Some(grouped[0].0)),
    ));
    for semantic in [
        base.semantic
            .clone()
            .with_progress(InteractionProgress::TurnFinished)
            .unwrap(),
        base.semantic.clone().with_usage(usage).unwrap(),
        base.semantic.clone().with_usage(prediction).unwrap(),
        response(call(
            ToolArguments::Structured(StructuredValue::new(json!({"n":1})).unwrap()),
            None,
        )),
        response(Item::Reasoning(ReasoningItem {
            parts: vec![],
            status: ItemLifecycle::Completed,
            replay: Some(ReplayValue::final_value(
                ReplayFormat::GoogleInteractionsV1Thought,
                event::text("synthetic"),
            )),
        })),
        GenerationResponse::new(grouped, Outcome::Completed).unwrap(),
    ] {
        let mut decoded = base.clone();
        decoded.semantic = semantic.clone();
        assert!(adapter.encode_response(&decoded, &contract()).is_err());
        assert_eq!(decoded.semantic, semantic);
    }
    let request = adapter
        .decode_request(
            &serde_json::to_vec(&json!({"model":"fixture-model","input":[
                {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"},
                {"type":"function_call_output","call_id":"c","output":"{}"}
            ]}))
            .unwrap(),
        )
        .unwrap();
    for structured in [false, true] {
        let mut request = request.clone();
        let mut items = request.task.semantic.items().to_vec();
        let Item::ToolResult(result) = &mut items[1].1 else {
            unreachable!()
        };
        if structured {
            result.output =
                ToolOutput::Structured(StructuredValue::new(json!({"ok":true})).unwrap());
        } else {
            result.execution = Some(ToolExecution::Succeeded);
        }
        request.task.semantic = request.task.semantic.with_items(items).unwrap();
        assert!(
            adapter
                .encode_request(&request, "fixture-model", &contract())
                .is_err()
        );
    }
}
#[test]
fn unrepresentable_events_fail_at_their_first_publication_boundary() {
    let adapter = client(Dialect::MorphieCore);
    let mut usage = Usage::operation(3, 5, 8);
    usage.scope = UsageScope::Session;
    for event in [
        StreamEvent::Progress(InteractionProgress::NeedsContinuation),
        StreamEvent::Usage(usage),
        StreamEvent::ItemStarted {
            item: ItemId::new(1),
            kind: ItemKind::Reasoning,
            replay: Some(ReasoningReplay {
                origin: None,
                value: ReplayValue::partial(
                    ReplayFormat::GoogleInteractionsV1Thought,
                    event::text("synthetic"),
                ),
            }),
        },
        event::start(
            1,
            ItemKind::ToolCall {
                call_id: event::text("c"),
                name: event::text("lookup"),
                message: None,
                context: Default::default(),
                format: ArgumentFormat::Json,
            },
        ),
    ] {
        let mut encoder = EventEncoder::new(Profile::Responses, event::metadata())
            .unwrap()
            .with_contract(adapter.contract(&contract()));
        encoder
            .encode(&StreamEvent::Started, &Default::default())
            .unwrap();
        assert!(encoder.encode(&event, &Default::default()).is_err());
        assert!(encoder.finish().is_err());
    }
}
