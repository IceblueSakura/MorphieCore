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
fn auto_uses_native_default_without_losing_images_or_mutating_source() {
    use morphiecore::{
        semantic::context::ConversationContext,
        topology::{GenerationProtocol, catalog},
    };
    for tool in [false, true] {
        let decode = |detailed| {
            let mut image = json!({"type":"input_image","image_url":"data:image/png;base64,AQID"});
            if detailed {
                image["detail"] = json!("auto");
            }
            let input = if tool {
                json!([
                    {"role":"user","content":"Read."},
                    {"type":"function_call","call_id":"c","name":"read","arguments":"{}"},
                    {"type":"function_call_output","call_id":"c","output":[image]}
                ])
            } else {
                json!([{"role":"user","content":[image]}])
            };
            let mut request = adapter(GenerationApi::Responses)
                .decode_client_request(
                    json!({"model":"synthetic","max_output_tokens":1024,
                    "input":input,"tools":[{"type":"function","name":"read","strict":false,
                    "parameters":{"type":"object"}}]})
                    .to_string()
                    .as_bytes(),
                )
                .unwrap();
            request.conversation = Some(ConversationContext::conversation("synthetic").unwrap());
            request
        };
        let request = decode(true);
        let before = request.clone();
        let mut native = vec![
            catalog::MESSAGES_BINDINGS[0].endpoint(),
            catalog::GOOGLE_BINDINGS[0].interactions_endpoint(),
        ];
        if !tool {
            native.push(catalog::GOOGLE_BINDINGS[0].endpoint());
        }
        for endpoint in native {
            let (projected, rules) = endpoint.encode_request_with_rules(&request).unwrap();
            assert_eq!(projected, endpoint.encode_request(&decode(false)).unwrap());
            assert!(rules.is_empty(), "model-default selection is lossless");
            match endpoint.protocol {
                GenerationProtocol::AnthropicMessages => {
                    let image = if tool {
                        &projected["messages"][2]["content"][0]["content"][0]
                    } else {
                        &projected["messages"][0]["content"][0]
                    };
                    assert_eq!(image["type"], "image");
                    assert_eq!(
                        image["source"],
                        json!({
                            "type":"base64","media_type":"image/png","data":"AQID"
                        })
                    );
                }
                GenerationProtocol::GoogleGenerateContent => {
                    assert_eq!(
                        projected["contents"][0]["parts"][0]["inlineData"],
                        json!({"mimeType":"image/png","data":"AQID"})
                    );
                }
                GenerationProtocol::GoogleInteractions => {
                    let image = if tool {
                        &projected["input"][2]["result"][0]
                    } else {
                        &projected["input"][0]["content"][0]
                    };
                    assert_eq!(
                        image,
                        &json!({
                            "type":"image","data":"AQID","mime_type":"image/png"
                        })
                    );
                }
                _ => unreachable!(),
            }
            assert_eq!(request, before);
        }
        let (carried, rules) = adapter(GenerationApi::Responses)
            .encode_request_with_rules(&request, "synthetic", &TargetContract::full())
            .unwrap();
        let image = if tool {
            &carried["input"][2]["output"][0]
        } else {
            &carried["input"][0]["content"][0]
        };
        assert_eq!(image["detail"], "auto");
        assert!(rules.is_empty());
    }
}

#[test]
fn image_admission_keeps_bounds_and_named_non_auto_omissions() {
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
    let (projected, rules) = xiaomi
        .encode_request_with_rules(&tool, "synthetic", &TargetContract::full())
        .unwrap();
    assert_eq!(
        projected["input"][1]["output"][0]["image_url"],
        "data:image/png;base64,AQID"
    );
    assert!(projected["input"][1]["output"][0].get("detail").is_none());
    assert!(rules.is_empty());
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
