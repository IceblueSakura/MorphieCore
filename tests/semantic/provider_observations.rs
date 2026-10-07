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
fn complete_actions_preserve_presence_and_match_independent_static_events() {
    let empty = Text::allowing_empty("", "query", MAX_TEXT_BYTES).unwrap();
    for action in [
        ProviderAction::Search {
            query: None,
            queries: None,
        },
        ProviderAction::Search {
            query: Some(empty.clone()),
            queries: Some(vec![]),
        },
        ProviderAction::Search {
            query: Some(text("single")),
            queries: Some(vec![text("second"), text("first"), text("second")]),
        },
        ProviderAction::OpenPage { url: None },
        ProviderAction::OpenPage {
            url: Some(text("https://example.test/page")),
        },
        ProviderAction::FindInPage {
            url: text("https://example.test/page"),
            pattern: empty,
        },
    ] {
        let observed = with_action(action.clone());
        let expected = GenerationResponse::new(
            vec![(ItemId::new(1), Item::ProviderTool(observed.clone()))],
            Outcome::Completed,
        )
        .unwrap();
        let Item::ProviderTool(value) = &expected.items()[0].1 else {
            unreachable!()
        };
        let ProviderOperation::Reported { action: actual, .. } = &value.operation else {
            unreachable!()
        };
        assert_eq!(actual.as_ref(), Some(&action));
        assert!(value.output.is_none() && value.execution.is_none());
        let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
        state = reduce(
            state,
            StreamEvent::ItemStarted {
                item: ItemId::new(1),
                kind: ItemKind::ProviderTool(observed),
                replay: None,
            },
        )
        .unwrap();
        assert!(materialize(&state).is_err());
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
        assert_eq!(materialize(&state).unwrap(), expected);
    }
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
fn three_source_actions_keep_reference_domains_and_do_not_infer_client_execution() {
    // Abstract expectations from provider-actions evidence, not native parsers.
    for (family, action) in [
        (
            "openai.responses",
            ProviderAction::Search {
                query: None,
                queries: None,
            },
        ),
        (
            "google.interactions.v1.step",
            ProviderAction::Search {
                query: None,
                queries: Some(vec![text("one"), text("two")]),
            },
        ),
        (
            "anthropic.messages.web_search_20250305",
            ProviderAction::Search {
                query: Some(text("one")),
                queries: None,
            },
        ),
    ] {
        let mut observed = with_action(action);
        observed.source.source = text(family);
        let r1 = GenerationResponse::new(
            vec![
                (ItemId::scoped(LocalScope::new(1), 1), client_call()),
                (
                    ItemId::scoped(LocalScope::new(1), 2),
                    Item::ProviderTool(observed.clone()),
                ),
            ],
            Outcome::Completed,
        )
        .unwrap();
        let mut result = report();
        result.source = observed.source.clone();
        let r2 = GenerationResponse::new(
            vec![(
                ItemId::scoped(LocalScope::new(2), 1),
                Item::ProviderTool(result.clone()),
            )],
            Outcome::Completed,
        )
        .unwrap();
        let history = ClientManaged::new(GenerationSettings::default())
            .unwrap()
            .select_response(&r1)
            .unwrap()
            .select_response(&r2)
            .unwrap()
            .build(&[])
            .unwrap();
        assert!(
            matches!(history.continuation(), Continuation::ToolResults(ref pending)
            if pending.len() == 1 && pending[0].call_id == "C")
        );
        assert_eq!(result.resolve(history.items()).unwrap(), r1.items()[1].0);
        assert_eq!(r1.items()[1].1, Item::ProviderTool(observed));
        // Same Google brand and native ID are not a shared Step/Part reference domain.
        result.source.source = text("google.generateContent.v1beta.part");
        assert_eq!(
            result.resolve(history.items()),
            Err(AliasResolutionError::Missing)
        );
        assert!(
            ClientManaged::new(GenerationSettings::default())
                .unwrap()
                .select_response(&r1)
                .unwrap()
                .append_items(vec![(
                    ItemId::scoped(LocalScope::new(3), 1),
                    Item::ProviderTool(result)
                )])
                .is_err()
        );
    }
}

#[test]
fn action_limits_cover_fields_collections_aggregate_and_redaction() {
    for length in [MAX_TEXT_BYTES - 1, MAX_TEXT_BYTES, MAX_TEXT_BYTES + 1] {
        let value = Text::allowing_empty("x".repeat(length), "synthetic", length).unwrap();
        for action in [
            ProviderAction::Search {
                query: Some(value.clone()),
                queries: None,
            },
            ProviderAction::Search {
                query: None,
                queries: Some(vec![value.clone()]),
            },
            ProviderAction::OpenPage {
                url: Some(value.clone()),
            },
            ProviderAction::FindInPage {
                url: text("url"),
                pattern: value.clone(),
            },
            ProviderAction::FindInPage {
                url: value.clone(),
                pattern: text("pattern"),
            },
        ] {
            let observed = with_action(action);
            assert_eq!(
                GenerationResponse::new(
                    vec![(ItemId::new(1), Item::ProviderTool(observed.clone()))],
                    Outcome::Completed,
                )
                .is_ok(),
                length <= MAX_TEXT_BYTES
            );
            let state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
            assert_eq!(
                reduce(
                    state,
                    StreamEvent::ItemStarted {
                        item: ItemId::new(1),
                        kind: ItemKind::ProviderTool(observed),
                        replay: None,
                    }
                )
                .is_ok(),
                length <= MAX_TEXT_BYTES
            );
        }
    }
    for count in [MAX_ITEMS - 1, MAX_ITEMS, MAX_ITEMS + 1] {
        let observed = with_action(ProviderAction::Search {
            query: None,
            queries: Some(vec![text("query"); count]),
        });
        assert_eq!(
            GenerationResponse::new(
                vec![(ItemId::new(1), Item::ProviderTool(observed.clone()))],
                Outcome::Completed,
            )
            .is_ok(),
            count <= MAX_ITEMS
        );
        let state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
        assert_eq!(
            reduce(
                state,
                StreamEvent::ItemStarted {
                    item: ItemId::new(1),
                    kind: ItemKind::ProviderTool(observed),
                    replay: None,
                }
            )
            .is_ok(),
            count <= MAX_ITEMS
        );
    }
    for total in [MAX_TOTAL_BYTES - 1, MAX_TOTAL_BYTES, MAX_TOTAL_BYTES + 1] {
        // Independent budget oracle: four Text slots and the reported header.
        let overhead = "abstract-provider".len()
            + "search".len()
            + "S".len()
            + 4 * std::mem::size_of::<Text>();
        let mut queries =
            vec![Text::new("x".repeat(MAX_TEXT_BYTES), "synthetic", MAX_TEXT_BYTES).unwrap(); 3];
        queries.push(
            Text::new(
                "x".repeat(total - overhead - 3 * MAX_TEXT_BYTES),
                "synthetic",
                MAX_TEXT_BYTES,
            )
            .unwrap(),
        );
        let observed = with_action(ProviderAction::Search {
            query: None,
            queries: Some(queries),
        });
        assert_eq!(
            GenerationResponse::new(
                vec![(ItemId::new(1), Item::ProviderTool(observed.clone()))],
                Outcome::Completed,
            )
            .is_ok(),
            total <= MAX_TOTAL_BYTES
        );
        let state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
        assert_eq!(
            reduce(
                state,
                StreamEvent::ItemStarted {
                    item: ItemId::new(1),
                    kind: ItemKind::ProviderTool(observed),
                    replay: None,
                }
            )
            .is_ok(),
            total <= MAX_TOTAL_BYTES
        );
    }
    let action = ProviderAction::Search {
        query: Some(text("private-query")),
        queries: None,
    };
    assert!(!format!("{action:?}").contains("private-query"));
    assert!(!format!("{:?}", with_action(action).operation).contains("private-query"));
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
#[test]
fn result_builders_reject_conflicting_headers_bad_json_and_unclosed_results() {
    let mut header = report();
    header.execution = None;
    let start = |observation| StreamEvent::ItemStarted {
        item: ItemId::new(1),
        kind: ItemKind::ProviderResult {
            observation,
            format: ProviderResultFormat::Json,
        },
        replay: None,
    };
    let fresh = || reduce(StreamState::new(), StreamEvent::Started).unwrap();
    let mut conflict = header.clone();
    conflict.output = Some("already reported".into());
    assert!(reduce(fresh(), start(conflict)).is_err());
    for raw in [r#"{"n":1,"n":2}"#, r#"{"n":"#] {
        let mut state = reduce(fresh(), start(header.clone())).unwrap();
        state = reduce(
            state,
            StreamEvent::PartStarted {
                item: ItemId::new(1),
                part: PartId::new(1),
                kind: PartKind::ResultJson,
            },
        )
        .unwrap();
        assert!(
            reduce(
                state.clone(),
                StreamEvent::PartStarted {
                    item: ItemId::new(1),
                    part: PartId::new(2),
                    kind: PartKind::ResultJson
                }
            )
            .is_err()
        );
        state = reduce(
            state,
            StreamEvent::Delta {
                item: ItemId::new(1),
                part: PartId::new(1),
                fragment: raw.into(),
                logprobs: vec![],
            },
        )
        .unwrap();
        assert!(
            reduce(
                state.clone(),
                StreamEvent::ValueFinished {
                    item: ItemId::new(1),
                    part: PartId::new(1)
                }
            )
            .is_err()
        );
        assert!(
            reduce(
                state,
                StreamEvent::ItemFinished {
                    item: ItemId::new(1),
                    status: ItemLifecycle::Incomplete,
                    replay: None
                }
            )
            .is_err()
        );
    }
    let state = reduce(fresh(), start(header.clone())).unwrap();
    assert!(
        reduce(
            state,
            StreamEvent::ItemFinished {
                item: ItemId::new(1),
                status: ItemLifecycle::Completed,
                replay: None
            }
        )
        .is_err()
    );
    for profile in [
        morphiecore::protocol::openai::Profile::Chat,
        morphiecore::protocol::openai::Profile::Responses,
    ] {
        assert!(
            morphiecore::lowering::events::check_event(
                &fresh(),
                &start(header.clone()),
                profile,
                &morphiecore::lowering::generation::GenerationRepresentationContract::full()
            )
            .is_err()
        );
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
fn mixed_operations_only_require_client_results_and_do_not_infer_execution() {
    let response = GenerationResponse::new(
        vec![
            (ItemId::new(1), client_call()),
            (ItemId::new(2), Item::ProviderTool(operation())),
        ],
        Outcome::Completed,
    )
    .unwrap();
    assert!(
        matches!(response.continuation(),Continuation::ToolResults(ref pending) if pending.len()==1 && pending[0].call_id=="C")
    );
    let Item::ProviderTool(observed) = &response.items()[1].1 else {
        unreachable!()
    };
    assert!(observed.execution.is_none() && observed.output.is_none());
    assert_eq!(response.progress(), InteractionProgress::Unreported);
}
#[test]
fn local_result_only_observation_requires_selected_history_without_a_fabricated_call() {
    let r1 = GenerationResponse::new(
        vec![(
            ItemId::scoped(LocalScope::new(1), 1),
            Item::ProviderTool(operation()),
        )],
        Outcome::Completed,
    )
    .unwrap();
    let observed = report();
    let r2 = GenerationResponse::new(
        vec![(
            ItemId::scoped(LocalScope::new(2), 1),
            Item::ProviderTool(observed.clone()),
        )],
        Outcome::Completed,
    )
    .unwrap();
    assert_eq!(observed.resolve(r1.items()).unwrap(), r1.items()[0].0);
    assert_eq!(
        observed.resolve(&[]).unwrap_err(),
        AliasResolutionError::Missing
    );
    assert!(GenerationRequest::new(r2.items().to_vec(), GenerationControls::default()).is_err());
    let mut items = r1.items().to_vec();
    items.extend_from_slice(r2.items());
    let history = GenerationRequest::new(items, GenerationControls::default()).unwrap();
    assert_eq!(history.continuation(), Continuation::Unreported);
    assert!(
        history
            .retain_items(|id, _| id.scope() != LocalScope::new(1))
            .is_err()
    );
    assert_eq!(r1.items().len(), 1);
}
#[test]
fn resolution_diagnoses_ambiguity_kind_and_domain_instead_of_nearest_match() {
    let observed = report();
    assert_eq!(
        observed
            .resolve(&[(ItemId::new(1), client_call())])
            .unwrap_err(),
        AliasResolutionError::Missing
    );
    let mut wrong = client_call();
    let Item::ToolCall(call) = &mut wrong else {
        unreachable!()
    };
    call.call_id = text("S");
    call.context.alias_domain = Some(domain());
    assert_eq!(
        observed.resolve(&[(ItemId::new(1), wrong)]).unwrap_err(),
        AliasResolutionError::WrongKind
    );
    assert_eq!(
        observed
            .resolve(&[
                (ItemId::new(1), Item::ProviderTool(operation())),
                (ItemId::new(2), Item::ProviderTool(operation()))
            ])
            .unwrap_err(),
        AliasResolutionError::Ambiguous
    );
    let mut other = operation();
    other.source.scope = LocalScope::new(8);
    assert_eq!(
        observed
            .resolve(&[(ItemId::new(1), Item::ProviderTool(other))])
            .unwrap_err(),
        AliasResolutionError::Missing
    );
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
fn no_standard_target_can_drop_provider_observations() {
    // No native branch is enabled by abstract observation support.
    use morphiecore::{
        lowering::generation::{
            GenerationRepresentationContract as Contract, lower_request, lower_response,
        },
        protocol::openai::Profile,
    };
    let response = GenerationResponse::new(
        vec![(ItemId::new(1), Item::ProviderTool(operation()))],
        Outcome::Completed,
    )
    .unwrap();
    let request =
        GenerationRequest::new(response.items().to_vec(), GenerationControls::default()).unwrap();
    let metadata = crate::events_support::metadata();
    for profile in [Profile::Chat, Profile::Responses] {
        assert!(lower_request(&request, &Default::default(), profile, Contract::full()).is_err());
        assert!(
            lower_response(
                &response,
                &Default::default(),
                &metadata,
                profile,
                Contract::full()
            )
            .is_err()
        );
    }
}

#[test]
fn changed_operation_cannot_reuse_identity_or_native_alias_and_proofs_bind_reports() {
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
    let mut observed = operation();
    observed.execution = Some(ToolExecution::Unknown);
    let changed = source
        .clone()
        .with_items(vec![(ItemId::new(1), Item::ProviderTool(observed))])
        .unwrap();
    assert!(proof.check(&changed).is_err());
    let mut other = operation();
    let ProviderOperation::Reported { tool, .. } = &mut other.operation else {
        unreachable!()
    };
    *tool = text("different-operation");
    for owner in [ItemId::new(1), ItemId::new(2)] {
        assert!(
            source
                .clone()
                .with_items(vec![(owner, Item::ProviderTool(other.clone()))])
                .is_err()
        );
    }
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
    let contract =
        morphiecore::lowering::generation::GenerationRepresentationContract::full().semantics;
    assert!(contract.check(&request).is_err());
    assert!(contract.check_response(&response).is_err());
}

#[test]
fn normative_observation_events_match_static_and_cannot_reopen_a_terminal() {
    // Full observed values enter before item and response closure.
    let observed = report();
    let start = StreamEvent::ItemStarted {
        item: ItemId::new(1),
        kind: ItemKind::ProviderTool(observed.clone()),
        replay: None,
    };
    let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
    for profile in [
        morphiecore::protocol::openai::Profile::Chat,
        morphiecore::protocol::openai::Profile::Responses,
    ] {
        assert!(
            morphiecore::lowering::events::check_event(
                &state,
                &start,
                profile,
                &morphiecore::lowering::generation::GenerationRepresentationContract::full()
            )
            .is_err()
        );
    }
    state = reduce(state, start).unwrap();
    assert!(
        reduce(
            state.clone(),
            StreamEvent::Terminal {
                terminal: StreamTerminal::Completed,
                details: TerminalDetails::default()
            }
        )
        .is_err()
    );
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
    let expected = GenerationResponse::new(
        vec![(ItemId::new(1), Item::ProviderTool(observed))],
        Outcome::Completed,
    )
    .unwrap();
    assert_eq!(materialize(&state).unwrap(), expected);
    assert!(reduce(state, StreamEvent::Started).is_err());
}

#[test]
fn aggregate_payload_and_part_identity_limits_apply_to_static_and_events() {
    for length in [MAX_TEXT_BYTES - 1, MAX_TEXT_BYTES, MAX_TEXT_BYTES + 1] {
        let mut observed = report();
        observed.output = Some(ToolOutput::Text("x".repeat(length)));
        let expected = length <= MAX_TEXT_BYTES;
        assert_eq!(
            GenerationResponse::new(
                vec![(ItemId::new(1), Item::ProviderTool(observed.clone()))],
                Outcome::Completed
            )
            .is_ok(),
            expected
        );
        let state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
        assert_eq!(
            reduce(
                state,
                StreamEvent::ItemStarted {
                    item: ItemId::new(1),
                    kind: ItemKind::ProviderTool(observed),
                    replay: None
                }
            )
            .is_ok(),
            expected
        );
    }
    let mut observed = report();
    observed.output = Some(ToolOutput::Parts(vec![(
        PartId::new(1),
        ToolResultPart::Text(text("private-result")),
    )]));
    assert!(!format!("{observed:?}").contains("private-result"));
    let duplicated = vec![
        (ItemId::new(1), Item::ProviderTool(observed.clone())),
        (ItemId::new(2), Item::ProviderTool(observed)),
    ];
    assert_eq!(
        GenerationResponse::new(duplicated, Outcome::Completed).unwrap_err(),
        GenerationError::DuplicatePartId
    );
    let items = (0..5)
        .map(|id| {
            let mut observed = report();
            observed.output = Some(ToolOutput::Text("x".repeat(MAX_TEXT_BYTES)));
            (ItemId::new(id), Item::ProviderTool(observed))
        })
        .collect();
    assert_eq!(
        GenerationResponse::new(items, Outcome::Completed).unwrap_err(),
        GenerationError::Limit
    );
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
    let mut contract = morphiecore::lowering::generation::GenerationRepresentationContract::full();
    contract.semantics.tools = false;
    assert!(
        morphiecore::lowering::events::check_event(
            &state,
            &call,
            morphiecore::protocol::openai::Profile::Chat,
            &contract
        )
        .is_err()
    );
}
