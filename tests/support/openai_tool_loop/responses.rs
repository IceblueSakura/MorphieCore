//! OpenRouter Responses wire, phase and stored/stateless effective-history expectations.
use super::RESULT;
use serde_json::{Value, json};

pub fn snapshot(turn: usize) -> Value {
    json!({"id":format!("response-{turn}"),"object":"response","created_at":1,
        "completed_at":2,"model":"openai/gpt-6-luna","status":"completed","error":null,
        "incomplete_details":null,"output":if turn == 1 {json!([
            {"id":"reason","type":"reasoning","status":"completed",
                "summary":[{"type":"summary_text","text":"Plan 🧪"}],
                "content":[{"type":"reasoning_text","text":"Check"}],
                "encrypted_content":"synthetic-final-token"},
            {"id":"comment","type":"message","role":"assistant","phase":"commentary",
                "status":"completed","content":[{"type":"output_text","text":"checking","annotations":[],"logprobs":[]}]},
            {"id":"call-item","type":"function_call","status":"completed","call_id":"call-lookup",
                "name":"lookup","arguments":"{ \"key\": \"A\" }"}
        ])}else{json!([
            {"id":format!("answer-{turn}"),"type":"message","role":"assistant","phase":"final_answer",
                "status":"completed","content":[{"type":"output_text","text":"cedar-71","annotations":[],"logprobs":[]}]}
        ])},"usage":{"input_tokens":11,"output_tokens":7,"total_tokens":18,
            "input_tokens_details":{"cached_tokens":3},"output_tokens_details":{"reasoning_tokens":2}}})
}
pub fn events(turn: usize) -> Vec<Value> {
    crate::responses_wire::response_events(snapshot(turn))
}

pub fn assert_request(body: &Value) -> usize {
    assert_eq!(body["model"], "openai/gpt-6-luna");
    assert_eq!(body["max_output_tokens"], 256);
    assert_eq!(body["reasoning"]["effort"], "low");
    assert_eq!(body["store"], false);
    assert!(body.get("previous_response_id").is_none());
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
        assert!(history.contains(&json!({"id":"reason","type":"reasoning",
            "summary":[{"type":"summary_text","text":"Plan 🧪"}],
            "content":[{"type":"reasoning_text","text":"Check"}],"encrypted_content":"synthetic-final-token"})));
        assert!(history.iter().any(|v| v["type"] == "function_call"
            && v["call_id"] == "call-lookup"
            && v["name"] == "lookup"
            && v["arguments"] == "{ \"key\": \"A\" }"));
        assert!(
            history
                .iter()
                .any(|v| v["phase"] == "commentary" && v["content"][0]["text"] == "checking")
        );
        assert!(history.contains(&json!({"type":"function_call_output",
            "call_id":"call-lookup","output":RESULT})));
    }
    if turn == 3 {
        assert!(history.iter().any(|v| v["role"] == "assistant"
            && v["phase"] == "final_answer"
            && v["content"][0]["text"] == "cedar-71"));
    }
    turn
}
