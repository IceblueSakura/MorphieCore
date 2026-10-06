//! Image-response byte ceilings do not enlarge conversation JSON budgets.
use base64::Engine;
use morphiecore::{
    protocol::{CodecError, openai::images, openrouter_images},
    semantic::{
        task::image_generation::*,
        value::{JsonError, JsonLimits, parse_json},
    },
};
use serde_json::json;

#[test]
fn image_response_above_conversation_ceiling_has_a_separate_bounded_codec() {
    let encoded = base64::engine::general_purpose::STANDARD.encode(vec![0; 3 << 20]);
    let wire = serde_json::to_vec(&json!({"created":1,"data":[{"b64_json":encoded}]})).unwrap();
    assert!(wire.len() > JsonLimits::ENVELOPE.bytes);
    assert_eq!(
        parse_json(
            &wire,
            JsonLimits {
                bytes: 16 << 20,
                ..JsonLimits::ENVELOPE
            }
        )
        .unwrap_err(),
        JsonError::Limit
    );
    for response in [
        images::decode_response(&wire).unwrap(),
        openrouter_images::decode_response(&wire).unwrap(),
    ] {
        assert_eq!(response.images[0].data.decoded_bytes(), 3 << 20);
    }
}
#[test]
fn image_json_hard_limit_counts_whitespace_and_does_not_relax_syntax() {
    let mut wire = br#"{"created":1,"data":[{"b64_json":"AQID"}]}"#.to_vec();
    wire.resize(JsonLimits::IMAGE_RESPONSE.bytes, b' ');
    assert!(images::decode_response(&wire).is_ok());
    wire.push(b' ');
    assert!(matches!(
        images::decode_response(&wire),
        Err(CodecError::Limit)
    ));
    assert!(
        images::decode_response(br#"{"created":1,"data":[{"b64_json":"AQID"}],"created":2}"#)
            .is_err()
    );
}
#[test]
fn decoded_image_budget_is_known_without_pixel_decoding() {
    let image = ImageData::new("AQID").unwrap();
    assert_eq!(image.decoded_bytes(), 3);
    assert_eq!(MAX_IMAGE_BYTES, 8 << 20);
}

#[test]
fn collection_budget_is_fixed_and_inclusive_before_and_after_typed_edits() {
    let encoded = base64::engine::general_purpose::STANDARD.encode(vec![0; MAX_IMAGES_BYTES / 2]);
    let image = GeneratedImage::new(ImageData::new(&encoded).unwrap());
    let mut response = ImageGenerationResponse::new(1, vec![image.clone(), image]).unwrap();
    assert_eq!(response.decoded_bytes().unwrap(), MAX_IMAGES_BYTES);
    response
        .images
        .push(GeneratedImage::new(ImageData::new("AA==").unwrap()));
    assert!(response.validate().is_err());
    assert!(images::encode_response(&response).is_err());
    for extra in [false, true] {
        let mut data = vec![json!({"b64_json":encoded}), json!({"b64_json":encoded})];
        if extra {
            data.push(json!({"b64_json":"AA=="}));
        }
        let wire = serde_json::to_vec(&json!({"created":1,"data":data})).unwrap();
        assert!(wire.len() < JsonLimits::IMAGE_RESPONSE.bytes);
        for decode in [images::decode_response, openrouter_images::decode_response] {
            assert_eq!(decode(&wire).is_ok(), !extra);
        }
    }
}
