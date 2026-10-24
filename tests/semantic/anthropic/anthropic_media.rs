//! Independent native image/document oracles; inert sources, no fetch or file API.
#[test]
fn native_image_count_includes_tool_result_images() {
    use morphiecore::protocol::anthropic::messages::Profile;
    use serde_json::json;
    let image = json!({"type":"image","source":{
            "type":"base64","media_type":"image/png","data":"AQID"}});
    let mut wire = json!({"model":"synthetic","max_tokens":32,"messages":[
            {"role":"user","content":vec![image.clone();599]},
            {"role":"assistant","content":[
                {"type":"tool_use","id":"c","name":"read","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"c",
                "content":[image.clone()]}]}]});
    let profile = Profile::AdaptiveTextTools;
    let request = profile.decode_request(wire.to_string().as_bytes()).unwrap();
    assert!(profile.encode_request(&request).is_ok());
    wire["messages"][2]["content"][0]["content"]
        .as_array_mut()
        .unwrap()
        .push(image);
    assert!(profile.decode_request(wire.to_string().as_bytes()).is_err());
}

#[test]
fn native_image_size_counts_encoded_base64_bytes() {
    use morphiecore::protocol::anthropic::messages::{
        Block, Media, MessageContent, Profile, Source,
    };
    use serde_json::json;
    let profile = Profile::AdaptiveTextTools;
    let mut request = profile
        .decode_request(
            json!({"model":"synthetic","max_tokens":32,"messages":[
                {"role":"user","content":[{"type":"image","source":{
                    "type":"base64","media_type":"image/png","data":"AQID"}}]}]})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    let MessageContent::Blocks(blocks) = &mut request.messages[0].content else {
        panic!("image blocks")
    };
    blocks[0].content = Block::Media(Media::Image {
        source: Source::Base64 {
            media_type: "image/png".into(),
            data: "AAAA".repeat(10_000_000 / 4),
        },
        reject_oversized: false,
    });
    assert!(profile.encode_request(&request).is_ok());
    let MessageContent::Blocks(blocks) = &mut request.messages[0].content else {
        unreachable!()
    };
    let Block::Media(Media::Image {
        source: Source::Base64 { data, .. },
        ..
    }) = &mut blocks[0].content
    else {
        unreachable!()
    };
    data.push_str("AAAA");
    assert!(profile.encode_request(&request).is_err());
}
use morphiecore::{
    protocol::{anthropic::messages::Profile, native::ReplayTarget},
    semantic::task::generation::*,
    topology::catalog,
};
use serde_json::{Value, json};

fn target() -> ReplayTarget {
    ReplayTarget::new("synthetic-provider", "synthetic-model").unwrap()
}
fn messages() -> Value {
    json!([
        {"role":"user","content":[
            {"type":"image","source":{"type":"base64","media_type":"image/png","data":"AQID"},
                "transformations":{"oversized_image":"error"}},
            {"type":"image","source":{"type":"url","url":"https://example.invalid/a.jpg"}},
            {"type":"document","source":{"type":"base64","media_type":"application/pdf","data":"BAUG"},"title":"Report"},
            {"type":"document","source":{"type":"text","media_type":"text/plain","data":"notes"}},
            {"type":"text","text":"Compare."}]},
        {"role":"assistant","content":[
            {"type":"tool_use","id":"toolu_1","name":"shot","input":{}}]},
        {"role":"user","content":[
            {"type":"tool_result","tool_use_id":"toolu_1","content":[
                {"type":"text","text":"screen"},
                {"type":"image","source":{"type":"base64","media_type":"image/webp","data":"BwgJ"},
                    "transformations":{"oversized_image":"error"}}]}]}
    ])
}
fn view<'a>(request: &'a GenerationRequest, resource: &'a ResourceUse) -> ResourceView<'a> {
    resource.media(request.resources()).unwrap()
}

#[test]
fn document_context_preserves_per_use_presence_in_messages_and_tool_history() {
    use morphiecore::semantic::value::{Presence, Text};
    let text = |s: &str| Text::allowing_empty(s, "fixture", MAX_TOTAL_BYTES).unwrap();
    let p = Profile::AdaptiveTextTools;
    let source = ResourceId::new(1);
    let resources = ResourceTable::new(vec![(
        source,
        ResourceLocation::Inline {
            media_type: text("text/plain"),
            data_base64: text("bm90ZXM="),
        },
    )])
    .unwrap();
    let contexts = [
        Presence::Absent,
        Presence::Null,
        Presence::Value(text("")),
        Presence::Value(text("private context")),
    ];
    for (index, context) in contexts.into_iter().enumerate() {
        let mut first = json!({"type":"document","source":{"type":"text","media_type":"text/plain","data":"notes"}});
        match &context {
            Presence::Absent => {}
            Presence::Null => first["context"] = Value::Null,
            Presence::Value(value) => first["context"] = json!(value.as_str()),
        }
        let second = json!({"type":"document","source":{"type":"text","media_type":"text/plain","data":"notes"},
            "context":"different use"});
        let expected = json!([
            {"role":"user","content":[first]},
            {"role":"assistant","content":[{"type":"tool_use","id":"call","name":"lookup","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"call","content":[second]}]}
        ]);
        let wire = json!({"model":"synthetic-model","max_tokens":32,"messages":expected});
        let decoded = p
            .decode_generation_request(wire.to_string().as_bytes(), LocalScope::ROOT, &target())
            .unwrap();
        let Item::Message(message) = &decoded.semantic.items()[0].1 else {
            panic!("message")
        };
        let ContentPart::Resource(resource) = &message.parts[0].content else {
            panic!("resource")
        };
        let ResourceDescription::File(file) = &resource.description else {
            panic!("file")
        };
        assert_eq!(file.context, context);
        let Item::ToolResult(result) = &decoded.semantic.items()[2].1 else {
            panic!("result")
        };
        let ToolOutput::Parts(parts) = &result.output else {
            panic!("parts")
        };
        let InputPart::Resource(resource) = &parts[0].content else {
            panic!("resource")
        };
        let ResourceDescription::File(file) = &resource.description else {
            panic!("file")
        };
        assert_eq!(file.context, Presence::Value(text("different use")));

        // Build the reverse direction independently, reusing one source identity
        // with distinct per-use context rather than duplicating the document.
        let use_with = |context| ResourceUse {
            id: source,
            description: ResourceDescription::File(FileDescription {
                filename: None,
                detail: None,
                context,
            }),
        };
        let mut controls = GenerationControls::default();
        controls.max_output_tokens = Some(32);
        let request = GenerationRequest::from_resources(
            vec![
                (
                    ItemId::new(1),
                    Item::Message(Message {
                        role: MessageRole::User,
                        status: ItemLifecycle::Completed,
                        phase: None,
                        parts: vec![Part::new(
                            PartId::new(1),
                            ContentPart::Resource(use_with(context.clone())),
                        )],
                    }),
                ),
                (
                    ItemId::new(2),
                    Item::ToolCall(ToolCall {
                        call_id: text("call"),
                        name: text("lookup"),
                        arguments: ToolArguments::Structured(
                            StructuredValue::new(json!({})).unwrap(),
                        ),
                        status: ItemLifecycle::Completed,
                        context: Default::default(),
                        cache: None,
                        replay: None,
                    }),
                ),
                (
                    ItemId::new(3),
                    Item::ToolResult(ToolResult {
                        call_id: Some(text("call")),
                        name: None,
                        output: ToolOutput::Parts(vec![Part::new(
                            PartId::new(2),
                            InputPart::Resource(use_with(Presence::Value(text("different use")))),
                        )]),
                        is_error: None,
                        execution: None,
                        status: None,
                        context: Default::default(),
                        cache: None,
                    }),
                ),
            ],
            GenerationSettings {
                controls,
                ..Default::default()
            },
            resources.clone(),
        )
        .unwrap()
        .with_message_envelopes(vec![
            MessageEnvelope::new(
                GroupId::new(LocalScope::ROOT, 1),
                MessageEnvelopeRole::User,
                vec![ItemId::new(1)],
            )
            .unwrap(),
            MessageEnvelope::new(
                GroupId::new(LocalScope::ROOT, 2),
                MessageEnvelopeRole::Assistant,
                vec![ItemId::new(2)],
            )
            .unwrap(),
            MessageEnvelope::new(
                GroupId::new(LocalScope::ROOT, 3),
                MessageEnvelopeRole::User,
                vec![ItemId::new(3)],
            )
            .unwrap(),
        ])
        .unwrap();
        assert_eq!(request.resources().len(), 1);
        let output = p
            .encode_generation_request(&request, &Default::default(), &target())
            .unwrap();
        assert_eq!(output.value["messages"], expected, "presence {index}");
        assert!(output.rules.is_empty());
        let mut input = morphiecore::protocol::anthropic::messages::client::decode_client_request(
            wire.to_string().as_bytes(),
        )
        .unwrap();
        assert!(GenerationRequirements::derive(&input.task.semantic).document_context);
        assert_eq!(
            p.semantic_contract().check(&input.task.semantic),
            Err(GenerationFeature::DocumentContext)
        );
        p.representation_contract()
            .check(&input.task.semantic)
            .unwrap();
        // The same closed capability applies when context appears only in tool results.
        let before = input.clone();
        for profile in [
            morphiecore::protocol::openai::Profile::Chat,
            morphiecore::protocol::openai::Profile::Responses,
        ] {
            assert!(
                morphiecore::protocol::openai::Adapter::new(
                    profile,
                    morphiecore::protocol::openai::Dialect::Standard,
                    None,
                )
                .encode_request(
                    &input,
                    "m",
                    &morphiecore::protocol::openai::TargetCaps::full()
                )
                .is_err()
            );
        }
        assert_eq!(input, before);
        input.task.semantic = request;
        input.conversation = Some(
            morphiecore::semantic::context::ConversationContext::conversation("synthetic").unwrap(),
        );
        catalog::MESSAGES_BINDINGS[0]
            .endpoint()
            .encode_request_with_rules(&input)
            .unwrap();
    }
    let mut too_large = Resource {
        location: ResourceLocation::Url(text("https://example.invalid/a.pdf")),
        description: ResourceDescription::File(FileDescription {
            filename: None,
            detail: None,
            context: Presence::Value(text(&"x".repeat(MAX_TEXT_BYTES + 1))),
        }),
    };
    assert_eq!(too_large.validate(), Err(GenerationError::Limit));
    let ResourceDescription::File(file) = &mut too_large.description else {
        unreachable!()
    };
    file.context = Presence::Value(text("private context"));
    assert!(!format!("{too_large:?}").contains("private context"));
}

#[test]
fn images_and_documents_map_to_inert_resources_and_back() {
    let raw = json!({"model":"synthetic-model","max_tokens":64,"messages":messages()});
    let p = Profile::AdaptiveTextTools;
    let decoded = p
        .decode_generation_request(
            &serde_json::to_vec(&raw).unwrap(),
            LocalScope::new(1),
            &target(),
        )
        .unwrap();
    let semantic = &decoded.semantic;
    let needs = GenerationRequirements::derive(semantic);
    assert_eq!(
        (
            needs.image_inputs,
            needs.file_inputs,
            needs.tool_result_media
        ),
        (3, 2, 1)
    );
    let inputs: Vec<_> = semantic
        .items()
        .iter()
        .filter_map(|(_, item)| match item {
            Item::Message(m) => match &m.parts[0].content {
                ContentPart::Resource(r) => Some(view(semantic, r)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    let inline = |media: &str, data: &str| -> bool {
        inputs.iter().any(|v| {
            matches!(v.location, ResourceLocation::Inline { media_type, data_base64 }
                    if media_type.as_str() == media && data_base64.as_str() == data)
        })
    };
    assert_eq!(inputs.len(), 4);
    assert_eq!(inputs[0].kind(), ResourceKind::Image);
    assert_eq!(
        inputs[0].description,
        &ResourceDescription::Image {
            detail: None,
            size_policy: ImageSizePolicy::RejectOversized,
        }
    );
    assert_eq!(
        inputs[1].description,
        &ResourceDescription::Image {
            detail: None,
            size_policy: ImageSizePolicy::AllowDownsize,
        }
    );
    assert!(inline("image/png", "AQID"));
    assert!(
        matches!(inputs[1].location, ResourceLocation::Url(u) if u.as_str() == "https://example.invalid/a.jpg")
    );
    assert_eq!(inputs[2].kind(), ResourceKind::File);
    // A document title is the model-visible file name.
    assert!(
        matches!(inputs[2].description, ResourceDescription::File(FileDescription {
            filename: Some(name), detail: None, context: morphiecore::semantic::value::Presence::Absent }) if name.as_str() == "Report")
    );
    assert!(inline("application/pdf", "BAUG"));
    // A text document is an inline text/plain file: base64("notes").
    assert!(inline("text/plain", "bm90ZXM="));
    let Item::ToolResult(result) = &semantic.items()[6].1 else {
        panic!("tool result")
    };
    let ToolOutput::Parts(parts) = &result.output else {
        panic!("parts")
    };
    let InputPart::Resource(shot) = &parts[1].content else {
        panic!("tool image")
    };
    assert_eq!(view(semantic, shot).kind(), ResourceKind::Image);
    assert_eq!(
        shot.description,
        ResourceDescription::Image {
            detail: None,
            size_policy: ImageSizePolicy::RejectOversized,
        }
    );
    // Client intake must retain the same per-use guarantee, including tool results.
    let client = morphiecore::protocol::anthropic::messages::client::decode_client_request(
        raw.to_string().as_bytes(),
    )
    .unwrap();
    let policies = |r: &GenerationRequest| {
        r.items()
            .iter()
            .flat_map(|(_, item)| item.resource_uses())
            .map(|r| r.description.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(policies(&client.task.semantic), policies(semantic));
    let encoded = p
        .encode_generation_request(semantic, &Default::default(), &target())
        .unwrap();
    assert_eq!(encoded.value["messages"], messages());
    assert!(encoded.rules.is_empty());

    // Same identities, different directives: old fidelity must not restore "error".
    let mut items = semantic.items().to_vec();
    for (_, item) in &mut items {
        let change = |r: &mut ResourceUse| {
            if let ResourceDescription::Image { size_policy, .. } = &mut r.description {
                *size_policy = ImageSizePolicy::AllowDownsize;
            }
        };
        match item {
            Item::Message(m) => {
                for part in &mut m.parts {
                    if let ContentPart::Resource(r) = &mut part.content {
                        change(r);
                    }
                }
            }
            Item::ToolResult(r) => {
                if let ToolOutput::Parts(parts) = &mut r.output {
                    for part in parts {
                        if let InputPart::Resource(r) = &mut part.content {
                            change(r);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let edited = semantic.clone().with_items(items).unwrap();
    let encoded = p
        .encode_generation_request(&edited, &decoded.fidelity, &target())
        .unwrap();
    let mut expected = messages();
    expected[0]["content"][0]
        .as_object_mut()
        .unwrap()
        .remove("transformations");
    expected[2]["content"][0]["content"][1]
        .as_object_mut()
        .unwrap()
        .remove("transformations");
    assert_eq!(encoded.value["messages"], expected);
    assert_eq!(policies(semantic), policies(&client.task.semantic));
}

#[test]
fn image_size_policy_belongs_to_each_use_not_the_resource_or_fidelity() {
    use morphiecore::protocol::openai::{Profile as OpenAI, TargetCaps, lowering};
    use morphiecore::semantic::value::Text;
    let id = ResourceId::new(1);
    let image = |part, size_policy| {
        Part::new(
            PartId::new(part),
            ContentPart::Resource(ResourceUse {
                id,
                description: ResourceDescription::Image {
                    detail: None,
                    size_policy,
                },
            }),
        )
    };
    let mut controls = GenerationControls::default();
    controls.max_output_tokens = Some(32);
    let envelope = |id, role| {
        MessageEnvelope::new(
            GroupId::new(LocalScope::ROOT, id),
            role,
            vec![ItemId::new(id)],
        )
        .unwrap()
    };
    let request = GenerationRequest::from_resources(
        vec![(
            ItemId::new(1),
            Item::Message(Message {
                role: MessageRole::User,
                status: ItemLifecycle::Completed,
                phase: None,
                parts: vec![
                    image(1, ImageSizePolicy::AllowDownsize),
                    image(2, ImageSizePolicy::RejectOversized),
                ],
            }),
        )],
        GenerationSettings {
            controls,
            ..Default::default()
        },
        ResourceTable::new(vec![(
            id,
            ResourceLocation::Url(
                Text::new("https://example.invalid/a.png", "image", 256).unwrap(),
            ),
        )])
        .unwrap(),
    )
    .unwrap()
    .with_message_envelopes(vec![envelope(1, MessageEnvelopeRole::User)])
    .unwrap();
    // Independent IR -> wire oracle, without decoding the expected request first.
    let encoded = Profile::AdaptiveTextTools
        .encode_generation_request(&request, &Default::default(), &target())
        .unwrap();
    assert_eq!(
        encoded.value["messages"],
        json!([{"role":"user","content":[
            {"type":"image","source":{"type":"url","url":"https://example.invalid/a.png"}},
            {"type":"image","source":{"type":"url","url":"https://example.invalid/a.png"},
                "transformations":{"oversized_image":"error"}}
        ]}])
    );
    assert_eq!(request.resources().len(), 1);
    let text = |s: &str| Text::new(s, "fixture", 256).unwrap();
    let tool_history = GenerationRequest::from_resources(
        vec![
            (
                ItemId::new(1),
                Item::Message(Message {
                    role: MessageRole::User,
                    status: ItemLifecycle::Completed,
                    phase: None,
                    parts: vec![Part::new(
                        PartId::new(1),
                        ContentPart::Text(text("go").into()),
                    )],
                }),
            ),
            (
                ItemId::new(2),
                Item::ToolCall(ToolCall {
                    call_id: text("c"),
                    name: text("shot"),
                    arguments: ToolArguments::Structured(StructuredValue::new(json!({})).unwrap()),
                    status: ItemLifecycle::Completed,
                    context: Default::default(),
                    replay: None,
                    cache: None,
                }),
            ),
            (
                ItemId::new(3),
                Item::ToolResult(ToolResult {
                    call_id: Some(text("c")),
                    name: None,
                    output: ToolOutput::Parts(vec![Part::new(
                        PartId::new(2),
                        InputPart::Resource(ResourceUse {
                            id,
                            description: ResourceDescription::Image {
                                detail: None,
                                size_policy: ImageSizePolicy::RejectOversized,
                            },
                        }),
                    )]),
                    is_error: None,
                    execution: None,
                    status: None,
                    context: Default::default(),
                    cache: None,
                }),
            ),
        ],
        request.settings().clone(),
        request.resources().clone(),
    )
    .unwrap()
    .with_message_envelopes(vec![
        envelope(1, MessageEnvelopeRole::User),
        envelope(2, MessageEnvelopeRole::Assistant),
        envelope(3, MessageEnvelopeRole::User),
    ])
    .unwrap();
    let encoded = Profile::AdaptiveTextTools
        .encode_generation_request(&tool_history, &Default::default(), &target())
        .unwrap();
    assert_eq!(
        encoded.value["messages"],
        json!([
            {"role":"user","content":[{"type":"text","text":"go"}]},
            {"role":"assistant","content":[{"type":"tool_use","id":"c","name":"shot","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"c","content":[
                {"type":"image","source":{"type":"url","url":"https://example.invalid/a.png"},
                    "transformations":{"oversized_image":"error"}}]}]}
        ])
    );
    for request in [request, tool_history] {
        // Exercise size-policy rejection independently of native envelope grouping.
        let request = request.with_message_envelopes(vec![]).unwrap();
        let original = request.clone();
        for profile in [OpenAI::Chat, OpenAI::Responses] {
            assert_eq!(
                lowering::lower_request(&request, &Default::default(), profile, TargetCaps::full())
                    .err(),
                Some(lowering::RepresentationError::ImageInput),
                "{profile:?}"
            );
        }
        assert_eq!(request, original);
    }
}

#[test]
fn image_transformation_known_fields_are_strict_and_defaults_are_equivalent() {
    // Pinned ImageBlockParam permits null transformations, but
    // ImageTransformationsParam.oversized_image is a non-null literal.
    // Source: docs/refs/2026-10-23-stable-protocol-contracts.md#anthropic-api-2026-10-21.
    use morphiecore::protocol::{CodecError, anthropic::messages::client::decode_client_request};
    let wire = |transformations| {
        json!({"model":"m","max_tokens":8,"messages":[{"role":"user","content":[
            {"type":"image","source":{"type":"url","url":"https://example.invalid/a.png"},
                "transformations":transformations}]}]})
    };
    for transformations in [
        Value::Null,
        json!({}),
        json!({"oversized_image":"downsize"}),
        json!({"future_option":true}),
    ] {
        let request = decode_client_request(wire(transformations).to_string().as_bytes()).unwrap();
        let resource = request.task.semantic.items()[0]
            .1
            .resource_uses()
            .next()
            .unwrap();
        assert_eq!(
            resource.description,
            ResourceDescription::Image {
                detail: None,
                size_policy: ImageSizePolicy::AllowDownsize,
            }
        );
    }
    for oversized in [Value::Null, json!(false), json!("resize")] {
        let raw = wire(json!({"oversized_image":oversized}));
        assert_eq!(
            decode_client_request(raw.to_string().as_bytes()).unwrap_err(),
            CodecError::Invalid("Anthropic oversized_image")
        );
        assert_eq!(
            Profile::AdaptiveTextTools
                .decode_request(raw.to_string().as_bytes())
                .unwrap_err(),
            CodecError::Invalid("Anthropic oversized_image")
        );
    }
}

#[test]
fn unrepresentable_sources_and_uncalibrated_options_fail_without_losing_filenames() {
    for block in [
        json!({"type":"image","source":{"type":"file","file_id":"file_synthetic"}}),
        json!({"type":"document","source":{"type":"content","content":"x"}}),
        json!({"type":"document","context":{},"source":{"type":"url","url":"https://example.invalid/a.pdf"}}),
        json!({"type":"image","source":{"type":"base64","media_type":"image/bmp","data":"AQID"}}),
        // Citations are deferred; dropping one would lose a known field silently.
        json!({"type":"text","text":"x","citations":[{"type":"char_location","cited_text":"x",
                "document_index":0,"document_title":null,"start_char_index":0,"end_char_index":1}]}),
    ] {
        let raw =
            json!({"model":"m","max_tokens":8,"messages":[{"role":"user","content":[block]}]});
        assert!(
            morphiecore::protocol::anthropic::messages::client::decode_client_request(
                &serde_json::to_vec(&raw).unwrap()
            )
            .is_err()
        );
    }
    let uncited = json!({"model":"m","max_tokens":8,"messages":[{"role":"user","content":[
            {"type":"text","text":"x","citations":null}]}]});
    assert!(
        morphiecore::protocol::anthropic::messages::client::decode_client_request(
            &serde_json::to_vec(&uncited).unwrap()
        )
        .is_ok()
    );
    use morphiecore::protocol::openai::{Adapter, Dialect, Profile as Client};
    let endpoint = catalog::MESSAGES_BINDINGS[0].endpoint();
    let chat = Adapter::new(Client::Chat, Dialect::Standard, None);
    let decode = |content: Value| {
        let mut request = chat
            .decode_client_request(
                &serde_json::to_vec(
                    &json!({"model":"m","messages":[{"role":"user","content":content}]}),
                )
                .unwrap(),
            )
            .unwrap();
        // The Go binding needs a session grouping.
        request.conversation = Some(
            morphiecore::semantic::context::ConversationContext::conversation("synthetic").unwrap(),
        );
        request
    };
    let request = decode(json!([
            {"type":"image_url","image_url":{"url":"https://example.invalid/a.png","detail":"low"}}]));
    assert!(endpoint.encode_request_with_rules(&request).is_err());
    let mut context = decode(json!("hello"));
    let mut settings = context.task.semantic.settings().clone();
    settings.reasoning = morphiecore::semantic::task::generation::ReasoningRequest::present(None);
    settings.reasoning.context = morphiecore::semantic::value::Presence::Value(
        morphiecore::semantic::task::generation::ReasoningContext::CurrentTurn,
    );
    context.task.semantic = context.task.semantic.with_settings(settings).unwrap();
    assert!(endpoint.encode_request_with_rules(&context).is_err());
    let named = decode(json!([{"type":"file","file":{
            "file_data":"data:application/pdf;base64,AQID","filename":"report.pdf"}}]));
    let (wire, rules) = endpoint.encode_request_with_rules(&named).unwrap();
    assert_eq!(
        wire["messages"][0]["content"],
        json!([{"type":"document","source":{"type":"base64","media_type":"application/pdf","data":"AQID"},"title":"report.pdf"}])
    );
    // The Chat source has no cap; Messages requires one and records the default.
    assert_eq!(wire["max_tokens"], 32_000);
    assert_eq!(
        rules,
        [
            "messages.default-max-tokens.v1",
            "messages.default-automatic-cache.v1"
        ]
    );
    // A later developer message becomes a mid-conversation system message on the
    // reviewed Haiku binding; the profile default keeps admission closed.
    let ordered = |messages: Value| {
        let mut request = chat
            .decode_client_request(
                &serde_json::to_vec(&json!({"model":"m","messages":messages})).unwrap(),
            )
            .unwrap();
        request.conversation = Some(
            morphiecore::semantic::context::ConversationContext::conversation("synthetic").unwrap(),
        );
        request
    };
    let later = ordered(json!([{"role":"user","content":"go"},
            {"role":"developer","content":"Be brief."}]));
    let mut closed = endpoint.clone();
    closed.representation = morphiecore::protocol::anthropic::messages::TargetCaps::new(
        Profile::AdaptiveTextTools,
        morphiecore::protocol::anthropic::messages::Dialect::OpenCodeGo,
    )
    .into();
    assert!(closed.encode_request_with_rules(&later).is_err());
    let widened = endpoint.clone();
    let (wire, rules) = widened.encode_request_with_rules(&later).unwrap();
    assert_eq!(
        wire["messages"],
        json!([{"role":"user","content":[{"type":"text","text":"go"}]},
                {"role":"system","content":[{"type":"text","text":"Be brief."}]}])
    );
    assert!(rules.contains(&"messages.developer-as-system.v1"));
    let between = ordered(json!([{"role":"user","content":"go"},
            {"role":"developer","content":"Be brief."},{"role":"user","content":"again"}]));
    assert!(widened.encode_request_with_rules(&between).is_err());
    // Messages has no audio or video carrier, and its system prompt is text.
    let audio =
        decode(json!([{"type":"input_audio","input_audio":{"data":"AQID","format":"wav"}}]));
    assert!(endpoint.encode_request_with_rules(&audio).is_err());
    let video =
        decode(json!([{"type":"video_url","video_url":{"url":"data:video/mp4;base64,AQID"}}]));
    assert!(endpoint.encode_request_with_rules(&video).is_err());
    assert_eq!(
            Adapter::new(Client::Responses, Dialect::Standard, None)
                .decode_client_request(
                    json!({"model":"m","input":[
                    {"role":"developer","content":[{"type":"input_image","image_url":"data:image/png;base64,AQID"}]},
                    {"role":"user","content":"Describe."}]})
                    .to_string()
                    .as_bytes(),
                )
                .unwrap_err(),
            morphiecore::protocol::CodecError::Unsupported("instruction content".into())
        );
    // `downsize` is the default; `error` survives only on a Messages target.
    let messages = |oversized: &str| {
        let raw = json!({"model":"m","max_tokens":8,"messages":[{"role":"user","content":[
                {"type":"image","source":{"type":"url","url":"https://example.invalid/a.png"},
                    "transformations":{"oversized_image":oversized}}]}]});
        let mut request =
            morphiecore::protocol::anthropic::messages::client::decode_client_request(
                &serde_json::to_vec(&raw).unwrap(),
            )
            .unwrap();
        request.conversation = Some(
            morphiecore::semantic::context::ConversationContext::conversation("synthetic").unwrap(),
        );
        request
    };
    let image =
        json!({"type":"image","source":{"type":"url","url":"https://example.invalid/a.png"}});
    let (wire, _) = endpoint
        .encode_request_with_rules(&messages("downsize"))
        .unwrap();
    assert_eq!(wire["messages"][0]["content"], json!([image]));
    let full = morphiecore::protocol::openai::TargetCaps::full();
    assert!(
        chat.encode_request(&messages("downsize"), "synthetic", &full)
            .is_ok()
    );
    let strict = messages("error");
    let (wire, _) = endpoint.encode_request_with_rules(&strict).unwrap();
    let mut expected = image;
    expected["transformations"] = json!({"oversized_image":"error"});
    assert_eq!(wire["messages"][0]["content"], json!([expected]));
    assert!(chat.encode_request(&strict, "synthetic", &full).is_err());
}
