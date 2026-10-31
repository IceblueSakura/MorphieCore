//! Compacted context is ordered history, not readable reasoning or disposable replay.
use crate::events_support::*;
use morphiecore::lowering::disposition::Dispositions;
use morphiecore::{
    execution::replay::{open_from_client, seal_event, seal_for_client},
    protocol::{
        anthropic::messages::client::{encode_client_response, sse::ClientEventEncoder},
        fidelity::FidelityRecords,
        openai::{
            Dialect, GenerationApi, GenerationCodec, TargetContract,
            events::{EventDecoder, EventEncoder},
            lowering::{lower_request, lower_response},
            responses,
        },
        replay_envelope::{self, CLIENT_ORIGIN},
    },
    semantic::task::generation::*,
};
use serde_json::json;

fn content() -> ReplayValue {
    ReplayValue::final_value(ReplayFormat::ResponsesCompaction, text("synthetic-context"))
}

#[test]
fn compaction_history_keeps_identity_payload_and_order() {
    let item = json!({"type":"compaction","id":"cmp_1","encrypted_content":"synthetic-context"});
    let decoded = responses::decode_generation(&json!({"input":[
        {"role":"user","content":"before"}, item, {"role":"user","content":"after"}
    ]}))
    .unwrap();
    assert_eq!(decoded.semantic.items()[1].1, Item::Compaction(content()));
    let requirements = (decoded.semantic).features();
    assert!(
        requirements.contains_key(&morphiecore::semantic::task::generation::Feature::Compaction)
    );
    assert!(
        !requirements.contains_key(&morphiecore::semantic::task::generation::Feature::Reasoning)
    );
    let mut fidelity = decoded.fidelity.clone();
    fidelity.bind_replay_origin(&origin()).unwrap();
    let target = lower_request(
        &decoded.semantic,
        &fidelity,
        GenerationApi::Responses,
        contract(),
    )
    .unwrap();
    let wire = responses::encode_generation(&target).unwrap();
    assert_eq!(wire["input"][1], item);
    assert_eq!(wire["input"][0]["content"][0]["text"], "before");
    assert_eq!(wire["input"][2]["content"][0]["text"], "after");
    assert!(
        lower_request(
            &decoded.semantic,
            &fidelity,
            GenerationApi::Chat,
            contract()
        )
        .is_err()
    );

    // Independent IR -> wire oracle, also selected into full request history.
    let id = ItemId::new(7);
    let response =
        GenerationResponse::new(vec![(id, Item::Compaction(content()))], Outcome::Completed)
            .unwrap();
    let mut records = FidelityRecords::default();
    records.record_response_item_id(id, "cmp_1").unwrap();
    records
        .record_compaction(id, &content(), Some(origin()))
        .unwrap();
    let metadata = metadata();
    let target = lower_response(
        &response,
        &records,
        &metadata,
        GenerationApi::Responses,
        contract(),
    )
    .unwrap();
    assert_eq!(
        responses::encode_response(&target).unwrap()["output"],
        json!([item])
    );
    let history = ClientManaged::new(GenerationSettings::default())
        .unwrap()
        .select_response(&response)
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(history.items(), response.items());
    let wire = responses::encode_generation(
        &lower_request(&history, &records, GenerationApi::Responses, contract()).unwrap(),
    )
    .unwrap();
    assert_eq!(wire["input"], json!([item]));
    let decoded_response =
        responses::decode_response(&envelope("completed", json!([item]))).unwrap();
    assert_eq!(
        decoded_response.semantic.items()[0].1,
        Item::Compaction(content())
    );
    assert_eq!(
        decoded_response
            .fidelity
            .response_item_id(decoded_response.semantic.items()[0].0),
        Some("cmp_1")
    );

    // Missing or mistyped critical content cannot be treated as an unknown field.
    for invalid in [
        json!({"type":"compaction","id":"cmp_1"}),
        json!({"type":"compaction","encrypted_content":false}),
    ] {
        assert!(responses::decode_generation(&json!({"input":[invalid]})).is_err());
        assert!(responses::decode_response(&envelope("completed", json!([invalid]))).is_err());
    }
    let idless = json!({"type":"compaction","encrypted_content":"synthetic-context"});
    assert!(responses::decode_generation(&json!({"input":[idless]})).is_ok());
    assert!(responses::decode_response(&envelope("completed", json!([idless]))).is_err());
    assert!(Dispositions::text_images().check(&history).is_err());
}

#[test]
fn compaction_stream_requires_a_matching_closed_snapshot() {
    let item = json!({"type":"compaction","id":"cmp_1","encrypted_content":"synthetic-context"});
    let opener = json!({"type":"compaction","id":"cmp_1","encrypted_content":""});
    let mut decoder = EventDecoder::new(GenerationApi::Responses).with_replay_origin(origin());
    decoder.push(&created()).unwrap();
    let opened = decoder
        .push(&json!({"type":"response.output_item.added","output_index":0,"item":opener}))
        .unwrap();
    let closed = decoder
        .push(&json!({"type":"response.output_item.done","output_index":0,"item":item}))
        .unwrap();
    decoder
        .push(&json!({"type":"response.completed","response":envelope("completed",json!([item]))}))
        .unwrap();
    decoder.finish().unwrap();
    let result = decoder.materialize().unwrap();
    let id = result.semantic.items()[0].0;
    assert_eq!(
        result.semantic.items(),
        &[(id, Item::Compaction(content()))]
    );
    let events = vec![
        StreamEvent::Started,
        StreamEvent::ItemStarted {
            item: id,
            kind: ItemKind::Compaction,
            replay: Some(ItemReplay {
                value: ReplayValue::partial(
                    ReplayFormat::ResponsesCompaction,
                    morphiecore::semantic::value::Text::allowing_empty(
                        "",
                        "synthetic",
                        MAX_TEXT_BYTES,
                    )
                    .unwrap(),
                ),
                origin: Some(origin()),
            }),
        },
        StreamEvent::ItemFinished {
            item: id,
            status: ItemLifecycle::Completed,
            replay: Some(ItemReplay {
                value: content(),
                origin: Some(origin()),
            }),
        },
        terminal(StreamTerminal::Completed),
    ];
    assert_eq!(opened, events[1..2]);
    assert_eq!(closed, events[2..3]);
    let wire = encode(&events, GenerationApi::Responses, &result.fidelity);
    assert_eq!(wire[2]["item"], opener);
    assert_eq!(wire[3]["item"], item);
    assert_eq!(wire[4]["response"]["output"], json!([item]));

    let mut client_caps = contract();
    client_caps.replay_origin = Some(CLIENT_ORIGIN.clone());
    let mut client = EventEncoder::new(GenerationApi::Responses, metadata())
        .unwrap()
        .with_contract(client_caps);
    let sealed: Vec<_> = events
        .iter()
        .cloned()
        .flat_map(|event| {
            client
                .encode(
                    &seal_event(event, &CLIENT_ORIGIN, &result.fidelity).unwrap(),
                    &result.fidelity,
                )
                .unwrap()
        })
        .collect();
    client.finish().unwrap();
    let (_, format, payload) =
        replay_envelope::open(sealed[3]["item"]["encrypted_content"].as_str().unwrap()).unwrap();
    assert_eq!(format, ReplayFormat::ResponsesCompaction);
    assert_eq!(payload.as_str(), "synthetic-context");
    assert_eq!(sealed[3]["item"], sealed[4]["response"]["output"][0]);
    let mut chat = EventEncoder::new(GenerationApi::Chat, metadata())
        .unwrap()
        .with_contract(contract());
    chat.encode(&events[0], &result.fidelity).unwrap();
    assert!(chat.encode(&events[1], &result.fidelity).is_err());
    let mut messages = ClientEventEncoder::new("r", "synthetic");
    messages.encode(&events[0]).unwrap();
    assert!(messages.encode(&events[1]).is_err());

    let mut truncated = EventDecoder::new(GenerationApi::Responses).with_replay_origin(origin());
    truncated.push(&created()).unwrap();
    truncated
        .push(&json!({"type":"response.output_item.added","output_index":0,"item":opener}))
        .unwrap();
    assert!(truncated.finish().is_err());
    assert!(
        truncated
            .push(
                &json!({"type":"response.completed","response":envelope("completed",json!([item]))})
            )
            .is_err()
    );
}

#[test]
fn compaction_replay_rejects_incompatible_or_unbound_context_without_stripping() {
    let upstream =
        GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, Some(origin()));
    let client = GenerationCodec::new(
        GenerationApi::Responses,
        Dialect::Standard,
        Some(CLIENT_ORIGIN.clone()),
    );
    let item = json!({"type":"compaction","id":"cmp_1","encrypted_content":"synthetic-context"});
    let mut unbound = responses::decode_response(&envelope("completed", json!([item]))).unwrap();
    assert!(seal_for_client(&mut unbound, &CLIENT_ORIGIN).is_err());
    let mut response = upstream
        .decode_response(envelope("completed", json!([item])).to_string().as_bytes())
        .unwrap();
    seal_for_client(&mut response, &CLIENT_ORIGIN).unwrap();
    let mut client_caps = contract();
    client_caps.replay_origin = Some(CLIENT_ORIGIN.clone());
    let delivered = client.encode_response(&response, &client_caps).unwrap();
    assert!(encode_client_response(&response, "synthetic").is_err());
    assert!(
        lower_response(
            &response.semantic,
            &response.fidelity,
            &response.metadata,
            GenerationApi::Chat,
            client_caps
        )
        .is_err()
    );
    let body = json!({"model":"synthetic","input":[delivered["output"][0],{"role":"user","content":"next"}]});
    let mut request = client.decode_request(body.to_string().as_bytes()).unwrap();
    open_from_client(&mut request).unwrap();
    let saved = request.clone();
    let wire = upstream
        .encode_request(&request, "synthetic", &contract())
        .unwrap();
    assert_eq!(wire["input"][0], item);
    assert_eq!(request, saved);

    let foreign = replay_envelope::issuer("other-provider", "other-model");
    let adapter = GenerationCodec::new(
        GenerationApi::Responses,
        Dialect::Standard,
        Some(foreign.clone()),
    );
    let caps = TargetContract {
        replay_origin: Some(foreign),
        ..TargetContract::full()
    };
    assert!(
        adapter
            .encode_request(&request, "other-model", &caps)
            .is_err()
    );
    assert!(
        upstream
            .encode_request(&request, "synthetic", &TargetContract::full())
            .is_err()
    );
    assert_eq!(request, saved);
    for caps in [
        morphiecore::protocol::anthropic::messages::Profile::AdaptiveTextTools.semantic_contract(),
        morphiecore::protocol::google::generate_content::semantic_contract(),
        morphiecore::protocol::google::interactions::semantic_contract(),
    ] {
        assert_eq!(
            caps.check(&request.task.semantic).unwrap_err(),
            Feature::Compaction
        );
    }
    let mut changed = request.task.semantic.items().to_vec();
    changed[0].1 = Item::Compaction(ReplayValue::final_value(
        ReplayFormat::ResponsesCompaction,
        text("changed"),
    ));
    request.task.semantic = request.task.semantic.clone().with_items(changed).unwrap();
    assert!(
        upstream
            .encode_request(&request, "synthetic", &contract())
            .is_err()
    );

    for invalid in ["bare-context", "mc1.invalid", "mc2.future"] {
        let mut body = body.clone();
        body["input"][0]["encrypted_content"] = json!(invalid);
        let mut request = client.decode_request(body.to_string().as_bytes()).unwrap();
        let saved = request.clone();
        assert!(open_from_client(&mut request).is_err());
        assert_eq!(request, saved);
    }
    let partial = ReplayValue::partial(ReplayFormat::ResponsesCompaction, text("partial"));
    assert!(
        GenerationRequest::new(
            vec![(ItemId::new(1), Item::Compaction(partial.clone()))],
            Default::default()
        )
        .is_err()
    );
    let incomplete = GenerationResponse::new(
        vec![(ItemId::new(1), Item::Compaction(partial.clone()))],
        Outcome::Incomplete,
    )
    .unwrap();
    let mut fidelity = FidelityRecords::default();
    fidelity
        .record_compaction(ItemId::new(1), &partial, Some(origin()))
        .unwrap();
    assert_eq!(
        lower_response(
            &incomplete,
            &fidelity,
            &metadata(),
            GenerationApi::Responses,
            contract()
        )
        .err(),
        Some(morphiecore::protocol::conversion::generation::RepresentationError::ReplayPhase),
    );
    assert!(!format!("{:?}", content()).contains("synthetic-context"));
    let oversized = morphiecore::semantic::value::Text::new(
        "x".repeat(MAX_TEXT_BYTES + 1),
        "synthetic",
        MAX_TEXT_BYTES + 1,
    )
    .unwrap();
    assert!(
        GenerationRequest::new(
            vec![(
                ItemId::new(1),
                Item::Compaction(ReplayValue::final_value(
                    ReplayFormat::ResponsesCompaction,
                    oversized
                ),)
            )],
            Default::default()
        )
        .is_err()
    );
}
#[test]
fn compaction_progress_and_output_creator_are_not_execution_or_request_authority() {
    let opener =
        json!({"type":"compaction","id":"cmp_1","encrypted_content":"","created_by":"actor"});
    let done = json!({"type":"compaction","id":"cmp_1","encrypted_content":"synthetic-context","created_by":"actor"});
    let mut decoder = EventDecoder::new(GenerationApi::Responses).with_replay_origin(origin());
    decoder.push(&created()).unwrap();
    let opened = decoder
        .push(&json!({"type":"response.output_item.added","output_index":0,"item":opener}))
        .unwrap();
    let id = match opened[0] {
        StreamEvent::ItemStarted { item, .. } => item,
        _ => panic!("compaction owner"),
    };
    for _ in 0..2 {
        assert_eq!(decoder.push(&json!({"type":"response.compaction.compacting","output_index":0,"item_id":"cmp_1"})).unwrap(),
            vec![StreamEvent::CompactionProgress{item:id}]);
    }
    decoder
        .push(&json!({"type":"response.output_item.done","output_index":0,"item":done}))
        .unwrap();
    decoder
        .push(&json!({"type":"response.completed","response":envelope("completed",json!([done]))}))
        .unwrap();
    decoder.finish().unwrap();
    let response = decoder.materialize().unwrap();
    assert_eq!(response.semantic.items()[0].1, Item::Compaction(content()));
    let (owner, item) = &response.semantic.items()[0];
    assert_eq!(
        response
            .fidelity
            .created_by(*owner, item, response.semantic.resources()),
        Some("actor")
    );
    // Independent typed event -> wire expectation. Progress remains repeatable,
    // content-free and ordered; no output snapshot is invented from it.
    let mut encoder = EventEncoder::new(GenerationApi::Responses, metadata())
        .unwrap()
        .with_contract(contract());
    encoder
        .encode(&StreamEvent::Started, &FidelityRecords::default())
        .unwrap();
    let start = StreamEvent::ItemStarted {
        item: ItemId::new(9),
        kind: ItemKind::Compaction,
        replay: Some(ItemReplay {
            origin: Some(origin()),
            value: ReplayValue::partial(
                ReplayFormat::ResponsesCompaction,
                morphiecore::semantic::value::Text::allowing_empty("", "test", 8).unwrap(),
            ),
        }),
    };
    let mut records = FidelityRecords::default();
    records
        .record_response_item_id(ItemId::new(9), "cmp_1")
        .unwrap();
    encoder.encode(&start, &records).unwrap();
    for sequence in [3, 4] {
        let event = encoder
            .encode(
                &StreamEvent::CompactionProgress {
                    item: ItemId::new(9),
                },
                &records,
            )
            .unwrap();
        assert_eq!(
            event,
            json!([{"type":"response.compaction.compacting","output_index":0,
            "item_id":"cmp_1","sequence_number":sequence}])
            .as_array()
            .unwrap()
            .clone()
        );
    }
    assert!(
        encoder
            .encode(
                &StreamEvent::CompactionProgress {
                    item: ItemId::new(99)
                },
                &records
            )
            .is_err()
    );
    // Output reports do not become official request-history fields.
    let request = GenerationRequest::from_settings(
        response.semantic.items().to_vec(),
        GenerationSettings::default(),
    )
    .unwrap();
    let wire = responses::encode_generation(
        &lower_request(
            &request,
            &response.fidelity,
            GenerationApi::Responses,
            contract(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(wire["input"][0].get("created_by").is_none());
    let changed = Item::Compaction(ReplayValue::final_value(
        ReplayFormat::ResponsesCompaction,
        text("edited"),
    ));
    assert_eq!(
        response
            .fidelity
            .created_by(*owner, &changed, response.semantic.resources()),
        None
    );
}
