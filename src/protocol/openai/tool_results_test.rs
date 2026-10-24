//! Encoding guards also protect crate-internal callers from forged representations.
use crate::protocol::openai::*;
use crate::semantic::{task::generation::*, value::Text};

#[test]
fn responses_final_encoding_and_managed_history_cannot_drop_participants() {
    let request = Adapter::new(Profile::Chat, Dialect::Standard, None)
        .decode_client_request(br#"{"model":"synthetic","messages":[{"role":"user","name":"alice","content":"hello"}]}"#)
        .unwrap();
    let forged = RequestRepresentation {
        semantic: std::borrow::Cow::Borrowed(&request.task.semantic),
        projection: vec![],
        fidelity: &request.task.fidelity,
        profile: Profile::Responses,
        adaptation: Default::default(),
    };
    assert_eq!(
        responses::encode_generation(&forged),
        Err(CodecError::Unsupported("participant carrier".into()))
    );
    assert_eq!(
        responses::managed::input_items(&request.task, "synthetic", None),
        Err(CodecError::Unsupported("participant carrier".into()))
    );
}

#[test]
fn static_codecs_reject_new_domains_even_for_forged_target_handles() {
    let base = GenerationResponse::new(
        vec![(
            ItemId::new(1),
            Item::ToolCall(ToolCall {
                replay: None,
                call_id: text("c"),
                cache: None,
                name: text("lookup"),
                arguments: "{}".into(),
                status: ItemLifecycle::Completed,
                context: CallContext::default(),
            }),
        )],
        Outcome::Completed,
    )
    .unwrap();
    let mut items = base.items().to_vec();
    items[0].0 = ItemId::new(99);
    let Item::ToolCall(call) = &mut items[0].1 else {
        unreachable!()
    };
    call.call_id = text("structured-proposal");
    call.arguments =
        ToolArguments::Structured(StructuredValue::new(serde_json::json!({"n":1})).unwrap());
    let structured = base.clone().with_items(items).unwrap();
    let progress = base
        .clone()
        .with_progress(InteractionProgress::AwaitingToolResults)
        .unwrap();
    let mut usage = Usage::operation(2, 3, 5);
    usage.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
    let disjoint = base.with_usage(Some(usage)).unwrap();
    let replay = GenerationResponse::new(
        vec![(
            ItemId::new(1),
            Item::Reasoning(ReasoningItem {
                status: ItemLifecycle::Completed,
                parts: vec![],
                replay: Some(ReplayValue::final_value(
                    ReplayFormat::GoogleInteractionsV1Thought,
                    text("opaque"),
                )),
            }),
        )],
        Outcome::Completed,
    )
    .unwrap();
    let fidelity = crate::protocol::fidelity::FidelityRecords::default();
    let metadata = ResponseMetadata {
        delivery: None,
        storage: None,
        id: Some("r".into()),
        model: "synthetic".into(),
        created: Some(1.into()),
        context: Default::default(),
        instruction_fidelity: Default::default(),
    };
    let provider = GenerationResponse::new(
        vec![(
            ItemId::new(1),
            Item::ProviderTool(ProviderToolObservation {
                source: NativeAliasDomain {
                    source: text("abstract"),
                    scope: LocalScope::ROOT,
                },
                operation: ProviderOperation::Reported {
                    tool: text("search"),
                    alias: None,
                    requester: ProviderRequester::Unreported,
                    action: None,
                },
                progress: None,
                execution: None,
                output: None,
                artifact_status: None,
            }),
        )],
        Outcome::Completed,
    )
    .unwrap();
    let result = GenerationResponse::new(
        vec![(
            ItemId::new(1),
            Item::ToolResult(ToolResult {
                call_id: None,
                cache: None,
                name: None,
                output: ToolOutput::Structured(
                    StructuredValue::new(serde_json::json!({"n":1})).unwrap(),
                ),
                status: Some(ItemLifecycle::Completed),
                execution: None,
                is_error: None,
                context: Default::default(),
            }),
        )],
        Outcome::Completed,
    )
    .unwrap();
    for semantic in [
        &structured,
        &progress,
        &disjoint,
        &replay,
        &provider,
        &result,
    ] {
        for profile in [Profile::Chat, Profile::Responses] {
            let forged = ResponseRepresentation {
                semantic: std::borrow::Cow::Borrowed(semantic),
                projection: vec![],
                fidelity: &fidelity,
                metadata: std::borrow::Cow::Borrowed(&metadata),
                profile,
                adaptation: Default::default(),
                attachment: None,
            };
            let result = match profile {
                Profile::Chat => crate::protocol::openai::chat::encode_response(&forged),
                Profile::Responses => crate::protocol::openai::responses::encode_response(&forged),
            };
            assert!(matches!(result, Err(CodecError::Unsupported(_))));
        }
    }
}

fn text(value: &str) -> Text {
    Text::new(value, "synthetic", 256).unwrap()
}
fn resource_table(url: &str) -> ResourceTable {
    ResourceTable::new(vec![(ResourceId::new(1), ResourceLocation::Url(text(url)))]).unwrap()
}
#[test]
fn request_codecs_enforce_result_carriers_before_rendering() {
    let outputs = [
        (
            ToolOutput::Structured(StructuredValue::new(serde_json::json!({"value":42})).unwrap()),
            None,
        ),
        (
            ToolOutput::Text("failed".into()),
            Some(ToolExecution::Failed { code: None }),
        ),
        (
            ToolOutput::Text("success".into()),
            Some(ToolExecution::Succeeded),
        ),
        (
            ToolOutput::Parts(vec![Part::new(
                PartId::new(1),
                InputPart::Resource(ResourceUse {
                    id: ResourceId::new(1),
                    description: ResourceDescription::Image {
                        detail: None,
                        size_policy: ImageSizePolicy::AllowDownsize,
                    },
                }),
            )]),
            None,
        ),
    ];
    for (output, execution) in outputs {
        let semantic = GenerationRequest::from_resources(
            vec![
                (
                    ItemId::new(1),
                    Item::ToolCall(ToolCall {
                        replay: None,
                        call_id: text("c"),
                        cache: None,
                        name: text("lookup"),
                        arguments: "{}".into(),
                        status: ItemLifecycle::Completed,
                        context: CallContext::default(),
                    }),
                ),
                (
                    ItemId::new(2),
                    Item::ToolResult(ToolResult {
                        is_error: None,
                        cache: None,
                        execution,
                        call_id: Some(text("c")),
                        name: None,
                        output,
                        status: None,
                        context: CallContext::default(),
                    }),
                ),
            ],
            GenerationSettings::default(),
            resource_table("https://example.invalid/image.png"),
        )
        .unwrap();
        let media = matches!(&semantic.items()[1].1, Item::ToolResult(r) if matches!(r.output, ToolOutput::Parts(_)));
        let fidelity = FidelityRecords::default();
        for profile in [Profile::Chat, Profile::Responses] {
            let forged = RequestRepresentation {
                semantic: std::borrow::Cow::Borrowed(&semantic),
                projection: vec![],
                fidelity: &fidelity,
                profile,
                adaptation: Default::default(),
            };
            let result = match profile {
                Profile::Chat => chat::encode_generation(&forged),
                Profile::Responses => responses::encode_generation(&forged),
            };
            if media && profile == Profile::Responses {
                assert_eq!(
                    result.unwrap()["input"][1]["output"],
                    serde_json::json!([
                        {"type":"input_image","image_url":"https://example.invalid/image.png"}
                    ])
                );
                let mut items = semantic.items().to_vec();
                let Item::ToolResult(result) = &mut items[1].1 else {
                    unreachable!()
                };
                let ToolOutput::Parts(parts) = &mut result.output else {
                    unreachable!()
                };
                parts[0].cache = Some(PrefixCacheBoundary::RequestLifetime);
                let marked = semantic.clone().with_items(items).unwrap();
                let forged = RequestRepresentation {
                    semantic: std::borrow::Cow::Borrowed(&marked),
                    projection: vec![],
                    fidelity: &fidelity,
                    profile,
                    adaptation: Default::default(),
                };
                assert_eq!(
                    responses::encode_generation(&forged).unwrap()["input"][1]["output"][0]["prompt_cache_breakpoint"],
                    serde_json::json!({"mode":"explicit"})
                );
                let mut items = marked.items().to_vec();
                let Item::ToolResult(result) = &mut items[1].1 else {
                    unreachable!()
                };
                let ToolOutput::Parts(parts) = &mut result.output else {
                    unreachable!()
                };
                parts[0].cache = Some(PrefixCacheBoundary::Ttl(PrefixCacheTtl::OneHour));
                let native_ttl = marked.clone().with_items(items).unwrap();
                for (semantic, profile) in
                    [(&marked, Profile::Chat), (&native_ttl, Profile::Responses)]
                {
                    let forged = RequestRepresentation {
                        semantic: std::borrow::Cow::Borrowed(semantic),
                        projection: vec![],
                        fidelity: &fidelity,
                        profile,
                        adaptation: Default::default(),
                    };
                    let result = match profile {
                        Profile::Chat => chat::encode_generation(&forged),
                        Profile::Responses => responses::encode_generation(&forged),
                    };
                    assert_eq!(
                        result,
                        Err(CodecError::Unsupported("cache boundary carrier".into()))
                    );
                }
            } else {
                assert!(matches!(result, Err(CodecError::Unsupported(_))));
            }
        }
    }
}
