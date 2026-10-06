//! Billing views must promote independent modality facts, never replay stale counters.
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract as Contract,
    protocol::openai::{Profile, events::EventEncoder},
    semantic::task::generation::Usage,
};
use serde_json::{Value, json};
fn fixture() -> Value {
    json!({"object":"response","id":"r","model":"m","created_at":1,"status":"completed",
        "output":[{"type":"message","id":"msg","role":"assistant","status":"completed","content":[{"type":"output_text","text":"x","annotations":[]}]}],
        "usage":{"input_tokens":10,"output_tokens":2,"total_tokens":12,
            "input_tokens_details":{"cached_tokens":1},"output_tokens_details":{"reasoning_tokens":0},
            "x_details":[{"x_billing_type":"response_api","input_tokens":10,"output_tokens":2,"total_tokens":12,"image_tokens":7,
                "input_tokens_details":{"image_tokens":7,"text_tokens":3},"prompt_tokens_details":{"cached_tokens":1},
                "output_tokens_details":{"reasoning_tokens":0,"text_tokens":2}}]}})
}
fn expected_usage() -> Value {
    json!({"input_tokens":10,"output_tokens":2,"total_tokens":12,"input_tokens_details":{"cached_tokens":1,"image_tokens":7,"text_tokens":3},"output_tokens_details":{"reasoning_tokens":0,"text_tokens":2}})
}
#[test]
fn modality_billing_view_promotes_typed_facts_and_edits_cannot_restore_the_view() {
    let provider = Adapter::new(Profile::Responses, Dialect::Bailian, None);
    let mut decoded = provider
        .decode_response(&serde_json::to_vec(&fixture()).unwrap())
        .unwrap();
    let usage = decoded.semantic.usage().unwrap();
    assert_eq!(
        (
            usage.input_image_tokens,
            usage.input_text_tokens,
            usage.output_text_tokens
        ),
        (Some(7), Some(3), Some(2))
    );
    let client = Adapter::new(Profile::Responses, Dialect::MorphieCore, None);
    assert_eq!(
        client.encode_response(&decoded, &Contract::full()).unwrap()["usage"],
        expected_usage()
    );
    assert!(
        Adapter::new(Profile::Responses, Dialect::Standard, None)
            .encode_response(&decoded, &Contract::full())
            .is_err()
    );
    let native = provider
        .encode_response(&decoded, &Contract::full())
        .unwrap();
    assert_eq!(
        native["usage"]["input_tokens_details"],
        json!({"cached_tokens":1})
    );
    assert_eq!(
        native["usage"]["x_details"][0]["input_tokens_details"],
        json!({"image_tokens":7,"text_tokens":3})
    );
    decoded.semantic = decoded
        .semantic
        .clone()
        .with_usage(Usage {
            scope: morphiecore::semantic::task::generation::UsageScope::Operation,
            basis: morphiecore::semantic::task::generation::UsageBasis::Final,
            output_relation:
                morphiecore::semantic::task::generation::OutputTokenRelation::IncludesReasoning,
            total_relation:
                morphiecore::semantic::task::generation::TotalTokenRelation::InputAndOutput,
            input_image_tokens: Some(5),
            input_text_tokens: Some(2),
            ..usage
        })
        .unwrap();
    let edited = provider
        .encode_response(&decoded, &Contract::full())
        .unwrap();
    assert_eq!(edited["usage"]["x_details"][0]["image_tokens"], 5);
    assert_eq!(
        edited["usage"]["x_details"][0]["input_tokens_details"]["text_tokens"],
        2
    );
    decoded.semantic = decoded
        .semantic
        .clone()
        .with_usage(Usage {
            scope: morphiecore::semantic::task::generation::UsageScope::Operation,
            basis: morphiecore::semantic::task::generation::UsageBasis::Final,
            output_relation:
                morphiecore::semantic::task::generation::OutputTokenRelation::IncludesReasoning,
            total_relation:
                morphiecore::semantic::task::generation::TotalTokenRelation::InputAndOutput,
            input_image_tokens: None,
            input_text_tokens: None,
            output_text_tokens: None,
            ..usage
        })
        .unwrap();
    let deleted = client.encode_response(&decoded, &Contract::full()).unwrap();
    assert_eq!(
        deleted["usage"]["input_tokens_details"],
        json!({"cached_tokens":1})
    );
    assert_eq!(
        deleted["usage"]["output_tokens_details"],
        json!({"reasoning_tokens":0})
    );
    let native = provider
        .encode_response(&decoded, &Contract::full())
        .unwrap();
    assert!(
        native["usage"]["x_details"][0]
            .get("image_tokens")
            .is_none()
    );
}
#[test]
fn billing_views_reject_alias_conflicts_unknown_breakdowns_and_invalid_counts() {
    let provider = Adapter::new(Profile::Responses, Dialect::Bailian, None);
    for path in [
        "/usage/x_details/0/image_tokens",
        "/usage/x_details/0/input_tokens",
        "/usage/x_details/0/prompt_tokens_details/cached_tokens",
        "/usage/x_details/0/output_tokens_details/reasoning_tokens",
    ] {
        let mut value = fixture();
        *value.pointer_mut(path).unwrap() = json!(9);
        assert!(
            provider
                .decode_response(&serde_json::to_vec(&value).unwrap())
                .is_err()
        );
    }
    for invalid in [json!(-1), json!(1.5), json!(true), json!("3"), json!(11)] {
        let mut value = fixture();
        value["usage"]["x_details"][0]["input_tokens_details"]["text_tokens"] = invalid;
        assert!(
            provider
                .decode_response(&serde_json::to_vec(&value).unwrap())
                .is_err()
        );
    }
    let mut unknown = fixture();
    unknown["usage"]["x_details"][0]["plugins"] = json!({});
    assert!(
        provider
            .decode_response(&serde_json::to_vec(&unknown).unwrap())
            .is_err()
    );
    let mut multiple = fixture();
    let row = multiple["usage"]["x_details"][0].clone();
    multiple["usage"]["x_details"]
        .as_array_mut()
        .unwrap()
        .push(row);
    assert!(
        provider
            .decode_response(&serde_json::to_vec(&multiple).unwrap())
            .is_err()
    );
}
#[test]
fn modality_usage_static_event_closure_uses_declared_client_slots() {
    let provider = Adapter::new(Profile::Responses, Dialect::Bailian, None);
    let final_response = fixture();
    let mut started = final_response.clone();
    started["status"] = json!("in_progress");
    started["output"] = json!([]);
    started["usage"] = Value::Null;
    let message = final_response["output"][0].clone();
    let mut added = message.clone();
    added["status"] = json!("in_progress");
    added["content"] = json!([]);
    let values = vec![
        json!({"type":"response.created","response":started}),
        json!({"type":"response.output_item.added","output_index":0,"item":added}),
        json!({"type":"response.content_part.added","item_id":"msg","output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
        json!({"type":"response.output_text.delta","item_id":"msg","output_index":0,"content_index":0,"delta":"x","logprobs":[]}),
        json!({"type":"response.output_text.done","item_id":"msg","output_index":0,"content_index":0,"text":"x","logprobs":[]}),
        json!({"type":"response.content_part.done","item_id":"msg","output_index":0,"content_index":0,"part":message["content"][0]}),
        json!({"type":"response.output_item.done","output_index":0,"item":message}),
        json!({"type":"response.completed","response":final_response}),
    ];
    let mut decoder = provider.event_decoder();
    let mut events = vec![];
    for (index, mut value) in values.into_iter().enumerate() {
        value["sequence_number"] = json!(index + 1);
        events.extend(decoder.push(&value).unwrap());
    }
    let decoded = decoder.materialize().unwrap();
    let mut encoder = EventEncoder::new(Profile::Responses, decoded.metadata.clone())
        .unwrap()
        .with_contract(
            Adapter::new(Profile::Responses, Dialect::MorphieCore, None)
                .contract(&Contract::full()),
        );
    let mut output = vec![];
    for event in events {
        output.extend(encoder.encode(&event, &decoded.fidelity).unwrap());
    }
    encoder.finish().unwrap();
    assert_eq!(
        output.last().unwrap()["response"]["usage"],
        expected_usage()
    );
}
