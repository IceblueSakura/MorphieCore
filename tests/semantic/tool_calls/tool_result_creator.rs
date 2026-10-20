use crate::events_support::*;
use morphiecore::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{Profile, events::EventDecoder, lowering::lower_response, responses},
    },
    semantic::task::generation::*,
};
use serde_json::json;
#[test]
fn creator_reports_are_item_owned_bounded_and_output_only() {
    let wire = json!({"id":"result","type":"function_call_output","call_id":"c","output":"value",
            "status":"completed","created_by":"actor"});
    let parsed = responses::decode_response(&envelope("completed", json!([wire]))).unwrap();
    let (id, item) = &parsed.semantic.items()[0];
    assert_eq!(
        parsed
            .fidelity
            .created_by(*id, item, parsed.semantic.resources()),
        Some("actor")
    );
    let owner = ItemId::new(9);
    let item = Item::ToolResult(ToolResult {
        call_id: Some(text("c")),
        name: None,
        output: "value".into(),
        status: Some(ItemLifecycle::Completed),
        is_error: None,
        execution: None,
        context: Default::default(),
        cache: None,
    });
    let semantic =
        GenerationResponse::new(vec![(owner, item.clone())], Outcome::Completed).unwrap();
    let mut fidelity = FidelityRecords::default();
    fidelity.record_response_item_id(owner, "result").unwrap();
    fidelity
        .observe_created_by(owner, &item, semantic.resources(), Some(&json!("actor")))
        .unwrap();
    let reported = metadata();
    let target = lower_response(
        &semantic,
        &fidelity,
        &reported,
        Profile::Responses,
        contract(),
    )
    .unwrap();
    assert_eq!(
        responses::encode_response(&target).unwrap()["output"],
        json!([wire])
    );
    assert!(
        fidelity
            .observe_created_by(owner, &item, semantic.resources(), Some(&json!("other")))
            .is_err()
    );
    assert!(
        fidelity
            .observe_created_by(
                ItemId::new(10),
                &item,
                semantic.resources(),
                Some(&json!("x".repeat(257)))
            )
            .is_err()
    );
    let mut edited = item.clone();
    let Item::ToolResult(result) = &mut edited else {
        unreachable!()
    };
    result.output = "edited".into();
    assert_eq!(
        fidelity.created_by(owner, &edited, semantic.resources()),
        None
    );
    fidelity.retain_owners(&[]);
    assert_eq!(
        fidelity.created_by(owner, &item, semantic.resources()),
        None
    );
}
#[test]
fn result_creator_snapshot_drift_cannot_be_hidden_at_done_or_terminal() {
    for terminal_drift in [false, true] {
        let mut decoder = EventDecoder::new(Profile::Responses);
        decoder.push(&created()).unwrap();
        let initial = json!({"id":"result","type":"function_call_output","call_id":"c","output":"",
                "status":"in_progress","created_by":"actor"});
        decoder
            .push(&json!({"type":"response.output_item.added","output_index":0,"item":initial}))
            .unwrap();
        let mut done = json!({"id":"result","type":"function_call_output","call_id":"c","output":"value",
                "status":"completed","created_by":"actor"});
        if !terminal_drift {
            done["created_by"] = json!("other");
            assert!(
                decoder
                    .push(&json!({"type":"response.output_item.done","output_index":0,"item":done}))
                    .is_err()
            );
        } else {
            decoder
                .push(&json!({"type":"response.output_item.done","output_index":0,"item":done}))
                .unwrap();
            done["created_by"] = json!("other");
            assert!(decoder.push(&json!({"type":"response.completed","response":envelope("completed",json!([done]))})).is_err());
        }
        assert!(decoder.finish().is_err());
    }
}
