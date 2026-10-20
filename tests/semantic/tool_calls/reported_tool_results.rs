use crate::events_support::{
    apply, close, created, encode, envelope, metadata, start, terminal, text,
};
use morphiecore::{
    protocol::{
        anthropic::{client::encode_client_response, client_events::ClientEventEncoder},
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
            "output":[{"type":"input_text","text":"caption"},
                {"type":"input_image","image_url":"https://example.invalid/result.png"}],
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
                        Part::new(PartId::new(1), InputPart::Text(text("caption"))),
                        Part::new(
                            PartId::new(2),
                            InputPart::Resource(crate::resources_support::image(1)),
                        ),
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
