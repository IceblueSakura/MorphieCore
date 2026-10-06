//! Independent inline-file wire oracles; no file parser, credentials or network.
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract as Contract,
    protocol::openai::Profile,
    semantic::task::generation::*,
};
use serde_json::{Value, json};

fn adapter() -> Adapter {
    Adapter::new(Profile::Responses, Dialect::Standard, None)
}
fn wire(parts: Value) -> Value {
    json!({"model":"synthetic","input":[{"role":"user","content":parts}]})
}
fn file() -> Value {
    json!({"type":"input_file","file_data":"data:application/pdf;base64,AQID","filename":"synthetic.pdf","detail":"low"})
}
fn decode(
    parts: Value,
) -> Result<morphiecore::adapter::Request, morphiecore::protocol::CodecError> {
    adapter().decode_request(&serde_json::to_vec(&wire(parts)).unwrap())
}
#[test]
fn inline_file_is_admitted_and_preserves_mixed_part_order() {
    let parts = json!([
        {"type":"input_text","text":"before"},
        file(),
        {"type":"input_image","image_url":"data:image/png;base64,AQID"},
        {"type":"input_text","text":"after"}
    ]);
    let request = decode(parts.clone()).unwrap();
    let Item::Message(message) = &request.task.semantic.items()[0].1 else {
        panic!("message")
    };
    let ContentPart::Resource(resource) = &message.parts[1].content else {
        panic!("file")
    };
    assert_eq!(resource.kind(), ResourceKind::File);
    assert_eq!(
        resource.description,
        ResourceDescription::File(FileDescription {
            filename: Some(text("synthetic.pdf")),
            detail: Some(FileDetail::Low),
        })
    );
    assert_eq!(
        resource.location,
        ResourceLocation::Inline {
            media_type: text("application/pdf"),
            data_base64: text("AQID"),
        }
    );
    let q = GenerationRequirements::derive(&request.task.semantic);
    assert_eq!(q.file_inputs, 1);
    assert_eq!(q.image_inputs, 1);
    assert_eq!(q.resource_count, 2);
    let encoded = adapter()
        .encode_request(&request, "synthetic", &Contract::full())
        .unwrap();
    assert_eq!(encoded["input"][0]["content"], parts);
}
#[test]
fn file_description_presence_has_independent_wire_expectations() {
    for filename in [None, Some(""), Some("synthetic.pdf")] {
        for detail in [None, Some("auto"), Some("low"), Some("high")] {
            let mut part =
                json!({"type":"input_file","file_data":"data:application/pdf;base64,AQID"});
            if let Some(name) = filename {
                part["filename"] = json!(name);
            }
            if let Some(detail) = detail {
                part["detail"] = json!(detail);
            }
            let request = decode(json!([part.clone()])).unwrap();
            let encoded = adapter()
                .encode_request(&request, "synthetic", &Contract::full())
                .unwrap();
            assert_eq!(encoded["input"][0]["content"], json!([part]));
        }
    }
}
#[test]
fn file_url_has_independent_wire_and_typed_oracles() {
    let raw = "https://example.invalid/a%2Fb?signature=synthetic-private&x=1#page=2";
    let request =
        decode(json!([{"type":"input_file","file_url":raw,"filename":"","detail":"high"}]))
            .unwrap();
    let Item::Message(message) = &request.task.semantic.items()[0].1 else {
        panic!()
    };
    assert!(matches!(&message.parts[0].content, ContentPart::Resource(r)
        if r.location == ResourceLocation::Url(text(raw)) && r.description == ResourceDescription::File(FileDescription { filename:Some(text("")), detail:Some(FileDetail::High) })));
    assert!(!format!("{request:?}").contains("synthetic-private"));
    let mut resource = typed_file(None);
    resource.location = ResourceLocation::Url(text(raw));
    let typed = typed_request(vec![resource]).unwrap();
    let encoded = encode_typed(&typed, Contract::full()).unwrap();
    assert_eq!(
        encoded["input"][0]["content"],
        json!([{"type":"input_file","file_url":raw}])
    );
    assert!(
        adapter()
            .encode_request(&request, "synthetic", &Contract::full())
            .is_ok()
    );
}

#[test]
fn malformed_or_unadmitted_file_sources_fail_closed() {
    for part in [
        json!({"type":"input_file"}),
        json!({"type":"input_file","file_url":null}),
        json!({"type":"input_file","file_url":""}),
        json!({"type":"input_file","file_url":42}),
        json!({"type":"input_file","file_url":"data:application/pdf;base64,AQID"}),
        json!({"type":"input_file","file_id":"file-synthetic"}),
        json!({"type":"input_file","file_id":null}),
        json!({"type":"input_file","file_data":"AQID"}),
        json!({"type":"input_file","file_data":null}),
        json!({"type":"input_file","file_data":42}),
        json!({"type":"input_file","file_data":"data:application/pdf;base64,"}),
        json!({"type":"input_file","file_data":"data:application/pdf;base64,%%%%"}),
        json!({"type":"input_file","file_data":"data:application/pdf;base64,AB=="}),
        json!({"type":"input_file","file_data":"data:application/pdf,AQID"}),
        json!({"type":"input_file","file_data":"data:;base64,AQID"}),
        json!({"type":"input_file","file_data":"data:bad type;base64,AQID"}),
    ] {
        assert!(decode(json!([part])).is_err());
    }
    for (key, value) in [
        ("filename", Value::Null),
        ("filename", json!(42)),
        ("detail", Value::Null),
        ("detail", json!("original")),
        ("detail", json!(false)),
        ("file_id", json!("file-synthetic")),
        ("file_id", Value::Null),
        ("file_url", json!("https://example.test/file.pdf")),
        ("unknown", json!(true)),
        ("prompt_cache_breakpoint", json!({"mode":"explicit"})),
    ] {
        let mut part = file();
        part[key] = value;
        assert!(decode(json!([part])).is_err(), "{key}");
    }
}
#[test]
fn file_model_admission_and_chat_rejection_do_not_mutate_input() {
    let request = decode(json!([file()])).unwrap();
    let before = request.clone();
    let mut contract = Contract::full();
    contract.semantics.file_input = false;
    assert!(
        adapter()
            .encode_request(&request, "synthetic", &contract)
            .is_err()
    );
    assert!(
        Adapter::new(Profile::Chat, Dialect::Standard, None)
            .encode_request(&request, "synthetic", &Contract::full())
            .is_err()
    );
    assert_eq!(request, before);
}

fn text(value: &str) -> morphiecore::semantic::value::Text {
    morphiecore::semantic::value::Text::allowing_empty(value, "synthetic", MAX_TOTAL_BYTES).unwrap()
}
fn typed_file(name: Option<&str>) -> Resource {
    Resource {
        location: ResourceLocation::Inline {
            media_type: text("application/pdf"),
            data_base64: text("AQID"),
        },
        description: ResourceDescription::File(FileDescription {
            filename: name.map(text),
            detail: None,
        }),
    }
}
fn typed_request(resources: Vec<Resource>) -> Result<GenerationRequest, GenerationError> {
    GenerationRequest::new(
        vec![(
            ItemId::new(1),
            Item::Message(Message {
                role: MessageRole::User,
                parts: resources
                    .into_iter()
                    .enumerate()
                    .map(|(i, resource)| Part {
                        id: PartId::new(i as u64 + 1),
                        content: ContentPart::Resource(resource),
                    })
                    .collect(),
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        )],
        GenerationControls::default(),
    )
}
fn encode_typed(
    request: &GenerationRequest,
    contract: Contract,
) -> Result<Value, morphiecore::lowering::generation::RepresentationError> {
    let fidelity = morphiecore::protocol::fidelity::FidelityRecords::default();
    let target = morphiecore::lowering::generation::lower_request(
        request,
        &fidelity,
        Profile::Responses,
        contract,
    )?;
    Ok(morphiecore::protocol::openai::responses::encode_generation(&target).unwrap())
}

#[test]
fn typed_consumer_encodes_files_without_a_protocol_dto_or_image_admission() {
    let mut resource = typed_file(Some("independent.pdf"));
    let ResourceDescription::File(file) = &mut resource.description else {
        panic!()
    };
    file.detail = Some(FileDetail::High);
    let request = typed_request(vec![resource]).unwrap();
    let mut contract = Contract::full();
    contract.semantics.image_input = false;
    contract.images.max_images = 0;
    contract.images.inline_formats.clear();
    contract.images.details.clear();
    let encoded = encode_typed(&request, contract).unwrap();
    assert_eq!(
        encoded["input"][0]["content"],
        json!([{
            "type":"input_file","filename":"independent.pdf",
            "file_data":"data:application/pdf;base64,AQID","detail":"high",
        }])
    );
    assert_eq!(GenerationRequirements::derive(&request).file_inputs, 1);
}

#[test]
fn target_file_limits_are_independent_and_do_not_mutate_or_expand_the_request() {
    use morphiecore::lowering::generation::RepresentationError;
    let request = typed_request(vec![
        typed_file(Some("synthetic.pdf")),
        typed_file(Some("second.pdf")),
    ])
    .unwrap();
    let before = request.clone();
    for case in 0..4 {
        let mut contract = Contract::full();
        match case {
            0 => contract.files.max_files = 1,
            1 => contract.files.max_inline_bytes = 2,
            2 => contract.files.max_total_inline_bytes = 5,
            _ => contract.files.inline_media_types = Some(vec!["text/plain".into()]),
        }
        assert_eq!(
            encode_typed(&request, contract).unwrap_err(),
            RepresentationError::FileInput
        );
    }
    let mut resource = typed_file(None);
    let ResourceDescription::File(description) = &mut resource.description else {
        panic!()
    };
    description.detail = Some(FileDetail::High);
    let without_name = typed_request(vec![resource]).unwrap();
    let mut contract = Contract::full();
    contract.files.details = vec![FileDetail::Auto, FileDetail::Low];
    assert_eq!(
        encode_typed(&without_name, contract).unwrap_err(),
        RepresentationError::FileInput
    );
    for name in [None, Some("")] {
        let mut contract = Contract::full();
        contract.files.require_filename = true;
        assert_eq!(
            encode_typed(&typed_request(vec![typed_file(name)]).unwrap(), contract).unwrap_err(),
            RepresentationError::FileInput
        );
    }
    assert_eq!(request, before);
    for dialect in [
        Dialect::Nvidia,
        Dialect::Xiaomi,
        Dialect::ModelBest,
        Dialect::Bailian,
    ] {
        let client = Adapter::new(Profile::Responses, dialect, None);
        let decoded = decode(json!([file()])).unwrap();
        assert_eq!(client.contract(&Contract::full()).files.max_files, 0);
        assert!(
            client
                .encode_request(&decoded, "synthetic", &Contract::full())
                .is_err()
        );
    }
}

#[test]
fn file_profile_intersection_never_unions_target_allowlists_or_budgets() {
    let mut profile = adapter();
    profile.adaptation.files.inline_media_types =
        Some(vec!["application/pdf".into(), "text/plain".into()]);
    profile.adaptation.files.details = vec![FileDetail::Auto, FileDetail::Low];
    profile.adaptation.files.require_filename = true;
    profile.adaptation.files.max_files = 2;
    profile.adaptation.files.max_inline_bytes = 4;
    profile.adaptation.files.max_total_inline_bytes = 5;
    let mut contract = Contract::full();
    contract.files.inline_media_types = Some(vec![
        "application/pdf".into(),
        "application/octet-stream".into(),
    ]);
    contract.files.details = vec![FileDetail::Low, FileDetail::High];
    contract.files.max_files = 3;
    contract.files.max_inline_bytes = 2;
    contract.files.max_total_inline_bytes = 6;
    let merged = profile.contract(&contract).files;
    assert_eq!(
        merged.inline_media_types,
        Some(vec!["application/pdf".into()])
    );
    assert_eq!(merged.details, vec![FileDetail::Low]);
    assert!(merged.require_filename);
    assert_eq!(
        (
            merged.max_files,
            merged.max_inline_bytes,
            merged.max_total_inline_bytes
        ),
        (2, 2, 5)
    );
    assert!(
        profile
            .encode_request(&decode(json!([file()])).unwrap(), "synthetic", &contract)
            .is_err()
    );
}

#[test]
fn file_url_limits_intersections_and_edits_do_not_claim_remote_bytes() {
    let mut remote = typed_file(None);
    remote.location =
        ResourceLocation::Url(text("https://example.invalid/not-a-pdf?sig=synthetic"));
    let request = typed_request(vec![remote.clone()]).unwrap();
    assert_eq!(remote.inline_decoded_bytes().unwrap(), None);
    let mut contract = Contract::full();
    contract.files.max_inline_bytes = 0;
    contract.files.max_total_inline_bytes = 0;
    contract.files.inline_media_types = Some(vec![]);
    assert!(encode_typed(&request, contract.clone()).is_ok());
    for (a, b) in [(true, false), (false, true), (false, false)] {
        let mut profile = adapter();
        profile.adaptation.files.urls = a;
        contract.files.urls = b;
        assert!(!profile.contract(&contract).files.urls);
        assert!(encode_typed(&request, profile.contract(&contract)).is_err());
    }
    contract.files.urls = true;
    contract.files.max_files = 1;
    assert!(
        encode_typed(
            &typed_request(vec![remote.clone(), typed_file(None)]).unwrap(),
            contract
        )
        .is_err()
    );
    let mut decoded =
        decode(json!([{"type":"input_file","file_url":"https://example.invalid/old"}])).unwrap();
    let source = decoded.task.semantic.clone();
    let proof = RequestDependencyProof::capture(
        &source,
        HistoryDependency::Owners(vec![ItemId::new(1)]),
        SettingsDependency::None,
    )
    .unwrap();
    let mut items = source.items().to_vec();
    let Item::Message(m) = &mut items[0].1 else {
        panic!()
    };
    let id = m.parts[0].id;
    m.parts[0].content = ContentPart::Resource(typed_file(Some("edited.pdf")));
    decoded.task.semantic = source.clone().with_items(items).unwrap();
    assert!(proof.check(&decoded.task.semantic).is_err());
    let wire = adapter()
        .encode_request(&decoded, "synthetic", &Contract::full())
        .unwrap();
    assert_eq!(
        wire["input"][0]["content"],
        json!([{"type":"input_file","file_data":"data:application/pdf;base64,AQID","filename":"edited.pdf"}])
    );
    let Item::Message(m) = &decoded.task.semantic.items()[0].1 else {
        panic!()
    };
    assert_eq!(m.parts[0].id, id);
    assert!(proof.check(&source).is_ok());
}

#[test]
fn malformed_file_urls_are_rejected_by_wire_and_typed_boundaries() {
    for raw in [
        "file:///tmp/x",
        "ftp://example.test/x",
        "https://user:secret@example.test/x",
        "https://@example.test/x",
        "https://example.test/ x",
        "https://example.test/\\\\x",
        "https://example.test/\n",
        "//example.test/x",
        "https:///",
    ] {
        assert!(decode(json!([{"type":"input_file","file_url":raw}])).is_err());
        let mut r = typed_file(None);
        r.location = ResourceLocation::Url(text(raw));
        assert!(typed_request(vec![r]).is_err());
    }
    let raw = format!(
        "https://example.test/{}",
        "a".repeat(MAX_RESOURCE_URL_BYTES)
    );
    assert!(decode(json!([{"type":"input_file","file_url":raw}])).is_err());
    for other in [json!(null), json!("data:application/pdf;base64,AQID")] {
        assert!(
            decode(
                json!([{"type":"input_file","file_url":"https://example.test/x","file_data":other}])
            )
            .is_err()
        );
    }
}

#[test]
fn file_edits_preserve_identity_and_never_resurrect_deleted_values() {
    let mut decoded = decode(json!([file(), {"type":"input_text","text":"keep"}])).unwrap();
    let mut items = decoded.task.semantic.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        panic!()
    };
    let id = message.parts[0].id;
    let ContentPart::Resource(resource) = &mut message.parts[0].content else {
        panic!()
    };
    resource.location = ResourceLocation::Inline {
        media_type: text("text/plain"),
        data_base64: text("BAUG"),
    };
    resource.description = ResourceDescription::File(FileDescription {
        filename: Some(text("changed.txt")),
        detail: None,
    });
    message.parts.insert(
        1,
        Part {
            id: PartId::new(100),
            content: ContentPart::Resource(typed_file(None)),
        },
    );
    message.parts.swap(0, 1);
    assert_eq!(message.parts[1].id, id);
    decoded.task.semantic = decoded.task.semantic.clone().with_items(items).unwrap();
    let encoded = adapter()
        .encode_request(&decoded, "synthetic", &Contract::full())
        .unwrap();
    assert_eq!(
        encoded["input"][0]["content"],
        json!([
            {"type":"input_file","file_data":"data:application/pdf;base64,AQID"},
            {"type":"input_file","file_data":"data:text/plain;base64,BAUG","filename":"changed.txt"},
            {"type":"input_text","text":"keep"}
        ])
    );
    let mut items = decoded.task.semantic.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        panic!()
    };
    message
        .parts
        .retain(|p| matches!(p.content, ContentPart::Text(_)));
    decoded.task.semantic = decoded.task.semantic.clone().with_items(items).unwrap();
    let mut contract = Contract::full();
    contract.semantics.file_input = false;
    contract.files.max_files = 0;
    let encoded = adapter()
        .encode_request(&decoded, "synthetic", &contract)
        .unwrap();
    assert_eq!(
        encoded["input"][0]["content"],
        json!([{"type":"input_text","text":"keep"}])
    );
    assert_eq!(
        GenerationRequirements::derive(&decoded.task.semantic).file_inputs,
        0
    );
}

#[test]
fn source_description_and_presence_are_dependencies_even_when_debug_is_redacted() {
    let source = typed_request(vec![typed_file(Some("private-name.pdf"))]).unwrap();
    let proof = RequestDependencyProof::capture(
        &source,
        HistoryDependency::Owners(vec![ItemId::new(1)]),
        SettingsDependency::None,
    )
    .unwrap();
    for case in 0..5 {
        let mut resource = typed_file(Some("private-name.pdf"));
        match case {
            0 => {
                resource.description = ResourceDescription::File(FileDescription {
                    filename: Some(text("different.pdf")),
                    detail: None,
                })
            }
            1 => {
                resource.description = ResourceDescription::File(FileDescription {
                    filename: None,
                    detail: None,
                })
            }
            2 => {
                resource.description = ResourceDescription::File(FileDescription {
                    filename: Some(text("private-name.pdf")),
                    detail: Some(FileDetail::Auto),
                })
            }
            3 => {
                resource.location = ResourceLocation::Inline {
                    media_type: text("text/plain"),
                    data_base64: text("AQID"),
                }
            }
            _ => {
                resource.location = ResourceLocation::Inline {
                    media_type: text("application/pdf"),
                    data_base64: text("BAUG"),
                }
            }
        }
        assert!(
            proof
                .check(&typed_request(vec![resource]).unwrap())
                .is_err()
        );
    }
    proof.check(&source).unwrap();
    let debug = format!("{source:?}");
    for private in ["private-name.pdf", "application/pdf", "AQID"] {
        assert!(!debug.contains(private));
    }
    let absent = typed_request(vec![typed_file(None)]).unwrap();
    let proof = RequestDependencyProof::capture(
        &absent,
        HistoryDependency::Owners(vec![ItemId::new(1)]),
        SettingsDependency::None,
    )
    .unwrap();
    assert!(
        proof
            .check(&typed_request(vec![typed_file(Some(""))]).unwrap())
            .is_err()
    );
}

#[test]
fn malformed_typed_files_and_aggregate_budgets_are_revalidated_after_edits() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    for mime in ["", "not-a-type", "*/*", "text/*", "application/pdf\n"] {
        let mut resource = typed_file(None);
        resource.location = ResourceLocation::Inline {
            media_type: text(mime),
            data_base64: text("AQID"),
        };
        assert!(typed_request(vec![resource]).is_err());
    }
    let mut resource = typed_file(Some(&"x".repeat(MAX_FILE_NAME_BYTES + 1)));
    assert_eq!(resource.validate(), Err(GenerationError::Limit));
    resource = typed_file(None);
    resource.location = ResourceLocation::Inline {
        media_type: text("application/pdf"),
        data_base64: text(&STANDARD.encode(vec![0; MAX_FILE_DECODED_BYTES + 1])),
    };
    assert_eq!(resource.validate(), Err(GenerationError::Limit));
    let mut large = typed_file(None);
    large.location = ResourceLocation::Inline {
        media_type: text("application/pdf"),
        data_base64: text(&STANDARD.encode(vec![0; 720 * 1024])),
    };
    assert!(typed_request(vec![large.clone(), large.clone()]).is_ok());
    assert_eq!(
        typed_request(vec![large.clone(), large.clone(), large]),
        Err(GenerationError::Limit)
    );

    let mut resource = typed_file(None);
    resource.location = ResourceLocation::Inline {
        media_type: text("application/pdf"),
        data_base64: text(&"A".repeat(MAX_TEXT_BYTES + 4)),
    };
    assert_eq!(resource.validate(), Err(GenerationError::Limit));
    let decoded = decode(json!([file()])).unwrap();
    let mut items = decoded.task.semantic.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        panic!()
    };
    message.parts[0].content = ContentPart::Resource(resource);
    assert!(decoded.task.semantic.clone().with_items(items).is_err());
    // Generic IDs remain typed but have no issuer-bound file carrier.
    let mut resource = typed_file(None);
    resource.location = ResourceLocation::OpaqueReference(text("file-synthetic"));
    let request = typed_request(vec![resource]).unwrap();
    assert!(encode_typed(&request, Contract::full()).is_err());
}

#[test]
fn files_cannot_be_instructions_assistant_outputs_or_tool_result_media() {
    for role in ["system", "developer", "assistant"] {
        let mut value = wire(json!([file()]));
        value["input"][0]["role"] = json!(role);
        assert!(
            adapter()
                .decode_request(&serde_json::to_vec(&value).unwrap())
                .is_err()
        );
    }
    for kind in ["function", "custom_tool"] {
        let (call, output) = if kind == "function" {
            (
                json!({"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"}),
                json!({"type":"function_call_output","call_id":"c","output":[file()]}),
            )
        } else {
            (
                json!({"type":"custom_tool_call","call_id":"c","name":"lookup","input":"x"}),
                json!({"type":"custom_tool_call_output","call_id":"c","output":[file()]}),
            )
        };
        assert!(
            adapter()
                .decode_request(
                    &serde_json::to_vec(&json!({"model":"synthetic","input":[call,output]}))
                        .unwrap()
                )
                .is_err()
        );
    }
    let request = typed_request(vec![typed_file(None)]).unwrap();
    let mut items = request.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        panic!()
    };
    message.role = MessageRole::Assistant;
    assert!(request.clone().with_items(items.clone()).is_err());
    assert!(GenerationResponse::new(items, Outcome::Completed).is_err());
}

#[test]
fn file_byte_boundary_rejects_duplicate_keys_and_does_not_echo_private_values() {
    let duplicate = br#"{"model":"synthetic","input":[{"role":"user","content":[{"type":"input_file","file_data":"data:application/pdf;base64,AQID","file_data":"data:application/pdf;base64,BAUG"}]}]}"#;
    assert!(adapter().decode_request(duplicate).is_err());
    let mut part = file();
    part["file_data"] = json!("data:private-type;base64,private-body");
    part["filename"] = json!("private-name.pdf");
    let error = decode(json!([part])).unwrap_err();
    for value in ["private-type", "private-body", "private-name.pdf"] {
        assert!(!format!("{error:?} {error}").contains(value));
    }
}
