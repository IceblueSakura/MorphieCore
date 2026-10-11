//! Independent static generation controls, not provider capability inference.
use morphiecore::{
    adapter::images::Profile,
    protocol::{openai::images, openrouter_images},
    semantic::{
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
