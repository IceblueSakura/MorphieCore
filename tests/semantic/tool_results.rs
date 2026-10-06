//! Tool result kind, artifact lifecycle and wire representability are independent.
use crate::events_support::text;
use morphiecore::{
    lowering::generation::{
        GenerationRepresentationContract as Contract, RepresentationError, lower_request,
    },
    protocol::{fidelity::FidelityRecords, openai::Profile},
    semantic::task::generation::*,
};
use serde_json::json;
fn call() -> (ItemId, Item) {
    (
        ItemId::new(1),
        Item::ToolCall(ToolCall {
            call_id: text("c"),
            name: text("lookup"),
            arguments: "{}".into(),
            message: None,
            status: ItemLifecycle::Completed,
            context: CallContext::default(),
        }),
    )
}
fn request(
    output: ToolOutput,
    status: Option<ItemLifecycle>,
) -> Result<GenerationRequest, GenerationError> {
    request_with_execution(output, status, None)
}
fn request_with_execution(
    output: ToolOutput,
    status: Option<ItemLifecycle>,
    execution: Option<ToolExecution>,
) -> Result<GenerationRequest, GenerationError> {
    GenerationRequest::new(
        vec![
            call(),
            (
                ItemId::new(2),
                Item::ToolResult(ToolResult {
                    execution,
                    call_id: text("c"),
                    output,
                    status,
                    context: CallContext::default(),
                }),
            ),
        ],
        GenerationControls::default(),
    )
}
fn image(url: &str) -> Resource {
    Resource {
        location: ResourceLocation::Url(text(url)),
        description: ResourceDescription::Image { detail: None },
    }
}
fn media(url: &str) -> ToolOutput {
    ToolOutput::Parts(vec![
        (PartId::new(10), ToolResultPart::Text(text("caption"))),
        (PartId::new(11), ToolResultPart::Resource(image(url))),
    ])
}
#[test]
fn structured_value_preserves_integer_precision_order_and_bounds() {
    let value: serde_json::Value =
        serde_json::from_str(r#"{"z":18446744073709551615,"a":null}"#).unwrap();
    let structured = StructuredValue::new(value.clone()).unwrap();
    assert_eq!(structured.value()["z"].as_u64(), Some(u64::MAX));
    assert_eq!(
        structured
            .value()
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["z", "a"]
    );
    let request = request(ToolOutput::Structured(structured), None).unwrap();
    assert!(GenerationRequirements::derive(&request).structured_tool_results);
    assert_eq!(request.continuation(), Continuation::Unreported);
    let mut deep = json!(null);
    for _ in 0..40 {
        deep = json!([deep]);
    }
    assert_eq!(
        StructuredValue::new(deep).unwrap_err(),
        GenerationError::Limit
    );
    assert!(StructuredValue::new(json!(vec![0; 8193])).is_err());
    assert!(StructuredValue::new(json!("x".repeat(MAX_TEXT_BYTES + 1))).is_err());
}
#[test]
fn execution_error_is_a_terminal_result_not_a_generation_failure() {
    let error = ToolExecution::Failed {
        code: Some(text("not_found")),
    };
    let payload = ToolOutput::Text("synthetic failure".into());
    assert!(
        request_with_execution(
            payload.clone(),
            Some(ItemLifecycle::InProgress),
            Some(error.clone())
        )
        .is_err()
    );
    let history =
        request_with_execution(payload, Some(ItemLifecycle::Completed), Some(error)).unwrap();
    let response = GenerationResponse::new(vec![call()], Outcome::Completed).unwrap();
    let exchange = ResponseContinuation::new(
        ResponseRelation::new(TurnId::new(1), ResponseId::new(1)),
        &response,
    );
    assert_eq!(
        exchange.inspect(&history).unwrap(),
        ResultReadiness::ResultsComplete
    );
    assert_eq!(response.outcome(), Outcome::Completed);
    assert!(GenerationRequirements::derive(&history).tool_execution_reports);
}
#[test]
fn ordered_media_has_unique_parts_and_distinct_requirements() {
    let history = request(
        media("https://example.invalid/a.png"),
        Some(ItemLifecycle::Incomplete),
    )
    .unwrap();
    let requirements = GenerationRequirements::derive(&history);
    assert_eq!(requirements.tool_result_images, 1);
    assert_eq!(requirements.image_inputs, 0);
    let proof = RequestDependencyProof::capture(
        &history,
        HistoryDependency::PrefixThrough(ItemId::new(2)),
        morphiecore::semantic::task::generation::SettingsDependency::All,
    )
    .unwrap();
    assert!(
        proof
            .check(
                &request(
                    media("https://example.invalid/b.png"),
                    Some(ItemLifecycle::Incomplete)
                )
                .unwrap()
            )
            .is_err()
    );
    assert!(
        request(
            ToolOutput::Parts(vec![
                (PartId::new(10), ToolResultPart::Text(text("one"))),
                (PartId::new(10), ToolResultPart::Text(text("two")))
            ]),
            None
        )
        .is_err()
    );
    assert!(request(media("https://user:secret@example.invalid/a.png"), None).is_err());
    let mut opaque = image("https://example.invalid/a.png");
    opaque.location = ResourceLocation::OpaqueReference(text("issuer-file"));
    assert!(
        request(
            ToolOutput::Parts(vec![(PartId::new(10), ToolResultPart::Resource(opaque))]),
            None
        )
        .is_err()
    );
}
#[test]
fn responses_tool_images_have_independent_decode_and_encode_oracles() {
    use morphiecore::protocol::openai::responses;
    let expected = json!({"input":[
        {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"},
        {"type":"function_call_output","call_id":"c","output":[
            {"type":"input_text","text":"caption"},
            {"type":"input_image","image_url":"https://example.invalid/a.png","detail":"low"},
            {"type":"input_text","text":""},
            {"type":"input_image","image_url":"data:image/png;base64,AQID"}
        ]}
    ]});
    let decoded = responses::decode_generation(&expected).unwrap();
    let Item::ToolResult(result) = &decoded.semantic.items()[1].1 else {
        panic!("result")
    };
    let ToolOutput::Parts(parts) = &result.output else {
        panic!("parts")
    };
    assert_eq!(parts.len(), 4);
    assert!(
        matches!(&parts[1].1, ToolResultPart::Resource(r) if r.kind() == ResourceKind::Image && r.image_detail() == Some(ImageDetail::Low))
    );
    assert!(matches!(&parts[2].1, ToolResultPart::Text(t) if t.as_str().is_empty()));
    assert!(
        matches!(&parts[3].1, ToolResultPart::Resource(r) if r.image_detail().is_none() && matches!(&r.location, ResourceLocation::Inline { data_base64, .. } if data_base64.as_str() == "AQID"))
    );
    let mut url = image("https://example.invalid/a.png");
    url.description = ResourceDescription::Image {
        detail: Some(ImageDetail::Low),
    };
    let inline = Resource {
        location: ResourceLocation::Inline {
            media_type: text("image/png"),
            data_base64: text("AQID"),
        },
        description: ResourceDescription::Image { detail: None },
    };
    let independent = request(
        ToolOutput::Parts(vec![
            (PartId::new(30), ToolResultPart::Text(text("caption"))),
            (PartId::new(31), ToolResultPart::Resource(url)),
            (
                PartId::new(32),
                ToolResultPart::Text(
                    morphiecore::semantic::value::Text::allowing_empty("", "synthetic", 10)
                        .unwrap(),
                ),
            ),
            (PartId::new(33), ToolResultPart::Resource(inline)),
        ]),
        None,
    )
    .unwrap();
    let fidelity = FidelityRecords::default();
    let target = lower_request(
        &independent,
        &fidelity,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    assert_eq!(responses::encode_generation(&target).unwrap(), expected);
    assert_eq!(
        lower_request(&independent, &fidelity, Profile::Chat, Contract::full()).err(),
        Some(RepresentationError::Tools)
    );
    let q = GenerationRequirements::derive(&independent);
    assert_eq!(q.tool_result_images, 2);
    assert_eq!(q.image_inputs, 0);
    assert!(
        GenerationSemanticContract::text_images()
            .check(&independent)
            .is_err()
    );
    let mut contract = Contract::full();
    contract.images.max_images = 1;
    assert_eq!(
        lower_request(&independent, &fidelity, Profile::Responses, contract).err(),
        Some(RepresentationError::ImageInput)
    );
}

#[test]
fn tool_image_decode_rejects_wrong_branches_and_unsafe_resources() {
    use morphiecore::protocol::openai::responses;
    for part in [
        json!({"type":"input_image","image_url":"https://example.invalid/a.png","text":"cross-kind"}),
        json!({"type":"input_text","text":"caption","image_url":"https://example.invalid/a.png"}),
        json!({"type":"input_image","file_id":"issuer-file"}),
        json!({"type":"input_file","file_id":"issuer-file"}),
        json!({"type":"input_image","image_url":"https://example.invalid/a.png","detail":null}),
        json!({"type":"input_image","image_url":"https://user:secret@example.invalid/a.png"}),
        json!({"type":"input_image","image_url":"file:///etc/passwd"}),
        json!({"type":"input_image","image_url":"data:image/png;base64,???"}),
        json!({"type":"input_image","image_url":"https://example.invalid/a.png","prompt_cache_breakpoint":{"mode":"explicit"}}),
    ] {
        let wire = json!({"input":[
            {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"},
            {"type":"function_call_output","call_id":"c","output":[part]}
        ]});
        assert!(responses::decode_generation(&wire).is_err());
    }
}

#[test]
fn custom_image_results_share_the_carrier_without_becoming_function_results() {
    use morphiecore::protocol::openai::responses;
    let expected = json!({"input":[
        {"type":"custom_tool_call","call_id":"c","name":"lookup","input":"query"},
        {"type":"custom_tool_call_output","call_id":"c","output":[{"type":"input_image","image_url":"https://example.invalid/a.png"}]}
    ]});
    let decoded = responses::decode_generation(&expected).unwrap();
    assert!(
        matches!(&decoded.semantic.items()[1].1, Item::CustomResult(r) if matches!(&r.output, ToolOutput::Parts(parts) if matches!(&parts[0].1, ToolResultPart::Resource(_))))
    );
    let independent = GenerationRequest::new(
        vec![
            (
                ItemId::new(20),
                Item::CustomCall(CustomCall {
                    call_id: text("c"),
                    name: text("lookup"),
                    input: "query".into(),
                    context: CallContext::default(),
                }),
            ),
            (
                ItemId::new(21),
                Item::CustomResult(ToolResult {
                    execution: None,
                    call_id: text("c"),
                    output: ToolOutput::Parts(vec![(
                        PartId::new(40),
                        ToolResultPart::Resource(image("https://example.invalid/a.png")),
                    )]),
                    status: None,
                    context: CallContext::default(),
                }),
            ),
        ],
        GenerationControls::default(),
    )
    .unwrap();
    let fidelity = FidelityRecords::default();
    let target = lower_request(
        &independent,
        &fidelity,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    assert_eq!(responses::encode_generation(&target).unwrap(), expected);
    let mut wrong_kind = expected;
    wrong_kind["input"][1]["type"] = json!("function_call_output");
    assert!(responses::decode_generation(&wrong_kind).is_err());
}

#[test]
fn tool_and_user_images_share_target_limits_and_edits_remove_media() {
    use morphiecore::protocol::openai::responses;
    let mut items = request(media("https://example.invalid/a.png"), None)
        .unwrap()
        .items()
        .to_vec();
    items.insert(
        0,
        (
            ItemId::new(9),
            Item::Message(Message {
                role: MessageRole::User,
                status: ItemLifecycle::Completed,
                phase: None,
                parts: vec![Part {
                    id: PartId::new(90),
                    content: ContentPart::Resource(image("https://example.invalid/user.png")),
                }],
            }),
        ),
    );
    let history = GenerationRequest::new(items, GenerationControls::default()).unwrap();
    let fidelity = FidelityRecords::default();
    let mut contract = Contract::full();
    contract.images.max_images = 1;
    assert_eq!(
        lower_request(&history, &fidelity, Profile::Responses, contract).err(),
        Some(RepresentationError::ImageInput)
    );
    let mut contract = Contract::full();
    contract.images.urls = false;
    assert_eq!(
        lower_request(&history, &fidelity, Profile::Responses, contract).err(),
        Some(RepresentationError::ImageInput)
    );
    let inline = request(
        ToolOutput::Parts(vec![(
            PartId::new(10),
            ToolResultPart::Resource(Resource {
                description: ResourceDescription::Image {
                    detail: Some(ImageDetail::Low),
                },
                location: ResourceLocation::Inline {
                    media_type: text("image/png"),
                    data_base64: text("AQID"),
                },
            }),
        )]),
        None,
    )
    .unwrap();
    for limit in 0..3 {
        let mut contract = Contract::full();
        match limit {
            0 => contract.images.max_inline_bytes = 2,
            1 => contract.images.inline_formats.clear(),
            _ => contract.images.details.clear(),
        }
        assert_eq!(
            lower_request(&inline, &fidelity, Profile::Responses, contract).err(),
            Some(RepresentationError::ImageInput)
        );
    }
    let mut breakpoint = FidelityRecords::default();
    breakpoint.record_cache_breakpoint(PartId::new(10)).unwrap();
    assert_eq!(
        lower_request(&inline, &breakpoint, Profile::Responses, Contract::full()).err(),
        Some(RepresentationError::Controls)
    );
    let mut items = history.items().to_vec();
    let Item::ToolResult(result) = &mut items[2].1 else {
        panic!("result")
    };
    let ToolOutput::Parts(parts) = &mut result.output else {
        panic!("parts")
    };
    parts.retain(|(_, p)| matches!(p, ToolResultPart::Text(_)));
    let edited = history.with_items(items).unwrap();
    assert_eq!(
        GenerationRequirements::derive(&edited).tool_result_images,
        0
    );
    let target = lower_request(&edited, &fidelity, Profile::Responses, Contract::full()).unwrap();
    assert_eq!(
        responses::encode_generation(&target).unwrap()["input"][2]["output"],
        json!([{"type":"input_text","text":"caption"}])
    );
}

#[test]
fn result_history_requires_payload_and_call_identity_but_preserves_explicit_empty_values() {
    use morphiecore::adapter::{Adapter, Dialect};
    let adapter = Adapter::new(Profile::Responses, Dialect::Standard, None);
    for custom in [false, true] {
        let call = if custom {
            json!({"type":"custom_tool_call","call_id":"c","name":"lookup","input":"query"})
        } else {
            json!({"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"})
        };
        let result = json!({"type":if custom { "custom_tool_call_output" } else { "function_call_output" },"call_id":"c","output":""});
        for key in ["call_id", "output"] {
            for invalid in [None, Some(json!(null)), Some(json!(false))] {
                let mut broken = result.clone();
                if let Some(value) = invalid {
                    broken[key] = value;
                } else {
                    broken.as_object_mut().unwrap().shift_remove(key);
                }
                let request = json!({"model":"m","input":[call.clone(),broken]});
                assert!(
                    adapter
                        .decode_request(request.to_string().as_bytes())
                        .is_err(),
                    "{custom} {key}"
                );
            }
        }
        for payload in [json!(""), json!([])] {
            let mut result = result.clone();
            result["output"] = payload.clone();
            let expected = json!({"model":"m","input":[call.clone(),result],"store":false});
            let decoded = adapter
                .decode_request(expected.to_string().as_bytes())
                .unwrap();
            let r = match &decoded.task.semantic.items()[1].1 {
                Item::ToolResult(r) | Item::CustomResult(r) => r,
                _ => panic!("result"),
            };
            assert!(match &r.output {
                ToolOutput::Text(text) => payload.is_string() && text.is_empty(),
                ToolOutput::Parts(parts) => payload.is_array() && parts.is_empty(),
                _ => false,
            });
            assert_eq!(
                adapter
                    .encode_request(&decoded, "m", &Contract::full())
                    .unwrap(),
                expected
            );
        }
    }
}

#[test]
fn replacing_inserting_reordering_and_deleting_tool_parts_never_revives_old_media() {
    use morphiecore::protocol::openai::responses;
    let source = responses::decode_generation(&json!({"input":[
        {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"},
        {"type":"function_call_output","call_id":"c","output":[
            {"type":"input_text","text":"old"},
            {"type":"input_image","image_url":"https://example.invalid/old.png","detail":"auto"}
        ]}
    ]}))
    .unwrap();
    let mut items = source.semantic.items().to_vec();
    let Item::ToolResult(result) = &mut items[1].1 else {
        panic!("result")
    };
    let ToolOutput::Parts(parts) = &mut result.output else {
        panic!("parts")
    };
    let old_owner = parts[1].0;
    parts[1].1 = ToolResultPart::Resource(image("https://example.invalid/new.png"));
    parts[0].1 = ToolResultPart::Text(text("new caption"));
    parts.swap(0, 1);
    parts.insert(1, (PartId::new(90), ToolResultPart::Text(text("inserted"))));
    assert_eq!(parts[0].0, old_owner);
    let edited = source.semantic.with_items(items).unwrap();
    let target = lower_request(
        &edited,
        &source.fidelity,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    assert_eq!(
        responses::encode_generation(&target).unwrap()["input"][1]["output"],
        json!([
            {"type":"input_image","image_url":"https://example.invalid/new.png"},
            {"type":"input_text","text":"inserted"},
            {"type":"input_text","text":"new caption"}
        ])
    );
    let removed = edited
        .retain_items(|_, item| !matches!(item, Item::ToolResult(_)))
        .unwrap();
    let target = lower_request(
        &removed,
        &source.fidelity,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    assert_eq!(
        responses::encode_generation(&target).unwrap()["input"],
        json!([
            {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"}
        ])
    );
}

#[test]
fn structured_and_error_results_fail_lowering_without_a_carrier() {
    let outputs = [
        (
            ToolOutput::Structured(StructuredValue::new(json!({"answer":42})).unwrap()),
            None,
        ),
        (
            ToolOutput::Text("failed".into()),
            Some(ToolExecution::Failed { code: None }),
        ),
    ];
    for (output, execution) in outputs {
        let history = request_with_execution(output, None, execution).unwrap();
        let fidelity = FidelityRecords::default();
        for profile in [Profile::Chat, Profile::Responses] {
            assert_eq!(
                lower_request(&history, &fidelity, profile, Contract::full()).err(),
                Some(RepresentationError::Tools)
            );
        }
        assert!(
            GenerationSemanticContract::text_images()
                .check(&history)
                .is_err()
        );
    }
}
