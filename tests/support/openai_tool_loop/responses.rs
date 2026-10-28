//! OpenRouter Responses wire, phase and stored/stateless effective-history expectations.
use super::RESULT;
use serde_json::{Value, json};

pub fn snapshot(turn: usize) -> Value {
    json!({"id":format!("response-{turn}"),"object":"response","created_at":1,
        "completed_at":2,"model":"openai/gpt-6-luna","status":"completed","error":null,
        "reasoning":{"effort":"low","summary":"detailed","context":"all_turns","mode":"standard"},
        "service_tier":"default","text":null,
        "incomplete_details":null,"output":if turn == 1 {json!([
            {"id":"reason","type":"reasoning","status":"completed",
                "summary":[{"type":"summary_text","text":"Plan 🧪"}],
                "content":[{"type":"reasoning_text","text":"Check"}],
                "encrypted_content":"synthetic-final-token"},
            {"id":"comment","type":"message","role":"assistant","phase":"commentary",
                "status":"completed","content":[{"type":"output_text","text":"checking","annotations":[],"logprobs":[]}]},
            {"id":"call-item","type":"function_call","status":"completed","call_id":"call-lookup",
                "name":"lookup","arguments":"{ \"key\": \"A\" }"},
            {"id":"second-item","type":"function_call","status":"completed","call_id":"call-second",
                "name":"lookup","arguments":"{\"key\":\"B\"}"}
        ])}else{json!([
            {"id":format!("answer-{turn}"),"type":"message","role":"assistant","phase":"final_answer",
                "status":"completed","content":[{"type":"output_text","text":"cedar-71","annotations":[],"logprobs":[]}]}
        ])},"usage":{"input_tokens":11,"output_tokens":7,"total_tokens":18,
            "input_tokens_details":{"cached_tokens":3},"output_tokens_details":{"reasoning_tokens":2}}})
}
pub fn make_incomplete(response: &mut Value) {
    response["status"] = json!("incomplete");
    response["incomplete_details"] = json!({"reason":"max_output_tokens"});
    for item in response["output"].as_array_mut().unwrap() {
        if item["type"] == "reasoning" {
            item["status"] = json!("incomplete");
        }
    }
}

pub fn events(turn: usize) -> Vec<Value> {
    let mut events = crate::responses_wire::response_events(snapshot(turn));
    if turn == 1 {
        // Announce both owners before completing either. Finish the later
        // call first, while preserving each call's independent fragment order.
        let first = events
            .iter()
            .position(|e| e["type"] == "response.output_item.added" && e["output_index"] == 2)
            .unwrap();
        let end = events
            .iter()
            .position(|e| e["type"] == "response.completed")
            .unwrap();
        let call_events = [
            json!({"type":"response.output_item.added","output_index":2,"item":{"id":"call-item",
                "type":"function_call","status":"in_progress","call_id":"call-lookup","name":"lookup","arguments":""}}),
            json!({"type":"response.output_item.added","output_index":3,"item":{"id":"second-item",
                "type":"function_call","status":"in_progress","call_id":"call-second","name":"lookup","arguments":""}}),
            json!({"type":"response.function_call_arguments.delta","output_index":2,"item_id":"call-item","delta":"{ \"key\":"}),
            json!({"type":"response.function_call_arguments.delta","output_index":3,"item_id":"second-item","delta":"{\"key\":"}),
            json!({"type":"response.function_call_arguments.delta","output_index":3,"item_id":"second-item","delta":"\"B\"}"}),
            json!({"type":"response.function_call_arguments.done","output_index":3,"item_id":"second-item","arguments":"{\"key\":\"B\"}"}),
            json!({"type":"response.output_item.done","output_index":3,"item":{"id":"second-item",
                "type":"function_call","status":"completed","call_id":"call-second","name":"lookup","arguments":"{\"key\":\"B\"}"}}),
            json!({"type":"response.function_call_arguments.delta","output_index":2,"item_id":"call-item","delta":" \"A\" }"}),
            json!({"type":"response.function_call_arguments.done","output_index":2,"item_id":"call-item","arguments":"{ \"key\": \"A\" }"}),
            json!({"type":"response.output_item.done","output_index":2,"item":{"id":"call-item",
                "type":"function_call","status":"completed","call_id":"call-lookup","name":"lookup","arguments":"{ \"key\": \"A\" }"}}),
        ];
        events.splice(first..end, call_events);
    }
    for (sequence, event) in events.iter_mut().enumerate() {
        event["sequence_number"] = json!(sequence);
        if matches!(
            event["type"].as_str(),
            Some("response.created" | "response.in_progress")
        ) {
            event["response"]["reasoning"] = json!({"effort":"low","summary":"auto"});
            event["response"]["service_tier"] = json!("auto");
        }
    }
    events
}

pub fn assert_request(body: &Value) -> usize {
    assert_eq!(body["model"], "openai/gpt-6-luna");
    assert_eq!(body["max_output_tokens"], 256);
    assert_eq!(body["reasoning"]["effort"], "low");
    assert_eq!(body["store"], false);
    assert!(body.get("previous_response_id").is_none());
    assert!(
        body.get("text").is_none(),
        "reported null is not request intent"
    );
    assert_eq!(body["provider"], json!({"require_parameters":true}));
    let tool = &body["tools"][0];
    assert_eq!(tool["name"], "lookup");
    assert_eq!(tool["strict"], false);
    assert_eq!(
        tool["parameters"],
        json!({"type":"object",
        "properties":{"key":{"type":"string"}},"required":["key"]})
    );
    let history = body["input"].as_array().unwrap();
    let incomplete =
        history[0]["content"] == "incomplete" || history[0]["content"][0]["text"] == "incomplete";
    let turn = if history.iter().any(|v| {
        v["role"] == "user" && (v["content"] == "next" || v["content"][0]["text"] == "next")
    }) {
        3
    } else if history.iter().any(|v| v["type"] == "function_call_output") {
        2
    } else {
        1
    };
    if turn >= 2 {
        let mut expected = vec![
            json!({"type":"message","role":"user","content":[{"type":"input_text","text":if incomplete{"incomplete"}else{"lookup"}}]}),
            json!({"id":"reason","type":"reasoning",
            "summary":[{"type":"summary_text","text":"Plan 🧪"}],
            "content":[{"type":"reasoning_text","text":"Check"}],"encrypted_content":"synthetic-final-token"}),
            json!({"id":"comment","type":"message","role":"assistant","status":"completed","phase":"commentary",
                "content":[{"type":"output_text","text":"checking","annotations":[],"logprobs":[]}]}),
            json!({"id":"call-item","type":"function_call","call_id":"call-lookup",
                "name":"lookup","arguments":"{ \"key\": \"A\" }"}),
            json!({"id":"second-item","type":"function_call","call_id":"call-second",
                "name":"lookup","arguments":"{\"key\":\"B\"}"}),
            json!({"type":"function_call_output","call_id":"call-second","output":"second-result"}),
            json!({"type":"function_call_output","call_id":"call-lookup","output":RESULT}),
        ];
        if incomplete {
            expected[1]["status"] = json!("incomplete");
        }
        if turn == 3 {
            expected.extend([
                json!({"id":"answer-2","type":"message","role":"assistant","status":"completed","phase":"final_answer",
                    "content":[{"type":"output_text","text":"cedar-71","annotations":[],"logprobs":[]}]}),
                json!({"type":"message","role":"user","content":[{"type":"input_text","text":"next"}]}),
            ]);
        }
        assert_eq!(
            history, &expected,
            "complete Responses continuation turn {turn}"
        );
    }
    turn
}
