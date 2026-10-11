//! Independent image admission and projection oracles; no network or media decoder.
use morphiecore::protocol::openai::{Adapter, Dialect, Profile, TargetCaps};
use serde_json::{Value, json};

fn adapter(profile: Profile) -> Adapter {
    Adapter::new(profile, Dialect::Standard, None)
}
fn wire(profile: Profile, parts: Value) -> Value {
    match profile {
        Profile::Chat => json!({"model":"synthetic","messages":[{"role":"user","content":parts}]}),
        Profile::Responses => {
            json!({"model":"synthetic","input":[{"role":"user","content":parts}]})
        }
    }
}
fn parts(profile: Profile) -> Value {
    match profile {
        Profile::Chat => json!([
            {"type":"text","text":"before"},
            {"type":"image_url","image_url":{"url":"https://example.test/a.png","detail":"low"}},
            {"type":"text","text":"between"},
            {"type":"image_url","image_url":{"url":"data:image/png;base64,AQID"}}
        ]),
        Profile::Responses => json!([
            {"type":"input_text","text":"before"},
            {"type":"input_image","image_url":"https://example.test/a.png","detail":"low"},
            {"type":"input_text","text":"between"},
            {"type":"input_image","image_url":"data:image/png;base64,AQID"}
        ]),
    }
}

#[test]
fn invalid_image_sources_shells_details_and_placements_fail_closed() {
    for profile in [Profile::Chat, Profile::Responses] {
        for url in [
            "",
            "file:///tmp/private.png",
            "https://user:secret@example.test/x",
            "https://@example.test/x",
            "https://example.test\\\\@other.test/x",
            "https://example.test/a\n",
            "data:text/plain;base64,AQID",
            "data:image/png;base64,",
            "data:image/png;base64,%%%%",
            "data:image/png;base64,AB==",
            "data:image/png,AQID",
        ] {
            let image = if profile == Profile::Chat {
                json!({"type":"image_url","image_url":{"url":url}})
            } else {
                json!({"type":"input_image","image_url":url})
            };
            assert!(
                adapter(profile)
                    .decode_request(&serde_json::to_vec(&wire(profile, json!([image]))).unwrap())
                    .is_err(),
                "{profile:?} source case"
            );
        }
        for detail in [Value::Null, json!(false), json!("unknown")] {
            let image = if profile == Profile::Chat {
                json!({"type":"image_url","image_url":{"url":"https://example.test/x","detail":detail}})
            } else {
                json!({"type":"input_image","image_url":"https://example.test/x","detail":detail})
            };
            assert!(
                adapter(profile)
                    .decode_request(&serde_json::to_vec(&wire(profile, json!([image]))).unwrap())
                    .is_err()
            );
        }
        for role in ["assistant", "system", "developer"] {
            let mut request = wire(profile, parts(profile));
            let field = if profile == Profile::Chat {
                "messages"
            } else {
                "input"
            };
            request[field][0]["role"] = json!(role);
            assert!(
                adapter(profile)
                    .decode_request(&serde_json::to_vec(&request).unwrap())
                    .is_err()
            );
        }
    }
    for image in [
        json!({"type":"input_image","file_id":"file-synthetic"}),
        json!({"type":"input_image"}),
        json!({"type":"input_image","image_url":"https://example.test/x","file_id":"file-synthetic"}),
    ] {
        assert!(
            adapter(Profile::Responses)
                .decode_request(
                    &serde_json::to_vec(&wire(Profile::Responses, json!([image]))).unwrap()
                )
                .is_err()
        );
    }
}

#[test]
fn lowering_rejects_disabled_images_and_undeclared_detail_without_mutating_ir() {
    let request = adapter(Profile::Responses)
        .decode_request(
            &serde_json::to_vec(&wire(Profile::Responses, parts(Profile::Responses))).unwrap(),
        )
        .unwrap();
    let before = request.clone();
    let mut contract = TargetCaps::full();
    contract.semantics.image_input = false;
    assert!(
        adapter(Profile::Responses)
            .encode_request(&request, "synthetic", &contract)
            .is_err()
    );
    let xiaomi = Adapter::new(Profile::Responses, Dialect::Xiaomi, None);
    assert!(
        xiaomi
            .encode_request(&request, "synthetic", &TargetCaps::full())
            .is_err()
    );
    assert_eq!(request, before);
    let original = wire(
        Profile::Responses,
        json!([{"type":"input_image","image_url":"https://example.test/x","detail":"original"}]),
    );
    let request = adapter(Profile::Responses)
        .decode_request(&serde_json::to_vec(&original).unwrap())
        .unwrap();
    assert!(
        adapter(Profile::Chat)
            .encode_request(&request, "synthetic", &TargetCaps::full())
            .is_err()
    );
    assert!(
        Adapter::new(Profile::Chat, Dialect::DeepSeek, None)
            .encode_request(&request, "synthetic", &TargetCaps::full())
            .is_ok()
    );
    let bmp = wire(
        Profile::Responses,
        json!([{"type":"input_image","image_url":"data:image/bmp;base64,AQID"}]),
    );
    let request = Adapter::new(Profile::Responses, Dialect::MorphieCore, None)
        .decode_request(&serde_json::to_vec(&bmp).unwrap())
        .unwrap();
    for profile in [Profile::Chat, Profile::Responses] {
        assert!(
            adapter(profile)
                .encode_request(&request, "synthetic", &TargetCaps::full())
                .is_err()
        );
        assert!(
            Adapter::new(profile, Dialect::DeepSeek, None)
                .encode_request(&request, "synthetic", &TargetCaps::full())
                .is_err()
        );
        assert!(
            Adapter::new(profile, Dialect::Xiaomi, None)
                .encode_request(&request, "synthetic", &TargetCaps::full())
                .is_ok()
        );
    }
}
