//! Compacted context is ordered history, not readable reasoning or disposable replay.
use crate::events_support::*;
use morphiecore::{
    execution::replay::{CLIENT_ORIGIN, open_from_client, seal_event, seal_for_client},
    protocol::{
        anthropic::{client::encode_client_response, client_events::ClientEventEncoder},
        fidelity::FidelityRecords,
        openai::{
            Adapter, Dialect, Profile, TargetCaps,
            events::{EventDecoder, EventEncoder},
            lowering::{lower_request, lower_response},
            responses,
        },
        replay_envelope,
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
    let requirements = GenerationRequirements::derive(&decoded.semantic);
    assert!(requirements.compaction);
    assert!(!requirements.reasoning);
    let mut fidelity = decoded.fidelity.clone();
    fidelity.bind_replay_origin(&origin()).unwrap();
    let target =
        lower_request(&decoded.semantic, &fidelity, Profile::Responses, contract()).unwrap();
    let wire = responses::encode_generation(&target).unwrap();
    assert_eq!(wire["input"][1], item);
    assert_eq!(wire["input"][0]["content"][0]["text"], "before");
    assert_eq!(wire["input"][2]["content"][0]["text"], "after");
    assert!(lower_request(&decoded.semantic, &fidelity, Profile::Chat, contract()).is_err());

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
        Profile::Responses,
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
        &lower_request(&history, &records, Profile::Responses, contract()).unwrap(),
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
    assert!(
        GenerationSemanticContract::text_images()
            .check(&history)
            .is_err()
    );
}

#[test]
fn compaction_stream_requires_a_matching_closed_snapshot() {
    let item = json!({"type":"compaction","id":"cmp_1","encrypted_content":"synthetic-context"});
    let opener = json!({"type":"compaction","id":"cmp_1","encrypted_content":""});
    let mut decoder = EventDecoder::new(Profile::Responses).with_replay_origin(origin());
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
    let wire = encode(&events, Profile::Responses, &result.fidelity);
    assert_eq!(wire[2]["item"], opener);
    assert_eq!(wire[3]["item"], item);
    assert_eq!(wire[4]["response"]["output"], json!([item]));

    let mut client_caps = contract();
    client_caps.replay_origin = Some(CLIENT_ORIGIN.clone());
    let mut client = EventEncoder::new(Profile::Responses, metadata())
        .unwrap()
        .with_contract(client_caps);
    let sealed: Vec<_> = events
        .iter()
        .cloned()
        .flat_map(|event| {
            client
                .encode(
                    &seal_event(event, &CLIENT_ORIGIN).unwrap(),
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
    let mut chat = EventEncoder::new(Profile::Chat, metadata())
        .unwrap()
        .with_contract(contract());
    chat.encode(&events[0], &result.fidelity).unwrap();
    assert!(chat.encode(&events[1], &result.fidelity).is_err());
    let mut messages = ClientEventEncoder::new("r", "synthetic");
    messages.encode(&events[0]).unwrap();
    assert!(messages.encode(&events[1]).is_err());

    let mut truncated = EventDecoder::new(Profile::Responses).with_replay_origin(origin());
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
    let upstream = Adapter::new(Profile::Responses, Dialect::Standard, Some(origin()));
    let client = Adapter::new(
        Profile::Responses,
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
            Profile::Chat,
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
    let adapter = Adapter::new(Profile::Responses, Dialect::Standard, Some(foreign.clone()));
    let caps = TargetCaps {
        replay_origin: Some(foreign),
        ..TargetCaps::full()
    };
    assert!(
        adapter
            .encode_request(&request, "other-model", &caps)
            .is_err()
    );
    assert!(
        upstream
            .encode_request(&request, "synthetic", &TargetCaps::full())
            .is_err()
    );
    assert_eq!(request, saved);
    for caps in [
        morphiecore::protocol::anthropic::Profile::AdaptiveTextTools.semantic_contract(),
        morphiecore::protocol::google::semantic_contract(),
        morphiecore::protocol::interactions::semantic_contract(),
    ] {
        assert_eq!(
            caps.check(&request.task.semantic).unwrap_err(),
            GenerationFeature::Compaction
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
            Profile::Responses,
            contract()
        )
        .err(),
        Some(morphiecore::lowering::generation::RepresentationError::ReplayPhase),
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
