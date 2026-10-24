use crate::events_support::{
    apply, close, created, encode, envelope, metadata, start, terminal, text,
};
use morphiecore::{
    protocol::{
        anthropic::messages::{client::encode_client_response, client::sse::ClientEventEncoder},
        fidelity::FidelityRecords,
        openai::{
            Profile, TargetCaps,
            events::{EventDecoder, EventEncoder},
            lowering::lower_response,
            responses,
        },
    },
    semantic::task::generation::*,
};
use serde_json::{Value, json};

fn result() -> Value {
    json!({"id":"out_1","type":"function_call_output","call_id":"c",
            "name":"lookup","output":"42","status":"completed"})
}
fn custom_result() -> Value {
    json!({"id":"out_2","type":"custom_tool_call_output","call_id":"custom",
            "output":[{"type":"input_text","text":"caption","prompt_cache_breakpoint":{"mode":"explicit"}},
                {"type":"input_image","image_url":"https://example.invalid/result.png","prompt_cache_breakpoint":{"mode":"explicit"}}],
            "status":"completed"})
}
fn semantic() -> GenerationResponse {
    let resources = ResourceTable::default()
        .insert(
            ResourceId::new(1),
            ResourceLocation::Url(text("https://example.invalid/result.png")),
        )
        .unwrap();
    GenerationResponse::from_resources(
        vec![
            (
                ItemId::new(1),
                Item::ToolResult(ToolResult {
                    call_id: Some(text("c")),
                    name: Some(text("lookup")),
                    cache: None,
                    output: "42".into(),
                    status: Some(ItemLifecycle::Completed),
                    is_error: None,
                    execution: None,
                    context: Default::default(),
                }),
            ),
            (
                ItemId::new(2),
                Item::CustomResult(ToolResult {
                    call_id: Some(text("custom")),
                    cache: None,
                    name: None,
                    output: ToolOutput::Parts(vec![
                        Part {
                            id: PartId::new(1),
                            content: InputPart::Text(text("caption")),
                            cache: Some(PrefixCacheBoundary::RequestLifetime),
                            replay: None,
                        },
                        Part {
                            id: PartId::new(2),
                            content: InputPart::Resource(crate::resources_support::image(1)),
                            cache: Some(PrefixCacheBoundary::RequestLifetime),
                            replay: None,
                        },
                    ]),
                    status: Some(ItemLifecycle::Completed),
                    is_error: None,
                    execution: None,
                    context: Default::default(),
                }),
            ),
        ],
        Outcome::Completed,
        resources,
    )
    .unwrap()
}
fn fidelity() -> FidelityRecords {
    let mut f = FidelityRecords::default();
    f.record_response_item_id(ItemId::new(1), "out_1").unwrap();
    f.record_response_item_id(ItemId::new(2), "out_2").unwrap();
    f
}

#[test]
fn reported_results_preserve_facts_and_reject_missing_downstream_carriers() {
    let output = json!([result(), custom_result()]);
    let decoded = responses::decode_response(&envelope("completed", output.clone())).unwrap();
    assert_eq!(decoded.semantic, semantic());
    // Encode independent IR, not the decoded response.
    let expected = semantic();
    let f = fidelity();
    let m = metadata();
    let target = lower_response(&expected, &f, &m, Profile::Responses, TargetCaps::full()).unwrap();
    assert_eq!(
        responses::encode_response(&target).unwrap()["output"],
        output
    );
    let mut no_media = TargetCaps::full();
    no_media.semantics.tool_result_media = false;
    assert!(lower_response(&expected, &f, &m, Profile::Responses, no_media).is_err());

    // Even a plain function result has no assistant-output carrier in these protocols.
    let decoded = responses::decode_response(&envelope("completed", json!([result()]))).unwrap();
    assert!(
        lower_response(
            &decoded.semantic,
            &decoded.fidelity,
            &decoded.metadata,
            Profile::Chat,
            TargetCaps::full()
        )
        .is_err()
    );
    assert!(encode_client_response(&decoded, "synthetic").is_err());

    let mut unlinked = result();
    unlinked.as_object_mut().unwrap().remove("call_id");
    let reported =
        responses::decode_response(&envelope("completed", json!([unlinked.clone()]))).unwrap();
    let Item::ToolResult(value) = &reported.semantic.items()[0].1 else {
        panic!("result")
    };
    assert_eq!(value.call_id, None);
    assert_eq!(value.name.as_ref().map(|n| n.as_str()), Some("lookup"));
    assert!(responses::decode_generation(&json!({"input":[unlinked.clone()]})).is_err());
    unlinked["status"] = json!("in_progress");
    assert!(responses::decode_response(&envelope("completed", json!([unlinked]))).is_err());

    let mut history = vec![
        (
            ItemId::new(3),
            Item::ToolCall(ToolCall {
                replay: None,
                call_id: text("c"),
                cache: None,
                name: text("lookup"),
                arguments: "{}".into(),
                status: ItemLifecycle::Completed,
                context: Default::default(),
            }),
        ),
        (
            ItemId::new(4),
            Item::CustomCall(CustomCall {
                call_id: text("custom"),
                name: text("run"),
                input: "raw".into(),
                context: Default::default(),
            }),
        ),
    ];
    history.extend_from_slice(expected.items());
    let request = GenerationRequest::from_resources(
        history,
        GenerationSettings::default(),
        expected.resources().clone(),
    )
    .unwrap();
    let target = morphiecore::protocol::openai::lowering::lower_request(
        &request,
        &f,
        Profile::Responses,
        TargetCaps::full(),
    )
    .unwrap();
    let replay = responses::encode_generation(&target).unwrap();
    assert_eq!(
        &replay["input"].as_array().unwrap()[2..],
        output.as_array().unwrap()
    );
    assert!(responses::decode_generation(&replay).is_ok());
}

#[test]
fn result_cache_snapshot_edits_are_semantic_and_native_ttls_reject_before_publication() {
    // Current pinned Responses output content has nullable breakpoint reports.
    let marked = custom_result();
    let mut cleared = marked.clone();
    for part in cleared["output"].as_array_mut().unwrap() {
        part["prompt_cache_breakpoint"] = Value::Null;
    }
    let decoded =
        responses::decode_response(&envelope("completed", json!([cleared.clone()]))).unwrap();
    assert!(!decoded.semantic.items()[0].1.has_cache_breakpoints());
    for (mut opening, closing, count) in [
        (marked.clone(), cleared.clone(), 0),
        (cleared, marked.clone(), 2),
        (marked.clone(), marked, 2),
    ] {
        let mut decoder = EventDecoder::new(Profile::Responses);
        opening["status"] = json!("in_progress");
        for event in [
            created(),
            json!({"type":"response.output_item.added","output_index":0,"item":opening}),
            json!({"type":"response.output_item.done","output_index":0,"item":closing.clone()}),
            json!({"type":"response.completed","response":envelope("completed", json!([closing.clone()]))}),
        ] {
            decoder.push(&event).unwrap();
        }
        let final_value = decoder.materialize().unwrap();
        assert_eq!(
            final_value.semantic.items()[0].1.cache_boundaries().count(),
            count
        );
        // Changed snapshots get fresh local identities; compare actual native output,
        // not unrelated static/stream allocation counters.
        let target = lower_response(
            &final_value.semantic,
            &final_value.fidelity,
            &final_value.metadata,
            Profile::Responses,
            TargetCaps::full(),
        )
        .unwrap();
        let mut expected = closing;
        for part in expected["output"].as_array_mut().unwrap() {
            if part["prompt_cache_breakpoint"].is_null() {
                part.as_object_mut()
                    .unwrap()
                    .remove("prompt_cache_breakpoint");
            }
        }
        assert_eq!(
            responses::encode_response(&target).unwrap()["output"],
            json!([expected])
        );
        assert!(final_value.semantic.resources().len() <= 2);
    }

    let Item::CustomResult(mut result) = semantic().items()[1].1.clone() else {
        unreachable!()
    };
    // A text-only independent result needs no resource declarations.
    result.output = ToolOutput::Parts(vec![Part {
        id: PartId::new(1),
        content: InputPart::Text(text("caption")),
        cache: Some(PrefixCacheBoundary::RequestLifetime),
        replay: None,
    }]);
    let mut outer_ttl = result.clone();
    outer_ttl.cache = Some(PrefixCacheTtl::FiveMinutes);
    assert_eq!(
        GenerationResponse::new(
            vec![(ItemId::new(1), Item::CustomResult(outer_ttl))],
            Outcome::Completed
        ),
        Err(GenerationError::InvalidResponse)
    );
    let mut native_ttl = result.clone();
    let ToolOutput::Parts(parts) = &mut native_ttl.output else {
        unreachable!()
    };
    parts[0].cache = Some(PrefixCacheBoundary::Ttl(PrefixCacheTtl::FiveMinutes));
    assert_eq!(
        GenerationResponse::new(
            vec![(ItemId::new(1), Item::CustomResult(native_ttl.clone()))],
            Outcome::Completed
        ),
        Err(GenerationError::InvalidResponse)
    );
    result.status = Some(ItemLifecycle::InProgress);
    native_ttl.status = Some(ItemLifecycle::InProgress);
    let started = reduce(StreamState::new(), StreamEvent::Started).unwrap();
    assert_eq!(
        reduce(
            started.clone(),
            start(1, ItemKind::CustomResult(native_ttl.clone()))
        ),
        Err(EventError::Semantic(GenerationError::InvalidResponse))
    );
    let state = reduce(started, start(1, ItemKind::CustomResult(result))).unwrap();
    assert_eq!(
        reduce(
            state.clone(),
            StreamEvent::ToolResultOutput {
                item: ItemId::new(1),
                output: native_ttl.output,
            }
        ),
        Err(EventError::Semantic(GenerationError::InvalidResponse))
    );
    let state = reduce(
        state,
        StreamEvent::ToolResultOutput {
            item: ItemId::new(1),
            output: ToolOutput::Parts(vec![Part::new(
                PartId::new(1),
                InputPart::Text(text("caption")),
            )]),
        },
    )
    .unwrap();
    let state = reduce(state, close(1, ItemLifecycle::Completed)).unwrap();
    let state = reduce(state, terminal(StreamTerminal::Completed)).unwrap();
    assert!(
        !materialize(&state).unwrap().items()[0]
            .1
            .has_cache_breakpoints()
    );
}

#[test]
fn result_stream_uses_item_snapshots_and_requires_honest_closure() {
    let final_item = result();
    let mut added = final_item.clone();
    added["status"] = json!("in_progress");
    added["output"] = json!("");
    let open = json!({"type":"response.output_item.added","output_index":0,"item":added});
    let done = json!({"type":"response.output_item.done","output_index":0,"item":final_item});
    let final_event = json!({"type":"response.completed",
            "response":envelope("completed", json!([result()]))});
    let mut decoder = EventDecoder::new(Profile::Responses);
    for event in [created(), open.clone(), done.clone(), final_event.clone()] {
        decoder.push(&event).unwrap();
    }
    let decoded = decoder.materialize().unwrap();
    let Item::ToolResult(reported) = &decoded.semantic.items()[0].1 else {
        panic!("result");
    };
    assert_eq!(reported.output, ToolOutput::Text("42".into()));
    assert_eq!(reported.status, Some(ItemLifecycle::Completed));

    let mut decoder = EventDecoder::new(Profile::Responses);
    decoder.push(&created()).unwrap();
    decoder.push(&open).unwrap();
    assert!(decoder.push(&final_event).is_err());
    let mut decoder = EventDecoder::new(Profile::Responses);
    decoder.push(&created()).unwrap();
    decoder.push(&open).unwrap();
    let mut changed = done;
    changed["item"]["call_id"] = json!("other");
    assert!(decoder.push(&changed).is_err());

    let expected = semantic();
    let mut events = vec![StreamEvent::Started];
    for (id, item) in expected.items() {
        let (Item::ToolResult(value) | Item::CustomResult(value)) = item else {
            unreachable!()
        };
        let mut initial = value.clone();
        initial.status = Some(ItemLifecycle::InProgress);
        let kind = if matches!(item, Item::CustomResult(_)) {
            events.push(StreamEvent::ResourceDeclared {
                id: ResourceId::new(1),
                resource: ResourceLocation::Url(text("https://example.invalid/result.png")),
            });
            ItemKind::CustomResult(initial)
        } else {
            initial.output = "".into();
            ItemKind::ToolResult(initial)
        };
        events.push(StreamEvent::ItemStarted {
            item: *id,
            kind,
            replay: None,
        });
        events.push(StreamEvent::ToolResultOutput {
            item: *id,
            output: value.output.clone(),
        });
        events.push(close(id.get(), ItemLifecycle::Completed));
    }
    events.push(terminal(StreamTerminal::Completed));
    assert_eq!(materialize(&apply(&events).unwrap()).unwrap(), expected);
    let wire = encode(&events, Profile::Responses, &fidelity());
    let finished: Vec<_> = wire
        .iter()
        .filter(|v| v["type"] == "response.output_item.done")
        .map(|v| v["item"].clone())
        .collect();
    assert_eq!(finished, vec![result(), custom_result()]);
    assert!(
        !wire
            .iter()
            .any(|v| v["type"].as_str().unwrap().ends_with(".delta"))
    );
    let mut decoder = EventDecoder::new(Profile::Responses);
    for event in wire {
        decoder.push(&event).unwrap();
    }
    assert_eq!(decoder.materialize().unwrap().semantic, expected);

    let starter = events[1].clone();
    let mut chat = EventEncoder::new(Profile::Chat, metadata()).unwrap();
    chat.encode(&StreamEvent::Started, &fidelity()).unwrap();
    assert!(chat.encode(&starter, &fidelity()).is_err());
    let mut messages = ClientEventEncoder::new("r", "synthetic");
    messages.encode(&StreamEvent::Started).unwrap();
    assert!(messages.encode(&starter).is_err());
    let closed = apply(&events[..events.len() - 1]).unwrap();
    assert!(
        reduce(
            closed.clone(),
            StreamEvent::ToolResultOutput {
                item: ItemId::new(1),
                output: "late".into(),
            }
        )
        .is_err()
    );
    let mut initial = match &expected.items()[0].1 {
        Item::ToolResult(r) => r.clone(),
        _ => unreachable!(),
    };
    initial.status = Some(ItemLifecycle::InProgress);
    assert!(reduce(closed, start(3, ItemKind::ToolResult(initial))).is_err());
}
