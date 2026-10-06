//! Independent static generation controls, not provider capability inference.
use morphiecore::{
    adapter::images::{Profile, Request},
    protocol::{openai::images, openrouter_images},
    semantic::{
        context::ClientIdentityHints,
        task::image_generation::*,
        value::{ImageFormat, Presence},
    },
};
use serde_json::json;

#[test]
fn standard_image_controls_decode_into_distinct_intents_and_identity() {
    let value = json!({"model":"image-model","prompt":"synthetic","size":"1536x1024","quality":"high","background":"transparent","output_format":"webp","output_compression":0,"moderation":"low","user":"synthetic-private-user"});
    let request = images::decode_request(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(
        request.task.size,
        Presence::Value(ImageSizeRequest::Exact(ImageSize {
            width: 1536,
            height: 1024
        }))
    );
    assert_eq!(
        request.task.quality,
        Presence::Value(ImageQualityRequest::Exact(ImageQuality::High))
    );
    assert_eq!(
        request.task.background,
        Presence::Value(ImageBackgroundRequest::Exact(ImageBackground::Transparent))
    );
    assert_eq!(request.task.compression, Presence::Value(0));
    assert_eq!(
        request.task.moderation,
        Presence::Value(ImageModeration::Low)
    );
    assert_eq!(
        request.identity.user,
        Presence::Value("synthetic-private-user".into())
    );
    assert!(!format!("{request:?}").contains("synthetic-private-user"));
}

#[test]
fn typed_controls_encode_final_values_and_deletions_without_wire_dtos() {
    let mut task = ImageGenerationRequest::new("draw a circle").unwrap();
    task.format = Presence::Value(ImageFormat::Jpeg);
    task.compression = Presence::Value(85);
    task.size = Presence::Value(ImageSizeRequest::Exact(ImageSize {
        width: 1024,
        height: 1536,
    }));
    task.quality = Presence::Value(ImageQualityRequest::Auto);
    task.background = Presence::Value(ImageBackgroundRequest::Exact(ImageBackground::Opaque));
    task.moderation = Presence::Value(ImageModeration::Auto);
    let mut request = Request::new("public", task);
    request.identity = ClientIdentityHints {
        user: Presence::Value("operator-user".into()),
        ..Default::default()
    };
    assert_eq!(
        images::encode_request(&request, "native-model").unwrap(),
        json!({"model":"native-model","prompt":"draw a circle","output_format":"jpeg","output_compression":85,"size":"1024x1536","quality":"auto","background":"opaque","moderation":"auto","user":"operator-user"})
    );
    request.task.compression = Presence::Absent;
    request.task.size = Presence::Absent;
    request.identity.user = Presence::Absent;
    let edited = images::encode_request(&request, "native-model").unwrap();
    for field in ["size", "output_compression", "user"] {
        assert!(edited.get(field).is_none());
    }
    request.task.background =
        Presence::Value(ImageBackgroundRequest::Exact(ImageBackground::Transparent));
    assert!(images::encode_request(&request, "native-model").is_err());
}

#[test]
fn static_control_presence_and_automatic_selection_do_not_become_reported_facts() {
    let value = json!({"model":"m","prompt":"x","size":null,"quality":null,"background":null,"output_compression":null,"moderation":null,"output_format":null});
    let request = images::decode_request(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(images::encode_request(&request, "m").unwrap(), value);
    let automatic =
        json!({"model":"m","prompt":"x","size":"auto","quality":"auto","background":"auto"});
    let request = images::decode_request(&serde_json::to_vec(&automatic).unwrap()).unwrap();
    assert_eq!(request.task.size, Presence::Value(ImageSizeRequest::Auto));
    assert_eq!(images::encode_request(&request, "m").unwrap(), automatic);
    let response = ImageGenerationResponse::new(
        1,
        vec![GeneratedImage::new(ImageData::new("AQID").unwrap())],
    )
    .unwrap();
    response.validate_for(&request.task).unwrap();
    assert!(response.images[0].size.is_absent());
    assert!(response.images[0].quality.is_absent());
    assert!(response.images[0].background.is_absent());
}

#[test]
fn all_quality_tiers_and_compression_edges_have_independent_expectations() {
    for (label, quality) in [
        ("low", ImageQuality::Low),
        ("medium", ImageQuality::Medium),
        ("high", ImageQuality::High),
        ("xhigh", ImageQuality::Xhigh),
        ("max", ImageQuality::Max),
    ] {
        let wire = json!({"model":"m","prompt":"x","quality":label});
        let request = images::decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
        assert_eq!(
            request.task.quality,
            Presence::Value(ImageQualityRequest::Exact(quality))
        );
        let mut typed = Request::new("m", ImageGenerationRequest::new("x").unwrap());
        typed.task.quality = Presence::Value(ImageQualityRequest::Exact(quality));
        assert_eq!(images::encode_request(&typed, "m").unwrap(), wire);
    }
    for format in ["jpeg", "webp"] {
        for compression in [0, 100] {
            let value = json!({"model":"m","prompt":"x","output_format":format,"output_compression":compression,"user":""});
            let request = images::decode_request(&serde_json::to_vec(&value).unwrap()).unwrap();
            assert_eq!(request.task.compression, Presence::Value(compression as u8));
            assert_eq!(images::encode_request(&request, "m").unwrap(), value);
        }
    }
}
#[test]
fn bad_control_combinations_are_rejected_without_dropping_intent() {
    for patch in [
        json!({"size":"0x1024"}),
        json!({"size":"01024x1024"}),
        json!({"size":"65537x1"}),
        json!({"size":"1K"}),
        json!({"quality":"hd"}),
        json!({"background":"checkerboard"}),
        json!({"moderation":"none"}),
        json!({"user":null}),
        json!({"output_compression":50}),
        json!({"output_format":"png","output_compression":0}),
        json!({"output_format":"jpeg","background":"transparent"}),
        json!({"output_format":"webp","output_compression":101}),
        json!({"output_format":"webp","output_compression":-1}),
        json!({"output_format":"webp","output_compression":1.5}),
        json!({"output_format":"webp","output_compression":true}),
        json!({"user":"x".repeat(257)}),
        json!({"response_format":"url"}),
        json!({"style":"natural"}),
    ] {
        let mut value = json!({"model":"m","prompt":"x"});
        value
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(
            images::decode_request(&serde_json::to_vec(&value).unwrap()).is_err(),
            "{patch}"
        );
    }
    let mut request = Request::new("m", ImageGenerationRequest::new("x").unwrap());
    request.identity.safety_identifier = Presence::Value("not-an-images-field".into());
    assert!(images::encode_request(&request, "m").is_err());
}

#[test]
fn default_encoding_is_a_target_constraint_not_a_shared_ir_default() {
    let mut task = ImageGenerationRequest::new("synthetic").unwrap();
    task.compression = Presence::Value(50);
    task.validate().unwrap();
    let request = Request::new("m", task);
    // The standard profile defaults to PNG; an unspecified encoding cannot carry this control.
    assert!(images::encode_request(&request, "m").is_err());
}
#[test]
fn output_reports_must_not_contradict_explicit_controls() {
    let mut request = ImageGenerationRequest::new("x").unwrap();
    request.size = Presence::Value(ImageSizeRequest::Exact(ImageSize {
        width: 1024,
        height: 1024,
    }));
    request.quality = Presence::Value(ImageQualityRequest::Exact(ImageQuality::High));
    request.background =
        Presence::Value(ImageBackgroundRequest::Exact(ImageBackground::Transparent));
    let response = ImageGenerationResponse::new(
        1,
        vec![GeneratedImage::new(ImageData::new("AQID").unwrap())],
    )
    .unwrap();
    response.validate_for(&request).unwrap();
    for case in 0..4 {
        let mut response = response.clone();
        match case {
            0 => {
                response.images[0].size = Presence::Value(ImageSize {
                    width: 512,
                    height: 512,
                })
            }
            1 => response.images[0].quality = Presence::Value(ImageQuality::Low),
            2 => response.images[0].background = Presence::Value(ImageBackground::Opaque),
            _ => response.images[0].format = Presence::Value(ImageFormat::Jpeg),
        };
        assert!(response.validate_for(&request).is_err());
    }
    let mut impossible = response;
    impossible.images[0].format = Presence::Value(ImageFormat::Jpeg);
    impossible.images[0].background = Presence::Value(ImageBackground::Transparent);
    assert!(images::encode_response(&impossible).is_err());
}

#[test]
fn openrouter_maps_only_declared_controls_with_fixed_provider_options() {
    let wire = json!({"model":"gpt-image-2.5-flare","prompt":"square","quality":"high","background":"transparent","moderation":"low","user":"synthetic-user"});
    let mut request = images::decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
    assert_eq!(
        openrouter_images::encode_request(&request, "openai/gpt-image-2.5-flare").unwrap(),
        json!({"model":"openai/gpt-image-2.5-flare","prompt":"square","n":1,"stream":false,"quality":"high","background":"transparent","user":"synthetic-user","provider":{"only":["openai"],"allow_fallbacks":false,"options":{"openai":{"moderation":"low"}}}})
    );
    request.task.size = Presence::Value(ImageSizeRequest::Exact(ImageSize {
        width: 1024,
        height: 1024,
    }));
    assert!(
        Profile::OpenRouterFlare
            .encode_request(&request, "openai/gpt-image-2.5-flare")
            .is_err()
    );
    request.task.size = Presence::Absent;
    request.task.format = Presence::Value(ImageFormat::Webp);
    request.task.compression = Presence::Value(80);
    assert!(
        Profile::OpenRouterFlare
            .encode_request(&request, "openai/gpt-image-2.5-flare")
            .is_err()
    );
}
