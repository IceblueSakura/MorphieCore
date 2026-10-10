//! A03/T16: alias domains are explicit, independent of response coordinates.
use morphiecore::{
    lowering::generation::{GenerationRepresentationContract, lower_request},
    protocol::{fidelity::FidelityRecords, openai::Profile},
    semantic::{task::generation::*, value::Text},
};
fn text(value: &str) -> Text {
    Text::new(value, "synthetic", 256).unwrap()
}
fn domain(scope: u64) -> NativeAliasDomain {
    NativeAliasDomain {
        source: text("abstract-contract-v0.2"),
        scope: LocalScope::new(scope),
    }
}
fn call(scope: u64) -> (ItemId, Item) {
    (
        ItemId::scoped(LocalScope::new(scope), 1),
        Item::ToolCall(ToolCall {
            call_id: text("same"),
            name: text("lookup"),
            arguments: "{}".into(),
            status: ItemLifecycle::Completed,
            context: CallContext {
                alias_domain: Some(domain(scope)),
                ..Default::default()
            },
        }),
    )
}
fn result(owner_scope: u64, reference_scope: u64) -> (ItemId, Item) {
    (
        ItemId::scoped(LocalScope::new(owner_scope), 2),
        Item::ToolResult(ToolResult {
            call_id: text("same"),
            output: "synthetic result".into(),
            execution: None,
            status: None,
            context: CallContext {
                alias_domain: Some(domain(reference_scope)),
                ..Default::default()
            },
        }),
    )
}
#[test]
fn equal_local_ids_and_native_aliases_in_distinct_domains_do_not_conflict() {
    let items = vec![call(1), call(2), result(3, 1)];
    let request = GenerationRequest::new(items, GenerationControls::default()).unwrap();
    let Continuation::ToolResults(pending) = request.continuation() else {
        panic!("pending")
    };
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].item, ItemId::scoped(LocalScope::new(2), 1));
    let alias = NativeCallAlias {
        domain: Some(domain(1)),
        kind: NativeIdKind::FunctionCall,
        value: text("same"),
    };
    assert_eq!(
        resolve_call_alias(request.items(), &alias).unwrap(),
        ItemId::scoped(LocalScope::new(1), 1)
    );
    assert_ne!(
        PartId::scoped(LocalScope::new(1), 1),
        PartId::scoped(LocalScope::new(2), 1)
    );
}
#[test]
fn missing_ambiguous_and_wrong_kind_aliases_are_distinct() {
    let mut items = vec![call(1)];
    let mut alias = NativeCallAlias {
        domain: Some(domain(2)),
        kind: NativeIdKind::FunctionCall,
        value: text("same"),
    };
    assert_eq!(
        resolve_call_alias(&items, &alias),
        Err(AliasResolutionError::Missing)
    );
    alias.domain = Some(domain(1));
    alias.kind = NativeIdKind::CustomCall;
    assert_eq!(
        resolve_call_alias(&items, &alias),
        Err(AliasResolutionError::WrongKind)
    );
    alias.kind = NativeIdKind::FunctionCall;
    let mut duplicate = call(1);
    duplicate.0 = ItemId::scoped(LocalScope::new(1), 2);
    items.push(duplicate);
    assert_eq!(
        resolve_call_alias(&items, &alias),
        Err(AliasResolutionError::Ambiguous)
    );
    assert_eq!(
        GenerationRequest::new(items, GenerationControls::default()).unwrap_err(),
        GenerationError::DuplicateCall
    );
    assert_eq!(
        GenerationRequest::new(vec![call(1), result(2, 2)], GenerationControls::default())
            .unwrap_err(),
        GenerationError::InvalidToolResult
    );
}
#[test]
fn scoped_wire_ids_do_not_grant_a_carrier_for_ambiguous_target_aliases() {
    let request =
        GenerationRequest::new(vec![call(1), call(2)], GenerationControls::default()).unwrap();
    let mut fidelity = FidelityRecords::default();
    for scope in [1, 2] {
        fidelity
            .record_response_item_id(ItemId::scoped(LocalScope::new(scope), 1), "same-wire-id")
            .unwrap();
    }
    assert!(
        fidelity
            .record_response_item_id(ItemId::scoped(LocalScope::new(1), 2), "same-wire-id")
            .is_err()
    );
    for profile in [Profile::Responses, Profile::Chat] {
        assert!(
            lower_request(
                &request,
                &fidelity,
                profile,
                GenerationRepresentationContract::full()
            )
            .is_err()
        );
    }
}

#[test]
fn event_and_static_calls_share_domain_isolation_without_a_wire_projection() {
    let items = vec![call(1), call(2)];
    let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
    for (owner, item) in &items {
        let Item::ToolCall(call) = item else {
            unreachable!()
        };
        let start = StreamEvent::ItemStarted {
            item: *owner,
            kind: ItemKind::ToolCall {
                format: ArgumentFormat::Raw,
                call_id: call.call_id.clone(),
                name: call.name.clone(),
                message: None,
                context: call.context.clone(),
            },
            replay: None,
        };
        for profile in [Profile::Responses, Profile::Chat] {
            assert!(
                morphiecore::lowering::events::check_event(
                    &state,
                    &start,
                    profile,
                    &GenerationRepresentationContract::full()
                )
                .is_err()
            );
        }
        state = reduce(state, start).unwrap();
        let part = PartId::scoped(owner.scope(), 1);
        for event in [
            StreamEvent::PartStarted {
                item: *owner,
                part,
                kind: PartKind::Arguments,
            },
            StreamEvent::Delta {
                item: *owner,
                part,
                fragment: "{}".into(),
                logprobs: vec![],
            },
            StreamEvent::ValueFinished { item: *owner, part },
            StreamEvent::PartFinished { item: *owner, part },
            StreamEvent::ItemFinished {
                item: *owner,
                status: ItemLifecycle::Completed,
                replay: None,
            },
        ] {
            state = reduce(state, event).unwrap();
        }
    }
    state = reduce(
        state,
        StreamEvent::Terminal {
            terminal: StreamTerminal::Completed,
            details: TerminalDetails::default(),
        },
    )
    .unwrap();
    let observed = materialize(&state).unwrap();
    let expected = GenerationResponse::new(items, Outcome::Completed).unwrap();
    assert_eq!(observed, expected);
}

#[test]
fn alias_lookup_binds_source_and_checks_its_independent_budget() {
    let items = vec![call(1)];
    let mut alias = NativeCallAlias {
        domain: Some(domain(1)),
        kind: NativeIdKind::FunctionCall,
        value: text("same"),
    };
    alias.domain.as_mut().unwrap().source = text("different-declared-source");
    assert_eq!(
        resolve_call_alias(&items, &alias),
        Err(AliasResolutionError::Missing)
    );
    for length in [255, 256, 257] {
        alias.domain.as_mut().unwrap().source =
            Text::new("s".repeat(length), "synthetic", 257).unwrap();
        assert_eq!(
            resolve_call_alias(&items, &alias),
            Err(if length <= 256 {
                AliasResolutionError::Missing
            } else {
                AliasResolutionError::Limit
            })
        );
    }
}

#[test]
fn scoped_local_wire_labels_are_deterministic_and_target_collisions_fail_before_emission() {
    use morphiecore::lowering::generation::lower_response;
    use morphiecore::protocol::{
        ResponseMetadata,
        openai::{envelope, events::EventEncoder},
    };
    let metadata = ResponseMetadata {
        id: "synthetic-response".into(),
        model: "synthetic".into(),
        created: Some(1.into()),
        context: Default::default(),
        instruction_fidelity: Default::default(),
    };
    let items = [1, 2].map(|scope| {
        (
            ItemId::scoped(LocalScope::new(scope), 1),
            Item::Message(Message {
                role: MessageRole::Assistant,
                parts: vec![],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        )
    });
    let response = GenerationResponse::new(items.to_vec(), Outcome::Completed).unwrap();
    let mut fidelity = FidelityRecords::default();
    let target = lower_response(
        &response,
        &fidelity,
        &metadata,
        Profile::Responses,
        GenerationRepresentationContract::full(),
    )
    .unwrap();
    let wire = envelope::encode_response(&target).unwrap();
    assert_eq!(wire["output"][0]["id"], "item_1_1");
    assert_eq!(wire["output"][1]["id"], "item_2_1");
    assert!(
        fidelity.response_item_id(items[0].0).is_none(),
        "generated output labels must not become upstream reports"
    );
    for (owner, _) in &items {
        fidelity.record_response_item_id(*owner, "same").unwrap();
    }
    assert!(
        lower_response(
            &response,
            &fidelity,
            &metadata,
            Profile::Responses,
            GenerationRepresentationContract::full()
        )
        .is_err()
    );
    let mut encoder = EventEncoder::new(Profile::Responses, metadata).unwrap();
    encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
    encoder
        .encode(
            &StreamEvent::ItemStarted {
                item: items[0].0,
                kind: ItemKind::Message { phase: None },
                replay: None,
            },
            &fidelity,
        )
        .unwrap();
    assert!(
        encoder
            .encode(
                &StreamEvent::ItemStarted {
                    item: items[1].0,
                    kind: ItemKind::Message { phase: None },
                    replay: None
                },
                &fidelity
            )
            .is_err()
    );
    assert!(
        encoder
            .encode(
                &StreamEvent::Terminal {
                    terminal: StreamTerminal::Completed,
                    details: TerminalDetails::default()
                },
                &fidelity
            )
            .is_err()
    );
}
