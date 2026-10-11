//! Historical definitions and explicit configuration revisions are not execution grants.
use morphiecore::{
    lowering::generation::RepresentationError,
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            Profile, TargetCaps,
            lowering::{events::check_event, lower_request, lower_response},
        },
    },
    semantic::{
        task::generation::*,
        value::{Presence, Text},
    },
};
use serde_json::json;

fn text(value: &str) -> Text {
    Text::new(value, "synthetic", MAX_TEXT_BYTES).unwrap()
}
fn settings(description: &str) -> GenerationSettings {
    GenerationSettings {
        tools: Some(vec![ToolDefinition::Function(FunctionTool {
            name: text("lookup"),
            description: Some(description.into()),
            parameters: Some(json!({"type":"object","properties":{}}).into()),
            strict: FunctionStrictness::Explicit(false),
            output_schema: None,
            dispatch: ToolDispatch::default(),
        })]),
        ..Default::default()
    }
}
fn snapshot(revision: u64, settings: GenerationSettings) -> ConfigurationSnapshot {
    ConfigurationSnapshot::new(ConfigurationId::new(LocalScope::new(1), revision), settings)
        .unwrap()
}
fn binding(snapshot: ConfigurationSnapshot) -> ToolDefinitionBinding {
    ToolDefinitionBinding::new(
        snapshot,
        ToolReference {
            kind: ToolKind::Function,
            name: text("lookup"),
            namespace: None,
        },
    )
    .unwrap()
}
fn call(id: &str, binding: ToolDefinitionBinding) -> Item {
    Item::ToolCall(ToolCall {
        call_id: text(id),
        name: text("lookup"),
        arguments: "{}".into(),
        status: ItemLifecycle::Completed,
        context: CallContext {
            definition: Some(binding),
            ..Default::default()
        },
    })
}
fn request(item: Item) -> GenerationRequest {
    GenerationRequest::new(vec![(ItemId::new(1), item)], GenerationControls::default()).unwrap()
}
fn old_binding(request: &GenerationRequest) -> &ToolDefinitionBinding {
    let Item::ToolCall(call) = &request.items()[0].1 else {
        panic!("call")
    };
    call.context.definition.as_ref().unwrap()
}

#[test]
fn selected_history_keeps_original_definition_without_inheriting_current_settings() {
    let original = snapshot(1, settings("old contract"));
    let original_binding = binding(original.clone());
    let response = GenerationResponse::new(
        vec![(ItemId::new(1), call("C", original_binding.clone()))],
        Outcome::Completed,
    )
    .unwrap();
    let current = snapshot(2, settings("new contract"));
    let successor = ClientManaged::from_configuration(current.clone())
        .unwrap()
        .select_response(&response)
        .unwrap()
        .append_items(vec![(
            ItemId::new(2),
            Item::ToolResult(ToolResult {
                is_error: None,
                call_id: text("C"),
                output: "reported".into(),
                execution: None,
                status: None,
                context: CallContext::default(),
            }),
        )])
        .unwrap()
        .build(&[])
        .unwrap();
    assert_eq!(successor.configuration_revision(), Some(current.revision()));
    assert_eq!(successor.settings(), current.settings());
    assert_eq!(old_binding(&successor), &original_binding);
    let ToolDefinition::Function(definition) = old_binding(&successor).definition() else {
        panic!("function")
    };
    assert_eq!(definition.description.as_deref(), Some("old contract"));
    assert_eq!(successor.continuation(), Continuation::Unreported);
    let unversioned = ClientManaged::new(GenerationSettings::default())
        .unwrap()
        .select_request(&successor)
        .unwrap()
        .build(&[])
        .unwrap();
    assert_eq!(unversioned.configuration_revision(), None);
    assert!(unversioned.tools().is_empty());
    assert_eq!(old_binding(&unversioned), &original_binding);
    assert_eq!(response.items().len(), 1);
}

#[test]
fn same_revision_checks_schema_order_not_only_json_value_equality() {
    let mut left = settings("ordered");
    let mut right = left.clone();
    let ToolDefinition::Function(tool) = &mut left.tools.as_mut().unwrap()[0] else {
        panic!()
    };
    tool.parameters = Some(
        serde_json::from_str::<serde_json::Value>(
            r#"{"type":"object","properties":{"a":{"type":"string"},"b":{"type":"string"}}}"#,
        )
        .unwrap()
        .into(),
    );
    let ToolDefinition::Function(tool) = &mut right.tools.as_mut().unwrap()[0] else {
        panic!()
    };
    tool.parameters = Some(
        serde_json::from_str::<serde_json::Value>(
            r#"{"type":"object","properties":{"b":{"type":"string"},"a":{"type":"string"}}}"#,
        )
        .unwrap()
        .into(),
    );
    assert_eq!(left, right, "serde value equality ignores object order");
    let source = request(call("C", binding(snapshot(1, left.clone()))))
        .with_configuration(snapshot(1, left))
        .unwrap();
    assert!(source.clone().with_settings(right.clone()).is_err());
    assert!(source.with_configuration(snapshot(1, right)).is_err());
}

#[test]
fn definitions_resolve_by_kind_namespace_and_name_and_results_cannot_own_them() {
    let custom = snapshot(
        3,
        GenerationSettings {
            tools: Some(vec![ToolDefinition::Custom(CustomTool {
                name: text("lookup"),
                description: None,
                format: Some(CustomFormat::Text),
                dispatch: ToolDispatch::default(),
            })]),
            ..Default::default()
        },
    );
    let custom = ToolDefinitionBinding::new(
        custom,
        ToolReference {
            kind: ToolKind::Custom,
            name: text("lookup"),
            namespace: None,
        },
    )
    .unwrap();
    let custom_call = Item::CustomCall(CustomCall {
        call_id: text("X"),
        name: text("lookup"),
        input: "raw".into(),
        context: CallContext {
            definition: Some(custom.clone()),
            ..Default::default()
        },
    });
    let response =
        GenerationResponse::new(vec![(ItemId::new(1), custom_call)], Outcome::Completed).unwrap();
    assert!(
        response
            .clone()
            .with_items(vec![])
            .unwrap()
            .items()
            .is_empty()
    );
    let custom_start = StreamEvent::ItemStarted {
        item: ItemId::new(1),
        replay: None,
        kind: ItemKind::CustomCall {
            call_id: text("X"),
            name: text("lookup"),
            context: CallContext {
                definition: Some(custom.clone()),
                ..Default::default()
            },
        },
    };
    assert!(
        reduce(
            reduce(StreamState::new(), StreamEvent::Started).unwrap(),
            custom_start
        )
        .is_ok()
    );
    assert!(
        GenerationResponse::new(
            vec![(ItemId::new(1), call("C", custom))],
            Outcome::Completed
        )
        .is_err()
    );
    let base = settings("qualified").tools.unwrap().remove(0);
    let config = snapshot(
        1,
        GenerationSettings {
            tools: Some(vec![ToolDefinition::Namespace(ToolNamespace {
                name: text("space"),
                description: String::new(),
                tools: vec![base],
            })]),
            ..Default::default()
        },
    );
    let reference = ToolReference {
        kind: ToolKind::Function,
        name: text("lookup"),
        namespace: Some(text("space")),
    };
    let valid = ToolDefinitionBinding::new(config.clone(), reference.clone()).unwrap();
    for bad in [
        ToolReference {
            namespace: None,
            ..reference.clone()
        },
        ToolReference {
            kind: ToolKind::Custom,
            ..reference.clone()
        },
        ToolReference {
            name: text("missing"),
            ..reference
        },
    ] {
        assert!(ToolDefinitionBinding::new(config.clone(), bad).is_err());
    }
    assert!(
        GenerationRequest::new(
            vec![(ItemId::new(1), call("C", valid.clone()))],
            GenerationControls::default()
        )
        .is_err()
    );
    let mut item = call("C", valid.clone());
    let Item::ToolCall(value) = &mut item else {
        panic!()
    };
    value.context.namespace = Some(text("space"));
    let source = request(item);
    let mut items = source.items().to_vec();
    items.push((
        ItemId::new(2),
        Item::ToolResult(ToolResult {
            is_error: None,
            call_id: text("C"),
            output: "reported".into(),
            execution: None,
            status: None,
            context: CallContext {
                definition: Some(valid),
                ..Default::default()
            },
        }),
    ));
    assert!(source.with_items(items).is_err());
}

#[test]
fn snapshot_budgets_and_debug_do_not_hide_retained_configuration() {
    let mut invalid = settings("invalid");
    invalid.controls.max_output_tokens = Some(0);
    assert!(
        ConfigurationSnapshot::new(ConfigurationId::new(LocalScope::ROOT, 1), invalid).is_err()
    );
    let mut large = settings("private-definition");
    large.instructions = Presence::Value(text(&"x".repeat(MAX_TEXT_BYTES)));
    let retained = binding(snapshot(1, large));
    assert!(!format!("{retained:?}").contains("private-definition"));
    assert!(!format!("{:?}", retained.snapshot()).contains("private-definition"));
    assert!(
        GenerationResponse::new(
            (0..3)
                .map(|n| (ItemId::new(n), call(&n.to_string(), retained.clone())))
                .collect(),
            Outcome::Completed
        )
        .is_ok()
    );
    assert!(matches!(
        GenerationResponse::new(
            (0..4)
                .map(|n| (ItemId::new(n), call(&n.to_string(), retained.clone())))
                .collect(),
            Outcome::Completed
        ),
        Err(GenerationError::Limit)
    ));
}

fn start(binding: ToolDefinitionBinding) -> StreamEvent {
    StreamEvent::ItemStarted {
        item: ItemId::new(1),
        kind: ItemKind::ToolCall {
            format: ArgumentFormat::Raw,
            call_id: text("C"),
            name: text("lookup"),
            message: None,
            context: CallContext {
                definition: Some(binding),
                ..Default::default()
            },
        },
        replay: None,
    }
}
#[test]
fn bound_events_match_static_and_reject_conflicting_revisions_at_intake() {
    let bound = binding(snapshot(1, settings("old")));
    let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
    state = reduce(state, start(bound.clone())).unwrap();
    let mut conflict = start(binding(snapshot(1, settings("changed"))));
    let StreamEvent::ItemStarted {
        item,
        kind: ItemKind::ToolCall { call_id, .. },
        ..
    } = &mut conflict
    else {
        panic!()
    };
    *item = ItemId::new(2);
    *call_id = text("D");
    assert!(reduce(state.clone(), conflict).is_err());
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
        StreamEvent::ItemFinished {
            item: ItemId::new(1),
            status: ItemLifecycle::Completed,
            replay: None,
        },
        StreamEvent::Terminal {
            terminal: StreamTerminal::Completed,
            details: TerminalDetails::default(),
        },
    ] {
        state = reduce(state, event).unwrap();
    }
    let actual = materialize(&state).unwrap();
    let expected =
        GenerationResponse::new(vec![(ItemId::new(1), call("C", bound))], Outcome::Completed)
            .unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn configuration_has_no_private_or_silently_dropped_standard_carrier() {
    let config = snapshot(1, settings("old"));
    let bound = binding(config.clone());
    let response = GenerationResponse::new(
        vec![(ItemId::new(1), call("C", bound.clone()))],
        Outcome::Completed,
    )
    .unwrap();
    let plain = GenerationRequest::from_settings(
        vec![],
        GenerationSettings {
            instructions: Presence::Value(text("hello")),
            ..Default::default()
        },
    )
    .unwrap();
    let versioned = plain
        .clone()
        .with_configuration(snapshot(2, plain.settings().clone()))
        .unwrap();
    for profile in [Profile::Responses, Profile::Chat] {
        let fidelity = FidelityRecords::default();
        let contract = TargetCaps::full();
        assert!(lower_request(&plain, &fidelity, profile, contract.clone()).is_ok());
        assert_eq!(
            lower_request(&versioned, &fidelity, profile, contract.clone()).err(),
            Some(RepresentationError::Admission(
                GenerationFeature::ConfigurationRevision
            ))
        );
        assert_eq!(
            lower_request(
                &request(call("C", bound.clone())),
                &fidelity,
                profile,
                contract.clone()
            )
            .err(),
            Some(RepresentationError::Admission(
                GenerationFeature::ConfigurationRevision
            ))
        );
        assert_eq!(
            lower_response(
                &response,
                &fidelity,
                &crate::events_support::metadata(),
                profile,
                contract.clone()
            )
            .err(),
            Some(RepresentationError::Admission(
                GenerationFeature::ConfigurationRevision
            ))
        );
        assert_eq!(
            check_event(
                &StreamState::new(),
                &start(bound.clone()),
                profile,
                &contract
            ),
            Err(RepresentationError::UnmigratedSemantic)
        );
    }
}
