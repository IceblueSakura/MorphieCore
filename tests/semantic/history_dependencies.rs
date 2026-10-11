//! Dependency proofs bind relationships and source values, not just surviving IDs.
use crate::events_support::text;
use morphiecore::{
    lowering::generation::RepresentationError,
    protocol::{
        fidelity::FidelityRecords,
        openai::{Profile, TargetCaps as Contract, lowering::lower_request},
    },
    semantic::{task::generation::*, value::ReplayOrigin},
};
fn message(id: u64, body: &str) -> (ItemId, Item) {
    (
        ItemId::new(id),
        Item::Message(Message {
            role: MessageRole::Assistant,
            parts: vec![Part {
                replay: None,
                id: PartId::new(id),
                content: ContentPart::Text(
                    TextContent::new(
                        text(body),
                        vec![],
                        morphiecore::semantic::value::Presence::Absent,
                    )
                    .unwrap(),
                ),
            }],
            status: ItemLifecycle::Completed,
            phase: None,
        }),
    )
}
fn call(id: u64) -> (ItemId, Item) {
    (
        ItemId::new(id),
        Item::ToolCall(ToolCall {
            call_id: text(&format!("c{id}")),
            name: text("lookup"),
            arguments: "{}".into(),
            status: ItemLifecycle::Completed,
            context: CallContext::default(),
        }),
    )
}
fn history(items: Vec<(ItemId, Item)>) -> GenerationRequest {
    let owners = items
        .iter()
        .filter_map(|(id, item)| matches!(item, Item::ToolCall(_)).then_some((*id, ItemId::new(1))))
        .collect();
    GenerationRequest::new(items, GenerationControls::default())
        .unwrap()
        .with_message_owners(owners)
        .unwrap()
}

#[test]
fn repeating_a_bound_report_cannot_erase_history_dependencies() {
    let origin = ReplayOrigin::new("synthetic-scope").unwrap();
    let owner = ItemId::new(1);
    let reasoning = ReasoningItem {
        status: ItemLifecycle::Completed,
        parts: vec![],
        replay: Some(ReplayValue::final_value(
            ReplayFormat::ResponsesEncrypted,
            text("synthetic-token"),
        )),
    };
    let source = history(vec![(owner, Item::Reasoning(reasoning.clone()))]);
    let mut fidelity = FidelityRecords::default();
    fidelity
        .record_replay(owner, &reasoning, Some(origin.clone()))
        .unwrap();
    let proof = RequestDependencyProof::capture(
        &source,
        HistoryDependency::PrefixThrough(owner),
        morphiecore::semantic::task::generation::SettingsDependency::All,
    )
    .unwrap();
    fidelity
        .bind_replay_dependency(owner, proof, &source)
        .unwrap();
    fidelity
        .record_replay(owner, &reasoning, Some(origin.clone()))
        .unwrap();
    assert!(!fidelity.replay_matches(owner, &reasoning, Some(&origin)));
    assert!(fidelity.replay_matches_request(owner, &reasoning, Some(&origin), &source));
    let inserted = history(vec![message(2, "inserted"), source.items()[0].clone()]);
    assert!(!fidelity.replay_matches_request(owner, &reasoning, Some(&origin), &inserted));
    let mut settings = source.settings().clone();
    settings.controls.max_output_tokens = Some(100);
    assert!(!fidelity.replay_matches_request(
        owner,
        &reasoning,
        Some(&origin),
        &source.clone().with_settings(settings).unwrap(),
    ));
    let old = fidelity.clone();
    let mut changed = reasoning.clone();
    changed.replay = Some(ReplayValue::final_value(
        ReplayFormat::ResponsesEncrypted,
        text("synthetic-replacement"),
    ));
    assert!(
        fidelity
            .record_replay(owner, &changed, Some(origin.clone()))
            .is_err()
    );
    assert_eq!(fidelity, old);
    changed = reasoning.clone();
    changed
        .parts
        .push((PartId::new(9), ReasoningContent::Summary(text("changed"))));
    assert!(
        fidelity
            .record_replay(owner, &changed, Some(origin))
            .is_err()
    );
    assert_eq!(fidelity, old);
}

#[test]
fn group_bound_replay_cannot_bypass_final_history_checks() {
    let origin = ReplayOrigin::new("synthetic-scope").unwrap();
    let reasoning = ReasoningItem {
        status: ItemLifecycle::Completed,
        parts: vec![],
        replay: Some(ReplayValue::final_value(
            ReplayFormat::ResponsesEncrypted,
            text("synthetic-token"),
        )),
    };
    let request = history(vec![
        (ItemId::new(9), Item::Reasoning(reasoning.clone())),
        message(1, "body"),
        call(2),
    ]);
    let mut fidelity = FidelityRecords::default();
    fidelity
        .record_replay(ItemId::new(9), &reasoning, Some(origin.clone()))
        .unwrap();
    let proof = RequestDependencyProof::capture(
        &request,
        HistoryDependency::MessageGroup(ItemId::new(1)),
        morphiecore::semantic::task::generation::SettingsDependency::All,
    )
    .unwrap();
    fidelity
        .bind_replay_dependency(ItemId::new(9), proof, &request)
        .unwrap();
    assert!(!fidelity.replay_matches(ItemId::new(9), &reasoning, Some(&origin)));
    assert!(fidelity.replay_matches_request(ItemId::new(9), &reasoning, Some(&origin), &request));
    let mut contract = Contract::full();
    contract.replay_origin = Some(origin);
    // Boundary omission cannot bypass a necessary group-bound replay dependency.
    assert_eq!(
        lower_request(&request, &fidelity, Profile::Responses, contract.clone()).err(),
        Some(RepresentationError::ReplayOrigin)
    );
    let mut items = request.items().to_vec();
    items[1] = message(1, "changed");
    let changed = history(items);
    assert_eq!(
        lower_request(&changed, &fidelity, Profile::Responses, contract).err(),
        Some(RepresentationError::ReplayOrigin)
    );
    assert!(
        fidelity
            .bind_replay_dependency(
                ItemId::new(9),
                RequestDependencyProof::capture(
                    &changed,
                    HistoryDependency::MessageGroup(ItemId::new(1)),
                    morphiecore::semantic::task::generation::SettingsDependency::None
                )
                .unwrap(),
                &changed
            )
            .is_err()
    );
}
