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
                role: MessageRole::Assistant,
                parts: vec![Part {
                    id: PartId::new(2),
                    replay: None,
                    content: ContentPart::Text(text("answer").into()),
                }],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        )],
        Outcome::Completed,
    )
    .unwrap()
    .with_usage(Usage {
        input_image_tokens: Some(7),
        ..Usage::operation(10, 2, 12)
    })
    .unwrap()
}

#[test]
fn responses_fingerprint_projection_is_explicit_and_keeps_source_metadata() {
    use crate::events_support::terminal;
    use morphiecore::{
        protocol::openai::{events::EventEncoder, responses},
        semantic::value::Presence,
    };
    let source = response().with_usage_reports(vec![]).unwrap();
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
        assert_eq!(wire["id"], meta.id);
        assert_eq!(meta, original);
        assert_eq!(
            projected
                .projection()
                .iter()
                .filter(|s| s
                    .loss
                    .is_some_and(|rule| rule.name() == "responses.omit-system-fingerprint.v1"))
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
                .filter(|s| s.loss.is_some())
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
fn unknown_rules_and_stage_overflow_are_rejected() {
    use morphiecore::{
        lowering::generation::RepresentationError,
        protocol::openai::{
            ResponseRepresentation,
            lowering::projection::{LossRule, MAX_PROJECTION_STAGES},
        },
    };
    assert_eq!(
        "chat.omit-input-image-tokens.v1"
            .parse::<LossRule>()
            .unwrap()
            .owner(),
        "generation.usage.input_image_tokens"
    );
    assert_eq!(
        "chat.merge-messages.v1".parse::<LossRule>(),
        Err(RepresentationError::UnknownProjectionRule)
    );
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
    contract.adaptation.rules.chat_image_usage = true;
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
            .with_usage(original.usage().unwrap())
            .unwrap(),
    );
    let mut items = original.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        unreachable!()
    };
    message.parts.push(Part {
        id: PartId::new(3),
        replay: None,
        content: ContentPart::Text(text("second").into()),
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
    let resources = ResourceTable::new(vec![(
        ResourceId::new(1),
        ResourceDeclaration {
            body: ResourceBody::Text(text("source")),
            conditions: ResourceConditions {
                access: Some(text("explicit-access")),
                ..Default::default()
            },
        },
    )])
    .unwrap();
    variants.push(
        GenerationResponse::from_resources(
            original.items().to_vec(),
            Outcome::Completed,
            resources,
        )
        .unwrap()
        .with_usage(original.usage().unwrap())
        .unwrap(),
    );
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
        assert_eq!(
            checked.requirements(),
            GenerationRequirements::derive(&request)
        );
        assert_eq!(checked.profile(), profile);
    }
}
