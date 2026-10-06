use morphiecore::{
    adapter::images::{Profile, Request},
    lowering::images::{AccountingPolicy, project_response},
    protocol::{openai::images, openrouter_images},
    semantic::{
        task::image_generation::*,
        value::{ImageFormat, Presence},
    },
};
use serde_json::json;
#[test]
fn openrouter_image_requests_pin_operation_routing_and_do_not_invent_format_support() {
    let request = Request::new(
        "gpt-image-2.5-flare",
        ImageGenerationRequest::new("a blue square").unwrap(),
    );
    assert_eq!(
        openrouter_images::encode_request(&request, "openai/gpt-image-2.5-flare").unwrap(),
        json!({"model":"openai/gpt-image-2.5-flare","prompt":"a blue square","stream":false,"n":1,"provider":{"only":["openai"],"allow_fallbacks":false}})
    );
    let mut request = request;
    request.task.count = Presence::Null;
    request.task.format = Presence::Null;
    request.delivery.stream = Presence::Null;
    assert_eq!(
        openrouter_images::encode_request(&request, "openai/gpt-image-2.5-flare").unwrap(),
        json!({"model":"openai/gpt-image-2.5-flare","prompt":"a blue square","stream":false,"n":1,"provider":{"only":["openai"],"allow_fallbacks":false}})
    );
    assert!(openrouter_images::encode_request(&request, "openai/another-image").is_err());
    request.task.format = Presence::Value(ImageFormat::Png);
    assert!(
        Profile::OpenRouterFlare
            .encode_request(&request, "openai/gpt-image-2.5-flare")
            .is_err()
    );
}
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
#[test]
fn complete_tokens_survive_billing_loss_and_edits_never_restore_usage() {
    let mut response = ImageGenerationResponse::new(
        9,
        vec![GeneratedImage::new(ImageData::new("AQID").unwrap())],
    )
    .unwrap();
    response.usage = Presence::Value(ImageUsage {
        input: 3,
        input_details: Presence::Value(ImageTokenBreakdown::text_image(3, 0)),
        output: 5,
        output_details: Presence::Absent,
        total: 8,
        billing: ImageBilling {
            cost: Presence::Value(
                UsdAmount::new("0.01234567890123456789".parse().unwrap()).unwrap(),
            ),
            ..Default::default()
        },
    });
    let projected =
        project_response(&response, AccountingPolicy::OmitUnrepresentableAccounting).unwrap();
    assert!(!projected.loss.usage_omitted);
    assert!(projected.loss.billing_omitted);
    assert_eq!(
        images::encode_response(&projected.response).unwrap(),
        json!({"created":9,"data":[{"b64_json":"AQID"}],"usage":{"input_tokens":3,"input_tokens_details":{"text_tokens":3,"image_tokens":0},"output_tokens":5,"total_tokens":8}})
    );
    response.usage = Presence::Absent;
    let projected =
        project_response(&response, AccountingPolicy::OmitUnrepresentableAccounting).unwrap();
    assert!(projected.response.usage.is_absent());
    assert!(!projected.loss.billing_omitted);
    response.images[0].format = Presence::Value(ImageFormat::Gif);
    assert!(project_response(&response, AccountingPolicy::OmitUnrepresentableAccounting).is_err());
}
#[test]
fn openrouter_cost_aliases_and_sparse_presence_are_not_fabricated_totals() {
    let wire=br#"{"created":1,"data":[{"b64_json":"AQID"}],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3,"prompt_tokens_details":null,"completion_tokens_details":{},"cost":null,"cost_details":{"upstream_inference_input_cost":0.0001,"upstream_inference_output_cost":0.0002}}}"#;
    let response = openrouter_images::decode_response(wire).unwrap();
    let usage = response.usage.value().unwrap();
    assert_eq!(usage.input_details, Presence::Null);
    assert_eq!(
        usage.output_details,
        Presence::Value(ImageTokenBreakdown::default())
    );
    assert_eq!(usage.billing.cost, Presence::Null);
    let cost = usage.billing.upstream.value().unwrap();
    assert!(cost.upstream_total.is_absent());
    assert_eq!(
        cost.upstream_input.value().unwrap().amount().to_string(),
        "0.0001"
    );
    assert_eq!(
        cost.upstream_output.value().unwrap().amount().to_string(),
        "0.0002"
    );
}
#[test]
fn image_accounting_loss_never_hides_bad_artifacts_or_invalid_reports() {
    for value in [
        json!({"created":1,"data":[]}),
        json!({"created":1,"data":[{"b64_json":"?"}]}),
        json!({"created":1,"data":[{"b64_json":"AQID","media_type":"image/svg+xml"}]}),
        json!({"created":1,"data":[{"b64_json":"AQID"}],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":0}}),
        json!({"created":1,"data":[{"b64_json":"AQID"}],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3,"cost":-1}}),
    ] {
        assert!(openrouter_images::decode_response(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    let response =
        openrouter_images::decode_response(br#"{"created":1,"data":[{"b64_json":"AQID"}]}"#)
            .unwrap();
    assert!(response.images[0].format.is_absent());
    let projected =
        project_response(&response, AccountingPolicy::OmitUnrepresentableAccounting).unwrap();
    assert!(!projected.loss.usage_omitted);
    assert!(projected.response.images[0].format.is_absent());
}
