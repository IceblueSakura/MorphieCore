//! Namespace expectations are independent of the encoder and provider catalog.
use morphiecore::{
    protocol::openai::{Profile, TargetCaps as Contract, lowering::lower_request, responses},
    semantic::{task::generation::*, value::Text},
};
use serde_json::json;

fn text(value: &str) -> Text {
    Text::new(value, "fixture", 1024).unwrap()
}
#[test]
fn typed_groups_calls_and_results_keep_qualified_identity_without_name_rewriting() {
    use morphiecore::protocol::fidelity::FidelityRecords;
    let groups = ["left", "right"].map(|namespace| {
        ToolDefinition::Namespace(ToolNamespace {
            name: text(namespace),
            description: format!("{namespace} tools"),
            tools: vec![ToolDefinition::Function(FunctionTool {
                name: text("status"),
                description: None,
                parameters: None,
                strict: FunctionStrictness::Explicit(false),
                output_schema: None,
                dispatch: ToolDispatch::default(),
            })],
        })
    });
    let mut items = vec![];
    for (i, namespace) in ["left", "right"].iter().enumerate() {
        let call_id = text(&format!("call_{i}"));
        items.push((
            ItemId::new((i * 2 + 1) as u64),
            Item::ToolCall(ToolCall {
                call_id: call_id.clone(),
                name: text("status"),
                arguments: "{ }".into(),
                status: ItemLifecycle::Completed,
                context: CallContext {
                    namespace: Some(text(namespace)),
                    ..Default::default()
                },
            }),
        ));
        items.push((
            ItemId::new((i * 2 + 2) as u64),
            Item::ToolResult(ToolResult {
                is_error: None,
                call_id,
                output: ToolOutput::Text(format!("{namespace} result")),
                status: None,
                execution: None,
                context: CallContext::default(),
            }),
        ));
    }
    let request = GenerationRequest::from_settings(
        items,
        GenerationSettings {
            tools: Some(groups.to_vec()),
            tool_choice: Some(ToolChoice::Auto),
            ..Default::default()
        },
    )
    .unwrap();
    let fidelity = FidelityRecords::default();
    let representation =
        lower_request(&request, &fidelity, Profile::Responses, Contract::full()).unwrap();
    let wire = responses::encode_generation(&representation).unwrap();
    assert_eq!(
        wire["input"],
        json!([
            {"type":"function_call","call_id":"call_0","name":"status","namespace":"left","arguments":"{ }"},
            {"type":"function_call_output","call_id":"call_0","output":"left result"},
            {"type":"function_call","call_id":"call_1","name":"status","namespace":"right","arguments":"{ }"},
            {"type":"function_call_output","call_id":"call_1","output":"right result"}
        ])
    );
    assert_eq!(
        wire["tools"],
        json!([
            {"type":"namespace","name":"left","description":"left tools","tools":[{"type":"function","name":"status","strict":false}]},
            {"type":"namespace","name":"right","description":"right tools","tools":[{"type":"function","name":"status","strict":false}]}
        ])
    );
    assert!(lower_request(&request, &fidelity, Profile::Chat, Contract::full()).is_err());
    let mut changed = request.items().to_vec();
    let Item::ToolResult(result) = &mut changed[1].1 else {
        unreachable!()
    };
    result.call_id = text("missing");
    assert!(request.clone().with_items(changed).is_err());
    let mut changed = request.items().to_vec();
    let Item::ToolResult(result) = &mut changed[1].1 else {
        unreachable!()
    };
    result.context.namespace = Some(text("right"));
    assert!(request.clone().with_items(changed).is_err());
    let mut settings = request.settings().clone();
    settings.tools = None;
    settings.tool_choice = None;
    let without_definitions = request.with_settings(settings).unwrap();
    let wire = responses::encode_generation(
        &lower_request(
            &without_definitions,
            &fidelity,
            Profile::Responses,
            Contract::full(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(wire.get("tools").is_none());
    assert_eq!(wire["input"][0]["namespace"], "left");
}

#[test]
fn grouped_definitions_and_qualified_selection_are_not_flattened() {
    let wire = json!({"input":[{"role":"user","content":"check"}],
        "tools":[
            {"type":"namespace","name":"left","description":"first","tools":[
                {"type":"function","name":"status","strict":false}]},
            {"type":"namespace","name":"right","description":"second","tools":[
                {"type":"function","name":"status","strict":false}]}],
        "tool_choice":"required"});
    let decoded = responses::decode_generation(&wire).expect("standard namespace");
    let representation = lower_request(
        &decoded.semantic,
        &decoded.fidelity,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    let encoded = responses::encode_generation(&representation).unwrap();
    assert_eq!(encoded["tools"], wire["tools"]);
    assert_eq!(encoded["tool_choice"], wire["tool_choice"]);
    assert!(
        lower_request(
            &decoded.semantic,
            &decoded.fidelity,
            Profile::Chat,
            Contract::full(),
        )
        .is_err()
    );
    let mut settings = decoded.semantic.settings().clone();
    settings.tool_choice = Some(ToolChoice::Qualified(ToolReference {
        kind: ToolKind::Function,
        name: Text::new("status", "name", 128).unwrap(),
        namespace: Some(Text::new("right", "namespace", 128).unwrap()),
    }));
    let selected = decoded
        .semantic
        .clone()
        .with_settings(settings.clone())
        .unwrap();
    assert!(
        lower_request(
            &selected,
            &decoded.fidelity,
            Profile::Responses,
            Contract::full()
        )
        .is_err()
    );
    settings.tools.as_mut().unwrap().pop();
    assert!(decoded.semantic.with_settings(settings).is_err());
}

#[test]
fn namespace_rejects_duplicates_nested_groups_and_unqualified_selection() {
    let group = json!({"type":"namespace","name":"ops","description":"operations",
        "tools":[{"type":"function","name":"status","strict":false}]});
    let wire = json!({"input":"check","tools":[group.clone()], "tool_choice":"auto"});
    assert!(responses::decode_generation(&wire).is_ok());
    let mut duplicate = wire.clone();
    duplicate["tools"][0]["tools"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"function","name":"status","strict":false}));
    assert!(responses::decode_generation(&duplicate).is_err());
    let mut nested = wire.clone();
    nested["tools"][0]["tools"] = json!([group]);
    assert!(responses::decode_generation(&nested).is_err());
    let mut missing = wire;
    missing["tool_choice"] = json!({"type":"function","name":"status"});
    assert!(responses::decode_generation(&missing).is_err());
    missing["tool_choice"]["namespace"] = json!("ops");
    assert!(responses::decode_generation(&missing).is_err());
}
#[test]
fn grouped_leaf_requirements_and_total_budget_cannot_bypass_admission() {
    use morphiecore::protocol::fidelity::FidelityRecords;
    let tools = vec![ToolDefinition::Namespace(ToolNamespace {
        name: text("ops"),
        description: String::new(),
        tools: vec![ToolDefinition::Custom(CustomTool {
            name: text("execute"),
            description: None,
            format: None,
            dispatch: Default::default(),
        })],
    })];
    let request = GenerationRequest::from_settings(
        vec![],
        GenerationSettings {
            instructions: morphiecore::semantic::value::Presence::Value(text("run")),
            tools: Some(tools.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    let requirements = GenerationRequirements::derive(&request);
    assert!(requirements.custom_tools);
    assert_eq!(requirements.tool_count, 1);
    let mut contract = Contract::full();
    contract.semantics.custom_tools = false;
    assert!(
        lower_request(
            &request,
            &FidelityRecords::default(),
            Profile::Responses,
            contract
        )
        .is_err()
    );
    let mut settings = request.settings().clone();
    let Some(ToolDefinition::Namespace(group)) =
        settings.tools.as_mut().and_then(|t| t.first_mut())
    else {
        unreachable!()
    };
    group.tools = (0..MAX_TOOLS)
        .map(|i| {
            ToolDefinition::Custom(CustomTool {
                name: text(&format!("tool_{i}")),
                description: None,
                format: None,
                dispatch: Default::default(),
            })
        })
        .collect();
    assert!(request.with_settings(settings).is_err());
}
