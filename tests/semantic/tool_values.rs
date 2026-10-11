//! Raw/structured authority and execution reports are independent of wire carriers.
use crate::events_support::text;
use morphiecore::{
    lowering::generation::RepresentationError,
    protocol::{
        fidelity::FidelityRecords,
        openai::{Profile, TargetCaps as Contract, lowering::lower_request},
    },
    semantic::task::generation::*,
};
use serde_json::json;

fn call(arguments: ToolArguments) -> (ItemId, Item) {
    (
        ItemId::new(1),
        Item::ToolCall(ToolCall {
            call_id: text("c"),
            name: text("lookup"),
            arguments,
            status: ItemLifecycle::Completed,
            context: CallContext::default(),
        }),
    )
}
fn history(arguments: ToolArguments, execution: Option<ToolExecution>) -> GenerationRequest {
    GenerationRequest::new(
        vec![
            call(arguments),
            (
                ItemId::new(2),
                Item::ToolResult(ToolResult {
                    is_error: None,
                    call_id: text("c"),
                    output: ToolOutput::Structured(
                        StructuredValue::new(json!({"diagnostic":42})).unwrap(),
                    ),
                    execution,
                    status: Some(ItemLifecycle::Completed),
                    context: CallContext::default(),
                }),
            ),
        ],
        GenerationControls::default(),
    )
    .unwrap()
}

#[test]
fn structured_arguments_and_failure_payload_keep_separate_authorities() {
    let arguments = StructuredValue::new(json!({"z":7,"a":null})).unwrap();
    let request = history(
        ToolArguments::Structured(arguments.clone()),
        Some(ToolExecution::Failed {
            code: Some(text("not_found")),
        }),
    );
    let Item::ToolCall(c) = &request.items()[0].1 else {
        panic!("call")
    };
    assert_eq!(c.arguments.as_raw(), None);
    assert_eq!(c.arguments.as_structured(), Some(&arguments));
    let q = GenerationRequirements::derive(&request);
    assert!(q.structured_arguments && q.structured_tool_results && q.tool_execution_reports);
    assert_eq!(request.continuation(), Continuation::Unreported);
    let response =
        GenerationResponse::new(vec![request.items()[0].clone()], Outcome::Completed).unwrap();
    assert_eq!(
        ResponseContinuation::new(
            ResponseRelation::new(TurnId::new(1), ResponseId::new(1)),
            &response
        )
        .inspect(&request)
        .unwrap(),
        ResultReadiness::ResultsComplete
    );
    assert_eq!(response.outcome(), Outcome::Completed);
}

#[test]
fn missing_argument_or_execution_carriers_reject_even_success_reports() {
    for execution in [
        Some(ToolExecution::Succeeded),
        Some(ToolExecution::Failed { code: None }),
    ] {
        let mut request = history("{}".into(), execution);
        let mut items = request.items().to_vec();
        let Item::ToolResult(result) = &mut items[1].1 else {
            panic!("result")
        };
        result.output = "plain".into();
        request = request.with_items(items).unwrap();
        for profile in [Profile::Chat, Profile::Responses] {
            assert_eq!(
                lower_request(
                    &request,
                    &FidelityRecords::default(),
                    profile,
                    Contract::full()
                )
                .err(),
                Some(RepresentationError::Tools)
            );
        }
    }
    let request = GenerationRequest::new(
        vec![call(ToolArguments::Structured(
            StructuredValue::new(json!({})).unwrap(),
        ))],
        GenerationControls::default(),
    )
    .unwrap();
    for profile in [Profile::Chat, Profile::Responses] {
        assert_eq!(
            lower_request(
                &request,
                &FidelityRecords::default(),
                profile,
                Contract::full()
            )
            .err(),
            Some(RepresentationError::Tools)
        );
    }
}
