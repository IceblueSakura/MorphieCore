//! Raw/structured authority and execution reports are independent of wire carriers.
use crate::events_support::text;
use morphiecore::{
    lowering::generation::{
        GenerationRepresentationContract as Contract, RepresentationError, lower_request,
    },
    protocol::{fidelity::FidelityRecords, openai::Profile},
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
            message: None,
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
fn edits_recompute_argument_and_execution_requirements_without_restoring_values() {
    let request = history(
        ToolArguments::Structured(StructuredValue::new(json!({"n":1})).unwrap()),
        Some(ToolExecution::Succeeded),
    );
    let mut items = request.items().to_vec();
    let Item::ToolCall(call) = &mut items[0].1 else {
        panic!("call")
    };
    call.arguments = "not valid JSON".into();
    let Item::ToolResult(result) = &mut items[1].1 else {
        panic!("result")
    };
    result.execution = None;
    result.output = "replacement".into();
    let changed = request.with_items(items).unwrap();
    let q = GenerationRequirements::derive(&changed);
    assert!(!q.structured_arguments && !q.structured_tool_results && !q.tool_execution_reports);
    for profile in [Profile::Chat, Profile::Responses] {
        lower_request(
            &changed,
            &FidelityRecords::default(),
            profile,
            Contract::full(),
        )
        .unwrap();
    }
    let only_item = changed.items()[0].clone();
    let only_call = changed.with_items(vec![only_item]).unwrap();
    assert!(matches!(
        only_call.continuation(),
        Continuation::ToolResults(_)
    ));
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
