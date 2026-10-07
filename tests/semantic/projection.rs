//! Named Chat loss never changes the source or repairs invalid reports.
use crate::events_support::{metadata, text};
use morphiecore::{
    lowering::generation::{GenerationRepresentationContract as Contract, lower_response},
    protocol::{
        fidelity::FidelityRecords,
        openai::{Profile, chat},
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
fn chat_omits_only_unrepresentable_image_accounting() {
    let source = response();
    let fidelity = FidelityRecords::default();
    let metadata = metadata();
    let projected = lower_response(
        &source,
        &fidelity,
        &metadata,
        Profile::Chat,
        Contract::full(),
    )
    .unwrap();
    use morphiecore::lowering::projection::{LossRule, ProjectionDirection};
    assert_eq!(
        projected.projection()[0].loss,
        Some(LossRule::OmitChatInputImageTokens)
    );
    assert_eq!(
        projected.projection()[0].direction,
        ProjectionDirection::Response
    );
    assert_eq!(projected.projection()[0].revision, 1);
    assert_eq!(projected.semantic().items(), source.items());
    assert_eq!(
        projected.requirements(),
        GenerationResponseRequirements::derive(projected.semantic())
    );
    let wire = chat::encode_response(&projected).unwrap();
    assert_eq!(
        wire["usage"],
        serde_json::json!({
            "prompt_tokens":10,"completion_tokens":2,"total_tokens":12
        })
    );
    assert_eq!(source.usage().unwrap().input_image_tokens, Some(7));
    assert!(
        lower_response(
            &source,
            &fidelity,
            &metadata,
            Profile::Responses,
            Contract::full()
        )
        .is_err()
    );
}

#[test]
fn chained_loss_is_not_erased_and_candidates_start_from_the_source() {
    use morphiecore::lowering::projection::LossRule;
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
    let second = first
        .reproject(Profile::Responses, Contract::full())
        .unwrap();
    let third = second.reproject(Profile::Chat, Contract::full()).unwrap();
    assert_eq!(
        third
            .projection()
            .iter()
            .map(|s| s.loss)
            .collect::<Vec<_>>(),
        vec![Some(LossRule::OmitChatInputImageTokens), None, None]
    );
    assert_eq!(
        third.semantic().usage().unwrap(),
        Usage::operation(10, 2, 12)
    );
    let mut full = Contract::full();
    full.adaptation.rules.chat_image_usage = true;
    full.adaptation.profile_id = "synthetic.image-carrier";
    let independent =
        lower_response(&source, &fidelity, &metadata, Profile::Chat, full.clone()).unwrap();
    assert_eq!(
        independent.projection()[0].profile_id,
        "synthetic.image-carrier"
    );
    assert_eq!(independent.projection()[0].loss, None);
    assert_eq!(independent.semantic(), &source);
    let chained = third.reproject(Profile::Chat, full).unwrap();
    assert_eq!(chained.semantic().usage().unwrap().input_image_tokens, None);
    let mut denied = Contract::full();
    denied.reported_facts = morphiecore::lowering::generation::ReportedFactPolicy::StrictComplete;
    assert!(first.reproject(Profile::Chat, denied).is_err());
}

#[test]
fn unknown_rules_and_stage_overflow_are_rejected() {
    use morphiecore::{
        lowering::{
            generation::RepresentationError,
            projection::{LossRule, MAX_PROJECTION_STAGES},
        },
        protocol::openai::ResponseRepresentation,
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
fn event_projection_agrees_with_static_and_rejects_malformed_usage() {
    use crate::events_support::terminal;
    use morphiecore::{
        lowering::projection::{LossRule, ProjectionDirection},
        protocol::openai::events::EventEncoder,
    };
    let fidelity = FidelityRecords::default();
    for image in [None, Some(0), Some(7), Some(10)] {
        let mut encoder = EventEncoder::new(Profile::Chat, metadata()).unwrap();
        for event in text_events() {
            encoder.encode(&event, &fidelity).unwrap();
        }
        encoder
            .encode(
                &StreamEvent::Usage(Usage {
                    input_image_tokens: image,
                    ..Usage::operation(10, 2, 12)
                }),
                &fidelity,
            )
            .unwrap();
        let chunks = encoder
            .encode(&terminal(StreamTerminal::Completed), &fidelity)
            .unwrap();
        assert_eq!(
            chunks.last().unwrap()["usage"],
            serde_json::json!({
                "prompt_tokens":10,"completion_tokens":2,"total_tokens":12
            })
        );
        assert_eq!(
            encoder.projection()[0].loss,
            image.map(|_| LossRule::OmitChatInputImageTokens)
        );
        assert_eq!(
            encoder.projection()[0].direction,
            ProjectionDirection::Event
        );
        encoder.finish().unwrap();
    }
    let mut encoder = EventEncoder::new(Profile::Chat, metadata()).unwrap();
    for event in text_events() {
        encoder.encode(&event, &fidelity).unwrap();
    }
    assert!(
        encoder
            .encode(
                &StreamEvent::Usage(Usage {
                    input_image_tokens: Some(11),
                    ..Usage::operation(10, 2, 12)
                }),
                &fidelity
            )
            .is_err()
    );
    assert!(
        encoder
            .encode(&terminal(StreamTerminal::Completed), &fidelity)
            .is_err()
    );
    assert!(encoder.projection().is_empty());
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
        let checked = morphiecore::lowering::generation::lower_request(
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
