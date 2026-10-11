//! Abstract Provider observations, not synthetic Google/Anthropic codecs.
use morphiecore::semantic::{task::generation::*, value::Text};

fn text(value: &str) -> Text {
    Text::new(value, "synthetic", 256).unwrap()
}
fn domain() -> NativeAliasDomain {
    NativeAliasDomain {
        source: text("abstract-provider"),
        scope: LocalScope::new(7),
    }
}
fn operation() -> ProviderToolObservation {
    ProviderToolObservation {
        replay: None,
        source: domain(),
        operation: ProviderOperation::Reported {
            tool: text("search"),
            alias: Some(text("S")),
            requester: ProviderRequester::Model,
            action: Some(ProviderAction::Search {
                query: Some(text("synthetic query")),
                queries: None,
            }),
        },
        progress: None,
        execution: None,
        output: None,
        artifact_status: None,
    }
}
fn report() -> ProviderToolObservation {
    ProviderToolObservation {
        replay: None,
        source: domain(),
        operation: ProviderOperation::Reference(ProviderOperationReference::Native(text("S"))),
        progress: None,
        execution: Some(ToolExecution::Unknown),
        output: None,
        artifact_status: None,
    }
}
fn with_action(action: ProviderAction) -> ProviderToolObservation {
    let mut observed = operation();
    let ProviderOperation::Reported { action: value, .. } = &mut observed.operation else {
        unreachable!()
    };
    *value = Some(action);
    observed
}

#[test]
fn action_edits_require_new_identity_and_derivation_without_retargeting_results() {
    let source = GenerationRequest::new(
        vec![(ItemId::new(1), Item::ProviderTool(operation()))],
        GenerationControls::default(),
    )
    .unwrap();
    let proof = RequestDependencyProof::capture(
        &source,
        HistoryDependency::Owners(vec![ItemId::new(1)]),
        SettingsDependency::All,
    )
    .unwrap();
    let mut changed = with_action(ProviderAction::Search {
        query: Some(text("changed query")),
        queries: None,
    });
    for owner in [ItemId::new(1), ItemId::new(2)] {
        assert!(
            source
                .clone()
                .with_items(vec![(owner, Item::ProviderTool(changed.clone()))])
                .is_err()
        );
    }
    // Independently constructed values still invalidate a content dependency.
    let rebuilt = GenerationRequest::new(
        vec![(ItemId::new(1), Item::ProviderTool(changed.clone()))],
        GenerationControls::default(),
    )
    .unwrap();
    assert!(proof.check(&rebuilt).is_err());
    let ProviderOperation::Reported { alias, .. } = &mut changed.operation else {
        unreachable!()
    };
    *alias = Some(text("new-S"));
    let replacement = (ItemId::new(3), Item::ProviderTool(changed.clone()));
    let revised = source
        .clone()
        .revise_call(ItemId::new(1), replacement.clone())
        .unwrap();
    assert_eq!(
        revised.call_derivations().get(&ItemId::new(3)),
        Some(&ItemId::new(1))
    );
    assert_eq!(revised.continuation(), Continuation::Unreported);
    assert!(!revised.items()[0].1.is_call());
    assert!(proof.check(&revised).is_err());
    let atomic = source
        .clone()
        .transform(
            vec![ContextEdit::ReviseCall {
                source: ItemId::new(1),
                replacement: replacement.clone(),
            }],
            &[],
        )
        .unwrap();
    assert_eq!(atomic.request(), &revised);
    assert_eq!(
        atomic.changes(),
        &[ContextChange::CallRevised {
            source: ItemId::new(1),
            owner: ItemId::new(3)
        }]
    );
    let with_result = source
        .clone()
        .with_items(vec![
            (ItemId::new(1), Item::ProviderTool(operation())),
            (ItemId::new(2), Item::ProviderTool(report())),
        ])
        .unwrap();
    assert!(
        with_result
            .clone()
            .revise_call(ItemId::new(1), replacement.clone())
            .is_err()
    );
    assert!(
        with_result
            .clone()
            .transform(
                vec![ContextEdit::ReviseCall {
                    source: ItemId::new(1),
                    replacement: replacement.clone(),
                }],
                &[]
            )
            .is_err()
    );
    let repaired = with_result
        .transform(
            vec![
                ContextEdit::Delete(vec![ItemId::new(2)]),
                ContextEdit::ReviseCall {
                    source: ItemId::new(1),
                    replacement,
                },
            ],
            &[],
        )
        .unwrap();
    assert_eq!(repaired.request(), &revised);
    assert_eq!(source.items()[0].1, Item::ProviderTool(operation()));
    assert_eq!(report().resolve(source.items()).unwrap(), ItemId::new(1));
    assert_eq!(
        report().resolve(revised.items()),
        Err(AliasResolutionError::Missing)
    );
    for dimension in 0..4 {
        // Each reported dimension must stay on the original operation.
        let mut fabricated = changed.clone();
        match dimension {
            0 => fabricated.progress = Some(ProviderExecutionProgress::Running),
            1 => fabricated.execution = Some(ToolExecution::Succeeded),
            2 => fabricated.output = Some("old report".into()),
            _ => fabricated.artifact_status = Some(ItemLifecycle::Completed),
        }
        assert!(
            source
                .clone()
                .revise_call(
                    ItemId::new(1),
                    (ItemId::new(4), Item::ProviderTool(fabricated.clone())),
                )
                .is_err()
        );
        assert!(
            source
                .clone()
                .transform(
                    vec![ContextEdit::ReviseCall {
                        source: ItemId::new(1),
                        replacement: (ItemId::new(4), Item::ProviderTool(fabricated)),
                    }],
                    &[]
                )
                .is_err()
        );
    }
    assert!(
        source
            .clone()
            .revise_call(ItemId::new(1), (ItemId::new(4), client_call()))
            .is_err()
    );
    // A result reference is not an operation proposal.
    assert!(
        source
            .revise_call(
                ItemId::new(1),
                (ItemId::new(4), Item::ProviderTool(report()))
            )
            .is_err()
    );
}

#[test]
fn action_dependencies_bind_kind_presence_order_and_each_parameter() {
    let source = GenerationRequest::new(
        vec![(ItemId::new(1), Item::ProviderTool(operation()))],
        GenerationControls::default(),
    )
    .unwrap();
    // Even an unchanged action cannot relabel the old invocation as a new proposal.
    let replacement = (ItemId::new(2), Item::ProviderTool(operation()));
    assert!(
        source
            .clone()
            .revise_call(ItemId::new(1), replacement.clone())
            .is_err()
    );
    assert!(
        source
            .transform(
                vec![ContextEdit::ReviseCall {
                    source: ItemId::new(1),
                    replacement,
                }],
                &[]
            )
            .is_err()
    );
    let actions = [
        None,
        Some(ProviderAction::Search {
            query: None,
            queries: None,
        }),
        Some(ProviderAction::Search {
            query: Some(text("a")),
            queries: None,
        }),
        Some(ProviderAction::Search {
            query: Some(text("b")),
            queries: None,
        }),
        Some(ProviderAction::Search {
            query: None,
            queries: Some(vec![]),
        }),
        Some(ProviderAction::Search {
            query: None,
            queries: Some(vec![text("a"), text("b")]),
        }),
        Some(ProviderAction::Search {
            query: None,
            queries: Some(vec![text("b"), text("a")]),
        }),
        Some(ProviderAction::OpenPage { url: None }),
        Some(ProviderAction::OpenPage {
            url: Some(text("a")),
        }),
        Some(ProviderAction::OpenPage {
            url: Some(text("b")),
        }),
        Some(ProviderAction::FindInPage {
            url: text("a"),
            pattern: text("a"),
        }),
        Some(ProviderAction::FindInPage {
            url: text("a"),
            pattern: text("b"),
        }),
        Some(ProviderAction::FindInPage {
            url: text("b"),
            pattern: text("a"),
        }),
    ];
    let requests: Vec<_> = actions
        .into_iter()
        .map(|action| {
            let mut observed = operation();
            let ProviderOperation::Reported { action: value, .. } = &mut observed.operation else {
                unreachable!()
            };
            *value = action;
            GenerationRequest::new(
                vec![(ItemId::new(1), Item::ProviderTool(observed))],
                GenerationControls::default(),
            )
            .unwrap()
        })
        .collect();
    for (i, source) in requests.iter().enumerate() {
        let proof = RequestDependencyProof::capture(
            source,
            HistoryDependency::Owners(vec![ItemId::new(1)]),
            SettingsDependency::All,
        )
        .unwrap();
        for (j, candidate) in requests.iter().enumerate() {
            assert_eq!(proof.check(candidate).is_ok(), i == j);
        }
    }
}

#[test]
fn fragmented_provider_results_have_independent_static_values_and_no_snapshot_repair() {
    for (format, parts, expected) in [
        (
            ProviderResultFormat::Text,
            vec!["你", "好"],
            ToolOutput::Text("你好".into()),
        ),
        (
            ProviderResultFormat::Text,
            vec![""],
            ToolOutput::Text(String::new()),
        ),
        (
            ProviderResultFormat::Json,
            vec![r#"{"n":"#, "9007199254740993", "}"],
            ToolOutput::Structured(
                StructuredValue::from_bytes(br#"{"n":9007199254740993}"#).unwrap(),
            ),
        ),
    ] {
        for fragments in [parts.clone(), vec![]] {
            let joined = parts.concat();
            let fragments = if fragments.is_empty() {
                vec![joined.as_str()]
            } else {
                fragments
            };
            let mut observed = report();
            observed.execution = None;
            let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
            state = reduce(
                state,
                StreamEvent::ItemStarted {
                    item: ItemId::new(1),
                    kind: ItemKind::ProviderResult {
                        observation: observed.clone(),
                        format,
                    },
                    replay: None,
                },
            )
            .unwrap();
            state = reduce(
                state,
                StreamEvent::PartStarted {
                    item: ItemId::new(1),
                    part: PartId::new(1),
                    kind: if format == ProviderResultFormat::Json {
                        PartKind::ResultJson
                    } else {
                        PartKind::ResultText
                    },
                },
            )
            .unwrap();
            for fragment in fragments {
                state = reduce(
                    state,
                    StreamEvent::Delta {
                        item: ItemId::new(1),
                        part: PartId::new(1),
                        fragment: fragment.into(),
                        logprobs: vec![],
                    },
                )
                .unwrap();
            }
            assert!(snapshot_items(&state).is_err());
            assert!(
                reduce(
                    state.clone(),
                    StreamEvent::ItemFinished {
                        item: ItemId::new(1),
                        status: ItemLifecycle::Completed,
                        replay: None
                    }
                )
                .is_err()
            );
            state = reduce(
                state,
                StreamEvent::ValueFinished {
                    item: ItemId::new(1),
                    part: PartId::new(1),
                },
            )
            .unwrap();
            state = reduce(
                state,
                StreamEvent::PartFinished {
                    item: ItemId::new(1),
                    part: PartId::new(1),
                },
            )
            .unwrap();
            state = reduce(
                state,
                StreamEvent::ItemFinished {
                    item: ItemId::new(1),
                    status: ItemLifecycle::Completed,
                    replay: None,
                },
            )
            .unwrap();
            state = reduce(
                state,
                StreamEvent::Terminal {
                    terminal: StreamTerminal::Completed,
                    details: TerminalDetails::default(),
                },
            )
            .unwrap();
            observed.output = Some(expected.clone());
            assert_eq!(
                materialize(&state).unwrap(),
                GenerationResponse::new(
                    vec![(ItemId::new(1), Item::ProviderTool(observed))],
                    Outcome::Completed
                )
                .unwrap()
            );
            assert!(
                reduce(
                    state,
                    StreamEvent::ValueFinished {
                        item: ItemId::new(1),
                        part: PartId::new(1)
                    }
                )
                .is_err()
            );
        }
    }
}
fn client_call() -> Item {
    Item::ToolCall(ToolCall {
        call_id: text("C"),
        name: text("lookup"),
        arguments: r#"{"executor":"provider"}"#.into(),
        status: ItemLifecycle::Completed,
        context: CallContext::default(),
    })
}
#[test]
fn report_payload_execution_and_artifact_facts_are_independent_and_bounded() {
    let mut observed = report();
    observed.execution = None;
    assert!(
        GenerationResponse::new(
            vec![(ItemId::new(1), Item::ProviderTool(observed.clone()))],
            Outcome::Completed
        )
        .is_err()
    );
    observed.execution = Some(ToolExecution::Cancelled);
    let response = GenerationResponse::new(
        vec![(ItemId::new(1), Item::ProviderTool(observed.clone()))],
        Outcome::Completed,
    )
    .unwrap();
    assert_eq!(response.outcome(), Outcome::Completed);
    observed.execution = Some(ToolExecution::Failed {
        code: Some(text("denied")),
    });
    observed.output = Some("diagnostic".into());
    observed.artifact_status = Some(ItemLifecycle::Incomplete);
    GenerationResponse::new(
        vec![(ItemId::new(1), Item::ProviderTool(observed.clone()))],
        Outcome::Completed,
    )
    .unwrap();
    observed.output = Some(ToolOutput::Text("x".repeat(MAX_TEXT_BYTES + 1)));
    assert!(
        GenerationResponse::new(
            vec![(ItemId::new(1), Item::ProviderTool(observed))],
            Outcome::Completed
        )
        .is_err()
    );
}

#[test]
fn local_references_need_no_native_alias_and_later_facts_do_not_erase_unknown_reports() {
    let mut initial = operation();
    let ProviderOperation::Reported { alias, .. } = &mut initial.operation else {
        unreachable!()
    };
    *alias = None;
    let mut unknown = report();
    unknown.operation =
        ProviderOperation::Reference(ProviderOperationReference::Local(ItemId::new(1)));
    let mut later = unknown.clone();
    later.execution = Some(ToolExecution::Succeeded);
    later.output = Some(ToolOutput::Text(String::new()));
    let history = GenerationRequest::new(
        vec![
            (ItemId::new(1), Item::ProviderTool(initial)),
            (ItemId::new(2), Item::ProviderTool(unknown.clone())),
            (ItemId::new(3), Item::ProviderTool(later)),
        ],
        GenerationControls::default(),
    )
    .unwrap();
    assert_eq!(history.items()[1].1, Item::ProviderTool(unknown));
    assert_eq!(history.continuation(), Continuation::Unreported);
    let mut changed = history.items().to_vec();
    changed.swap(0, 1);
    assert!(history.with_items(changed).is_err());
}

#[test]
fn provider_observations_are_not_implicitly_admitted_by_existing_product_contracts() {
    // A core type is not a product capability.
    let response = GenerationResponse::new(
        vec![(ItemId::new(1), Item::ProviderTool(operation()))],
        Outcome::Completed,
    )
    .unwrap();
    let request =
        GenerationRequest::new(response.items().to_vec(), GenerationControls::default()).unwrap();
    let contract = morphiecore::protocol::openai::TargetCaps::full().semantics;
    assert!(contract.check(&request).is_err());
    assert!(contract.check_response(&response).is_err());
}

#[test]
fn stream_alias_conflicts_and_existing_call_capability_checks_remain_strict() {
    let state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
    let state = reduce(
        state,
        StreamEvent::ItemStarted {
            item: ItemId::new(1),
            kind: ItemKind::ProviderTool(operation()),
            replay: None,
        },
    )
    .unwrap();
    assert!(
        reduce(
            state,
            StreamEvent::ItemStarted {
                item: ItemId::new(2),
                kind: ItemKind::ProviderTool(operation()),
                replay: None
            }
        )
        .is_err()
    );
    let state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
    let state = reduce(
        state,
        StreamEvent::ItemStarted {
            item: ItemId::new(1),
            kind: ItemKind::Message { phase: None },
            replay: None,
        },
    )
    .unwrap();
    let call = StreamEvent::ItemStarted {
        item: ItemId::new(2),
        kind: ItemKind::ToolCall {
            format: ArgumentFormat::Raw,
            call_id: text("C"),
            name: text("lookup"),
            message: Some(ItemId::new(1)),
            context: CallContext::default(),
        },
        replay: None,
    };
    let mut contract = morphiecore::protocol::openai::TargetCaps::full();
    contract.semantics.tools = false;
    assert!(
        morphiecore::protocol::openai::lowering::events::check_event(
            &state,
            &call,
            morphiecore::protocol::openai::Profile::Chat,
            &contract
        )
        .is_err()
    );
}
