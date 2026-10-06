//! Format identity and selected dependencies never prove issuer authenticity.
use crate::events_support::{metadata, text};
use morphiecore::{
    lowering::generation::{
        GenerationRepresentationContract as Contract, RepresentationError, lower_response,
    },
    protocol::{fidelity::FidelityRecords, openai::Profile},
    semantic::{task::generation::*, value::ReplayOrigin},
};
fn reasoning(format: ReplayFormat) -> ReasoningItem {
    ReasoningItem {
        parts: vec![],
        status: ItemLifecycle::Completed,
        replay: Some(ReplayValue::final_value(format, text("synthetic-opaque"))),
    }
}
fn message(id: u64, value: &str) -> (ItemId, Item) {
    (
        ItemId::new(id),
        Item::Message(Message {
            role: MessageRole::User,
            parts: vec![Part {
                id: PartId::new(id),
                content: ContentPart::Text(text(value).into()),
            }],
            status: ItemLifecycle::Completed,
            phase: None,
        }),
    )
}
fn request(items: Vec<(ItemId, Item)>) -> GenerationRequest {
    GenerationRequest::new(items, GenerationControls::default()).unwrap()
}
#[test]
fn opaque_format_is_semantic_redacted_and_cannot_use_another_carrier() {
    let origin = ReplayOrigin::new("synthetic-scope").unwrap();
    let value = reasoning(ReplayFormat::GoogleInteractionsV1Thought);
    assert!(!format!("{value:?}").contains("synthetic-opaque"));
    let mut fidelity = FidelityRecords::default();
    fidelity
        .record_replay(ItemId::new(1), &value, Some(origin.clone()))
        .unwrap();
    let response = GenerationResponse::new(
        vec![(ItemId::new(1), Item::Reasoning(value.clone()))],
        Outcome::Completed,
    )
    .unwrap();
    let mut contract = Contract::full();
    contract.replay_origin = Some(origin.clone());
    assert_eq!(
        lower_response(
            &response,
            &fidelity,
            &metadata(),
            Profile::Responses,
            contract
        )
        .err(),
        Some(RepresentationError::ReplayFormat)
    );
    let changed = reasoning(ReplayFormat::ResponsesEncrypted);
    assert!(!fidelity.replay_matches(ItemId::new(1), &changed, Some(&origin)));
    let value = ReplayValue::partial(ReplayFormat::GoogleInteractionsV1Thought, text("partial"));
    assert_eq!(value.replay_token(), None);
}
#[test]
fn selected_owners_bind_values_order_and_only_selected_settings() {
    let source = request(vec![
        message(1, "a"),
        message(2, "unrelated"),
        message(3, "b"),
    ]);
    let scope = HistoryDependency::Owners(vec![ItemId::new(1), ItemId::new(3)]);
    let proof = RequestDependencyProof::capture(
        &source,
        scope.clone(),
        SettingsDependency::only(SettingsField::Tools)
            .union(SettingsDependency::only(SettingsField::Text)),
    )
    .unwrap();
    proof.check(&source).unwrap();
    let mut settings = source.settings().clone();
    settings.controls.max_output_tokens = Some(100);
    proof
        .check(&source.clone().with_settings(settings).unwrap())
        .unwrap();
    proof
        .check(&request(vec![
            message(1, "a"),
            message(2, "changed"),
            message(3, "b"),
            message(4, "new"),
        ]))
        .unwrap();
    for items in [
        vec![message(3, "b"), message(1, "a")],
        vec![message(1, "changed"), message(3, "b")],
        vec![message(1, "a")],
    ] {
        assert!(proof.check(&request(items)).is_err());
    }
    for owners in [
        vec![],
        vec![ItemId::new(1), ItemId::new(1)],
        vec![ItemId::new(9)],
    ] {
        assert!(
            RequestDependencyProof::capture(
                &source,
                HistoryDependency::Owners(owners),
                SettingsDependency::None
            )
            .is_err()
        );
    }
    let mut settings = source.settings().clone();
    settings.tools = Some(vec![]);
    assert!(
        proof
            .check(&source.with_settings(settings).unwrap())
            .is_err()
    );
}
