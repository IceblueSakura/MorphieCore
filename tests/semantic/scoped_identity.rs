//! A03/T16: alias domains are explicit, independent of response coordinates.
use morphiecore::semantic::{task::generation::*, value::Text};
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
            is_error: None,
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
