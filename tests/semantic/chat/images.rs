//! Independent image admission and projection oracles; no network or media decoder.
use morphiecore::protocol::openai::{Dialect, GenerationApi, GenerationCodec, TargetContract};
use serde_json::{Value, json};

fn adapter(profile: GenerationApi) -> GenerationCodec {
    GenerationCodec::new(profile, Dialect::Standard, None)
}
fn wire(parts: Value) -> Value {
    json!({"model":"synthetic","input":[{"role":"user","content":parts}]})
}

#[test]
fn lowering_rejects_disabled_images_and_undeclared_detail_without_mutating_ir() {
    let request = adapter(GenerationApi::Responses)
        .decode_request(
            &serde_json::to_vec(&wire(json!([
                {"type":"input_text","text":"before"},
                {"type":"input_image","image_url":"https://example.test/a.png","detail":"low"},
                {"type":"input_text","text":"between"},
                {"type":"input_image","image_url":"data:image/png;base64,AQID"}
            ])))
            .unwrap(),
        )
        .unwrap();
    let before = request.clone();
    let mut contract = TargetContract::full();
    contract.dispositions.set_native(
        morphiecore::semantic::task::generation::Feature::ImageInput,
        false,
    );
    assert!(
        adapter(GenerationApi::Responses)
            .encode_request(&request, "synthetic", &contract)
            .is_err()
    );
    let xiaomi = GenerationCodec::new(GenerationApi::Responses, Dialect::Xiaomi, None);
    let (omitted, rules) = xiaomi
        .encode_request_with_rules(&request, "synthetic", &TargetContract::full())
        .unwrap();
    assert!(omitted["input"][0]["content"][1].get("detail").is_none());
    assert!(rules.contains(&"generation.omit-image-detail.v1"));
    assert_eq!(
        adapter(GenerationApi::Responses)
            .encode_request(&request, "synthetic", &TargetContract::full())
            .unwrap()["input"][0]["content"][1]["detail"],
        "low"
    );
    assert_eq!(request, before);
    let tool = adapter(GenerationApi::Responses).decode_request(
            json!({"model":"synthetic","input":[
                {"type":"function_call","call_id":"c","name":"read","arguments":"{}"},
                {"type":"function_call_output","call_id":"c","output":[
                    {"type":"input_image","image_url":"data:image/png;base64,AQID","detail":"auto"}]}]})
                .to_string().as_bytes(),
        ).unwrap();
    let (omitted, _) = xiaomi
        .encode_request_with_rules(&tool, "synthetic", &TargetContract::full())
        .unwrap();
    assert!(omitted["input"][1]["output"][0].get("detail").is_none());
    let original = wire(
        json!([{"type":"input_image","image_url":"https://example.test/x","detail":"original"}]),
    );
    let request = adapter(GenerationApi::Responses)
        .decode_request(&serde_json::to_vec(&original).unwrap())
        .unwrap();
    let (omitted, rules) = adapter(GenerationApi::Chat)
        .encode_request_with_rules(&request, "synthetic", &TargetContract::full())
        .unwrap();
    assert!(
        omitted["messages"][0]["content"][0]["image_url"]
            .get("detail")
            .is_none()
    );
    assert!(rules.contains(&"generation.omit-image-detail.v1"));
    assert!(
        GenerationCodec::new(GenerationApi::Chat, Dialect::DeepSeek, None)
            .encode_request(&request, "synthetic", &TargetContract::full())
            .is_ok()
    );
    let bmp = wire(json!([{"type":"input_image","image_url":"data:image/bmp;base64,AQID"}]));
    let request = GenerationCodec::new(GenerationApi::Responses, Dialect::MorphieCore, None)
        .decode_request(&serde_json::to_vec(&bmp).unwrap())
        .unwrap();
    for profile in [GenerationApi::Chat, GenerationApi::Responses] {
        assert!(
            adapter(profile)
                .encode_request(&request, "synthetic", &TargetContract::full())
                .is_err()
        );
        assert!(
            GenerationCodec::new(profile, Dialect::DeepSeek, None)
                .encode_request(&request, "synthetic", &TargetContract::full())
                .is_err()
        );
        assert!(
            GenerationCodec::new(profile, Dialect::Xiaomi, None)
                .encode_request(&request, "synthetic", &TargetContract::full())
                .is_ok()
        );
    }
}
