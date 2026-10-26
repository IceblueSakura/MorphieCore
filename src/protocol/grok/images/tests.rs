use super::*;
use crate::protocol::failure::ShapeReason;
use crate::{
    adapter::images::Profile,
    lowering::images::{AccountingPolicy, project_response},
};

const MODEL: &str = "grok-imagine-image-2.0";
fn request() -> Request {
    Request::new(
        "public-image",
        ImageGenerationRequest::new("A blue square").unwrap(),
    )
}
fn decode(value: Value) -> Result<ImageGenerationResponse, CodecError> {
    Profile::GrokImagineImage2.decode_response(&serde_json::to_vec(&value).unwrap())
}

#[test]
fn request_projection_has_an_independent_wire_oracle() {
    let mut source = request();
    source.task.count = Presence::Value(2);
    source.task.quality = Presence::Value(ImageQualityRequest::Exact(ImageQuality::Low));
    source.identity.user = Presence::Value("fixture-user".into());
    let before = source.clone();
    assert_eq!(
        Profile::GrokImagineImage2
            .encode_request(&source, MODEL)
            .unwrap(),
        json!({
            "model": MODEL, "prompt": "A blue square", "n": 2,
            "quality": "low", "user": "fixture-user", "response_format": "b64_json"
        })
    );
    assert_eq!(source, before);
    let mut defaults = request();
    defaults.task.count = Presence::Null;
    defaults.task.quality = Presence::Null;
    assert_eq!(
        encode_request(&defaults, MODEL).unwrap(),
        json!({
            "model": MODEL, "prompt": "A blue square", "n": 1, "response_format": "b64_json"
        })
    );
    defaults.task.quality = Presence::Value(ImageQualityRequest::Exact(ImageQuality::Medium));
    assert_eq!(
        encode_request(&defaults, MODEL).unwrap()["quality"],
        "medium"
    );
    defaults.task.quality = Presence::Value(ImageQualityRequest::Auto);
    assert_eq!(encode_request(&defaults, MODEL).unwrap()["quality"], "auto");
}

#[test]
fn explicit_unrepresentable_controls_are_not_approximated() {
    let base = request();
    assert!(encode_request(&base, MODEL).is_ok());
    let mut cases = Vec::new();
    for size in [
        ImageSizeRequest::Auto,
        ImageSizeRequest::Exact(ImageSize {
            width: 1024,
            height: 1024,
        }),
    ] {
        let mut r = base.clone();
        r.task.size = Presence::Value(size);
        cases.push(r);
    }
    for quality in [
        ImageQualityRequest::Exact(ImageQuality::High),
        ImageQualityRequest::Exact(ImageQuality::Max),
    ] {
        let mut r = base.clone();
        r.task.quality = Presence::Value(quality);
        cases.push(r);
    }
    let mut r = base.clone();
    r.task.format = Presence::Value(ImageFormat::Jpeg);
    cases.push(r);
    let mut r = base.clone();
    r.task.background = Presence::Value(ImageBackgroundRequest::Auto);
    cases.push(r);
    let mut r = base.clone();
    r.task.compression = Presence::Value(80);
    cases.push(r);
    let mut r = base.clone();
    r.task.moderation = Presence::Value(ImageModeration::Low);
    cases.push(r);
    let mut r = base.clone();
    r.delivery.stream = Presence::Value(true);
    cases.push(r);
    for r in cases {
        r.task.validate().unwrap();
        assert!(encode_request(&r, MODEL).is_err());
    }
    assert!(encode_request(&base, "grok-imagine-image-quality").is_err());
}

#[test]
fn sparse_reports_remain_unknown_and_cannot_fabricate_openai_completion() {
    let result = decode(json!({
        "data":[{"b64_json":"AQ==","future_field":{"anything":1}}],
        "model":MODEL,"usage":{"cost_in_usd_ticks":400000000,"input_tokens":null},
        "future_field":true
    }))
    .unwrap();
    assert_eq!(result.created, Presence::Absent);
    assert_eq!(result.model, Presence::Value(MODEL.into()));
    assert_eq!(result.images[0].format, Presence::Absent);
    assert_eq!(result.images[0].data.as_base64(), "AQ==");
    let usage = result.usage.value().unwrap();
    assert_eq!(usage.input, Presence::Null);
    assert_eq!(usage.output, Presence::Absent);
    assert_eq!(usage.total, Presence::Absent);
    assert_eq!(
        usage.billing.cost.value().unwrap().amount().to_string(),
        "0.0400000000"
    );
    result.validate_for(&request().task).unwrap();
    assert!(crate::protocol::openai::images::encode_response(&result).is_err());
    let projected =
        project_response(&result, AccountingPolicy::OmitUnrepresentableAccounting).unwrap();
    assert!(projected.loss.usage_omitted);
    assert!(crate::protocol::openai::images::encode_response(&projected.response).is_err());
    let plain = decode(json!({"data":[{"b64_json":"AQ=="}]})).unwrap();
    assert!(crate::protocol::openai::images::encode_response(&plain).is_err());
}

#[test]
fn native_accounting_keeps_exact_ticks_and_nonadditive_cache() {
    let result = decode(json!({
        "created":123,"data":[{"b64_json":"AQ==","mime_type":"image/jpeg","respect_moderation":true}],
        "usage":{
            "cost_in_usd_ticks":9007199254740993u64,
            "input_tokens":10,"output_tokens":12,"total_tokens":22,
            "input_tokens_details":{"text_tokens":6,"image_tokens":4,"cached_tokens":5},
            "output_tokens_details":{"text_tokens":3,"image_tokens":7,"reasoning_tokens":2}
        }
    })).unwrap();
    assert_eq!(result.created, Presence::Value(123));
    assert_eq!(result.images[0].format, Presence::Value(ImageFormat::Jpeg));
    let usage = result.usage.value().unwrap();
    assert_eq!(usage.input, Presence::Value(10));
    assert_eq!(usage.output, Presence::Value(12));
    assert_eq!(usage.total, Presence::Value(22));
    assert_eq!(
        usage.input_details.value().unwrap().cached,
        Presence::Value(5)
    );
    assert_eq!(
        usage.output_details.value().unwrap().reasoning,
        Presence::Value(2)
    );
    assert_eq!(
        usage.billing.cost.value().unwrap().amount().to_string(),
        "900719.9254740993"
    );
}

#[test]
fn invalid_known_fields_and_nonfinal_artifacts_reject() {
    let valid = json!({"data":[{"b64_json":"AQ=="}]});
    assert!(decode(valid.clone()).is_ok());
    for (field, value) in [
        ("mime_type", json!(7)),
        ("mime_type", json!("image/gif")),
        ("respect_moderation", json!(false)),
        ("respect_moderation", json!("true")),
        ("url", json!("https://example.com/image")),
        ("file_output", json!({"file_id":"untrusted"})),
        ("storage_error", json!("fixture failure")),
        ("b64_json", json!("!")),
    ] {
        let mut wire = valid.clone();
        wire["data"][0][field] = value;
        assert!(decode(wire).is_err(), "{field}");
    }
    for (field, value) in [
        ("request_id", json!("pending")),
        ("status", json!("failed")),
        ("status", json!(7)),
        ("error", json!({})),
        ("created", json!("123")),
    ] {
        let mut wire = valid.clone();
        wire[field] = value;
        assert!(decode(wire).is_err());
    }
    assert!(decode(json!({"data":[]})).is_err());
    assert!(decode(json!({"data":vec![json!({"b64_json":"AQ=="});11]})).is_err());
    assert_eq!(
        decode_response(br#"{"data":[{"b64_json":"AQ==","b64_json":"Ag=="}]}"#).unwrap_err(),
        CodecError::Invalid(ShapeReason::Json)
    );
    for usage in [
        json!({"cost_in_usd_ticks":-1}),
        json!({"cost_in_usd_ticks":1.5}),
        json!({"cost_in_usd_ticks":1,"input_tokens":5,"output_tokens":2,"total_tokens":8}),
        json!({"cost_in_usd_ticks":1,"input_tokens_details":{"text_tokens":2,"image_tokens":0,"cached_tokens":3}}),
        json!({"cost_in_usd_ticks":1,"output_tokens":4,"output_tokens_details":{"text_tokens":2,"image_tokens":1,"reasoning_tokens":2}}),
        json!({"cost_in_usd_ticks":1,"total_tokens":1,"input_tokens_details":{"text_tokens":2,"image_tokens":0,"cached_tokens":0}}),
        json!({"cost_in_usd_ticks":1,"input_tokens":null,"total_tokens":1,"input_tokens_details":{"text_tokens":2,"image_tokens":0,"cached_tokens":0}}),
        json!({"cost_in_usd_ticks":1,"total_tokens":5,
            "input_tokens_details":{"text_tokens":2,"image_tokens":0,"cached_tokens":0},
            "output_tokens_details":{"text_tokens":2,"image_tokens":0,"reasoning_tokens":0}}),
    ] {
        let mut wire = valid.clone();
        wire["usage"] = usage;
        assert!(decode(wire).is_err());
    }
}

#[test]
fn standard_image_codec_still_requires_complete_standard_reports() {
    let wire = json!({"created":123,"data":[{"b64_json":"AQ=="}],"usage":{
        "input_tokens":3,"input_tokens_details":{"text_tokens":2,"image_tokens":1},
        "output_tokens":5,"total_tokens":8
    }});
    let decoded =
        crate::protocol::openai::images::decode_response(&serde_json::to_vec(&wire).unwrap())
            .unwrap();
    assert_eq!(decoded.created, Presence::Value(123));
    assert_eq!(decoded.usage.value().unwrap().input, Presence::Value(3));
    assert_eq!(
        crate::protocol::openai::images::encode_response(&decoded).unwrap(),
        wire
    );
    let mut incomplete = decoded;
    if let Presence::Value(usage) = &mut incomplete.usage {
        usage.total = Presence::Absent;
    }
    assert!(crate::protocol::openai::images::encode_response(&incomplete).is_err());
}
