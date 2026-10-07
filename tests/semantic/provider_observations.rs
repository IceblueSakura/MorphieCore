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
        source: domain(),
        operation: ProviderOperation::Reported {
            tool: text("search"),
            alias: Some(text("S")),
            requester: ProviderRequester::Model,
        },
        progress: None,
        execution: None,
        output: None,
        artifact_status: None,
    }
}
fn report() -> ProviderToolObservation {
    ProviderToolObservation {
        source: domain(),
        operation: ProviderOperation::Reference(ProviderOperationReference::Native(text("S"))),
        progress: None,
        execution: Some(ToolExecution::Unknown),
        output: None,
        artifact_status: None,
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
