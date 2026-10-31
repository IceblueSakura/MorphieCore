use super::*;
use crate::protocol::{
    CodecError,
    conversion::generation::{Change, Field},
    failure::*,
};
fn effect() -> Effect {
    Effect::new(
        "synthetic.omit-field.v1",
        Field::SafetyReports,
        ConversionBoundary::EventProject,
        Change::Omit,
    )
}
#[test]
fn complete_units_and_partial_prefixes_have_independent_handoff_proofs() {
    for with_effect in [false, true] {
        let report = Report::default();
        report.begin(1);
        let delta = if with_effect {
            std::iter::once(effect()).collect()
        } else {
            EvidenceDelta::default()
        };
        let mut frames = report
            .batch(
                View::DownstreamSse,
                vec![Bytes::from_static(b"one"), Bytes::from_static(b"two")],
                delta,
            )
            .into_acknowledged_frames();
        let (_, ack) = frames.next().unwrap();
        assert!(ack.unwrap().acknowledge(|| true));
        let active = report.attempt();
        assert_eq!(active.active_units[0].acknowledged, 1);
        assert_eq!(active.active_units[0].frames, 2);
        assert!(active.evidence.entries.iter().all(|e| e.handed_off == 0));
        drop(frames);
        let abandoned = report.attempt();
        assert_eq!(abandoned.partial_units, 1);
        assert_eq!(abandoned.abandoned_units, 1);
        assert_eq!(abandoned.handed_off_bytes, 3);
        assert_eq!(abandoned.last_abandoned_unit.unwrap().acknowledged, 1);
        assert!(abandoned.evidence.entries.iter().all(|e| e.handed_off == 0));
    }
    let report = Report::default();
    report.begin(1);
    for (_, ack) in report
        .batch(
            View::DownstreamJson,
            vec![Bytes::from_static(b"body")],
            std::iter::once(effect()).collect(),
        )
        .into_acknowledged_frames()
    {
        assert!(ack.unwrap().acknowledge(|| true));
    }
    assert_eq!(report.attempt().evidence.entries[0].handed_off, 1);
    assert!(report.attempt().active_units.is_empty());
}
#[test]
fn evidence_and_association_saturation_never_disable_acknowledgment() {
    let report = Report::default();
    report.begin(1);
    let mut delta = EvidenceDelta::default();
    for index in 0..100 {
        delta.push(
            effect().at(crate::protocol::conversion::generation::Owner::Item { scope: 1, index }),
        );
    }
    assert!(delta.omitted_entries > 0);
    let mut held = Vec::new();
    for _ in 0..32 {
        held.push(report.batch(
            View::DownstreamSse,
            vec![Bytes::from_static(b"x")],
            delta.clone(),
        ));
    }
    let overflow = report.batch(View::DownstreamSse, vec![Bytes::from_static(b"x")], delta);
    assert_eq!(report.attempt().active_units.len(), 32);
    assert_eq!(report.attempt().association_omissions, 1);
    for (_, ack) in overflow.into_acknowledged_frames() {
        assert!(ack.unwrap().acknowledge(|| true));
    }
    assert_eq!(report.attempt().handed_off_frames, 1);
    assert!(
        report
            .attempt()
            .evidence
            .entries
            .iter()
            .all(|e| e.handed_off == 0)
    );
    drop(held);
    assert!(report.attempt().active_units.is_empty());
}
#[test]
fn failed_candidate_and_output_views_do_not_pollute_the_producer() {
    let report = Report::default();
    let mut intake = EvidenceDelta::default();
    intake.push(Effect::new(
        "synthetic.default.v1",
        Field::HistoryAnnotations,
        ConversionBoundary::RequestDecode,
        Change::Default(crate::protocol::conversion::generation::DefaultBasis::SelectedPolicy),
    ));
    report.intake(intake);
    report.begin(1);
    report.accept(View::DownstreamSse, std::iter::once(effect()).collect());
    report.failure(
        Some(ConversionBoundary::EventDecode),
        &AttemptError::Codec(CodecError::Invalid(ShapeReason::Json)),
    );
    report.end(false);
    report.begin(2);
    assert!(report.notice(View::DownstreamJson).is_none());
    report.accept(View::RetainedJson, std::iter::once(effect()).collect());
    assert!(report.notice(View::DownstreamSse).is_none());
    report.accept(View::DownstreamJson, std::iter::once(effect()).collect());
    report.end(true);
    report.result(ResultState::Succeeded);
    let r = report.snapshot();
    assert!(r.intake.entries.is_empty());
    assert_eq!(r.producing_attempt, Some(2));
    assert!(r.failure.is_none());
    assert!(r.attempts[0].failure.is_some());
    assert!(r.attempts[1].failure.is_none());
    assert_eq!(r.attempts[1].evidence.entries.len(), 2);
}
#[test]
fn source_cause_survives_handling_failure_and_never_replaces_http_observation() {
    let report = Report::default();
    report.begin(1);
    report.update(|a| a.observed_status = Some(200));
    let source = SourceFailureEvidence::envelope(
        429,
        RecognitionBasis::EnvelopeType,
        None,
        Some("rate_limit_error"),
        Some("synthetic-private credit balance"),
    );
    assert_eq!(source.kind, SourceFailureKind::Quota);
    assert_eq!(source.basis, RecognitionBasis::QuotaPattern);
    report.failure(
        Some(ConversionBoundary::EventDecode),
        &AttemptError::Codec(CodecError::StreamFailure(source)),
    );
    report.failure(
        None,
        &AttemptError::Delivery(crate::execution::attempt::DeliveryReason::BodyConsumerClosed),
    );
    let r = report.attempt();
    assert_eq!(r.observed_status, Some(200));
    assert_eq!(r.source_failure.unwrap().status_hint.unwrap().get(), 429);
    assert!(
        matches!(r.failure.unwrap().cause,AttemptError::Codec(CodecError::StreamFailure(e)) if e.kind==SourceFailureKind::Quota)
    );
    assert!(matches!(
        r.handling_failure.unwrap().cause,
        AttemptError::Delivery(crate::execution::attempt::DeliveryReason::BodyConsumerClosed)
    ));
    assert!(
        !serde_json::to_string(&report.snapshot())
            .unwrap()
            .contains("synthetic-private")
    );
}
#[test]
fn failed_ack_does_not_certify_delivery() {
    // A checked output is not an acknowledged delivery.
    let report = Report::default();
    report.begin(1);
    for (_, ack) in report
        .batch(
            View::DownstreamJson,
            vec![Bytes::from_static(b"x")],
            std::iter::once(effect()).collect(),
        )
        .into_acknowledged_frames()
    {
        assert!(!ack.unwrap().acknowledge(|| false));
    }
    let r = report.attempt();
    assert_eq!(r.handed_off_frames, 0);
    assert_eq!(r.evidence.entries[0].handed_off, 0);
    assert_eq!(r.abandoned_units, 1);
}

#[test]
fn failure_codes_preserve_leaf_identity_without_debug_or_input() {
    use crate::{
        protocol::stream::SseError, semantic::context::ContextError, transport::sse::SseDecodeError,
    };
    for (cause, expected) in [
        (
            AttemptError::Sse(SseError::Framing(SseDecodeError::InvalidUtf8)),
            "sse.invalid_utf8",
        ),
        (
            AttemptError::Sse(SseError::Framing(SseDecodeError::UnexpectedEof)),
            "sse.unexpected_eof",
        ),
        (
            AttemptError::Codec(CodecError::Context(ContextError::Limit)),
            "context.limit",
        ),
        (
            AttemptError::Codec(CodecError::Context(ContextError::Stateful)),
            "context.stateful_unsupported",
        ),
        (
            AttemptError::Transport(crate::transport::http::TransportError::RequestConstruction),
            "transport.request_construction",
        ),
        (
            AttemptError::Transport(crate::transport::http::TransportError::Connect),
            "transport.connect_failed",
        ),
    ] {
        let value = serde_json::to_value(Failure {
            boundary: None,
            cause,
        })
        .unwrap();
        assert_eq!(value["code"], expected);
        assert!(value.get("reason").is_none());
    }
}

#[test]
fn failed_task_source_is_primary_even_without_a_codec_error() {
    let report = Report::default();
    report.begin(1);
    report.update(|attempt| {
        attempt.observed_status = Some(200);
        attempt.source_eof = true;
        attempt.task_outcome = Some(TaskOutcome::Failed);
    });
    report.source_failure(SourceFailureEvidence::envelope(
        429,
        RecognitionBasis::EnvelopeCode,
        Some("rate_limit_exceeded"),
        None,
        None,
    ));
    report.failure(
        None,
        &AttemptError::Delivery(crate::execution::attempt::DeliveryReason::BodyConsumerClosed),
    );
    let attempt = report.attempt();
    assert!(attempt.failure.is_none());
    assert_eq!(
        attempt.source_failure.unwrap().kind,
        SourceFailureKind::RateLimit
    );
    assert_eq!(
        attempt.handling_failure.unwrap().cause.code(),
        "delivery.body_consumer_closed"
    );
    assert!(attempt.source_eof);
}
