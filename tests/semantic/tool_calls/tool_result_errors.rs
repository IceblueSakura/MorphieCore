//! Error reports and execution facts have independent presence and admission.
use morphiecore::{
    protocol::openai::{GenerationApi, chat::envelope as chat_envelope, responses::envelope},
    semantic::{task::generation::*, value::Text},
};

fn text(s: &str) -> Text {
    Text::new(s, "synthetic", 256).unwrap()
}
fn history(
    is_error: Option<bool>,
    execution: Option<ToolExecution>,
) -> Result<GenerationRequest, GenerationError> {
    GenerationRequest::new(
        vec![
            (
                ItemId::new(1),
                Item::ToolCall(ToolCall {
                    replay: None,
                    call_id: text("c"),
                    cache: None,
                    name: text("lookup"),
                    arguments: "{}".into(),
                    status: ItemLifecycle::Completed,
                    context: Default::default(),
                }),
            ),
            (
                ItemId::new(2),
                Item::ToolResult(ToolResult {
                    call_id: Some(text("c")),
                    name: None,
                    output: "actual feedback".into(),
                    cache: None,
                    is_error,
                    execution,
                    status: None,
                    context: Default::default(),
                }),
            ),
        ],
        GenerationControls::default(),
    )
}
#[test]
fn not_executed_and_execution_failure_remain_distinct_with_the_same_feedback() {
    for fact in [
        ToolExecution::NotExecuted,
        ToolExecution::Failed { code: None },
    ] {
        let request = history(Some(true), Some(fact.clone())).unwrap();
        let Item::ToolResult(result) = &request.items()[1].1 else {
            panic!()
        };
        assert_eq!(result.execution, Some(fact));
        assert_eq!(result.output, ToolOutput::Text("actual feedback".into()));
    }
    assert!(history(Some(true), Some(ToolExecution::Succeeded)).is_err());
    assert!(history(Some(false), Some(ToolExecution::Failed { code: None })).is_err());
    assert!(history(None, Some(ToolExecution::Failed { code: None })).is_ok());
}
#[test]
fn standard_codecs_cannot_silently_omit_an_error_report() {
    for profile in [GenerationApi::Chat, GenerationApi::Responses] {
        let body = match profile {
            GenerationApi::Chat => br#"{"model":"synthetic","messages":[{"role":"assistant","tool_calls":[{"id":"c","type":"function","function":{"name":"lookup","arguments":"{}"}}]},{"role":"tool","tool_call_id":"c","content":"feedback"}]}"#.as_slice(),
            GenerationApi::Responses => br#"{"model":"synthetic","input":[{"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"},{"type":"function_call_output","call_id":"c","output":"feedback"}]}"#.as_slice(),
        };
        let mut decoded = match profile {
            GenerationApi::Chat => chat_envelope::decode_request_bytes(body).unwrap().task,
            GenerationApi::Responses => envelope::decode_request_bytes(body).unwrap().task,
        };
        let mut items = decoded.semantic.items().to_vec();
        let result = items
            .iter_mut()
            .find_map(|(_, item)| match item {
                Item::ToolResult(result) => Some(result),
                _ => None,
            })
            .unwrap();
        result.is_error = Some(false);
        decoded.semantic = decoded.semantic.with_items(items).unwrap();
        assert!(
            morphiecore::protocol::openai::lowering::lower_request(
                &decoded.semantic,
                &decoded.fidelity,
                profile,
                morphiecore::protocol::openai::TargetContract::full(),
            )
            .is_err()
        );
    }
}
