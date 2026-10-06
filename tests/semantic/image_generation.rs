use morphiecore::{
    protocol::openai::images,
    semantic::{task::image_generation::*, value::Presence},
};
use serde_json::json;

#[test]
fn multi_image_request_and_complete_wire_are_admitted() {
    let request =
        images::decode_request(br#"{"model":"public-image","prompt":"two squares","n":2}"#)
            .unwrap();
    assert_eq!(request.task.count, Presence::Value(2));
    assert_eq!(
        images::encode_request(&request, "upstream").unwrap(),
        json!({"model":"upstream","prompt":"two squares","n":2}),
    );
    let response = images::decode_response(
        br#"{"created":1,"data":[{"b64_json":"AQID"},{"b64_json":"BAUG"}]}"#,
    )
    .unwrap();
    response.validate_for(&request.task).unwrap();
    assert_eq!(
        images::encode_response(&response).unwrap(),
        json!({"created":1,"data":[{"b64_json":"AQID"},{"b64_json":"BAUG"}]}),
    );
}

#[test]
fn image_generation_request_decode_and_typed_edit_have_independent_wire_expectations() {
    let wire = br#"{"model":"public-image","prompt":"draw a square","n":1,"stream":false,"output_format":"png"}"#;
    let mut request = images::decode_request(wire).unwrap();
    assert_eq!(request.task.prompt(), "draw a square");
    assert_eq!(request.task.count, Presence::Value(1));
    request.task.set_prompt("draw a circle").unwrap();
    assert_eq!(
        images::encode_request(&request, "upstream-image").unwrap(),
        json!({
            "model":"upstream-image","prompt":"draw a circle","n":1,"stream":false,"output_format":"png"
        })
    );
    let task = ImageGenerationRequest::new("pure library consumer").unwrap();
    assert_eq!(task.prompt(), "pure library consumer");
    assert!(ImageGenerationRequest::new("").is_err());
    assert!(ImageGenerationRequest::new("x".repeat(32_001)).is_err());
    assert!(ImageGenerationRequest::new("图".repeat(32_000)).is_ok());
    assert!(!format!("{task:?}").contains("pure library consumer"));
    let mut request = morphiecore::adapter::images::Request::new("public-image", task);
    assert!(request.task.set_prompt("").is_err());
    assert_eq!(request.task.prompt(), "pure library consumer");
    request.task.count = Presence::Value(11);
    assert!(images::encode_request(&request, "upstream-image").is_err());
}

#[test]
fn image_generation_request_rejects_unsupported_semantics_and_duplicate_keys() {
    for patch in [
        json!({"n":11}),
        json!({"stream":true}),
        json!({"output_format":"bmp"}),
        json!({"response_format":"b64_json"}),
        json!({"size":"not-a-size"}),
        json!({"provider":null}),
        json!({"_openbridge":{}}),
        json!({"prompt":null}),
    ] {
        let mut v = json!({"model":"public-image","prompt":"square"});
        v.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(
            images::decode_request(&serde_json::to_vec(&v).unwrap()).is_err(),
            "{patch}"
        );
    }
    assert!(images::decode_request(br#"{"model":"a","prompt":"x","prompt":"y"}"#).is_err());
    for fields in [
        json!({}),
        json!({"n":null,"stream":null,"output_format":null}),
    ] {
        let mut v = json!({"model":"public-image","prompt":"square"});
        v.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let request = images::decode_request(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert_eq!(images::encode_request(&request, "public-image").unwrap(), v);
    }
}

#[test]
fn image_generation_reported_values_are_not_request_defaults() {
    let v = json!({"created":123,"data":[{"b64_json":"AQID"}],"output_format":"png", "size":"1536x1024",
        "background":"opaque","quality":"high","usage":{"input_tokens":3,"input_tokens_details":{"text_tokens":3,"image_tokens":0},"output_tokens":5,"total_tokens":8}});
    let mut response = images::decode_response(&serde_json::to_vec(&v).unwrap()).unwrap();
    assert_eq!(response.images[0].data.as_base64(), "AQID");
    assert_eq!(response.images[0].size.value().unwrap().width, 1536);
    response.images[0].data = ImageData::new("BAUG").unwrap();
    let mut expected = v;
    expected["data"][0]["b64_json"] = json!("BAUG");
    assert_eq!(images::encode_response(&response).unwrap(), expected);
    response.usage = Presence::Absent;
    response.images[0].size = Presence::Absent;
    let edited = images::encode_response(&response).unwrap();
    assert!(edited.get("usage").is_none());
    assert!(edited.get("size").is_none());
    let response = ImageGenerationResponse::new(
        123,
        vec![GeneratedImage::new(ImageData::new("AQID").unwrap())],
    )
    .unwrap();
    assert_eq!(
        images::encode_response(&response).unwrap(),
        json!({"created":123,"data":[{"b64_json":"AQID"}]})
    );
    assert!(!format!("{response:?}").contains("AQID"));
    for v in [
        json!({"created":123,"data":[{"b64_json":"AQID"}]}),
        json!({"created":123,"data":[{"b64_json":"AQID"}],"output_format":null,"size":null,"background":null,"quality":null,"usage":null}),
    ] {
        let r = images::decode_response(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert_eq!(images::encode_response(&r).unwrap(), v);
    }
}

#[test]
fn image_generation_typed_reports_encode_without_wire_round_trips() {
    use morphiecore::semantic::value::ImageFormat;
    let mut response = ImageGenerationResponse::new(
        987,
        vec![GeneratedImage::new(ImageData::new("BAUG").unwrap())],
    )
    .unwrap();
    response.images[0].format = Presence::Value(ImageFormat::Webp);
    response.images[0].size = Presence::Value(ImageSize {
        width: 640,
        height: 480,
    });
    response.images[0].background = Presence::Value(ImageBackground::Transparent);
    response.images[0].quality = Presence::Value(ImageQuality::Medium);
    response.usage = Presence::Value(ImageUsage {
        input: 5,
        input_details: Presence::Value(ImageTokenBreakdown::text_image(2, 3)),
        output: 4,
        output_details: Presence::Value(ImageTokenBreakdown::text_image(0, 4)),
        total: 9,
        billing: ImageBilling::default(),
    });
    assert_eq!(
        images::encode_response(&response).unwrap(),
        json!({
            "created":987,"data":[{"b64_json":"BAUG"}],"output_format":"webp","size":"640x480",
            "background":"transparent","quality":"medium","usage":{"input_tokens":5,
            "input_tokens_details":{"text_tokens":2,"image_tokens":3},"output_tokens":4,
            "output_tokens_details":{"text_tokens":0,"image_tokens":4},"total_tokens":9}
        })
    );
    let mut request = ImageGenerationRequest::new("square").unwrap();
    request.format = Presence::Value(ImageFormat::Png);
    assert!(response.validate_for(&request).is_err());
    response.images[0].format = Presence::Absent;
    response.validate_for(&request).unwrap();
    assert!(
        response.images[0].format.is_absent(),
        "validation never invents reported format"
    );
    response.usage.value().unwrap().validate().unwrap();
    response.usage = Presence::Value(ImageUsage {
        input: u64::MAX,
        input_details: Presence::Value(ImageTokenBreakdown::text_image(u64::MAX, 0)),
        output: 1,
        output_details: Presence::Absent,
        total: 0,
        billing: ImageBilling::default(),
    });
    assert!(images::encode_response(&response).is_err());
}

#[test]
fn image_generation_invalid_incomplete_and_oversized_artifacts_fail_closed() {
    for v in [
        json!({"created":1}),
        json!({"created":1,"data":[]}),
        json!({"created":1,"data":[{"b64_json":"AQID"},{"b64_json":"!bad"}]}),
        json!({"created":1,"data":[{"url":"https://example.invalid/a"}]}),
        json!({"created":1,"data":[{"b64_json":""}]}),
        json!({"created":1,"data":[{"b64_json":"!bad"}]}),
        json!({"created":1,"data":[{"b64_json":"AQID"}],"size":"auto"}),
        json!({"created":1,"data":[{"b64_json":"AQID"}],"usage":{"input_tokens":3,"input_tokens_details":{"text_tokens":2,"image_tokens":0},"output_tokens":5,"total_tokens":8}}),
    ] {
        assert!(images::decode_response(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    assert!(
        images::decode_response(br#"{"created":1,"data":[{"b64_json":"AQID"}]} trailing"#).is_err()
    );
    assert!(
        images::decode_response(br#"{"created":1,"data":[{"b64_json":"AQID","b64_json":"BAUG"}]}"#)
            .is_err()
    );
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(vec![0; MAX_IMAGE_BYTES]);
    assert!(ImageData::new(&encoded).is_ok());
    let encoded = base64::engine::general_purpose::STANDARD.encode(vec![0; MAX_IMAGE_BYTES + 1]);
    assert!(ImageData::new(&encoded).is_err());
}
