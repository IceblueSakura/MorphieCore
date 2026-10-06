//! Independent value/proof, mutation and finality oracles for Responses encrypted reasoning.
use crate::events_support::{contract, envelope, metadata, origin, text};
use morphiecore::{
    lowering::generation::{lower_request, lower_response},
    protocol::{
        fidelity::FidelityRecords,
        openai::{Profile, responses},
    },
    semantic::task::generation::*,
};
use serde_json::{Value, json};

fn wire(id: &str, token: &str, status: &str) -> Value {
    json!({"type":"reasoning","id":id,"status":status,"summary":[],"encrypted_content":token})
}
fn request(values: Value) -> morphiecore::protocol::openai::DecodedRequest {
    let mut decoded = responses::decode_generation(&json!({"input":values})).unwrap();
    decoded.fidelity.bind_replay_origin(&origin()).unwrap();
    decoded
}
fn reason(items: &mut [(ItemId, Item)], index: usize) -> &mut ReasoningItem {
    let Item::Reasoning(r) = &mut items[index].1 else {
        panic!("reasoning owner")
    };
    r
}
fn encode(r: &GenerationRequest, f: &FidelityRecords) -> Result<Value, ()> {
    let lowered = lower_request(r, f, Profile::Responses, contract()).map_err(|_| ())?;
    responses::encode_generation(&lowered).map_err(|_| ())
}

#[test]
fn opaque_value_is_semantic_and_debug_is_redacted() {
    let a = request(json!([wire("rs", "synthetic-secret-a", "completed")]));
    let b = request(json!([wire("rs", "synthetic-secret-b", "completed")]));
    assert_ne!(a.semantic, b.semantic);
    let debug = format!("{a:?}");
    assert!(!debug.contains("synthetic-secret-a"));
    let Item::Reasoning(r) = &a.semantic.items()[0].1 else {
        panic!("reasoning")
    };
    assert_eq!(
        r.replay.as_ref().unwrap().replay_token(),
        Some("synthetic-secret-a")
    );
    assert_eq!(
        encode(&a.semantic, &a.fidelity).unwrap()["input"][0]["encrypted_content"],
        "synthetic-secret-a"
    );
}

#[test]
fn deleting_value_never_restores_source_payload_and_removing_proof_never_removes_value() {
    let d = request(json!([wire("rs", "synthetic-old", "completed")]));
    let mut items = d.semantic.items().to_vec();
    reason(&mut items, 0).replay = None;
    let cleared = d.semantic.clone().with_items(items).unwrap();
    assert!(
        encode(&cleared, &d.fidelity).unwrap()["input"][0]
            .get("encrypted_content")
            .is_none()
    );
    let mut no_proof = d.fidelity.clone();
    no_proof.remove_replay(d.semantic.items()[0].0);
    assert!(encode(&d.semantic, &no_proof).is_err());
    assert!(encode(&d.semantic, &FidelityRecords::default()).is_err());
}

#[test]
fn replacements_and_owner_transplants_cannot_reuse_proofs() {
    let mut value = wire("rs", "synthetic-old", "completed");
    value["summary"] =
        json!([{"type":"summary_text","text":"first"},{"type":"summary_text","text":"second"}]);
    let d = request(json!([value]));
    for mutation in 0..6 {
        let mut items = d.semantic.items().to_vec();
        match mutation {
            0 => {
                reason(&mut items, 0).replay = Some(ReplayValue::final_value(
                    ReplayFormat::ResponsesEncrypted,
                    text("synthetic-new"),
                ))
            }
            1 => reason(&mut items, 0)
                .parts
                .push((PartId::new(17), ReasoningContent::Summary(text("inserted")))),
            2 => reason(&mut items, 0).status = ItemLifecycle::Incomplete,
            3 => items[0].0 = ItemId::new(97),
            4 => reason(&mut items, 0).parts.swap(0, 1),
            _ => reason(&mut items, 0).parts[0].0 = PartId::new(97),
        }
        let changed = d.semantic.clone().with_items(items).unwrap();
        assert!(encode(&changed, &d.fidelity).is_err());
        let mut rebound = d.fidelity.clone();
        rebound.bind_replay_origin(&origin()).unwrap();
        assert!(
            encode(&changed, &rebound).is_err(),
            "origin binding cannot recapture changed dependencies"
        );
    }
}

#[test]
fn independent_owner_reordering_and_deletion_keep_surviving_bindings() {
    let d = request(json!([
        wire("a", "synthetic-a", "completed"),
        wire("b", "synthetic-b", "completed")
    ]));
    let mut items = d.semantic.items().to_vec();
    items.swap(0, 1);
    let changed = d.semantic.clone().with_items(items).unwrap();
    let out = encode(&changed, &d.fidelity).unwrap();
    assert_eq!(
        out["input"][0],
        json!({"id":"b","type":"reasoning","summary":[],"encrypted_content":"synthetic-b"})
    );
    assert_eq!(out["input"][1]["encrypted_content"], "synthetic-a");
    let deleted = changed.retain_items(|id, _| id == ItemId::new(2)).unwrap();
    let out = encode(&deleted, &d.fidelity).unwrap();
    assert_eq!(out["input"].as_array().unwrap().len(), 1);
    assert_eq!(out["input"][0]["id"], "b");
    assert_eq!(out["input"][0]["encrypted_content"], "synthetic-b");
}

#[test]
fn partial_output_is_reportable_but_incomplete_owner_cannot_be_replayed() {
    for status in ["in_progress", "incomplete", "completed"] {
        let mut d = responses::decode_response(&envelope(
            "incomplete",
            json!([wire("rs", "synthetic-token", status)]),
        ))
        .unwrap();
        d.fidelity.bind_replay_origin(&origin()).unwrap();
        let m = metadata();
        let projected =
            lower_response(&d.semantic, &d.fidelity, &m, Profile::Responses, contract()).unwrap();
        assert_eq!(
            responses::encode_response(&projected).unwrap()["output"][0]["encrypted_content"],
            "synthetic-token"
        );
        let history =
            GenerationRequest::new(d.semantic.items().to_vec(), GenerationControls::default())
                .unwrap();
        assert_eq!(encode(&history, &d.fidelity).is_ok(), status == "completed");
    }
}

#[test]
fn independently_authored_ir_encodes_only_its_typed_value() {
    let r = ReasoningItem {
        parts: vec![],
        status: ItemLifecycle::Completed,
        replay: Some(ReplayValue::final_value(
            ReplayFormat::ResponsesEncrypted,
            text("synthetic-independent"),
        )),
    };
    let id = ItemId::new(17);
    let mut f = FidelityRecords::default();
    f.record_replay(id, &r, Some(origin())).unwrap();
    let semantic = GenerationRequest::new(
        vec![(id, Item::Reasoning(r))],
        GenerationControls::default(),
    )
    .unwrap();
    let out = encode(&semantic, &f).unwrap();
    assert_eq!(
        out["input"],
        json!([{"id":"item_17","type":"reasoning","summary":[],"encrypted_content":"synthetic-independent"}])
    );
}

#[test]
fn token_only_edits_invalidate_extras_even_when_debug_is_identical() {
    use morphiecore::adapter::{Adapter, Dialect};
    let adapter = Adapter::new(Profile::Responses, Dialect::DeepSeek, Some(origin()));
    let mut input = envelope(
        "completed",
        json!([wire("rs", "synthetic-old", "completed")]),
    );
    input["content_filters"] = json!({"result":"synthetic"});
    let mut d = adapter
        .decode_response(input.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        adapter.encode_response(&d, &contract()).unwrap()["content_filters"],
        input["content_filters"]
    );
    let debug = format!("{:?}", d.semantic);
    let mut items = d.semantic.items().to_vec();
    let id = items[0].0;
    let r = reason(&mut items, 0);
    r.replay = Some(ReplayValue::final_value(
        ReplayFormat::ResponsesEncrypted,
        text("synthetic-new"),
    ));
    // Simulate an independently admitted replacement value, not a new extras report.
    d.fidelity.record_replay(id, r, Some(origin())).unwrap();
    d.semantic = d.semantic.with_items(items).unwrap();
    assert_eq!(debug, format!("{:?}", d.semantic));
    let out = adapter.encode_response(&d, &contract()).unwrap();
    assert_eq!(out["output"][0]["encrypted_content"], "synthetic-new");
    assert!(out.get("content_filters").is_none());
}

#[test]
fn stream_snapshots_and_materialization_own_partial_final_and_removed_values() {
    use crate::events_support::{created, token};
    use morphiecore::protocol::openai::events::EventDecoder;
    for final_token in [Some("synthetic-final"), None] {
        let mut decoder = EventDecoder::new(Profile::Responses).with_replay_origin(origin());
        let mut state = StreamState::new();
        let mut final_item = wire("rs", "synthetic-final", "completed");
        if final_token.is_none() {
            final_item
                .as_object_mut()
                .unwrap()
                .remove("encrypted_content");
        }
        let frames = [
            created(),
            json!({"type":"response.output_item.added","output_index":0,"item":wire("rs","synthetic-partial","in_progress")}),
            json!({"type":"response.output_item.done","output_index":0,"item":final_item}),
            json!({"type":"response.completed","response":envelope("completed", json!([final_item]))}),
        ];
        for (i, frame) in frames.iter().enumerate() {
            for event in decoder.push(frame).unwrap() {
                state = reduce(state, event).unwrap();
            }
            if i == 1 {
                let items = snapshot_items(&state).unwrap();
                let Item::Reasoning(r) = &items[0].1 else {
                    panic!("reasoning")
                };
                assert_eq!(r.replay, Some(token("synthetic-partial", false).value));
            }
        }
        let decoded = decoder.materialize().unwrap();
        let Item::Reasoning(r) = &decoded.semantic.items()[0].1 else {
            panic!("reasoning")
        };
        assert_eq!(
            r.replay.as_ref().and_then(ReplayValue::replay_token),
            final_token
        );
        let static_value =
            responses::decode_response(&envelope("completed", json!([final_item]))).unwrap();
        assert_eq!(decoded.semantic, static_value.semantic);
        assert_eq!(materialize(&state).unwrap(), decoded.semantic);
        let history = GenerationRequest::new(
            decoded.semantic.items().to_vec(),
            GenerationControls::default(),
        )
        .unwrap();
        let out = encode(&history, &decoded.fidelity).unwrap();
        assert_eq!(
            out["input"][0].get("encrypted_content"),
            final_token.map(|v| json!(v)).as_ref()
        );
    }
}

#[test]
fn stream_replacement_releases_budget_and_materialization_does_not_double_charge() {
    use crate::events_support::{start, terminal, token};
    let large = "x".repeat(MAX_TEXT_BYTES);
    let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
    for id in 1..=4 {
        let mut event = start(id, ItemKind::Reasoning);
        let StreamEvent::ItemStarted { replay, .. } = &mut event else {
            unreachable!()
        };
        *replay = Some(token(&large, false));
        state = reduce(state, event).unwrap();
    }
    let begin = StreamEvent::PartStarted {
        item: ItemId::new(2),
        part: PartId::new(1),
        kind: PartKind::Summary,
    };
    let extra = StreamEvent::Delta {
        item: ItemId::new(2),
        part: PartId::new(1),
        fragment: "x".into(),
        logprobs: vec![],
    };
    assert!(matches!(
        reduce(reduce(state.clone(), begin.clone()).unwrap(), extra),
        Err(EventError::Limit)
    ));
    state = reduce(
        state,
        StreamEvent::ItemFinished {
            item: ItemId::new(1),
            status: ItemLifecycle::Completed,
            replay: None,
        },
    )
    .unwrap();
    state = reduce(state, begin).unwrap();
    state = reduce(
        state,
        StreamEvent::Delta {
            item: ItemId::new(2),
            part: PartId::new(1),
            fragment: large.clone(),
            logprobs: vec![],
        },
    )
    .unwrap();
    state = reduce(
        state,
        StreamEvent::ValueFinished {
            item: ItemId::new(2),
            part: PartId::new(1),
        },
    )
    .unwrap();
    state = reduce(
        state,
        StreamEvent::PartFinished {
            item: ItemId::new(2),
            part: PartId::new(1),
        },
    )
    .unwrap();
    for id in 2..=4 {
        state = reduce(
            state,
            StreamEvent::ItemFinished {
                item: ItemId::new(id),
                status: ItemLifecycle::Completed,
                replay: Some(token(&large, true)),
            },
        )
        .unwrap();
    }
    state = reduce(state, terminal(StreamTerminal::Completed)).unwrap();
    let response = materialize(&state).unwrap();
    assert_eq!(response.items().len(), 4);
    assert!(matches!(&response.items()[0].1, Item::Reasoning(r) if r.replay.is_none()));
    assert!(
        matches!(&response.items()[1].1, Item::Reasoning(r) if r.replay.as_ref().unwrap().replay_token() == Some(large.as_str()))
    );
}

#[test]
fn opaque_and_readable_values_share_the_semantic_budget() {
    for len in [0, MAX_TEXT_BYTES + 1] {
        let invalid = morphiecore::semantic::value::Text::allowing_empty(
            "x".repeat(len),
            "synthetic",
            MAX_TEXT_BYTES + 1,
        )
        .unwrap();
        let owner = ReasoningItem {
            parts: vec![],
            status: ItemLifecycle::Completed,
            replay: Some(ReplayValue::final_value(
                ReplayFormat::ResponsesEncrypted,
                invalid,
            )),
        };
        assert!(
            FidelityRecords::default()
                .record_replay(ItemId::new(1), &owner, Some(origin()))
                .is_err()
        );
        assert!(matches!(
            GenerationRequest::new(
                vec![(ItemId::new(1), Item::Reasoning(owner))],
                GenerationControls::default()
            ),
            Err(GenerationError::Limit)
        ));
    }
    let payload = text(&"x".repeat(MAX_TEXT_BYTES));
    let mut items: Vec<_> = (1..=4)
        .map(|id| {
            (
                ItemId::new(id),
                Item::Reasoning(ReasoningItem {
                    parts: vec![],
                    status: ItemLifecycle::Completed,
                    replay: Some(ReplayValue::final_value(
                        ReplayFormat::ResponsesEncrypted,
                        payload.clone(),
                    )),
                }),
            )
        })
        .collect();
    assert!(GenerationRequest::new(items.clone(), GenerationControls::default()).is_ok());
    reason(&mut items, 0)
        .parts
        .push((PartId::new(1), ReasoningContent::Summary(text("x"))));
    assert!(matches!(
        GenerationRequest::new(items, GenerationControls::default()),
        Err(GenerationError::Limit)
    ));
}

#[test]
fn every_bounded_owner_subset_replays_only_surviving_values_in_final_order() {
    for count in 1..=6 {
        let source = request(Value::Array(
            (0..count)
                .map(|i| wire(&format!("r{i}"), &format!("synthetic-{i}"), "completed"))
                .collect(),
        ));
        for mask in 1..(1 << count) {
            let mut selected: Vec<_> = (0..count).filter(|i| mask & (1 << i) != 0).collect();
            for reverse in [false, true] {
                if reverse {
                    selected.reverse();
                }
                let items = selected
                    .iter()
                    .map(|&i| source.semantic.items()[i].clone())
                    .collect();
                let edited = source.semantic.clone().with_items(items).unwrap();
                let actual = encode(&edited, &source.fidelity).unwrap();
                let expected: Vec<_> = selected
                    .iter()
                    .map(|i| {
                        json!({
                            "type":"reasoning", "id":format!("r{i}"), "summary":[],
                            "encrypted_content":format!("synthetic-{i}")
                        })
                    })
                    .collect();
                assert_eq!(
                    actual["input"],
                    json!(expected),
                    "mask {mask}, reversed {reverse}"
                );
            }
        }
    }
}
