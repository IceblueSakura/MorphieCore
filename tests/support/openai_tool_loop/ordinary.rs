//! Independent ordinary-function wire expectations shared by the nine SDK lanes.
use axum::{body::Body, http::HeaderMap, response::Response};
use morphiecore::topology::GenerationProtocol;
use serde_json::{Value, json};

fn text(value: &Value) -> &str {
    value
        .as_str()
        .or_else(|| value[0]["text"].as_str())
        .unwrap()
}

pub fn assert_request(target: GenerationProtocol, headers: &HeaderMap, body: &Value) -> usize {
    let native = target == GenerationProtocol::AnthropicMessages;
    let responses = target == GenerationProtocol::OpenAiResponses;
    assert_eq!(
        body["model"],
        match target {
            GenerationProtocol::OpenAiChat => "qwen3.8-flash",
            GenerationProtocol::OpenAiResponses => "openai/gpt-6-luna",
            GenerationProtocol::AnthropicMessages => "claude-haiku-5-5",
            _ => unreachable!(),
        }
    );
    assert_eq!(
        body[if native {
            "max_tokens"
        } else if responses {
            "max_output_tokens"
        } else {
            "max_completion_tokens"
        }],
        256
    );
    assert_eq!(
        headers[if native { "x-api-key" } else { "authorization" }],
        if native {
            "synthetic-tool-history-upstream"
        } else {
            "Bearer synthetic-tool-history-upstream"
        }
    );
    let tools = body["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    let tool = if target == GenerationProtocol::OpenAiChat {
        &tools[0]["function"]
    } else {
        &tools[0]
    };
    assert_eq!(tool["name"], "lookup");
    assert_eq!(
        tool[if native { "input_schema" } else { "parameters" }],
        json!({"type":"object","properties":{"key":{"type":"string"}},"required":["key"]})
    );
    assert!(tool.get("strict").is_none_or(|v| v == false));
    let history = body[if responses { "input" } else { "messages" }]
        .as_array()
        .unwrap();
    let source = text(&history[0]["content"]);
    assert!(matches!(
        source,
        "ordinary-chat" | "ordinary-responses" | "ordinary-messages"
    ));
    let turn = if history.len() == 1 {
        1
    } else if history.iter().any(|item| {
        item["role"] == "user"
            && (item["content"] == "next" || item["content"][0]["text"] == "next")
    }) {
        3
    } else {
        2
    };
    // Result content on OpenAI targets is a scalar, whereas native Messages groups results.
    let chat_envelope = usize::from(responses && source == "ordinary-chat");
    let expected_len = match (target, turn) {
        (_, 1) => 1,
        (GenerationProtocol::OpenAiChat, 2) => 4,
        (GenerationProtocol::OpenAiChat, 3) => 6,
        (GenerationProtocol::OpenAiResponses, 2) => 5 + chat_envelope,
        (GenerationProtocol::OpenAiResponses, 3) => 7 + chat_envelope,
        (GenerationProtocol::AnthropicMessages, 2) => 3,
        (GenerationProtocol::AnthropicMessages, 3) => 5,
        _ => unreachable!(),
    };
    assert_eq!(
        history.len(),
        expected_len,
        "complete ordinary history {target:?} turn {turn}"
    );
    if turn >= 2 {
        if native {
            assert_eq!(history[1]["role"], "assistant");
            assert_eq!(
                history[1]["content"],
                json!([
                    {"type":"tool_use","id":"call-lookup","name":"lookup","input":{"key":"A"}},
                    {"type":"tool_use","id":"call-second","name":"lookup","input":{"key":"B"}}
                ])
            );
            assert_eq!(history[2]["role"], "user");
            let results = history[2]["content"].as_array().unwrap();
            assert_eq!(results.len(), 2);
            for (result, id, output) in [
                (&results[0], "call-second", "second-result"),
                (&results[1], "call-lookup", super::RESULT),
            ] {
                assert_eq!(result["type"], "tool_result");
                assert_eq!(result["tool_use_id"], id);
                assert_eq!(text(&result["content"]), output);
                assert!(result.get("is_error").is_none_or(|v| v == false));
            }
        } else {
            let calls: Vec<&Value> = if responses {
                if chat_envelope == 1 {
                    // Chat retains its empty assistant message separately from calls.
                    assert_eq!(history[1]["type"], "message");
                    assert_eq!(history[1]["role"], "assistant");
                    assert_eq!(history[1]["content"], json!([]));
                    assert_eq!(history[1]["status"], "completed");
                }
                vec![&history[1 + chat_envelope], &history[2 + chat_envelope]]
            } else {
                assert_eq!(history[1]["role"], "assistant");
                let calls = history[1]["tool_calls"].as_array().unwrap();
                assert_eq!(calls.len(), 2);
                calls.iter().collect()
            };
            for (call, id, key, raw) in [
                (calls[0], "call-lookup", "A", "{ \"key\": \"A\" }"),
                (calls[1], "call-second", "B", "{\"key\":\"B\"}"),
            ] {
                assert_eq!(
                    call["type"],
                    if responses {
                        "function_call"
                    } else {
                        "function"
                    }
                );
                assert_eq!(call[if responses { "call_id" } else { "id" }], id);
                let function = if responses { call } else { &call["function"] };
                assert_eq!(function["name"], "lookup");
                let arguments = function["arguments"].as_str().unwrap();
                assert_eq!(
                    serde_json::from_str::<Value>(arguments).unwrap(),
                    json!({"key":key})
                );
                if source != "ordinary-messages" {
                    assert_eq!(arguments, raw, "string-carrier raw argument authority");
                }
            }
            let offset = if responses { 3 + chat_envelope } else { 2 };
            for (result, id, output) in [
                (&history[offset], "call-second", "second-result"),
                (&history[offset + 1], "call-lookup", super::RESULT),
            ] {
                if responses {
                    assert_eq!(result["type"], "function_call_output");
                } else {
                    assert_eq!(result["role"], "tool");
                }
                assert_eq!(
                    result[if responses { "call_id" } else { "tool_call_id" }],
                    id
                );
                assert_eq!(result[if responses { "output" } else { "content" }], output);
            }
        }
        if turn == 3 {
            let answer = &history[history.len() - 2];
            assert_eq!(answer["role"], "assistant");
            assert_eq!(text(&answer["content"]), "cedar-71");
            assert_eq!(history.last().unwrap()["role"], "user");
        }
    }
    turn
}

pub fn answer(target: GenerationProtocol, turn: usize, stream: bool) -> Response {
    if target == GenerationProtocol::AnthropicMessages {
        let content = if turn == 1 {
            json!([
                {"type":"tool_use","id":"call-lookup","name":"lookup","input":{"key":"A"}},
                {"type":"tool_use","id":"call-second","name":"lookup","input":{"key":"B"}}
            ])
        } else {
            json!([{"type":"text","text":"cedar-71"}])
        };
        return crate::messages_wire::native_response(
            json!({"id":format!("msg-{turn}"),
            "type":"message","role":"assistant","model":"claude-haiku-5-5","content":content,
            "stop_reason":if turn == 1 { "tool_use" } else { "end_turn" },"stop_sequence":null,
            "usage":{"input_tokens":11,"output_tokens":7}}),
            stream,
        );
    }
    let chat = target == GenerationProtocol::OpenAiChat;
    let mut snapshot = if chat {
        super::chat::snapshot(turn, false)
    } else {
        super::responses::snapshot(turn)
    };
    if !chat {
        snapshot["output"].as_array_mut().unwrap().retain(|item| {
            !matches!(item["type"].as_str(), Some("reasoning")) && item["phase"] != "commentary"
        });
        snapshot.as_object_mut().unwrap().remove("reasoning");
    }
    let bytes = if !stream {
        snapshot.to_string()
    } else if chat {
        let events = super::chat::events(turn, false);
        events
            .iter()
            .map(|e| format!("data: {e}\n\n"))
            .collect::<String>()
            + "data: [DONE]\n\n"
    } else {
        crate::responses_wire::response_events(snapshot)
            .iter()
            .map(|e| format!("data: {e}\n\n"))
            .collect()
    };
    Response::builder()
        .header(
            "content-type",
            if stream {
                "text/event-stream"
            } else {
                "application/json"
            },
        )
        .body(Body::from(bytes))
        .unwrap()
}
