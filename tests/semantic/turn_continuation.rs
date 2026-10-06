//! Explicit local relations never manufacture wire identity or execution permission.
use crate::events_support::text;
use morphiecore::semantic::task::generation::*;

fn call(id: u64, name: &str) -> (ItemId, Item) {
    (
        ItemId::new(id),
        Item::ToolCall(ToolCall {
            call_id: text(name),
            name: text("lookup"),
            arguments: "{}".into(),
            message: None,
            status: ItemLifecycle::Completed,
            context: CallContext::default(),
        }),
    )
}
fn result(id: u64, call_id: &str, status: ItemLifecycle) -> (ItemId, Item) {
    (
        ItemId::new(id),
        Item::ToolResult(ToolResult {
            execution: None,
            call_id: text(call_id),
            output: "done".into(),
            status: Some(status),
            context: CallContext::default(),
        }),
    )
}
fn history(items: Vec<(ItemId, Item)>) -> GenerationRequest {
    GenerationRequest::new(items, GenerationControls::default()).unwrap()
}
fn relation() -> ResponseRelation {
    ResponseRelation::new(TurnId::new(7), ResponseId::new(10))
}

#[test]
fn parallel_exchange_distinguishes_pending_partial_and_complete_results() {
    let response =
        GenerationResponse::new(vec![call(1, "a"), call(2, "b")], Outcome::Completed).unwrap();
    let exchange = ResponseContinuation::new(relation(), &response);
    let request = history(response.items().to_vec());
    assert!(
        matches!(exchange.inspect(&request).unwrap(), ResultReadiness::Awaiting { ref missing, ref in_progress } if missing.len() == 2 && in_progress.is_empty())
    );
    let mut items = request.items().to_vec();
    items.push(result(3, "b", ItemLifecycle::InProgress));
    let partial = history(items.clone());
    assert!(matches!(
        exchange.inspect(&partial).unwrap(),
        ResultReadiness::Awaiting { ref missing, ref in_progress }
            if missing[0].call_id == "a" && in_progress[0].call_id == "b"
    ));
    assert!(
        exchange
            .advance(&partial, ResponseId::new(11), &response)
            .is_err()
    );
    let Item::ToolResult(r) = &mut items[2].1 else {
        panic!("result")
    };
    r.status = Some(ItemLifecycle::Incomplete);
    let request = history(items.clone());
    assert!(
        matches!(exchange.inspect(&request).unwrap(), ResultReadiness::Awaiting { ref missing, ref in_progress } if missing[0].call_id == "a" && in_progress.is_empty())
    );
    items.push(result(4, "a", ItemLifecycle::Completed));
    let request = history(items);
    assert_eq!(
        exchange.inspect(&request).unwrap(),
        ResultReadiness::ResultsComplete
    );
    let next_response = GenerationResponse::new(vec![call(5, "c")], Outcome::Completed).unwrap();
    let next = exchange
        .advance(&request, ResponseId::new(11), &next_response)
        .unwrap();
    assert_eq!(next.relation().turn(), TurnId::new(7));
    assert_eq!(next.relation().previous(), Some(ResponseId::new(10)));
    assert_eq!(response.outcome(), Outcome::Completed);
    assert!(
        matches!(response.continuation(), Continuation::ToolResults(ref calls) if calls.len() == 2)
    );
    let mut items = request.items().to_vec();
    items.extend_from_slice(next_response.items());
    assert!(
        matches!(next.inspect(&history(items)).unwrap(), ResultReadiness::Awaiting { ref missing, .. } if missing[0].call_id == "c")
    );
    assert!(
        next.advance(&request, ResponseId::new(10), &next_response)
            .is_err()
    );
}

#[test]
fn scope_binds_final_call_values_and_does_not_infer_turn_completion() {
    let response = GenerationResponse::new(vec![call(1, "a")], Outcome::Completed).unwrap();
    let exchange = ResponseContinuation::new(relation(), &response);
    let request = history(vec![call(1, "a"), result(2, "a", ItemLifecycle::Completed)]);
    assert!(
        exchange
            .advance(&request, ResponseId::new(10), &response)
            .is_err()
    );
    for items in [vec![call(9, "a")], vec![call(1, "changed")]] {
        assert_eq!(
            exchange.inspect(&history(items)).unwrap_err(),
            ContinuationError::ChangedResponse
        );
    }
    let mut items = vec![call(1, "a")];
    let Item::ToolCall(c) = &mut items[0].1 else {
        panic!("call")
    };
    c.arguments = "edited".into();
    assert_eq!(
        exchange.inspect(&history(items)).unwrap_err(),
        ContinuationError::ChangedResponse
    );
    let empty = GenerationResponse::new(vec![], Outcome::Completed).unwrap();
    assert_eq!(
        ResponseContinuation::new(relation(), &empty)
            .inspect(&request)
            .unwrap(),
        ResultReadiness::Unreported
    );
    assert!(
        ResponseContinuation::new(relation(), &empty)
            .advance(&request, ResponseId::new(11), &response)
            .is_err()
    );
    let failed = GenerationResponse::new(vec![], Outcome::Failed).unwrap();
    assert_eq!(
        ResponseContinuation::new(relation(), &failed)
            .inspect(&request)
            .unwrap(),
        ResultReadiness::Unreported
    );
}

#[test]
fn declared_dependencies_are_checked_even_after_results_arrive() {
    let response = GenerationResponse::new(vec![call(1, "a")], Outcome::Completed).unwrap();
    let source = history(response.items().to_vec());
    let proofs = [RequestDependencyProof::capture(
        &source,
        HistoryDependency::PrefixThrough(ItemId::new(1)),
        morphiecore::semantic::task::generation::SettingsDependency::All,
    )
    .unwrap()];
    let exchange = ResponseContinuation::new(relation(), &response).with_dependencies(&proofs);
    let complete = history(vec![call(1, "a"), result(2, "a", ItemLifecycle::Completed)]);
    assert_eq!(
        exchange.inspect(&complete).unwrap(),
        ResultReadiness::ResultsComplete
    );
    exchange
        .advance(&complete, ResponseId::new(11), &response)
        .unwrap();
    let mut settings = complete.settings().clone();
    settings.controls.max_output_tokens = Some(100);
    let changed = complete.with_settings(settings).unwrap();
    assert_eq!(
        exchange.inspect(&changed).unwrap_err(),
        ContinuationError::ChangedDependency
    );
    assert_eq!(
        exchange
            .advance(&changed, ResponseId::new(11), &response)
            .unwrap_err(),
        ContinuationError::ChangedDependency
    );
    let inserted = history(vec![call(3, "earlier"), call(1, "a")]);
    assert_eq!(
        exchange.inspect(&inserted).unwrap_err(),
        ContinuationError::ChangedDependency
    );
}

#[test]
fn unrelated_history_partial_calls_do_not_obscure_this_response_scope() {
    let response = GenerationResponse::new(vec![call(1, "a")], Outcome::Completed).unwrap();
    let mut unrelated = call(3, "other");
    let Item::ToolCall(c) = &mut unrelated.1 else {
        panic!("call")
    };
    c.status = ItemLifecycle::InProgress;
    let request = history(vec![
        call(1, "a"),
        result(2, "a", ItemLifecycle::Completed),
        unrelated,
    ]);
    assert_eq!(request.continuation(), Continuation::Unreported);
    assert_eq!(
        ResponseContinuation::new(relation(), &response)
            .inspect(&request)
            .unwrap(),
        ResultReadiness::ResultsComplete
    );
}
