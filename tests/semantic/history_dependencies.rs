//! Dependency proofs bind relationships and source values, not just surviving IDs.
use crate::events_support::text;
use morphiecore::{
    lowering::generation::{
        GenerationRepresentationContract as Contract, RepresentationError, lower_request,
    },
    protocol::{fidelity::FidelityRecords, openai::Profile},
    semantic::{task::generation::*, value::ReplayOrigin},
};
fn message(id: u64, body: &str) -> (ItemId, Item) {
    (
        ItemId::new(id),
        Item::Message(Message {
            role: MessageRole::Assistant,
            parts: vec![Part {
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
            message: Some(ItemId::new(1)),
            status: ItemLifecycle::Completed,
            context: CallContext::default(),
        }),
    )
}
fn history(items: Vec<(ItemId, Item)>) -> GenerationRequest {
    GenerationRequest::new(items, GenerationControls::default()).unwrap()
}

#[test]
fn group_proof_rejects_value_and_membership_edits_but_ignores_unrelated_items() {
    let source = history(vec![message(1, "body"), call(2)]);
    let proof = RequestDependencyProof::capture(
        &source,
        HistoryDependency::MessageGroup(ItemId::new(1)),
        morphiecore::semantic::task::generation::SettingsDependency::None,
    )
    .unwrap();
    proof.check(&source).unwrap();
    let mut items = source.items().to_vec();
    items.push(message(3, "unrelated"));
    proof.check(&history(items)).unwrap();
    for items in [
        vec![message(1, "changed"), call(2)],
        vec![message(1, "body")],
        vec![message(1, "body"), call(2), call(4)],
        vec![message(9, "body")],
    ] {
        assert!(proof.check(&history(items)).is_err());
    }
    assert!(
        RequestDependencyProof::capture(
            &source,
            HistoryDependency::MessageGroup(ItemId::new(2)),
            morphiecore::semantic::task::generation::SettingsDependency::None
        )
        .is_err()
    );
}
#[test]
fn prefix_proof_preserves_append_but_rejects_insertion_reorder_and_settings_changes() {
    let source = history(vec![message(1, "first"), message(2, "second")]);
    let proof = RequestDependencyProof::capture(
        &source,
        HistoryDependency::PrefixThrough(ItemId::new(2)),
        morphiecore::semantic::task::generation::SettingsDependency::All,
    )
    .unwrap();
    let mut items = source.items().to_vec();
    items.push(message(3, "later"));
    proof.check(&history(items)).unwrap();
    for items in [
        vec![
            message(1, "first"),
            message(3, "insert"),
            message(2, "second"),
        ],
        vec![message(2, "second"), message(1, "first")],
        vec![message(1, "first")],
    ] {
        assert!(proof.check(&history(items)).is_err());
    }
    let mut settings = source.settings().clone();
    settings.controls.max_output_tokens = Some(100);
    assert!(
        proof
            .check(&source.clone().with_settings(settings).unwrap())
            .is_err()
    );
    let unbound = RequestDependencyProof::capture(
        &source,
        HistoryDependency::PrefixThrough(ItemId::new(2)),
        morphiecore::semantic::task::generation::SettingsDependency::None,
    )
    .unwrap();
    let mut settings = source.settings().clone();
    settings.controls.max_output_tokens = Some(100);
    unbound
        .check(&source.with_settings(settings).unwrap())
        .unwrap();
}
#[test]
fn explicit_continuation_rejects_lost_output_owners_and_changed_groups() {
    let source =
        GenerationResponse::new(vec![message(1, "body"), call(2)], Outcome::Completed).unwrap();
    let exchange = ResponseContinuation::new(
        ResponseRelation::new(TurnId::new(1), ResponseId::new(1)),
        &source,
    );
    assert!(
        exchange
            .inspect(&history(vec![message(1, "edited"), call(2)]))
            .is_err()
    );
    assert!(
        exchange
            .inspect(&history(vec![message(1, "body"), call(2), call(3)]))
            .is_err()
    );
}
#[test]
fn redacted_values_and_schema_order_remain_real_dependencies() {
    let reasoning = |value| {
        (
            ItemId::new(1),
            Item::Reasoning(ReasoningItem {
                status: ItemLifecycle::Completed,
                parts: vec![],
                replay: Some(ReplayValue::final_value(
                    ReplayFormat::ResponsesEncrypted,
                    text(value),
                )),
            }),
        )
    };
    let source = history(vec![reasoning("synthetic-one")]);
    let proof = RequestDependencyProof::capture(
        &source,
        HistoryDependency::PrefixThrough(ItemId::new(1)),
        morphiecore::semantic::task::generation::SettingsDependency::All,
    )
    .unwrap();
    assert!(
        proof
            .check(&history(vec![reasoning("synthetic-two")]))
            .is_err()
    );
    let image = |url| {
        (
            ItemId::new(1),
            Item::Message(Message {
                role: MessageRole::User,
                parts: vec![Part {
                    id: PartId::new(1),
                    content: ContentPart::Resource(Resource {
                        location: ResourceLocation::Url(text(url)),
                        description: ResourceDescription::Image { detail: None },
                    }),
                }],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        )
    };
    let source = history(vec![image("https://example.invalid/a.png")]);
    let proof = RequestDependencyProof::capture(
        &source,
        HistoryDependency::PrefixThrough(ItemId::new(1)),
        morphiecore::semantic::task::generation::SettingsDependency::All,
    )
    .unwrap();
    assert!(
        proof
            .check(&history(vec![image("https://example.invalid/b.png")]))
            .is_err()
    );
    let mut settings = source.settings().clone();
    settings.text.presence = true;
    settings.text.format =
        morphiecore::semantic::value::Presence::Value(OutputConstraint::JsonSchema {
            name: text("answer"),
            description: None,
            strict: Some(false),
            schema: serde_json::from_str(
                r#"{"type":"object","properties":{"a":{"type":"string"},"b":{"type":"string"}}}"#,
            )
            .unwrap(),
        });
    let source = source.with_settings(settings.clone()).unwrap();
    let proof = RequestDependencyProof::capture(
        &source,
        HistoryDependency::PrefixThrough(ItemId::new(1)),
        morphiecore::semantic::task::generation::SettingsDependency::All,
    )
    .unwrap();
    let OutputConstraint::JsonSchema { schema, .. } = settings.text.format.value().unwrap() else {
        panic!("schema")
    };
    let mut reordered = schema.clone();
    reordered["properties"] =
        serde_json::from_str(r#"{"b":{"type":"string"},"a":{"type":"string"}}"#).unwrap();
    assert_eq!(*schema, reordered);
    let morphiecore::semantic::value::Presence::Value(OutputConstraint::JsonSchema {
        schema, ..
    }) = &mut settings.text.format
    else {
        panic!("schema")
    };
    *schema = reordered;
    assert!(
        proof
            .check(&source.with_settings(settings).unwrap())
            .is_err()
    );
}

#[test]
fn prefix_bound_replay_lowers_only_unchanged_final_history() {
    let origin = ReplayOrigin::new("synthetic-scope").unwrap();
    let reasoning = ReasoningItem {
        status: ItemLifecycle::Completed,
        parts: vec![],
        replay: Some(ReplayValue::final_value(
            ReplayFormat::ResponsesEncrypted,
            text("synthetic-token"),
        )),
    };
    let source = history(vec![(ItemId::new(1), Item::Reasoning(reasoning.clone()))]);
    let mut fidelity = FidelityRecords::default();
    fidelity
        .record_replay(ItemId::new(1), &reasoning, Some(origin.clone()))
        .unwrap();
    fidelity
        .bind_replay_dependency(
            ItemId::new(1),
            RequestDependencyProof::capture(
                &source,
                HistoryDependency::PrefixThrough(ItemId::new(1)),
                morphiecore::semantic::task::generation::SettingsDependency::All,
            )
            .unwrap(),
            &source,
        )
        .unwrap();
    let mut contract = Contract::full();
    contract.replay_origin = Some(origin);
    lower_request(&source, &fidelity, Profile::Responses, contract.clone()).unwrap();
    let mut items = source.items().to_vec();
    items.push(message(2, "append"));
    lower_request(
        &history(items),
        &fidelity,
        Profile::Responses,
        contract.clone(),
    )
    .unwrap();
    let changed = history(vec![message(2, "insert"), source.items()[0].clone()]);
    assert_eq!(
        lower_request(&changed, &fidelity, Profile::Responses, contract).err(),
        Some(RepresentationError::ReplayOrigin)
    );
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
    // Standard Responses still cannot carry explicit membership, independent of valid replay.
    assert_eq!(
        lower_request(&request, &fidelity, Profile::Responses, contract.clone()).err(),
        Some(RepresentationError::MessageGrouping)
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
