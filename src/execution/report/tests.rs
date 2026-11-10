use super::*;
use crate::protocol::{
    CodecError,
    conversion::generation::{Change, Field},
    failure::{RecognitionBasis, ShapeReason, SourceFailureKind},
};

fn effect() -> Effect {
    Effect::new(
        "test.omit.v1",
        Field::Prediction,
        ConversionBoundary::ResponseEncode,
        Change::Omit,
    )
}
#[test]
fn failed_attempt_and_other_output_views_do_not_pollute_a_new_notice() {
    let report = Report::default();
    report.begin();
    report.accept(View::DownstreamSse, std::iter::once(effect()).collect());
    report.failure(
        Some(ConversionBoundary::EventDecode),
        &AttemptError::Codec(CodecError::Invalid(ShapeReason::Json)),
    );
    report.begin();
    assert!(report.notice(View::DownstreamJson).is_none());
    report.accept(View::RetainedJson, std::iter::once(effect()).collect());
    assert!(report.notice(View::DownstreamSse).is_none());
    report.accept(View::DownstreamJson, std::iter::once(effect()).collect());
    let notice = serde_json::to_value(report.notice(View::DownstreamJson).unwrap()).unwrap();
    assert_eq!(notice["adaptations"].as_array().unwrap().len(), 1);
    assert_eq!(notice["adaptations"][0]["occurrences"], 1);
    assert!(notice.get("failure").is_none());
}
#[test]
fn terminal_preview_does_not_mutate_the_live_notice() {
    let report = Report::default();
    report.begin();
    let delta = std::iter::once(effect()).collect();
    let preview = report.preview_notice(View::DownstreamSse, &delta).unwrap();
    assert!(report.notice(View::DownstreamSse).is_none());
    report.accept(View::DownstreamSse, delta);
    assert_eq!(
        serde_json::to_value(preview).unwrap(),
        serde_json::to_value(report.notice(View::DownstreamSse).unwrap()).unwrap()
    );
}
#[test]
fn source_cause_survives_later_handling_failure_in_the_public_notice() {
    let report = Report::default();
    report.begin();
    let source = SourceFailureEvidence::envelope(
        429,
        RecognitionBasis::EnvelopeType,
        None,
        Some("rate_limit_error"),
        Some("synthetic-private balance"),
    );
    report.failure(
        Some(ConversionBoundary::EventDecode),
        &AttemptError::Codec(CodecError::StreamFailure(source)),
    );
    report.failure(
        None,
        &AttemptError::Delivery(crate::execution::attempt::DeliveryReason::BodyConsumerClosed),
    );
    let notice = report.notice(View::ErrorDelivery).unwrap();
    let value = serde_json::to_value(notice).unwrap();
    assert_eq!(value["failure"]["category"], "source");
    assert_eq!(
        value["failure"]["source_kind"],
        serde_json::to_value(SourceFailureKind::RateLimit).unwrap()
    );
    assert!(!value.to_string().contains("synthetic-private"));
}
