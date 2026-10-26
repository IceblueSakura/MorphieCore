//! Independent inline-file wire oracles; no file parser, credentials or network.
use crate::resources_support as resources;
use morphiecore::{
    protocol::openai::{Dialect, GenerationApi, GenerationCodec, TargetContract as Contract},
    semantic::task::generation::*,
};
use serde_json::{Value, json};

fn adapter() -> GenerationCodec {
    GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None)
}
fn wire(parts: Value) -> Value {
    json!({"model":"synthetic","input":[{"role":"user","content":parts}]})
}
fn file() -> Value {
    json!({"type":"input_file","file_data":"data:application/pdf;base64,AQID","filename":"synthetic.pdf","detail":"low"})
}
fn decode(
    parts: Value,
) -> Result<morphiecore::protocol::GenerationRequestEnvelope, morphiecore::protocol::CodecError> {
    adapter().decode_request(&serde_json::to_vec(&wire(parts)).unwrap())
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
            context: morphiecore::semantic::value::Presence::Absent,
            detail: None,
        }),
    }
}
fn typed_request(values: Vec<Resource>) -> Result<GenerationRequest, GenerationError> {
    let parts = values
        .iter()
        .enumerate()
        .map(|(i, value)| Part {
            replay: None,
            id: PartId::new(i as u64 + 1),
            cache: None,
            content: ContentPart::Resource(ResourceUse {
                description: value.description.clone(),
                ..resources::image(i as u64 + 1)
            }),
        })
        .collect();
    let table = ResourceTable::new(
        values
            .into_iter()
            .enumerate()
            .map(|(i, value)| (ResourceId::new(i as u64 + 1), value.location))
            .collect(),
    )?;
    GenerationRequest::from_resources(
        vec![(
            ItemId::new(1),
            Item::Message(Message {
                participant: None,
                role: MessageRole::User,
                parts,
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        )],
        GenerationSettings::default(),
        table,
    )
}
fn encode_typed(
    request: &GenerationRequest,
    contract: Contract,
) -> Result<Value, morphiecore::protocol::conversion::generation::RepresentationError> {
    let fidelity = morphiecore::protocol::fidelity::FidelityRecords::default();
    let target = morphiecore::protocol::openai::lowering::lower_request(
        request,
        &fidelity,
        GenerationApi::Responses,
        contract,
    )?;
    Ok(morphiecore::protocol::openai::responses::encode_generation(&target).unwrap())
}

#[test]
fn chat_file_part_is_an_inline_pdf_without_detail_or_provider_id() {
    let chat = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None);
    let chat_wire = json!({"model":"synthetic","messages":[{"role":"user","content":[
            {"type":"file","file":{"file_data":"data:application/pdf;base64,AQID","filename":"synthetic.pdf"}},
            {"type":"text","text":"Summarize."}]}]});
    let decoded = chat
        .decode_request(&serde_json::to_vec(&chat_wire).unwrap())
        .unwrap();
    let semantic = &decoded.task.semantic;
    assert_eq!(GenerationRequirements::derive(semantic).file_inputs, 1);
    let Item::Message(message) = &semantic.items()[0].1 else {
        panic!("message")
    };
    let ContentPart::Resource(pdf) = &message.parts[0].content else {
        panic!("file")
    };
    assert_eq!(
        pdf.media(semantic.resources()).unwrap().to_owned(),
        typed_file(Some("synthetic.pdf"))
    );
    let output = chat
        .encode_request(&decoded, "synthetic", &Contract::full())
        .unwrap();
    assert_eq!(output["messages"], chat_wire["messages"]);
    // Responses carries the same file natively; detail has no Chat slot.
    let responses = adapter()
        .decode_request(&serde_json::to_vec(&wire(json!([file()]))).unwrap())
        .unwrap();
    assert!(
        chat.encode_request(&responses, "synthetic", &Contract::full())
            .is_err()
    );
    let mut by_id = chat_wire.clone();
    by_id["messages"][0]["content"][0]["file"] = json!({"file_id":"file-synthetic"});
    assert!(
        chat.decode_request(&serde_json::to_vec(&by_id).unwrap())
            .is_err()
    );
}
#[test]
fn responses_tool_output_carries_files_and_needs_file_admission() {
    // Explicit cache breakpoints on media parts are standard Responses fields.
    let input = json!([
            {"type":"message","role":"user","content":[
                {"type":"input_image","image_url":"https://example.invalid/a.png","detail":"auto","prompt_cache_breakpoint":{"mode":"explicit"}}]},
            {"type":"function_call","call_id":"call_1","name":"read","arguments":"{}"},
            {"type":"function_call_output","call_id":"call_1","output":[
                {"type":"input_file","file_data":"data:application/pdf;base64,AQID","filename":"synthetic.pdf","prompt_cache_breakpoint":{"mode":"explicit"}}]}]);
    let request = json!({"model":"synthetic","input":input,"tools":[{"type":"function","name":"read","parameters":{"type":"object"}}]});
    let decoded = adapter()
        .decode_request(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    let semantic = &decoded.task.semantic;
    let needs = GenerationRequirements::derive(semantic);
    assert_eq!((needs.tool_result_media, needs.file_inputs), (1, 1));
    assert!(
        !needs.explicit_cache,
        "inherited lifetime is not native TTL admission"
    );
    assert_eq!(
        semantic.items()[0].1.cache_boundaries().collect::<Vec<_>>(),
        [PrefixCacheBoundary::RequestLifetime]
    );
    let Item::ToolResult(result) = &semantic.items()[2].1 else {
        panic!("tool result")
    };
    let ToolOutput::Parts(parts) = &result.output else {
        panic!("parts")
    };
    assert_eq!(parts[0].cache, Some(PrefixCacheBoundary::RequestLifetime));
    let InputPart::Resource(pdf) = &parts[0].content else {
        panic!("file part")
    };
    assert_eq!(
        pdf.media(semantic.resources()).unwrap().to_owned(),
        typed_file(Some("synthetic.pdf"))
    );
    let output = adapter()
        .encode_request(&decoded, "synthetic", &Contract::full())
        .unwrap();
    assert_eq!(output["input"][0]["content"], input[0]["content"]);
    assert_eq!(output["input"][2]["output"], input[2]["output"]);
    let mut fresh = decoded.clone();
    fresh.task.fidelity = Default::default();
    assert_eq!(
        adapter()
            .encode_request(&fresh, "synthetic", &Contract::full())
            .unwrap()["input"],
        output["input"]
    );
    let mut items = semantic.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        unreachable!()
    };
    message.parts[0].cache = None;
    let Item::ToolResult(result) = &mut items[2].1 else {
        unreachable!()
    };
    let ToolOutput::Parts(parts) = &mut result.output else {
        unreachable!()
    };
    parts[0].cache = None;
    let mut edited = decoded.clone();
    edited.task.semantic = semantic.clone().with_items(items).unwrap();
    let edited = adapter()
        .encode_request(&edited, "synthetic", &Contract::full())
        .unwrap();
    let mut expected = input.clone();
    expected[0]["content"][0]
        .as_object_mut()
        .unwrap()
        .remove("prompt_cache_breakpoint");
    expected[2]["output"][0]
        .as_object_mut()
        .unwrap()
        .remove("prompt_cache_breakpoint");
    assert_eq!(edited["input"], expected);
    assert_eq!(semantic.items()[2].1.cache_boundaries().count(), 1);
    let mut invalid = request.clone();
    invalid["input"][2]["output"][0]["prompt_cache_breakpoint"] = Value::Null;
    assert_eq!(
        adapter()
            .decode_request(&serde_json::to_vec(&invalid).unwrap())
            .unwrap_err(),
        morphiecore::protocol::CodecError::Invalid(
            morphiecore::protocol::failure::ShapeReason::InvalidValue
        )
    );
    let mut no_files = Contract::full();
    no_files.semantics.file_input = false;
    assert!(
        adapter()
            .encode_request(&decoded, "synthetic", &no_files)
            .is_err()
    );
    // Target file limits cover tool results, not only user messages.
    let mut images_only = Contract::full();
    images_only.files.inline_media_types = Some(vec!["image/png".into()]);
    assert!(
        adapter()
            .encode_request(&decoded, "synthetic", &images_only)
            .is_err()
    );
}
#[test]
fn request_lifetime_boundaries_have_independent_ir_and_reject_unrelated_ttls() {
    use morphiecore::protocol::{
        CodecError,
        anthropic::messages::Profile as Messages,
        google::{generate_content, interactions},
        native::ReplayTarget,
        openai::lowering::{RepresentationError, lower_request},
    };
    let mut controls = GenerationControls::default();
    controls.max_output_tokens = Some(32);
    let mut part = Part::new(PartId::new(1), text("stable"));
    part.cache = Some(PrefixCacheBoundary::RequestLifetime);
    let request = GenerationRequest::new(
        vec![
            (
                ItemId::new(1),
                Item::Instruction(Instruction {
                    participant: None,
                    parts: vec![part],
                    status: None,
                }),
            ),
            (
                ItemId::new(2),
                Item::Message(Message {
                    participant: None,
                    role: MessageRole::User,
                    phase: None,
                    status: ItemLifecycle::Completed,
                    parts: vec![Part::new(
                        PartId::new(2),
                        ContentPart::Text(text("go").into()),
                    )],
                }),
            ),
        ],
        controls,
    )
    .unwrap()
    .with_message_envelopes(vec![
        MessageEnvelope::new(
            GroupId::new(LocalScope::ROOT, 1),
            MessageEnvelopeRole::User,
            vec![ItemId::new(2)],
        )
        .unwrap(),
    ])
    .unwrap();
    // The exact boundary does not need source fidelity or a copied 30m default.
    let fidelity = Default::default();
    let target = lower_request(
        &request,
        &fidelity,
        GenerationApi::Responses,
        Contract::full(),
    )
    .unwrap();
    let wire = morphiecore::protocol::openai::responses::encode_generation(&target).unwrap();
    assert_eq!(
        wire["input"][0],
        json!({"role":"developer","content":[
            {"type":"input_text","text":"stable","prompt_cache_breakpoint":{"mode":"explicit"}}
        ]})
    );
    assert!(!GenerationRequirements::derive(&request).explicit_cache);
    // Keep the native envelope contract from masking the cache-carrier rejection.
    let ungrouped = request.clone().with_message_envelopes(vec![]).unwrap();
    assert_eq!(
        lower_request(
            &ungrouped,
            &Default::default(),
            GenerationApi::Chat,
            Contract::full()
        )
        .err(),
        Some(RepresentationError::Controls)
    );
    let native = ReplayTarget::new("synthetic", "model").unwrap();
    assert_eq!(
        Messages::AdaptiveTextTools
            .encode_generation_request(&request, &Default::default(), &native)
            .unwrap_err(),
        CodecError::Unsupported(morphiecore::protocol::failure::CarrierReason::Unavailable)
    );
    assert_eq!(
        generate_content::encode_generation_request(&request, &Default::default(), &native, None)
            .unwrap_err(),
        CodecError::Unsupported(morphiecore::protocol::failure::CarrierReason::Unavailable)
    );
    assert_eq!(
        interactions::encode_generation_request(&request, &Default::default(), &native)
            .unwrap_err(),
        CodecError::Unsupported(morphiecore::protocol::failure::CarrierReason::Unavailable)
    );

    let mut items = request.items().to_vec();
    let Item::Instruction(instruction) = &mut items[0].1 else {
        unreachable!()
    };
    instruction.parts[0].cache = Some(PrefixCacheBoundary::Ttl(PrefixCacheTtl::FiveMinutes));
    let native_ttl = request.clone().with_items(items).unwrap();
    assert!(GenerationRequirements::derive(&native_ttl).explicit_cache);
    assert_eq!(
        lower_request(
            &native_ttl,
            &Default::default(),
            GenerationApi::Responses,
            Contract::full()
        )
        .err(),
        Some(RepresentationError::Controls)
    );
    let native_wire = Messages::AdaptiveTextTools
        .encode_generation_request(&native_ttl, &Default::default(), &native)
        .unwrap();
    assert_eq!(
        native_wire.value["system"],
        json!([{"type":"text","text":"stable","cache_control":{"type":"ephemeral"}}])
    );
    assert_eq!(
        request.items()[0].1.cache_boundaries().collect::<Vec<_>>(),
        [PrefixCacheBoundary::RequestLifetime]
    );
    let mut items = request.items().to_vec();
    let mut generated = Part::new(PartId::new(1), ContentPart::Text(text("answer").into()));
    generated.cache = Some(PrefixCacheBoundary::RequestLifetime);
    items[0].1 = Item::Message(Message {
        participant: None,
        role: MessageRole::Assistant,
        phase: None,
        status: ItemLifecycle::Completed,
        parts: vec![generated],
    });
    assert_eq!(
        request.with_items(items),
        Err(GenerationError::InvalidControl)
    );
}

#[test]
fn inline_file_media_need_their_input_modality() {
    // opencode sends audio over Responses as a file with an audio data URL.
    let decoded = decode(json!([
            {"type":"input_file","file_data":"data:audio/wav;base64,AQID","filename":"clip.wav"}]))
    .unwrap();
    let semantic = &decoded.task.semantic;
    let needs = GenerationRequirements::derive(semantic);
    assert_eq!((needs.file_inputs, needs.audio_inputs), (1, 1));
    let mut files_only = GenerationFeatureSet::full();
    files_only.audio_input = false;
    assert_eq!(
        files_only.check(semantic),
        Err(GenerationFeature::AudioInput)
    );
    assert!(GenerationFeatureSet::full().check(semantic).is_ok());
    // A PDF needs only file input.
    let pdf = decode(json!([file()])).unwrap();
    let needs = GenerationRequirements::derive(&pdf.task.semantic);
    assert_eq!(
        (needs.file_inputs, needs.audio_inputs, needs.image_inputs),
        (1, 0, 0)
    );
}
#[test]
fn responses_instructions_reject_media_and_preserve_text() {
    assert_eq!(
            adapter()
                .decode_request(
                    json!({"model":"gemini","input":[
                        {"role":"developer","content":[{"type":"input_image","image_url":"data:image/png;base64,AQID"}]},
                        {"role":"user","content":"Describe."}]})
                        .to_string()
                        .as_bytes()
                )
                .unwrap_err(),
            morphiecore::protocol::CodecError::Unsupported(morphiecore::protocol::failure::CarrierReason::Unavailable)
        );
    let mut input = json!([
            {"role":"developer","content":[
                {"type":"input_text","text":"Follow the attached guide."},
                {"type":"input_file","file_data":"data:application/pdf;base64,AQID","filename":"synthetic.pdf"}]},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"Draft."}]}]);
    assert_eq!(
        adapter()
            .decode_request(
                &serde_json::to_vec(&json!({"model":"synthetic","input":input})).unwrap(),
            )
            .unwrap_err(),
        morphiecore::protocol::CodecError::Unsupported(
            morphiecore::protocol::failure::CarrierReason::Unavailable
        )
    );
    input[0]["content"] = json!("Follow the guide.");
    let decoded = adapter()
        .decode_request(&serde_json::to_vec(&json!({"model":"synthetic","input":input})).unwrap())
        .unwrap();
    let semantic = &decoded.task.semantic;
    let Item::Instruction(instruction) = &semantic.items()[0].1 else {
        panic!("instruction")
    };
    assert_eq!(instruction.parts[0].content.as_str(), "Follow the guide.");
    assert!(semantic.resources().is_empty());
    assert_eq!(GenerationRequirements::derive(semantic).file_inputs, 0);
    let output = adapter()
        .encode_request(&decoded, "synthetic", &Contract::full())
        .unwrap();
    assert_eq!(output["input"][0], input[0]);
    let chat = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None);
    assert_eq!(
        chat.encode_request(&decoded, "synthetic", &Contract::full())
            .unwrap()["messages"][0],
        json!({"role":"developer","content":"Follow the guide."})
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
    let mut items = source.items().to_vec();
    let Item::Message(m) = &mut items[0].1 else {
        panic!()
    };
    let id = m.parts[0].id;
    let ContentPart::Resource(reference) = &mut m.parts[0].content else {
        panic!()
    };
    reference.description = typed_file(Some("edited.pdf")).description;
    let table = resources::replace(
        source.resources(),
        reference.id,
        typed_file(Some("edited.pdf")),
    );
    decoded.task.semantic = source
        .clone()
        .with_resources(table)
        .unwrap()
        .with_items(items)
        .unwrap();
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
        data_base64: text(&STANDARD.encode(vec![0; MAX_TOTAL_FILE_DECODED_BYTES * 3 / 8])),
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
    assert!(ResourceTable::new(vec![(ResourceId::new(1), resource.location)]).is_err());
    // Media uses cannot borrow generic citation identities as input files.
    let mut resource = typed_file(None);
    resource.location = ResourceLocation::OpaqueReference(text("file-synthetic"));
    assert_eq!(
        typed_request(vec![resource]),
        Err(GenerationError::InvalidResource)
    );
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
