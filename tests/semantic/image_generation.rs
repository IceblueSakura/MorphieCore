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
