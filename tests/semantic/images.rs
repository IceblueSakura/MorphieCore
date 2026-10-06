//! Independent image admission and projection oracles; no network or media decoder.
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract,
    protocol::openai::Profile,
    semantic::task::generation::*,
};
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
fn image_sources_detail_and_order_decode_then_project_independent_wire() {
    for source in [Profile::Chat, Profile::Responses] {
        let request = adapter(source)
            .decode_request(&serde_json::to_vec(&wire(source, parts(source))).unwrap())
            .unwrap();
        let Item::Message(message) = &request.task.semantic.items()[0].1 else {
            panic!()
        };
        assert_eq!(message.parts.len(), 4);
        assert!(matches!(&message.parts[0].content, ContentPart::Text(t) if t.as_str()=="before"));
        assert!(
            matches!(&message.parts[1].content, ContentPart::Resource(r) if r.kind()==ResourceKind::Image && matches!(&r.location,ResourceLocation::Url(url) if url.as_str()=="https://example.test/a.png"))
        );
        assert!(
            matches!(&message.parts[3].content, ContentPart::Resource(r) if matches!(&r.location,ResourceLocation::Inline{media_type,data_base64} if media_type.as_str()=="image/png" && data_base64.as_str()=="AQID"))
        );
        assert_eq!(
            GenerationRequirements::derive(&request.task.semantic).image_inputs,
            2
        );
        for target in [Profile::Chat, Profile::Responses] {
            let encoded = adapter(target)
                .encode_request(
                    &request,
                    "synthetic",
                    &GenerationRepresentationContract::full(),
                )
                .unwrap();
            let field = if target == Profile::Chat {
                "messages"
            } else {
                "input"
            };
            assert_eq!(encoded[field][0]["content"], parts(target));
        }
    }
}

#[test]
fn edits_delete_replace_insert_and_reorder_images_without_source_resurrection() {
    let mut request = adapter(Profile::Responses)
        .decode_request(
            &serde_json::to_vec(&wire(Profile::Responses, parts(Profile::Responses))).unwrap(),
        )
        .unwrap();
    let mut items = request.task.semantic.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        panic!()
    };
    let ContentPart::Resource(resource) = &mut message.parts[1].content else {
        panic!()
    };
    resource.location = ResourceLocation::Url(
        morphiecore::semantic::value::Text::new(
            "https://example.test/replaced.png",
            "synthetic",
            8192,
        )
        .unwrap(),
    );
    resource.description = ResourceDescription::Image {
        detail: Some(ImageDetail::High),
    };
    let inserted = Part {
        id: PartId::new(100),
        content: ContentPart::Resource(Resource {
            location: ResourceLocation::Url(
                morphiecore::semantic::value::Text::new(
                    "https://example.test/new.png",
                    "synthetic",
                    8192,
                )
                .unwrap(),
            ),
            description: ResourceDescription::Image {
                detail: Some(ImageDetail::Auto),
            },
        }),
    };
    message.parts.remove(3);
    message.parts.insert(0, inserted);
    message.parts.swap(0, 2);
    request.task.semantic = request.task.semantic.clone().with_items(items).unwrap();
    for target in [Profile::Chat, Profile::Responses] {
        let encoded = adapter(target)
            .encode_request(
                &request,
                "synthetic",
                &GenerationRepresentationContract::full(),
            )
            .unwrap();
        let field = if target == Profile::Chat {
            "messages"
        } else {
            "input"
        };
        let expected = if target == Profile::Chat {
            json!([
                {"type":"image_url","image_url":{"url":"https://example.test/replaced.png","detail":"high"}},
                {"type":"text","text":"before"},
                {"type":"image_url","image_url":{"url":"https://example.test/new.png","detail":"auto"}},
                {"type":"text","text":"between"}
            ])
        } else {
            json!([
                {"type":"input_image","image_url":"https://example.test/replaced.png","detail":"high"},
                {"type":"input_text","text":"before"},
                {"type":"input_image","image_url":"https://example.test/new.png","detail":"auto"},
                {"type":"input_text","text":"between"}
            ])
        };
        assert_eq!(encoded[field][0]["content"], expected);
    }
    let mut items = request.task.semantic.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        panic!()
    };
    message
        .parts
        .retain(|p| matches!(p.content, ContentPart::Text(_)));
    request.task.semantic = request.task.semantic.clone().with_items(items).unwrap();
    assert_eq!(
        GenerationRequirements::derive(&request.task.semantic).image_inputs,
        0
    );
    let mut contract = GenerationRepresentationContract::full();
    contract.semantics.image_input = false;
    assert!(
        adapter(Profile::Chat)
            .encode_request(&request, "synthetic", &contract)
            .is_ok()
    );
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
fn typed_images_are_bounded_after_transforms_and_cannot_become_output() {
    use morphiecore::semantic::value::Text;
    let resource = Resource {
        location: ResourceLocation::Inline {
            media_type: Text::new("image/png", "synthetic", 64).unwrap(),
            data_base64: Text::new("A".repeat(MAX_TEXT_BYTES + 4), "synthetic", MAX_TOTAL_BYTES)
                .unwrap(),
        },
        description: ResourceDescription::Image { detail: None },
    };
    assert_eq!(resource.validate(), Err(GenerationError::Limit));
    let part = Part {
        id: PartId::new(0),
        content: ContentPart::Resource(resource),
    };
    let message = Message {
        role: MessageRole::User,
        parts: vec![part],
        status: ItemLifecycle::Completed,
        phase: None,
    };
    assert!(
        GenerationRequest::new(
            vec![(ItemId::new(0), Item::Message(message))],
            Default::default()
        )
        .is_err()
    );
    let decoded = adapter(Profile::Responses)
        .decode_request(
            &serde_json::to_vec(&wire(Profile::Responses, parts(Profile::Responses))).unwrap(),
        )
        .unwrap();
    let mut items = decoded.task.semantic.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        panic!()
    };
    message.parts = (0..5)
        .map(|i| Part {
            id: PartId::new(i),
            content: ContentPart::Resource(Resource {
                location: ResourceLocation::Inline {
                    media_type: Text::new("image/png", "synthetic", 64).unwrap(),
                    data_base64: Text::new("AAAA".repeat(220_000), "synthetic", MAX_TEXT_BYTES)
                        .unwrap(),
                },
                description: ResourceDescription::Image { detail: None },
            }),
        })
        .collect();
    assert!(decoded.task.semantic.clone().with_items(items).is_err());
    let mut items = decoded.task.semantic.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        panic!()
    };
    message.role = MessageRole::Assistant;
    assert!(decoded.task.semantic.clone().with_items(items).is_err());
    assert!(format!("{:?}", decoded.task.semantic).contains("Inline([redacted])"));
}

#[test]
fn lowering_rejects_disabled_images_and_undeclared_detail_without_mutating_ir() {
    let request = adapter(Profile::Responses)
        .decode_request(
            &serde_json::to_vec(&wire(Profile::Responses, parts(Profile::Responses))).unwrap(),
        )
        .unwrap();
    let before = request.clone();
    let mut contract = GenerationRepresentationContract::full();
    contract.semantics.image_input = false;
    assert!(
        adapter(Profile::Responses)
            .encode_request(&request, "synthetic", &contract)
            .is_err()
    );
    let xiaomi = Adapter::new(Profile::Responses, Dialect::Xiaomi, None);
    assert!(
        xiaomi
            .encode_request(
                &request,
                "synthetic",
                &GenerationRepresentationContract::full()
            )
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
            .encode_request(
                &request,
                "synthetic",
                &GenerationRepresentationContract::full()
            )
            .is_err()
    );
    assert!(
        Adapter::new(Profile::Chat, Dialect::DeepSeek, None)
            .encode_request(
                &request,
                "synthetic",
                &GenerationRepresentationContract::full()
            )
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
                .encode_request(
                    &request,
                    "synthetic",
                    &GenerationRepresentationContract::full()
                )
                .is_err()
        );
        assert!(
            Adapter::new(profile, Dialect::DeepSeek, None)
                .encode_request(
                    &request,
                    "synthetic",
                    &GenerationRepresentationContract::full()
                )
                .is_err()
        );
        assert!(
            Adapter::new(profile, Dialect::Xiaomi, None)
                .encode_request(
                    &request,
                    "synthetic",
                    &GenerationRepresentationContract::full()
                )
                .is_ok()
        );
    }
}
