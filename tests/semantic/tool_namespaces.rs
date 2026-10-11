//! Namespace expectations are independent of the encoder and provider catalog.
use morphiecore::{
    protocol::openai::{Profile, TargetCaps as Contract, lowering::lower_request, responses},
    semantic::{task::generation::*, value::Text},
};
use serde_json::json;

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
