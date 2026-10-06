//! Named subtraction views borrow reported facts and never add overlapping details.
use morphiecore::semantic::task::generation::*;
fn usage() -> Usage {
    Usage {
        scope: UsageScope::Operation,
        basis: UsageBasis::Final,
        output_relation: OutputTokenRelation::IncludesReasoning,
        total_relation: TotalTokenRelation::InputAndOutput,
        input_tokens: Some(12),
        output_tokens: Some(10),
        total_tokens: Some(22),
        cached_input_tokens: Some(7),
        input_cache_write_tokens: Some(9),
        reasoning_tokens: Some(6),
        input_text_tokens: Some(12),
        input_image_tokens: Some(8),
        input_audio_tokens: None,
        output_audio_tokens: None,
        output_text_tokens: Some(10),
        accepted_prediction_tokens: Some(2),
        rejected_prediction_tokens: Some(3),
    }
}
#[test]
fn formulas_have_explicit_provenance_and_do_not_subtract_overlapping_details() {
    let usage = usage();
    let input = usage
        .derive(UsageFormula::InputMinusCacheRead)
        .unwrap()
        .unwrap();
    assert_eq!(input.tokens(), 5);
    assert_eq!(input.formula(), UsageFormula::InputMinusCacheRead);
    assert!(std::ptr::eq(input.source(), &usage));
    let output = usage
        .derive(UsageFormula::OutputMinusReasoning)
        .unwrap()
        .unwrap();
    assert_eq!(output.tokens(), 4);
    // This is not visible text, billing, or a sum of modality/prediction counts.
    assert_ne!(Some(output.tokens()), usage.output_text_tokens);
    assert_eq!(usage.total_tokens, Some(22));
}
#[test]
fn absent_zero_maximum_and_invalid_reports_are_distinct() {
    let mut usage = usage();
    usage.cached_input_tokens = None;
    usage.reasoning_tokens = None;
    assert!(
        usage
            .derive(UsageFormula::InputMinusCacheRead)
            .unwrap()
            .is_none()
    );
    assert!(
        usage
            .derive(UsageFormula::OutputMinusReasoning)
            .unwrap()
            .is_none()
    );
    usage.cached_input_tokens = Some(0);
    assert_eq!(
        usage
            .derive(UsageFormula::InputMinusCacheRead)
            .unwrap()
            .unwrap()
            .tokens(),
        12
    );
    usage.reasoning_tokens = Some(0);
    assert_eq!(
        usage
            .derive(UsageFormula::OutputMinusReasoning)
            .unwrap()
            .unwrap()
            .tokens(),
        10
    );
    usage.cached_input_tokens = Some(13);
    assert!(usage.derive(UsageFormula::InputMinusCacheRead).is_err());
    usage = super_usage_max();
    assert_eq!(
        usage
            .derive(UsageFormula::InputMinusCacheRead)
            .unwrap()
            .unwrap()
            .tokens(),
        0
    );
    usage.output_tokens = Some(1);
    assert!(usage.derive(UsageFormula::InputMinusCacheRead).is_err());
}
fn super_usage_max() -> Usage {
    Usage {
        scope: UsageScope::Operation,
        basis: UsageBasis::Final,
        output_relation: OutputTokenRelation::IncludesReasoning,
        total_relation: TotalTokenRelation::InputAndOutput,
        input_tokens: Some(u64::MAX),
        output_tokens: Some(0),
        total_tokens: Some(u64::MAX),
        cached_input_tokens: Some(u64::MAX),
        input_cache_write_tokens: None,
        reasoning_tokens: None,
        input_text_tokens: None,
        input_image_tokens: None,
        input_audio_tokens: None,
        output_audio_tokens: None,
        output_text_tokens: None,
        accepted_prediction_tokens: None,
        rejected_prediction_tokens: None,
    }
}
