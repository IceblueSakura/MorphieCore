use morphiecore::{
    lowering::images::{AccountingPolicy, project_response},
    protocol::{openai::images, openrouter_images},
};
use serde_json::json;
#[test]
fn sparse_image_accounting_stays_in_ir_and_has_explicit_downstream_loss() {
    let bytes=br#"{"created":42,"data":[{"b64_json":"AQID","media_type":"image/png"}],"usage":{"prompt_tokens":7,"completion_tokens":9,"total_tokens":16,"cost":0.01234567890123456789,"is_byok":false,"prompt_tokens_details":{"cached_tokens":2},"completion_tokens_details":{"image_tokens":8,"reasoning_tokens":1}}}"#;
    let response = openrouter_images::decode_response(bytes).unwrap();
    let usage = response.usage.value().unwrap();
    assert_eq!(usage.input, 7);
    assert!(usage.input_details.value().unwrap().text.is_absent());
    assert_eq!(
        usage.input_details.value().unwrap().cached.value(),
        Some(&2)
    );
    assert_eq!(
        usage.output_details.value().unwrap().image.value(),
        Some(&8)
    );
    assert_eq!(
        usage.billing.cost.value().unwrap().amount().to_string(),
        "0.01234567890123456789"
    );
    assert!(project_response(&response, AccountingPolicy::Strict).is_err());
    let projected =
        project_response(&response, AccountingPolicy::OmitUnrepresentableAccounting).unwrap();
    assert!(projected.loss.usage_omitted);
    assert!(projected.loss.billing_omitted);
    assert_eq!(
        images::encode_response(&projected.response).unwrap(),
        json!({"created":42,"data":[{"b64_json":"AQID"}],"output_format":"png"})
    );
    assert_eq!(
        response.usage.value().unwrap().input,
        7,
        "projection cannot mutate the source"
    );
}
