//! Ordered artifact authority and exact local satisfaction, independent of catalogs.
use morphiecore::{
    adapter::images::Request,
    lowering::images::{AccountingPolicy, project_response},
    protocol::{openai::images, openrouter_images},
    semantic::{
        task::image_generation::*,
        value::{ImageFormat, Presence},
    },
};
use serde_json::json;

fn artifact(data: &str) -> GeneratedImage {
    GeneratedImage::new(ImageData::new(data).unwrap())
}

#[test]
fn count_boundaries_and_final_request_edits_control_both_wires() {
    for n in [0, 11, 255] {
        let mut request = Request::new("m", ImageGenerationRequest::new("x").unwrap());
        request.task.count = Presence::Value(n);
        assert!(images::encode_request(&request, "m").is_err());
        assert!(openrouter_images::encode_request(&request, "openai/gpt-image-2.5-flare").is_err());
    }
    for n in [json!(-1), json!(1.5), json!(true), json!("2"), json!(256)] {
        assert!(
            images::decode_request(
                &serde_json::to_vec(&json!({"model":"m","prompt":"x","n":n})).unwrap()
            )
            .is_err()
        );
    }
    for n in [1, 2, 10] {
        let wire = json!({"model":"m","prompt":"x","n":n});
        let mut request = images::decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
        assert_eq!(request.task.requested_count(), n);
        assert_eq!(
            openrouter_images::encode_request(&request, "openai/gpt-image-2.5-flare").unwrap(),
            json!({"model":"openai/gpt-image-2.5-flare","prompt":"x","n":n,"stream":false,"provider":{"only":["openai"],"allow_fallbacks":false}})
        );
        request.task.count = Presence::Absent;
        assert_eq!(
            images::encode_request(&request, "m").unwrap(),
            json!({"model":"m","prompt":"x"})
        );
        assert_eq!(request.task.requested_count(), 1);
    }
}

#[test]
fn typed_collection_edits_keep_order_bytes_and_operation_accounting() {
    let mut response =
        ImageGenerationResponse::new(7, vec![artifact("AQID"), artifact("BAUG")]).unwrap();
    response.usage = Presence::Value(ImageUsage {
        input: 3,
        output: 5,
        total: 8,
        input_details: Presence::Value(ImageTokenBreakdown::text_image(3, 0)),
        output_details: Presence::Absent,
        billing: ImageBilling::default(),
    });
    let mut request = ImageGenerationRequest::new("x").unwrap();
    request.count = Presence::Value(2);
    response.validate_for(&request).unwrap();
    response.images.swap(0, 1);
    response.images[1].data = ImageData::new("BwgJ").unwrap();
    let expected = json!({"created":7,"data":[{"b64_json":"BAUG"},{"b64_json":"BwgJ"}],
        "usage":{"input_tokens":3,"input_tokens_details":{"text_tokens":3,"image_tokens":0},"output_tokens":5,"total_tokens":8}});
    assert_eq!(images::encode_response(&response).unwrap(), expected);
    response.images.remove(0);
    assert!(response.validate_for(&request).is_err());
    assert_eq!(
        images::encode_response(&response).unwrap()["data"],
        json!([{"b64_json":"BwgJ"}])
    );
    response.images.push(artifact("AQID"));
    response.validate_for(&request).unwrap();
    response.images.clear();
    assert!(images::encode_response(&response).is_err());
    assert!(ImageGenerationResponse::new(0, vec![artifact("AQID"); 11]).is_err());
}

#[test]
fn legal_openrouter_short_result_decodes_but_does_not_satisfy_exact_intent() {
    let wire = br#"{"created":1,"data":[{"b64_json":"AQID","media_type":"image/png"}]}"#;
    let response = openrouter_images::decode_response(wire).unwrap();
    let mut request = ImageGenerationRequest::new("x").unwrap();
    request.count = Presence::Value(2);
    response.validate().unwrap();
    assert!(response.validate_for(&request).is_err());
    request.count = Presence::Null;
    response.validate_for(&request).unwrap();
    let mut over = response;
    over.images.push(artifact("BAUG"));
    assert!(over.validate_for(&request).is_err());
}

#[test]
fn standard_shared_reports_apply_to_all_images_without_inventing_per_image_facts() {
    let wire = br#"{"created":1,"data":[{"b64_json":"AQID"},{"b64_json":"BAUG"}],"output_format":"png","size":"2x3","quality":"low","background":null}"#;
    let response = images::decode_response(wire).unwrap();
    assert_eq!(response.images.len(), 2);
    for (i, expected) in ["AQID", "BAUG"].iter().enumerate() {
        let image = &response.images[i];
        assert_eq!(image.data.as_base64(), *expected);
        assert_eq!(image.format, Presence::Value(ImageFormat::Png));
        assert_eq!(
            image.size,
            Presence::Value(ImageSize {
                width: 2,
                height: 3
            })
        );
        assert_eq!(image.quality, Presence::Value(ImageQuality::Low));
        assert_eq!(image.background, Presence::Null);
    }
    let mut request = ImageGenerationRequest::new("x").unwrap();
    request.count = Presence::Value(2);
    for case in 0..4 {
        let mut edited = response.clone();
        match case {
            0 => {
                request.format = Presence::Value(ImageFormat::Png);
                edited.images[1].format = Presence::Value(ImageFormat::Jpeg);
            }
            1 => {
                request.size = Presence::Value(ImageSizeRequest::Exact(ImageSize {
                    width: 2,
                    height: 3,
                }));
                edited.images[1].size = Presence::Value(ImageSize {
                    width: 3,
                    height: 2,
                });
            }
            2 => {
                request.quality = Presence::Value(ImageQualityRequest::Exact(ImageQuality::Low));
                edited.images[1].quality = Presence::Value(ImageQuality::High);
            }
            _ => {
                request.background =
                    Presence::Value(ImageBackgroundRequest::Exact(ImageBackground::Opaque));
                edited.images[1].background = Presence::Value(ImageBackground::Transparent);
            }
        }
        assert!(edited.validate_for(&request).is_err());
        assert!(images::encode_response(&edited).is_err());
    }
}

#[test]
fn heterogeneous_or_missing_reports_survive_ir_but_cannot_be_silently_projected() {
    let wire = br#"{"created":1,"data":[{"b64_json":"AQID","media_type":"image/png"},{"b64_json":"BAUG","media_type":"image/webp"}]}"#;
    let response = openrouter_images::decode_response(wire).unwrap();
    assert_eq!(
        response.images[1].format,
        Presence::Value(ImageFormat::Webp)
    );
    for policy in [
        AccountingPolicy::Strict,
        AccountingPolicy::OmitUnrepresentableAccounting,
    ] {
        let projected = project_response(&response, policy).unwrap();
        assert_eq!(projected.response, response);
        assert!(images::encode_response(&projected.response).is_err());
    }
    for case in 0..4 {
        let mut response =
            ImageGenerationResponse::new(1, vec![artifact("AQID"), artifact("BAUG")]).unwrap();
        match case {
            0 => response.images[1].format = Presence::Null,
            1 => response.images[1].size = Presence::Null,
            2 => response.images[1].background = Presence::Null,
            _ => response.images[1].quality = Presence::Null,
        }
        response.validate().unwrap();
        assert!(images::encode_response(&response).is_err());
        response.images.remove(1);
        assert_eq!(
            images::encode_response(&response).unwrap(),
            json!({"created":1,"data":[{"b64_json":"AQID"}]})
        );
    }
}

#[test]
fn bad_later_artifact_and_incomplete_envelopes_never_return_a_prefix() {
    for decode in [images::decode_response, openrouter_images::decode_response] {
        for wire in [
            br#"{"created":1,"data":[{"b64_json":"AQID"},{"b64_json":"!"}]}"#.as_slice(),
            br#"{"created":1,"data":[{"b64_json":"AQID"},{"b64_json":"BAUG"}]}x"#,
            br#"{"created":1,"data":[{"b64_json":"AQID"},{"b64_json":"BAUG"}]"#,
            br#"{"created":1,"data":[]}"#,
        ] {
            assert!(decode(wire).is_err());
        }
        let wire =
            serde_json::to_vec(&json!({"created":1,"data":vec![json!({"b64_json":"AQID"});11]}))
                .unwrap();
        assert!(decode(&wire).is_err());
    }
}
