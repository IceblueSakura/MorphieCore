//! Prefix stability is an explicit dependency check, not a cache hit or route choice.
use morphiecore::{
    adapter::{Adapter, Dialect},
    protocol::openai::Profile,
    semantic::{
        cache::{CachePrefixIntent, CachePrefixScope},
        task::generation::*,
        value::Presence,
    },
};
use serde_json::{Value, json};
fn request() -> morphiecore::adapter::Request {
    Adapter::new(Profile::Responses, Dialect::Standard, None)
        .decode_request(
            &serde_json::to_vec(
                &json!({"model":"synthetic","input":[{"role":"user","content":"prefix"}]}),
            )
            .unwrap(),
        )
        .unwrap()
}
fn scope() -> CachePrefixScope {
    CachePrefixScope::new("synthetic-cache-scope").unwrap()
}
fn intent() -> CachePrefixIntent {
    CachePrefixIntent::through(ItemId::new(1))
}
#[test]
fn adapter_prefix_check_allows_append_and_rejects_prefix_context_and_scope_edits() {
    let source = request();
    let proof = source.capture_cache_prefix(intent(), &scope()).unwrap();
    source.check_cache_prefix(&proof, &scope()).unwrap();
    let mut append = source.clone();
    let mut items = append.task.semantic.items().to_vec();
    let mut new_item = items[0].clone();
    new_item.0 = ItemId::new(2);
    let Item::Message(m) = &mut new_item.1 else {
        panic!("message")
    };
    m.parts[0].id = PartId::new(2);
    items.push(new_item.clone());
    append.task.semantic = append.task.semantic.with_items(items).unwrap();
    append.check_cache_prefix(&proof, &scope()).unwrap();
    let inserted = source
        .task
        .semantic
        .clone()
        .with_items(vec![new_item, source.task.semantic.items()[0].clone()])
        .unwrap();
    append.task.semantic = inserted;
    assert!(append.check_cache_prefix(&proof, &scope()).is_err());
    let mut changed = request();
    let mut items = changed.task.semantic.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        panic!("message")
    };
    message.parts[0].content = ContentPart::Text(
        TextContent::new(
            morphiecore::semantic::value::Text::new("changed", "fixture", 32).unwrap(),
            vec![],
            Presence::Absent,
        )
        .unwrap(),
    );
    changed.task.semantic = changed.task.semantic.with_items(items).unwrap();
    assert!(changed.check_cache_prefix(&proof, &scope()).is_err());
    let mut changed = request();
    changed.model = "other".into();
    assert!(changed.check_cache_prefix(&proof, &scope()).is_err());
    let mut changed = request();
    changed.context.cache.prompt_cache_key = Presence::Value("new-key".into());
    assert!(changed.check_cache_prefix(&proof, &scope()).is_err());
    let mut changed = request();
    changed.context.service_tier =
        Presence::Value(morphiecore::semantic::context::ServiceTier::Priority);
    assert!(changed.check_cache_prefix(&proof, &scope()).is_err());
    assert!(
        source
            .check_cache_prefix(&proof, &CachePrefixScope::new("other-scope").unwrap())
            .is_err()
    );
    let mut changed = request();
    changed.context.metadata = Presence::Value(std::collections::BTreeMap::from([(
        "label".into(),
        "changed".into(),
    )]));
    assert!(changed.check_cache_prefix(&proof, &scope()).is_err());
    let mut changed = request();
    changed.cache_session =
        Some(morphiecore::protocol::cache::CacheSession::new("new-group").unwrap());
    assert!(changed.check_cache_prefix(&proof, &scope()).is_err());
}
#[test]
fn prefix_settings_bind_schema_and_tool_order_without_a_second_value_authority() {
    let mut source = request();
    let tools: Vec<ToolDefinition> = ["one", "two"]
        .into_iter()
        .map(|name| {
            ToolDefinition::Function(FunctionTool {
                name: morphiecore::semantic::value::Text::new(name, "fixture", 32).unwrap(),
                description: None,
                parameters: None,
                output_schema: None,
                strict: FunctionStrictness::Explicit(false),
                dispatch: ToolDispatch::default(),
            })
        })
        .collect();
    source.task.semantic = source
        .task
        .semantic
        .with_tool_settings(Some(tools), None, None)
        .unwrap();
    let proof = source.capture_cache_prefix(intent(), &scope()).unwrap();
    let mut settings = source.task.semantic.settings().clone();
    settings.tools.as_mut().unwrap().reverse();
    let mut changed = source.clone();
    changed.task.semantic = changed.task.semantic.with_settings(settings).unwrap();
    assert!(changed.check_cache_prefix(&proof, &scope()).is_err());
    let mut settings = source.task.semantic.settings().clone();
    settings.controls.max_output_tokens = Some(100);
    changed.task.semantic = source
        .task
        .semantic
        .clone()
        .with_settings(settings)
        .unwrap();
    assert!(changed.check_cache_prefix(&proof, &scope()).is_err());
    let mut settings = source.task.semantic.settings().clone();
    settings.text.presence = true;
    settings.text.format = Presence::Value(OutputConstraint::JsonSchema {
        name: morphiecore::semantic::value::Text::new("answer", "fixture", 32).unwrap(),
        description: None,
        strict: Some(false),
        schema: serde_json::from_str(
            r#"{"type":"object","properties":{"a":{"type":"string"},"b":{"type":"string"}}}"#,
        )
        .unwrap(),
    });
    source.task.semantic = source
        .task
        .semantic
        .with_settings(settings.clone())
        .unwrap();
    let proof = source.capture_cache_prefix(intent(), &scope()).unwrap();
    let Presence::Value(OutputConstraint::JsonSchema { schema, .. }) = &mut settings.text.format
    else {
        panic!("schema")
    };
    schema["properties"] =
        serde_json::from_str(r#"{"b":{"type":"string"},"a":{"type":"string"}}"#).unwrap();
    changed.task.semantic = source.task.semantic.with_settings(settings).unwrap();
    assert!(changed.check_cache_prefix(&proof, &scope()).is_err());
}
#[test]
fn cache_boundary_cannot_split_a_declared_message_group() {
    let wire: Value = json!({"model":"synthetic","messages":[{"role":"assistant","content":null,"tool_calls":[{"id":"c","type":"function","function":{"name":"lookup","arguments":"{}"}}]}]});
    let source = Adapter::new(Profile::Chat, Dialect::Standard, None)
        .decode_request(&serde_json::to_vec(&wire).unwrap())
        .unwrap();
    assert!(
        source
            .capture_cache_prefix(CachePrefixIntent::through(ItemId::new(1)), &scope())
            .is_err()
    );
    let intent = CachePrefixIntent::through(ItemId::new(2));
    let proof = source.capture_cache_prefix(intent, &scope()).unwrap();
    let mut extended = source.clone();
    let mut items = extended.task.semantic.items().to_vec();
    let mut call = items[1].clone();
    call.0 = ItemId::new(3);
    let Item::ToolCall(value) = &mut call.1 else {
        panic!("call")
    };
    value.call_id = morphiecore::semantic::value::Text::new("new", "fixture", 32).unwrap();
    items.push(call);
    extended.task.semantic = extended.task.semantic.with_items(items).unwrap();
    assert!(extended.check_cache_prefix(&proof, &scope()).is_err());
}
