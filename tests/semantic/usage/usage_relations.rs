//! Counting relationships remain explicit reported facts.
#[test]
fn prediction_subsets_obey_the_joint_bound_not_only_individual_bounds() {
    use morphiecore::semantic::task::generation::*;
    for (output, accepted, rejected, valid) in [
        (10, 9, 1, true),
        (10, 6, 6, false),
        (u64::MAX, u64::MAX, 1, false),
    ] {
        let mut usage = Usage::operation(0, output, output);
        usage.accepted_prediction_tokens = Some(accepted);
        usage.rejected_prediction_tokens = Some(rejected);
        assert!(accepted <= output && rejected <= output);
        assert_eq!(
            usage.validate(),
            if valid {
                Ok(())
            } else {
                Err(GenerationError::InvalidResponse)
            }
        );
    }
}

use crate::{events_support as events, events_support::text};
use morphiecore::{
    protocol::{
        conversion::generation::RepresentationError,
        fidelity::FidelityRecords,
        openai::{GenerationApi, TargetContract as Contract, lowering::lower_response},
    },
    semantic::task::generation::*,
};
fn message() -> (ItemId, Item) {
    (
        ItemId::new(1),
        Item::Message(Message {
            participant: None,
            role: MessageRole::Assistant,
            parts: vec![Part {
                replay: None,
                id: PartId::new(2),
                content: ContentPart::Text(text("a").into()),
                cache: None,
            }],
            status: ItemLifecycle::Completed,
            phase: None,
        }),
    )
}
#[test]
fn separate_reasoning_and_missing_counts_do_not_guess_totals_or_views() {
    let mut usage = Usage::operation(7, 20, 49);
    usage.reasoning_tokens = Some(22);
    usage.output_relation = OutputTokenRelation::ExcludesReasoning;
    usage.total_relation = TotalTokenRelation::InputOutputAndReasoning;
    usage.validate().unwrap();
    assert_eq!(
        usage.derive(UsageFormula::OutputMinusReasoning).unwrap(),
        None
    );
    usage.input_tokens = None;
    usage.total_relation = TotalTokenRelation::Unreported;
    usage.validate().unwrap();
    assert_eq!(usage.input_tokens, None);
    let response = GenerationResponse::new(vec![message()], Outcome::Completed)
        .unwrap()
        .with_usage(Some(usage))
        .unwrap();
    assert_eq!(response.usage().unwrap().input_tokens, None);
    for profile in [GenerationApi::Chat, GenerationApi::Responses] {
        assert_eq!(
            lower_response(
                &response,
                &FidelityRecords::default(),
                &events::metadata(),
                profile,
                Contract::full()
            )
            .err(),
            Some(RepresentationError::UsageProjection)
        );
    }
}
#[test]
fn contradictory_relations_and_a_second_final_report_are_rejected() {
    let mut partial = Usage::operation(2, 3, 5);
    partial.input_tokens = None;
    partial.total_tokens = Some(2);
    assert!(partial.validate().is_err());
    let state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
    let state = reduce(state, StreamEvent::Usage(Usage::operation(2, 3, 5))).unwrap();
    assert_eq!(
        reduce(state, StreamEvent::Usage(Usage::operation(2, 3, 5))).err(),
        Some(EventError::Lifecycle)
    );
}
