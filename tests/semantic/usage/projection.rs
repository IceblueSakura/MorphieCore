//! Named Chat loss never changes the source or repairs invalid reports.
use crate::events_support::{metadata, text};
use morphiecore::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{Profile, TargetCaps as Contract, chat, lowering::lower_response},
    },
    semantic::task::generation::*,
};

fn response() -> GenerationResponse {
    GenerationResponse::new(
        vec![(
            ItemId::new(1),
            Item::Message(Message {
                participant: None,
                role: MessageRole::Assistant,
                parts: vec![Part {
                    replay: None,
                    id: PartId::new(2),
                    content: ContentPart::Text(text("answer").into()),
                    cache: None,
                }],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        )],
        Outcome::Completed,
    )
    .unwrap()
    .with_usage(Some(Usage {
        input_image_tokens: Some(7),
        ..Usage::operation(10, 2, 12)
    }))
    .unwrap()
}

#[test]
fn canonical_report_events_reach_terminal_carriers_without_rewriting_source_facts() {
    use morphiecore::protocol::openai::events::EventEncoder;
    let report = UsageReport::operation(Usage {
        input_image_tokens: Some(7),
        input_audio_tokens: Some(0),
        cached_input_tokens: Some(0),
        input_cache_write_tokens: Some(0),
        reasoning_tokens: Some(0),
        ..Usage::operation(10, 2, 12)
    });
    let original = report.clone();
    let fidelity = FidelityRecords::default();
    for profile in [Profile::Chat, Profile::Responses] {
        let mut encoder = EventEncoder::new(profile, metadata()).unwrap();
        for event in text_events() {
            encoder.encode(&event, &fidelity).unwrap();
        }
        assert!(
            encoder
                .encode(&StreamEvent::UsageReport(report.clone()), &fidelity)
                .unwrap()
                .is_empty()
        );
        let frames = encoder
            .encode(
                &crate::events_support::terminal(StreamTerminal::Completed),
                &fidelity,
            )
            .unwrap();
        encoder.finish().unwrap();
        if profile == Profile::Chat {
            let usage = frames
                .iter()
                .filter_map(|frame| frame.get("usage"))
                .find(|usage| usage.is_object())
                .unwrap();
            assert_eq!(usage["prompt_tokens_details"]["image_tokens"], 7);
            assert_eq!(usage["prompt_tokens_details"]["audio_tokens"], 0);
        } else {
            let usage = &frames.last().unwrap()["response"]["usage"];
            assert_eq!(usage["input_tokens"], 10);
            assert_eq!(usage["input_tokens_details"]["cached_tokens"], 0);
            assert!(usage["input_tokens_details"].get("image_tokens").is_none());
            assert!(
                encoder
                    .projection()
                    .iter()
                    .any(|stage| { stage.rule() == "responses.omit-input-image-tokens.v1" })
            );
        }
        assert!(
            !encoder
                .projection()
                .iter()
                .any(|stage| { stage.rule() == "generation.usage-report-view.v1" })
        );
        assert_eq!(report, original);
    }
}

#[test]
fn canonical_modalities_use_exact_chat_carriers_without_report_loss() {
    let source = response();
    let before = source.clone();
    let fidelity = FidelityRecords::default();
    let meta = metadata();
    let target =
        lower_response(&source, &fidelity, &meta, Profile::Chat, Contract::full()).unwrap();
    assert!(
        !target
            .projection()
            .iter()
            .any(|stage| { stage.rule() == "generation.usage-report-view.v1" })
    );
    let wire = chat::encode_response(&target).unwrap();
    assert_eq!(wire["usage"]["prompt_tokens_details"]["image_tokens"], 7);
    assert_eq!(
        source
            .usage_reports()
            .operation()
            .unwrap()
            .details
            .modalities,
        vec![ModalityTokens {
            bucket: UsageBucket::Input,
            modality: Some(TokenModality::Image),
            tokens: Some(7),
            defaulted: false
        }]
    );
    assert_eq!(source, before);
}

#[test]
fn responses_fingerprint_projection_is_explicit_and_keeps_source_metadata() {
    use crate::events_support::terminal;
    use morphiecore::{
        protocol::openai::{events::EventEncoder, responses},
        semantic::value::Presence,
    };
    let source = response().with_usage(None).unwrap();
    let fidelity = FidelityRecords::default();
    for fingerprint in [
        Presence::Absent,
        Presence::Null,
        Presence::Value("synthetic-fp".into()),
    ] {
        let mut meta = metadata();
        meta.context.system_fingerprint = fingerprint.clone();
        let original = meta.clone();
        let projected = lower_response(
            &source,
            &fidelity,
            &meta,
            Profile::Responses,
            Contract::full(),
        )
        .unwrap();
        let wire = responses::encode_response(&projected).unwrap();
        assert!(wire.get("system_fingerprint").is_none());
        assert_eq!(wire["id"].as_str(), meta.id.as_deref());
        assert_eq!(meta, original);
        assert_eq!(
            projected
                .projection()
                .iter()
                .filter(|s| s.rule() == "responses.omit-system-fingerprint.v1")
                .count(),
            usize::from(!fingerprint.is_absent())
        );
        let back = projected
            .reproject(Profile::Chat, Contract::full())
            .unwrap();
        assert!(
            chat::encode_response(&back)
                .unwrap()
                .get("system_fingerprint")
                .is_none()
        );
        let native =
            lower_response(&source, &fidelity, &meta, Profile::Chat, Contract::full()).unwrap();
        assert_eq!(
            chat::encode_response(&native)
                .unwrap()
                .get("system_fingerprint")
                .is_some(),
            !fingerprint.is_absent()
        );
        let mut encoder = EventEncoder::new(Profile::Responses, meta.clone()).unwrap();
        for event in text_events() {
            encoder.encode(&event, &fidelity).unwrap();
        }
        assert_eq!(
            encoder
                .projection()
                .iter()
                .filter(|stage| stage.change().is_loss())
                .count(),
            usize::from(!fingerprint.is_absent())
        );
        let mut failed = EventEncoder::new(Profile::Responses, meta.clone()).unwrap();
        for event in text_events() {
            failed.encode(&event, &fidelity).unwrap();
        }
        assert!(failed.encode(&StreamEvent::Started, &fidelity).is_err());
        assert_eq!(
            failed
                .projection()
                .iter()
                .filter(|stage| stage.change().is_loss())
                .count(),
            usize::from(!fingerprint.is_absent())
        );
        assert!(failed.finish().is_err());
        let output = encoder
            .encode(&terminal(StreamTerminal::Completed), &fidelity)
            .unwrap();
        encoder.finish().unwrap();
        assert!(
            output.last().unwrap()["response"]
                .get("system_fingerprint")
                .is_none()
        );
        assert_eq!(
            encoder
                .projection()
                .iter()
                .filter(|s| s.change().is_loss())
                .count(),
            usize::from(!fingerprint.is_absent())
        );
    }
    for value in [String::new(), "x".repeat(257)] {
        let mut meta = metadata();
        meta.context.system_fingerprint = Presence::Value(value);
        assert!(
            lower_response(
                &source,
                &fidelity,
                &meta,
                Profile::Responses,
                Contract::full()
            )
            .is_err()
        );
        assert!(EventEncoder::new(Profile::Responses, meta).is_err());
    }
}

#[test]
fn projection_stage_overflow_is_rejected() {
    use morphiecore::{
        lowering::generation::RepresentationError,
        protocol::openai::{ResponseRepresentation, lowering::projection::MAX_PROJECTION_STAGES},
    };
    fn descend(value: &ResponseRepresentation<'_>, remaining: usize) {
        if remaining == 0 {
            assert!(matches!(
                value.reproject(Profile::Chat, Contract::full()),
                Err(RepresentationError::ProjectionLimit)
            ));
        } else {
            let next = value.reproject(Profile::Chat, Contract::full()).unwrap();
            descend(&next, remaining - 1);
        }
    }
    let source = response();
    let fidelity = FidelityRecords::default();
    let metadata = metadata();
    let first = lower_response(
        &source,
        &fidelity,
        &metadata,
        Profile::Chat,
        Contract::full(),
    )
    .unwrap();
    assert_eq!(
        chat::encode_response(&first).unwrap()["usage"]["prompt_tokens_details"]["image_tokens"],
        7
    );
    descend(&first, MAX_PROJECTION_STAGES - 1);
}

fn text_events() -> Vec<StreamEvent> {
    use crate::events_support::{close, part, start};
    let mut events = vec![
        StreamEvent::Started,
        start(1, ItemKind::Message { phase: None }),
    ];
    events.extend(part(1, 2, PartKind::Text, "answer"));
    events.push(close(1, ItemLifecycle::Completed));
    events
}

#[test]
fn a_stream_cannot_switch_target_policy_after_publication() {
    use morphiecore::protocol::openai::events::EventEncoder;
    let mut encoder = EventEncoder::new(Profile::Chat, metadata()).unwrap();
    let fidelity = FidelityRecords::default();
    encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
    let mut contract = Contract::full();
    contract.adaptation.rules.structured_chat_reasoning = true;
    encoder = encoder.with_contract(contract);
    assert!(encoder.encode(&text_events()[1], &fidelity).is_err());
}

#[test]
fn accounting_loss_cannot_hide_group_phase_replay_or_resource_rejections() {
    let original = response();
    let fidelity = FidelityRecords::default();
    let metadata = metadata();
    let mut variants = vec![];
    let mut items = original.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        unreachable!()
    };
    message.phase = Some(Phase::FinalAnswer);
    variants.push(
        GenerationResponse::new(items, Outcome::Completed)
            .unwrap()
            .with_usage(original.usage())
            .unwrap(),
    );
    let mut items = original.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        unreachable!()
    };
    message.parts.push(Part {
        replay: None,
        id: PartId::new(3),
        content: ContentPart::Text(text("second").into()),
        cache: None,
    });
    variants.push(original.clone().with_items(items).unwrap());
    let mut items = original.items().to_vec();
    let Item::Message(mut second) = items[0].1.clone() else {
        unreachable!()
    };
    second.parts[0].id = PartId::new(3);
    items.push((ItemId::new(3), Item::Message(second)));
    variants.push(original.clone().with_items(items).unwrap());
    let mut items = original.items().to_vec();
    items.insert(
        0,
        (
            ItemId::new(4),
            Item::Reasoning(ReasoningItem {
                parts: vec![],
                replay: Some(ReplayValue::final_value(
                    ReplayFormat::ResponsesEncrypted,
                    text("synthetic-opaque"),
                )),
                status: ItemLifecycle::Completed,
            }),
        ),
    );
    variants.push(original.clone().with_items(items).unwrap());
    for changed in variants {
        assert!(
            lower_response(
                &changed,
                &fidelity,
                &metadata,
                Profile::Chat,
                Contract::full()
            )
            .is_err()
        );
        assert_eq!(changed.usage(), original.usage());
    }
    for profile in [Profile::Chat, Profile::Responses] {
        let request =
            GenerationRequest::new(original.items().to_vec(), GenerationControls::default())
                .unwrap();
        let checked = morphiecore::protocol::openai::lowering::lower_request(
            &request,
            &fidelity,
            profile,
            Contract::full(),
        )
        .unwrap();
        assert_eq!(checked.semantic(), &request);
        assert_eq!(checked.profile(), profile);
    }
}
