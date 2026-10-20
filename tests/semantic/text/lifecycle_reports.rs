//! Independent lifecycle report intake and delivery expectations.
use morphiecore::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            Adapter, Dialect, Profile, ResponseMetadata, TargetCaps, events::EventEncoder,
            lowering::lower_response,
        },
    },
    semantic::{
        context::{ReportPhase, ResponseContext, ResponseReport, ResponseReports, ServiceClass},
        task::generation::*,
        value::{Presence, Text},
    },
};
use serde_json::{Value, json};

fn adapter() -> Adapter {
    let mut adapter = Adapter::new(Profile::Responses, Dialect::OpenRouter, None);
    adapter.adaptation.rules.responses_lifecycle_reports = true;
    adapter
}
fn event(kind: &str, sequence: u64, reasoning: Value, tier: Value) -> Value {
    let mut response = json!({"id":"report-response","object":"response","created_at":1,
        "model":"m","status":if kind == "completed" {"completed"} else {"in_progress"},"output":[]});
    response["reasoning"] = reasoning;
    response["service_tier"] = tier;
    json!({"type":format!("response.{kind}"),"sequence_number":sequence,"response":response})
}
fn report(terminal: bool) -> ResponseReport {
    let mut reasoning = ReasoningRequest::present(Some(if terminal {
        ReasoningSummary::Detailed
    } else {
        ReasoningSummary::Auto
    }));
    if terminal {
        reasoning.context = Presence::Value(ReasoningContext::AllTurns);
        reasoning.mode = Presence::Value(ReasoningMode::Standard);
    }
    ResponseReport {
        settings: Some(GenerationSettings {
            reasoning,
            ..Default::default()
        }),
        service_class: Presence::Value(if terminal {
            ServiceClass::StandardProcessing
        } else {
            ServiceClass::ConfiguredProcessing
        }),
    }
}
fn metadata(reports: ResponseReports) -> ResponseMetadata {
    ResponseMetadata {
        id: Some("report-response".into()),
        model: "m".into(),
        created: Some(1.into()),
        context: ResponseContext {
            reports,
            ..Default::default()
        },
        storage: None,
        instruction_fidelity: Default::default(),
    }
}
fn lifecycle() -> ResponseReports {
    ResponseReports::Lifecycle {
        opening: Some(Box::new(report(false))),
        terminal: Some(Box::new(report(true))),
    }
}

#[test]
fn native_lifecycle_reports_preserve_whole_phase_observations() {
    let mut decoder = adapter().event_decoder();
    decoder
        .push(&event(
            "created",
            0,
            json!({"summary":"auto"}),
            json!("auto"),
        ))
        .unwrap();
    decoder
        .push(&event(
            "in_progress",
            1,
            json!({"summary":"auto"}),
            json!("auto"),
        ))
        .unwrap();
    assert_eq!(
        decoder.metadata().unwrap().context.reports,
        ResponseReports::phased(ReportPhase::Opening, report(false))
    );
    decoder
        .push(&event(
            "completed",
            2,
            json!({"summary":"detailed","context":"all_turns","mode":"standard"}),
            json!("default"),
        ))
        .unwrap();
    decoder.finish().unwrap();
    assert_eq!(decoder.metadata().unwrap().context.reports, lifecycle());
    assert!(
        decoder
            .push(&event("completed", 3, json!({}), json!("default")))
            .is_err()
    );
    assert!(decoder.finish().is_err());
}

#[test]
fn each_resolution_is_closed_phase_local_and_source_checked() {
    // Independently isolate every admitted transition instead of allowing the
    // settings failure to hide a service-class failure.
    for (opening, terminal, a, b) in [
        (
            json!({"summary":"auto"}),
            json!({"summary":"detailed"}),
            json!("default"),
            json!("default"),
        ),
        (
            json!({}),
            json!({"context":"all_turns"}),
            json!("default"),
            json!("default"),
        ),
        (
            json!({}),
            json!({"mode":"standard"}),
            json!("default"),
            json!("default"),
        ),
        (json!({}), json!({}), json!("auto"), json!("default")),
    ] {
        let first = event("created", 0, opening, a);
        let last = event("completed", 1, terminal.clone(), b.clone());
        let mut decoder = adapter().event_decoder();
        decoder.push(&first).unwrap();
        decoder.push(&last).unwrap();
        decoder.finish().unwrap();
        let mut strict =
            Adapter::new(Profile::Responses, Dialect::OpenRouter, None).event_decoder();
        strict.push(&first).unwrap();
        assert!(matches!(
            strict.push(&last),
            Err(morphiecore::protocol::CodecError::Invalid(
                "reported settings changed" | "service tier changed"
            ))
        ));
        let mut same_phase = adapter().event_decoder();
        same_phase.push(&first).unwrap();
        assert!(matches!(
            same_phase.push(&event("in_progress", 1, terminal, b)),
            Err(morphiecore::protocol::CodecError::Invalid(
                "reported settings changed" | "service tier changed"
            ))
        ));
        assert!(same_phase.finish().is_err());
    }
    for (opening, terminal) in [
        (json!({"summary":"concise"}), json!({"summary":"detailed"})),
        (json!({"summary":null}), json!({"summary":"detailed"})),
        (json!({}), json!({"context":"current_turn"})),
        (json!({}), json!({"mode":"pro"})),
        (json!({"context":null}), json!({"context":"all_turns"})),
        (json!({"mode":null}), json!({"mode":"standard"})),
    ] {
        let mut decoder = adapter().event_decoder();
        decoder
            .push(&event("created", 0, opening, json!("default")))
            .unwrap();
        assert!(matches!(
            decoder.push(&event("completed", 1, terminal, json!("default"))),
            Err(morphiecore::protocol::CodecError::Invalid(
                "reported settings changed"
            ))
        ));
    }
    for initial in [Value::Null, json!("priority")] {
        let mut decoder = adapter().event_decoder();
        decoder
            .push(&event("created", 0, json!({}), initial))
            .unwrap();
        assert!(matches!(
            decoder.push(&event("completed", 1, json!({}), json!("default"))),
            Err(morphiecore::protocol::CodecError::Invalid(
                "service tier changed"
            ))
        ));
    }
    let mut malformed = event("completed", 1, json!({"mode":7}), json!("default"));
    let mut decoder = adapter().event_decoder();
    decoder
        .push(&event("created", 0, json!({}), json!("default")))
        .unwrap();
    assert!(decoder.push(&malformed).is_err());
    malformed["response"]["reasoning"] = json!({});
    assert!(
        decoder.push(&malformed).is_err(),
        "rejection poisons intake"
    );
}

#[test]
fn independently_constructed_reports_use_phase_carriers_and_terminal_snapshots() {
    let original = metadata(lifecycle());
    let fidelity = FidelityRecords::default();
    let terminal = StreamEvent::Terminal {
        terminal: StreamTerminal::Completed,
        details: Default::default(),
    };
    let mut encoder = EventEncoder::new(Profile::Responses, original.clone()).unwrap();
    let opening = encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
    assert_eq!(
        opening[0]["response"]["reasoning"],
        json!({"summary":"auto"})
    );
    assert_eq!(opening[1]["response"]["service_tier"], "auto");
    let frames = encoder.encode(&terminal, &fidelity).unwrap();
    assert_eq!(
        frames[0]["response"]["reasoning"],
        json!({"summary":"detailed","context":"all_turns","mode":"standard"})
    );
    assert_eq!(frames[0]["response"]["service_tier"], "default");
    encoder.finish().unwrap();
    assert!(
        !encoder.projection().iter().any(|s| s.loss.is_some()),
        "full SSE retains both observations"
    );
    let response = GenerationResponse::new(vec![], Outcome::Completed).unwrap();
    let target = lower_response(
        &response,
        &fidelity,
        &original,
        Profile::Responses,
        TargetCaps::full(),
    )
    .unwrap();
    let json =
        morphiecore::protocol::openai::responses::envelope::encode_response(&target).unwrap();
    assert_eq!(json, frames[0]["response"]);
    assert_eq!(
        target.metadata().context.reports,
        ResponseReports::phased(ReportPhase::Terminal, report(true))
    );
    assert!(target.projection().iter().any(|s| {
        s.loss
            .is_some_and(|l| l.name() == "responses.terminal-report-snapshot.v1")
    }));
    assert_eq!(original.context.reports, lifecycle());
    let mut strict = TargetCaps::full();
    strict.reported_facts = morphiecore::lowering::generation::ReportedFactPolicy::StrictComplete;
    assert!(lower_response(&response, &fidelity, &original, Profile::Responses, strict).is_err());
    assert!(
        EventEncoder::new(Profile::Chat, original).is_err(),
        "no cross-protocol tier drift"
    );
}

#[test]
fn terminal_absence_never_inherits_opening_and_terminal_only_never_invents_opening() {
    let mut decoder = adapter().event_decoder();
    decoder
        .push(&event(
            "created",
            0,
            json!({"summary":"auto"}),
            json!("auto"),
        ))
        .unwrap();
    let mut last = event("completed", 1, json!({}), json!("default"));
    last["response"]
        .as_object_mut()
        .unwrap()
        .remove("reasoning");
    last["response"]
        .as_object_mut()
        .unwrap()
        .remove("service_tier");
    decoder.push(&last).unwrap();
    decoder.finish().unwrap();
    let m = decoder.metadata().unwrap();
    assert_eq!(
        m.context.reports.at(ReportPhase::Terminal),
        Some(&ResponseReport::default())
    );
    assert!(m.context.settings().is_none());
    assert!(m.context.service_class().is_absent());
    let mut encoder = EventEncoder::new(
        Profile::Responses,
        metadata(ResponseReports::phased(ReportPhase::Terminal, report(true))),
    )
    .unwrap();
    let opening = encoder
        .encode(&StreamEvent::Started, &FidelityRecords::default())
        .unwrap();
    assert!(opening[0]["response"].get("reasoning").is_none());
    assert!(opening[0]["response"].get("service_tier").is_none());
}

#[test]
fn emitted_reports_cannot_be_rewritten_and_both_slots_count_toward_the_bound() {
    for opening_event in [StreamEvent::Started, StreamEvent::Queued] {
        let mut encoder = EventEncoder::new(
            Profile::Responses,
            metadata(ResponseReports::phased(ReportPhase::Terminal, report(true))),
        )
        .unwrap();
        encoder
            .encode(&opening_event, &FidelityRecords::default())
            .unwrap();
        assert!(
            encoder.update_metadata(metadata(lifecycle())).is_err(),
            "an absent emitted opening report cannot be inserted later"
        );
        assert!(encoder.finish().is_err());
    }
    let opening = metadata(ResponseReports::phased(ReportPhase::Opening, report(false)));
    let mut encoder = EventEncoder::new(Profile::Responses, opening.clone()).unwrap();
    encoder
        .encode(&StreamEvent::Started, &FidelityRecords::default())
        .unwrap();
    encoder.update_metadata(metadata(lifecycle())).unwrap();
    assert!(
        encoder.update_metadata(opening).is_err(),
        "terminal observation cannot disappear"
    );
    assert!(encoder.update_metadata(metadata(lifecycle())).is_err());
    assert!(
        encoder
            .encode(
                &StreamEvent::Terminal {
                    terminal: StreamTerminal::Completed,
                    details: Default::default()
                },
                &FidelityRecords::default()
            )
            .is_err()
    );
    assert!(encoder.finish().is_err());
    let settings = GenerationSettings {
        instructions: Presence::Value(
            Text::new("x".repeat(MAX_TEXT_BYTES), "test", MAX_TEXT_BYTES).unwrap(),
        ),
        ..Default::default()
    };
    settings.validate().unwrap();
    let reports = ResponseReports::Lifecycle {
        opening: Some(Box::new(ResponseReport {
            settings: Some(settings.clone()),
            ..Default::default()
        })),
        terminal: Some(Box::new(ResponseReport {
            settings: Some(settings),
            ..Default::default()
        })),
    };
    assert!(matches!(
        reports.validate(),
        Err(morphiecore::semantic::context::ContextError::Limit)
    ));
}
