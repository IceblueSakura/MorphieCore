//! Named subtraction views borrow reported facts and never add overlapping details.
use morphiecore::semantic::task::generation::*;
#[test]
fn exclusive_cache_counts_require_explicit_disjoint_premises_without_zero_fill() {
    let mut report = Usage::operation(10, 2, 4108);
    report.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
    report.total_relation = TotalTokenRelation::InputCacheAndOutput;
    report.cached_input_tokens = Some(4096);
    report.input_cache_write_tokens = Some(0);
    report.validate().unwrap();
    assert_eq!(
        report
            .derive(UsageFormula::InputPlusCacheReadAndWrite)
            .unwrap()
            .unwrap()
            .tokens(),
        4106
    );
    assert!(
        report
            .derive(UsageFormula::InputMinusCacheRead)
            .unwrap()
            .is_none()
    );
    let original = report;
    report.input_cache_write_tokens = None;
    assert!(
        report
            .derive(UsageFormula::InputPlusCacheReadAndWrite)
            .unwrap()
            .is_none()
    );
    report = original;
    report.input_relation = InputTokenRelation::IncludesCache;
    assert!(report.validate().is_err());
    report = original;
    report.total_tokens = None;
    report.total_relation = TotalTokenRelation::Unreported;
    report.input_relation = InputTokenRelation::Unreported;
    assert!(
        report
            .derive(UsageFormula::InputPlusCacheReadAndWrite)
            .unwrap()
            .is_none()
    );
    report.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
    report.input_tokens = Some(u64::MAX);
    assert!(
        report
            .derive(UsageFormula::InputPlusCacheReadAndWrite)
            .is_err()
    );
    assert_eq!(original.input_tokens, Some(10));
}
fn usage() -> Usage {
    Usage {
        input_relation: morphiecore::semantic::task::generation::InputTokenRelation::IncludesCache,
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
