use morphiecore::semantic::{
    task::generation::*,
    value::{ReplayOrigin, Text},
};
use proptest::prelude::*;

fn event(op: u8) -> StreamEvent {
    let item = ItemId::new(1);
    let part = PartId::new(1);
    match op {
        0 => StreamEvent::Started,
        1 => StreamEvent::ItemStarted {
            item,
            kind: ItemKind::Message { phase: None },
            replay: None,
        },
        2 => StreamEvent::PartStarted {
            item,
            part,
            kind: PartKind::Text,
        },
        3 => StreamEvent::Delta {
            item,
            part,
            fragment: "x".into(),
            logprobs: vec![],
        },
        4 => StreamEvent::ValueFinished { item, part },
        5 => StreamEvent::PartFinished { item, part },
        6 => StreamEvent::ItemFinished {
            item,
            status: ItemLifecycle::Completed,
            replay: None,
        },
        7 => StreamEvent::Terminal {
            terminal: StreamTerminal::Completed,
            details: TerminalDetails::default(),
        },
        _ => unreachable!(),
    }
}
proptest! {
    #![proptest_config(crate::test_properties::config())]
    #[test]
    fn property_completed_text_stream_obeys_independent_event_grammar(
        prefix in 0u8..=8, suffix in prop::collection::vec(0u8..8, 0..16)
    ) {
        let (mut started, mut item, mut part, mut value_done, mut part_done, mut item_done, mut terminal) =
            (false, false, false, false, false, false, false);
        let mut state = StreamState::new();
        let mut text = String::new();
        // Start at every lifecycle depth, not mostly at an invalid first event.
        for op in (0..prefix).chain(suffix) {
            let allowed = !terminal && match op {
                0 => !started,
                1 => started && !item,
                2 => item && !item_done && !part,
                3 | 4 => part && !value_done && !item_done,
                5 => value_done && !part_done && !item_done,
                6 => item && !item_done && (!part || part_done),
                7 => started && (!item || item_done),
                _ => false,
            };
            let next = reduce(state, event(op));
            prop_assert_eq!(next.is_ok(), allowed, "operation {}", op);
            if !allowed { return Ok(()); }
            state = next.unwrap();
            match op {
                0 => started = true,
                1 => item = true,
                2 => part = true,
                3 => text.push('x'),
                4 => value_done = true,
                5 => part_done = true,
                6 => item_done = true,
                7 => terminal = true,
                _ => unreachable!(),
            }
            prop_assert_eq!(end_of_stream(&state).is_ok(), terminal);
            if terminal {
                let response = materialize(&state).unwrap();
                prop_assert_eq!(response.outcome(), Outcome::Completed);
                prop_assert_eq!(response.items().len(), usize::from(item));
                if part {
                    let Item::Message(message) = &response.items()[0].1 else { panic!("message") };
                    let ContentPart::Text(value) = &message.parts[0].content else { panic!("text") };
                    prop_assert_eq!(value.as_str(), text.as_str());
                }
            }
        }
    }
}

#[test]
fn probability_deltas_and_final_metadata_share_the_part_budget() {
    use morphiecore::semantic::value::Presence;
    let item = ItemId::new(1);
    let part = PartId::new(1);
    let mut state = StreamState::new();
    for event in [event(0), event(1), event(2)] {
        state = reduce(state, event).unwrap();
    }
    let probability = |token: &str| Logprob {
        token: token.into(),
        logprob: serde_json::Number::from_f64(-0.5).unwrap(),
        bytes: None,
        top_logprobs: None,
    };
    let probabilities = vec![probability("a"), probability("b")];
    let metadata_bytes = serde_json::to_vec(&probabilities).unwrap().len();
    for (fragment, probability) in [
        (
            "x".repeat(MAX_TEXT_BYTES - metadata_bytes),
            probabilities[0].clone(),
        ),
        (String::new(), probabilities[1].clone()),
    ] {
        state = reduce(
            state,
            StreamEvent::Delta {
                item,
                part,
                fragment,
                logprobs: vec![probability],
            },
        )
        .unwrap();
    }
    assert_eq!(
        state.part(item, part).unwrap().logprobs,
        Presence::Value(probabilities.clone())
    );
    assert!(matches!(
        reduce(
            state.clone(),
            StreamEvent::Delta {
                item,
                part,
                fragment: "!".into(),
                logprobs: vec![],
            }
        ),
        Err(EventError::Limit)
    ));
    state = reduce(state, StreamEvent::ValueFinished { item, part }).unwrap();
    let metadata = |logprobs| StreamEvent::TextMetadata {
        item,
        part,
        annotations: vec![],
        logprobs: Presence::Value(logprobs),
    };
    assert!(matches!(
        reduce(
            state.clone(),
            metadata(vec![probability("changed"), probability("b")])
        ),
        Err(EventError::Lifecycle)
    ));
    state = reduce(state, metadata(probabilities)).unwrap();
    for event in [event(5), event(6), event(7)] {
        state = reduce(state, event).unwrap();
    }
    assert_eq!(materialize(&state).unwrap().outcome(), Outcome::Completed);
}

#[test]
fn replay_replacement_refunds_only_its_previous_charge_at_the_stream_limit() {
    let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
    let item = ItemId::new(1);
    state = reduce(
        state,
        StreamEvent::ItemStarted {
            item,
            kind: ItemKind::Message { phase: None },
            replay: None,
        },
    )
    .unwrap();
    // Both parts satisfy their own limit. The second remains open so the
    // last assertion reaches the aggregate charge, not a per-part/lifecycle gate.
    let tail = MAX_TOTAL_BYTES - MAX_TEXT_BYTES - 4;
    assert!(tail < MAX_TEXT_BYTES);
    for (id, len) in [(1, MAX_TEXT_BYTES), (2, tail)] {
        let part = PartId::new(id);
        state = reduce(
            state,
            StreamEvent::PartStarted {
                item,
                part,
                kind: PartKind::Text,
            },
        )
        .unwrap();
        state = reduce(
            state,
            StreamEvent::Delta {
                item,
                part,
                fragment: "x".repeat(len),
                logprobs: vec![],
            },
        )
        .unwrap();
    }
    let part = PartId::new(1);
    state = reduce(state, StreamEvent::ValueFinished { item, part }).unwrap();
    let replay = |size, final_value| {
        let text = Text::new("s".repeat(size), "fixture", MAX_TEXT_BYTES).unwrap();
        StreamEvent::PartReplay {
            item,
            part,
            replay: ItemReplay {
                value: if final_value {
                    ReplayValue::final_value(ReplayFormat::GoogleGenerateContentSignature, text)
                } else {
                    ReplayValue::partial(ReplayFormat::GoogleGenerateContentSignature, text)
                },
                origin: Some(ReplayOrigin::new("fixture").unwrap()),
            },
        }
    };
    state = reduce(state, replay(4, false)).unwrap();
    assert!(matches!(
        reduce(state.clone(), replay(5, true)),
        Err(EventError::Limit)
    ));
    let state = reduce(state, replay(4, true)).unwrap();
    assert_eq!(
        state
            .part(item, part)
            .unwrap()
            .replay
            .as_ref()
            .unwrap()
            .value
            .replay_token(),
        Some("ssss")
    );
    assert!(matches!(
        reduce(
            state,
            StreamEvent::Delta {
                item,
                part: PartId::new(2),
                fragment: "!".into(),
                logprobs: vec![]
            }
        ),
        Err(EventError::Limit)
    ));
}

#[test]
fn owned_replay_belongs_to_assistant_text_and_calls_with_final_values() {
    let text = |value: &str| Text::new(value, "fixture", 64).unwrap();
    let signature = |final_value| {
        if final_value {
            ReplayValue::final_value(ReplayFormat::GoogleGenerateContentSignature, text("s"))
        } else {
            ReplayValue::partial(ReplayFormat::GoogleGenerateContentSignature, text("s"))
        }
    };
    let message = |role, status, content, replay| {
        let mut part = Part::new(PartId::new(2), content);
        part.replay = Some(replay);
        Item::Message(Message {
            role,
            status,
            phase: None,
            parts: vec![part],
        })
    };
    let call = |status, replay| {
        Item::ToolCall(ToolCall {
            replay: Some(replay),
            call_id: text("C"),
            cache: None,
            name: text("lookup"),
            arguments: "{}".into(),
            status,
            context: CallContext::default(),
        })
    };
    let request = |item| {
        GenerationRequest::new(
            vec![
                (
                    ItemId::new(1),
                    Item::Message(Message {
                        role: MessageRole::User,
                        status: ItemLifecycle::Completed,
                        phase: None,
                        parts: vec![Part::new(
                            PartId::new(1),
                            ContentPart::Text(text("go").into()),
                        )],
                    }),
                ),
                (ItemId::new(2), item),
            ],
            GenerationControls::default(),
        )
    };
    let assistant = |status, replay| {
        message(
            MessageRole::Assistant,
            status,
            ContentPart::Text(text("OK").into()),
            replay,
        )
    };
    assert!(request(assistant(ItemLifecycle::Completed, signature(true))).is_ok());
    assert!(request(assistant(ItemLifecycle::Incomplete, signature(false))).is_ok());
    assert!(request(call(ItemLifecycle::Completed, signature(true))).is_ok());
    assert!(request(call(ItemLifecycle::Incomplete, signature(false))).is_ok());
    let mut instruction = Part::new(PartId::new(2), text("rules"));
    instruction.replay = Some(signature(true));
    let reasoning_format =
        ReplayValue::final_value(ReplayFormat::GoogleGenerateContentPart, text("s"));
    for invalid in [
        Item::Instruction(Instruction {
            authority: InstructionAuthority::System,
            parts: vec![instruction],
            status: None,
        }),
        message(
            MessageRole::User,
            ItemLifecycle::Completed,
            ContentPart::Text(text("OK").into()),
            signature(true),
        ),
        message(
            MessageRole::Assistant,
            ItemLifecycle::Completed,
            ContentPart::Refusal(text("no").into()),
            signature(true),
        ),
        assistant(ItemLifecycle::Completed, signature(false)),
        assistant(ItemLifecycle::Completed, reasoning_format.clone()),
        call(ItemLifecycle::Completed, signature(false)),
        call(ItemLifecycle::Completed, reasoning_format.clone()),
    ] {
        assert!(
            matches!(request(invalid), Err(GenerationError::InvalidReplay)),
            "an invalid owner or value must fail at replay validation"
        );
    }

    // The stream reducer applies the same owner rules before materialization.
    let item = ItemId::new(1);
    let replay = |part, final_value| StreamEvent::PartReplay {
        item,
        part,
        replay: ItemReplay {
            value: signature(final_value),
            origin: Some(ReplayOrigin::new("fixture").unwrap()),
        },
    };
    let open = |kind, part_kind| {
        let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
        for event in [
            StreamEvent::ItemStarted {
                item,
                kind,
                replay: None,
            },
            StreamEvent::PartStarted {
                item,
                part: PartId::new(1),
                kind: part_kind,
            },
        ] {
            state = reduce(state, event).unwrap();
        }
        state
    };
    let text_part = open(ItemKind::Message { phase: None }, PartKind::Text);
    assert!(matches!(
        reduce(text_part.clone(), replay(PartId::new(1), true)),
        Err(EventError::Lifecycle)
    ));
    let finished = |state| {
        reduce(
            state,
            StreamEvent::ValueFinished {
                item,
                part: PartId::new(1),
            },
        )
        .unwrap()
    };
    for (kind, part_kind) in [
        (ItemKind::Message { phase: None }, PartKind::Refusal),
        (ItemKind::Reasoning, PartKind::ReasoningText),
    ] {
        assert!(matches!(
            reduce(
                finished(open(kind, part_kind)),
                replay(PartId::new(1), true)
            ),
            Err(EventError::Lifecycle)
        ));
    }
    let mut partial = reduce(finished(text_part), replay(PartId::new(1), false)).unwrap();
    partial = reduce(
        partial,
        StreamEvent::PartFinished {
            item,
            part: PartId::new(1),
        },
    )
    .unwrap();
    assert!(matches!(
        reduce(
            partial.clone(),
            StreamEvent::ItemFinished {
                item,
                status: ItemLifecycle::Completed,
                replay: None,
            }
        ),
        Err(EventError::Lifecycle)
    ));
    assert!(
        reduce(
            partial,
            StreamEvent::ItemFinished {
                item,
                status: ItemLifecycle::Incomplete,
                replay: None,
            }
        )
        .is_ok()
    );
}
