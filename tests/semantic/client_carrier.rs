//! Public protocol boundaries reject private attachments without weakening typed IR.
use crate::{events_support as event, wire};
use morphiecore::{
    protocol::openai::{Adapter, Dialect, Profile, TargetCaps as Contract},
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
fn unsupported_typed_values_remain_rejected_instead_of_becoming_text_or_extras() {
    let adapter = client(Dialect::MorphieCore);
    let base = adapter
        .decode_response(&serde_json::to_vec(&wire::response(2)).unwrap())
        .unwrap();
    let call = |arguments| {
        Item::ToolCall(ToolCall {
            call_id: event::text("c"),
            name: event::text("lookup"),
            arguments,
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
    grouped.push((ItemId::new(2), call(ToolArguments::Raw("{}".into()))));
    for semantic in [
        base.semantic
            .clone()
            .with_progress(InteractionProgress::TurnFinished)
            .unwrap(),
        base.semantic.clone().with_usage(usage).unwrap(),
        response(call(ToolArguments::Structured(
            StructuredValue::new(json!({"n":1})).unwrap(),
        ))),
        response(Item::Reasoning(ReasoningItem {
            parts: vec![],
            status: ItemLifecycle::Completed,
            replay: Some(ReplayValue::final_value(
                ReplayFormat::GoogleInteractionsV1Thought,
                event::text("synthetic"),
            )),
        })),
    ] {
        let mut decoded = base.clone();
        decoded.semantic = semantic.clone();
        assert!(adapter.encode_response(&decoded, &contract()).is_err());
        assert_eq!(decoded.semantic, semantic);
    }
    let mut decoded = base.clone();
    decoded.semantic = GenerationResponse::from_resources(
        grouped,
        Outcome::Completed,
        base.semantic.resources().clone(),
    )
    .unwrap()
    .with_message_owners(vec![(ItemId::new(2), ItemId::new(1))])
    .unwrap();
    let encoded = adapter.encode_response(&decoded, &contract()).unwrap();
    no_attachment(&encoded);
    assert_eq!(encoded["output"][1]["call_id"], "c");
    assert_eq!(decoded.semantic.message_owners().len(), 1);
    let mut counted = base.clone();
    counted.semantic = counted.semantic.with_usage(prediction).unwrap();
    let projected = adapter.encode_response(&counted, &contract()).unwrap();
    no_attachment(&projected);
    assert_eq!(projected["usage"]["total_tokens"], 8);
    assert!(
        projected["usage"]["output_tokens_details"]
            .get("accepted_prediction_tokens")
            .is_none()
    );
    assert_eq!(
        counted.semantic.usage().unwrap().accepted_prediction_tokens,
        Some(1)
    );
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
