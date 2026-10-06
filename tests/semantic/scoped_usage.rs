//! Counting scope, basis and relationships remain explicit reported facts.
use crate::events_support::{self as events, text};
use morphiecore::{
    lowering::generation::{
        GenerationRepresentationContract as Contract, RepresentationError, lower_response,
    },
    protocol::{fidelity::FidelityRecords, openai::Profile},
    semantic::task::generation::*,
};
fn message() -> (ItemId, Item) {
    (
        ItemId::new(1),
        Item::Message(Message {
            role: MessageRole::Assistant,
            parts: vec![Part {
                id: PartId::new(2),
                content: ContentPart::Text(text("a").into()),
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
        .with_usage(usage)
        .unwrap();
    assert_eq!(response.usage().unwrap().input_tokens, None);
    for profile in [Profile::Chat, Profile::Responses] {
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
fn cumulative_item_reports_replace_not_add_and_final_reports_cannot_change() {
    let mut first = Usage::operation(2, 3, 5);
    first.scope = UsageScope::Item(ItemId::new(1));
    first.basis = UsageBasis::Cumulative;
    let mut last = first;
    last.output_tokens = Some(6);
    last.total_tokens = Some(8);
    last.basis = UsageBasis::Final;
    let response = GenerationResponse::new(vec![message()], Outcome::Completed)
        .unwrap()
        .with_usage_reports(vec![first, last])
        .unwrap();
    assert_eq!(response.usage(), None);
    assert_eq!(response.usage_reports(), &[last]);
    assert!(response.clone().with_items(vec![]).is_err());
    assert!(
        GenerationResponse::new(vec![message()], Outcome::Completed)
            .unwrap()
            .with_usage_reports(vec![last, last])
            .is_err()
    );
    let mut lower = first;
    lower.output_tokens = Some(1);
    lower.total_tokens = Some(3);
    assert!(
        GenerationResponse::new(vec![message()], Outcome::Completed)
            .unwrap()
            .with_usage_reports(vec![first, lower])
            .is_err()
    );
}
#[test]
fn partial_relations_and_delta_lower_bounds_reject_contradictions_without_filling_counts() {
    let mut partial = Usage::operation(2, 3, 5);
    partial.input_tokens = None;
    partial.total_tokens = Some(2);
    assert!(partial.validate().is_err());
    let mut delta = Usage::operation(2, 3, 5);
    delta.basis = UsageBasis::Delta;
    let too_small = Usage::operation(2, 3, 5);
    assert!(
        GenerationResponse::new(vec![message()], Outcome::Completed)
            .unwrap()
            .with_usage_reports(vec![delta, delta, too_small])
            .is_err()
    );
    let total = Usage::operation(4, 6, 10);
    let response = GenerationResponse::new(vec![message()], Outcome::Completed)
        .unwrap()
        .with_usage_reports(vec![delta, delta, total])
        .unwrap();
    assert_eq!(response.usage(), Some(total));
    assert_eq!(response.usage_reports().len(), 3);
    let mut incomplete = total;
    incomplete.input_tokens = None;
    assert_eq!(
        GenerationResponse::new(vec![], Outcome::Completed)
            .unwrap()
            .with_usage(incomplete)
            .unwrap()
            .usage()
            .unwrap()
            .input_tokens,
        None
    );
}

#[test]
fn static_and_event_usage_scopes_and_finality_agree() {
    let mut input = vec![
        StreamEvent::Started,
        events::start(1, ItemKind::Message { phase: None }),
    ];
    input.extend(events::part(1, 2, PartKind::Text, "a"));
    input.push(events::close(1, ItemLifecycle::Completed));
    let mut first = Usage::operation(2, 3, 5);
    first.basis = UsageBasis::Cumulative;
    let mut last = first;
    last.output_tokens = Some(6);
    last.total_tokens = Some(8);
    last.basis = UsageBasis::Final;
    input.extend([
        StreamEvent::Usage(first),
        StreamEvent::Usage(last),
        events::terminal(StreamTerminal::Completed),
    ]);
    let actual = materialize(&events::apply(&input).unwrap()).unwrap();
    let expected = GenerationResponse::new(vec![message()], Outcome::Completed)
        .unwrap()
        .with_usage(last)
        .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(actual.usage().unwrap().output_tokens, Some(6));
    let mut delta = first;
    delta.basis = UsageBasis::Delta;
    let report = GenerationResponse::new(vec![message()], Outcome::Completed)
        .unwrap()
        .with_usage_reports(vec![delta, delta])
        .unwrap();
    assert_eq!(report.usage_reports().len(), 2);
    assert_eq!(report.usage(), None);
}
