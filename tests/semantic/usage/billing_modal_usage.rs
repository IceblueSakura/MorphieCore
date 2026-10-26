//! Billing views must promote independent modality facts, never replay stale counters.
use morphiecore::{
    protocol::openai::{Dialect, GenerationApi, GenerationCodec, TargetContract as Contract},
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
    let provider = GenerationCodec::new(GenerationApi::Responses, Dialect::Bailian, None);
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
    let client = GenerationCodec::new(GenerationApi::Responses, Dialect::MorphieCore, None);
    assert_eq!(
        client.encode_response(&decoded, &Contract::full()).unwrap()["usage"],
        expected_usage()
    );
    let standard = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None)
        .encode_response(&decoded, &Contract::full())
        .unwrap();
    assert_eq!(
        standard["usage"],
        json!({"input_tokens":10,"output_tokens":2,"total_tokens":12,
        "input_tokens_details":{"cached_tokens":1},"output_tokens_details":{"reasoning_tokens":0}})
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
        .with_usage(Some(Usage {
            input_relation:
                morphiecore::semantic::task::generation::InputTokenRelation::IncludesCache,
            output_relation:
                morphiecore::semantic::task::generation::OutputTokenRelation::IncludesReasoning,
            total_relation:
                morphiecore::semantic::task::generation::TotalTokenRelation::InputAndOutput,
            input_image_tokens: Some(5),
            input_text_tokens: Some(2),
            ..usage
        }))
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
        .with_usage(Some(Usage {
            input_relation:
                morphiecore::semantic::task::generation::InputTokenRelation::IncludesCache,
            output_relation:
                morphiecore::semantic::task::generation::OutputTokenRelation::IncludesReasoning,
            total_relation:
                morphiecore::semantic::task::generation::TotalTokenRelation::InputAndOutput,
            input_image_tokens: None,
            input_text_tokens: None,
            output_text_tokens: None,
            ..usage
        }))
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
