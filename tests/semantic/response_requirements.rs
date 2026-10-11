//! Output requirements do not manufacture a request or inherit request controls.
use crate::events_support::text;
use morphiecore::semantic::task::generation::*;

#[test]
fn output_requirements_still_enforce_tool_and_reasoning_domains() {
    let response = GenerationResponse::new(
        vec![(
            ItemId::new(1),
            Item::ToolCall(ToolCall {
                call_id: text("c"),
                name: text("lookup"),
                arguments: "{}".into(),
                status: ItemLifecycle::Completed,
                context: CallContext::default(),
            }),
        )],
        Outcome::Completed,
    )
    .unwrap();
    let q = GenerationResponseRequirements::derive(&response);
    assert!(q.tools);
    let mut semantic = GenerationSemanticContract::full();
    semantic.tools = false;
    assert_eq!(
        semantic.check_response(&response).err(),
        Some(GenerationFeature::Tools)
    );
    let changed = response.with_items(vec![]).unwrap();
    assert!(!GenerationResponseRequirements::derive(&changed).tools);
}
