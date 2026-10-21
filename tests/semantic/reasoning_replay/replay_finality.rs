use morphiecore::semantic::{
    task::generation::*,
    value::{ReplayOrigin, Text},
};

fn value(
    format: ReplayFormat,
    final_value: bool,
    payload: &str,
    origin: Option<&str>,
) -> ItemReplay {
    let text = Text::new(payload, "fixture", 1024).unwrap();
    ItemReplay {
        value: if final_value {
            ReplayValue::final_value(format, text)
        } else {
            ReplayValue::partial(format, text)
        },
        origin: origin.map(|s| ReplayOrigin::new(s).unwrap()),
    }
}
fn ready(kind: ItemKind, replay: ItemReplay) -> StreamState {
    let call = matches!(kind, ItemKind::ToolCall { .. });
    let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
    state = reduce(
        state,
        StreamEvent::ItemStarted {
            item: ItemId::new(1),
            kind,
            replay: Some(replay),
        },
    )
    .unwrap();
    if call {
        for event in [
            StreamEvent::PartStarted {
                item: ItemId::new(1),
                part: PartId::new(1),
                kind: PartKind::Arguments,
            },
            StreamEvent::Delta {
                item: ItemId::new(1),
                part: PartId::new(1),
                fragment: "{}".into(),
                logprobs: vec![],
            },
            StreamEvent::ValueFinished {
                item: ItemId::new(1),
                part: PartId::new(1),
            },
            StreamEvent::PartFinished {
                item: ItemId::new(1),
                part: PartId::new(1),
            },
        ] {
            state = reduce(state, event).unwrap();
        }
    }
    state
}
fn call() -> ItemKind {
    ItemKind::ToolCall {
        call_id: Text::new("call", "fixture", 64).unwrap(),
        name: Text::new("lookup", "fixture", 64).unwrap(),
        format: ArgumentFormat::Raw,
        context: Default::default(),
    }
}
fn close(status: ItemLifecycle, replay: Option<ItemReplay>) -> StreamEvent {
    StreamEvent::ItemFinished {
        item: ItemId::new(1),
        status,
        replay,
    }
}

#[test]
fn final_item_replay_cannot_be_revoked_or_rewritten_at_closure() {
    for (kind, format) in [
        (ItemKind::Reasoning, ReplayFormat::ResponsesEncrypted),
        (ItemKind::Compaction, ReplayFormat::ResponsesCompaction),
        (call(), ReplayFormat::GoogleGenerateContentSignature),
        (call(), ReplayFormat::GoogleInteractionsV1FunctionCall),
    ] {
        let final_value = value(format, true, "synthetic-final", Some("origin-a"));
        let state = ready(kind, final_value.clone());
        let closed = reduce(
            state.clone(),
            close(ItemLifecycle::Completed, Some(final_value.clone())),
        )
        .unwrap();
        assert_eq!(
            closed.item(ItemId::new(1)).unwrap().replay,
            Some(final_value)
        );
        assert!(matches!(
            reduce(state.clone(), close(ItemLifecycle::Completed, None)),
            Err(EventError::Lifecycle)
        ));
        assert!(matches!(
            reduce(
                state.clone(),
                close(
                    ItemLifecycle::Incomplete,
                    Some(value(format, false, "synthetic-final", Some("origin-a")))
                )
            ),
            Err(EventError::Lifecycle)
        ));
        assert!(matches!(
            reduce(
                state,
                close(
                    ItemLifecycle::Completed,
                    Some(value(format, true, "synthetic-edited", Some("origin-a")))
                )
            ),
            Err(EventError::Identity)
        ));
    }
}

#[test]
fn partial_call_replay_may_finish_but_cannot_change_intake_origin() {
    let format = ReplayFormat::GoogleInteractionsV1FunctionCall;
    for (old, new) in [
        (Some("origin-a"), Some("origin-b")),
        (None, Some("origin-a")),
        (Some("origin-a"), None),
    ] {
        let state = ready(call(), value(format, false, "prefix", old));
        assert!(matches!(
            reduce(
                state,
                close(
                    ItemLifecycle::Completed,
                    Some(value(format, true, "final", new))
                )
            ),
            Err(EventError::Identity)
        ));
    }
    let state = ready(call(), value(format, false, "prefix", Some("origin-a")));
    let closed = reduce(
        state.clone(),
        close(
            ItemLifecycle::Completed,
            Some(value(format, true, "final", Some("origin-a"))),
        ),
    )
    .unwrap();
    assert_eq!(
        closed
            .item(ItemId::new(1))
            .unwrap()
            .replay
            .as_ref()
            .unwrap()
            .value
            .replay_token(),
        Some("final")
    );
    let incomplete = reduce(
        state,
        close(
            ItemLifecycle::Incomplete,
            Some(value(format, false, "prefix", Some("origin-a"))),
        ),
    )
    .unwrap();
    assert_eq!(
        incomplete.item(ItemId::new(1)).unwrap().status,
        Some(ItemLifecycle::Incomplete)
    );
    assert_eq!(
        incomplete
            .item(ItemId::new(1))
            .unwrap()
            .replay
            .as_ref()
            .unwrap()
            .value
            .replay_token(),
        None
    );
}

#[test]
fn function_call_format_does_not_widen_reasoning_or_part_purpose() {
    let format = ReplayFormat::GoogleInteractionsV1FunctionCall;
    let replay = value(format, true, "synthetic-call", Some("origin-a"));
    assert!(replay.validate(&call()).is_ok());
    assert!(replay.validate(&ItemKind::Reasoning).is_err());
    assert!(replay.validate(&ItemKind::Compaction).is_err());
    let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
    for event in [
        StreamEvent::ItemStarted {
            item: ItemId::new(1),
            kind: ItemKind::Message { phase: None },
            replay: None,
        },
        StreamEvent::PartStarted {
            item: ItemId::new(1),
            part: PartId::new(1),
            kind: PartKind::Text,
        },
        StreamEvent::Delta {
            item: ItemId::new(1),
            part: PartId::new(1),
            fragment: "X".into(),
            logprobs: vec![],
        },
        StreamEvent::ValueFinished {
            item: ItemId::new(1),
            part: PartId::new(1),
        },
    ] {
        state = reduce(state, event).unwrap();
    }
    let attach = |replay| StreamEvent::PartReplay {
        item: ItemId::new(1),
        part: PartId::new(1),
        replay,
    };
    assert!(reduce(state.clone(), attach(replay)).is_err());
    assert!(
        reduce(
            state,
            attach(value(
                ReplayFormat::GoogleGenerateContentSignature,
                true,
                "synthetic-part",
                Some("origin-a")
            ))
        )
        .is_ok()
    );
    let oversized = ReplayValue::final_value(
        format,
        Text::new(
            "x".repeat(MAX_TEXT_BYTES + 1),
            "oversized",
            MAX_TEXT_BYTES + 1,
        )
        .unwrap(),
    );
    assert!(oversized.validate().is_err());
}
