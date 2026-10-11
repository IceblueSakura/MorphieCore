//! Prefix stability is an explicit dependency check, not a cache hit or route choice.
use morphiecore::{
    protocol::openai::{Adapter, Dialect, Profile},
    semantic::{
        cache::{CachePrefixIntent, CachePrefixScope},
        task::generation::*,
        value::Presence,
    },
};
use serde_json::json;
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
    let mut changed = request();
    changed.conversation = Some(
        morphiecore::semantic::context::ConversationContext::conversation("logical-one").unwrap(),
    );
    assert!(changed.check_cache_prefix(&proof, &scope()).is_err());
    let scoped = changed.capture_cache_prefix(intent(), &scope()).unwrap();
    changed.conversation = Some(
        morphiecore::semantic::context::ConversationContext::independent_request("logical-one")
            .unwrap(),
    );
    assert!(changed.check_cache_prefix(&scoped, &scope()).is_err());
}

#[test]
fn retired_cache_part_records_cannot_reappear_on_a_later_value() {
    let source = request();
    let Item::Message(message) = &source.task.semantic.items()[0].1 else {
        panic!()
    };
    let owner = message.parts[0].id;
    let foreign = PartId::scoped(LocalScope::new(99), owner.get());
    let mut fidelity = morphiecore::protocol::fidelity::FidelityRecords::default();
    fidelity.record_cache_breakpoint(owner).unwrap();
    fidelity.record_cache_breakpoint(foreign).unwrap();
    fidelity.retain_owners(source.task.semantic.items());
    assert!(fidelity.cache_breakpoint(owner));
    assert!(!fidelity.cache_breakpoint(foreign));
    fidelity.retain_owners(&[]);
    assert!(!fidelity.cache_breakpoint(owner));
    fidelity.retain_owners(source.task.semantic.items());
    assert!(!fidelity.cache_breakpoint(owner));
}
