//! Source-derived pi 1.4.0 shapes; native outputs reuse the repair-and-recall corpus.
//! pi-ai dist/api/{openai-completions,openai-responses-shared,anthropic-messages}.js
//! supplies defaults/replay, independently of MorphieCore codecs. Synthetic provider,
//! no compat overrides; nonreasoning controls differ explicitly from medium reasoning.
use serde_json::{Value, json};
#[path = "../../../../../tests/support/messages_stream.rs"]
mod messages_stream;
#[path = "../../../../../tests/support/messages_wire.rs"]
#[allow(dead_code)]
mod messages_wire;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Api {
    Chat,
    Responses,
    Messages,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Profile {
    Text,
    Tools,
    Reasoning,
}
impl Api {
    pub(super) fn path(self) -> &'static str {
        match self {
            Self::Chat => "/v1/chat/completions",
            Self::Responses => "/v1/responses",
            Self::Messages => "/v1/messages",
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Responses => "responses",
            Self::Messages => "messages",
        }
    }
    fn history(self) -> &'static str {
        if self == Self::Responses {
            "input"
        } else {
            "messages"
        }
    }
}
fn corpus(api: Api) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata/repair-and-recall")
        .join(api.name())
        .join("session.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

pub(super) fn raw_messages_headers() -> (String, String) {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../../../testdata/repair-and-recall/agent-defaults.json"
    ))
    .unwrap();
    let headers = &corpus["raw_messages_headers"];
    (
        headers["anthropic-version"].as_str().unwrap().into(),
        headers["anthropic-beta"].as_str().unwrap().into(),
    )
}

pub(super) fn request(api: Api, profile: Profile, model: &str, stream: bool) -> Value {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../../../testdata/repair-and-recall/agent-defaults.json"
    ))
    .unwrap();
    let profile = match profile {
        Profile::Text => "text",
        Profile::Tools => "tools",
        Profile::Reasoning => "reasoning",
    };
    let mut body = corpus["requests"][api.name()][profile].clone();
    assert!(body.is_object());
    body["model"] = json!(model);
    body["stream"] = json!(stream);
    // pi is streaming-only. JSON controls change only transport-required fields.
    if !stream {
        body.as_object_mut().unwrap().remove("stream_options");
    }
    body
}
fn cache(body: &mut Value) {
    without_cache(body);
    body["messages"].as_array_mut().unwrap().last_mut().unwrap()["content"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap()["cache_control"] = json!({"type":"ephemeral"});
    if let Some(tools) = body.get_mut("tools").and_then(Value::as_array_mut) {
        tools.last_mut().unwrap()["cache_control"] = json!({"type":"ephemeral"});
    }
    if let Some(system) = body.get_mut("system").and_then(Value::as_array_mut) {
        system.last_mut().unwrap()["cache_control"] = json!({"type":"ephemeral"});
    }
}
pub(super) fn without_cache(body: &mut Value) {
    for message in body["messages"].as_array_mut().unwrap() {
        if let Some(blocks) = message["content"].as_array_mut() {
            for block in blocks {
                block.as_object_mut().unwrap().remove("cache_control");
            }
        }
    }
    for key in ["tools", "system"] {
        if let Some(values) = body.get_mut(key).and_then(Value::as_array_mut) {
            for value in values {
                value.as_object_mut().unwrap().remove("cache_control");
            }
        }
    }
}
fn turn_index(profile: Profile, turn: usize) -> usize {
    if profile == Profile::Text {
        6 + turn
    } else {
        turn
    }
}
fn snapshot(api: Api, profile: Profile, turn: usize) -> Value {
    let mut value = corpus(api)["turns"][turn_index(profile, turn)]["source"]["json"].clone();
    if profile != Profile::Reasoning {
        let key = if api == Api::Responses {
            "output"
        } else {
            "content"
        };
        if let Some(items) = value.get_mut(key).and_then(Value::as_array_mut) {
            items.retain(|i| {
                !matches!(
                    i["type"].as_str(),
                    Some("reasoning" | "thinking" | "redacted_thinking")
                )
            });
        }
    }
    match api {
        Api::Chat => {
            value["choices"][0]["logprobs"] = Value::Null;
            if profile == Profile::Reasoning {
                value["choices"][0]["message"]["reasoning_content"] = json!("Plan 🧪");
            }
        }
        Api::Responses => {
            // Required-nullable envelope members are present even without observations.
            for key in [
                "access_programs",
                "instructions",
                "metadata",
                "temperature",
                "top_p",
            ] {
                value[key] = Value::Null;
            }
            value["parallel_tool_calls"] = json!(true);
            value["tool_choice"] = json!("auto");
            value["tools"] = if profile == Profile::Text {
                json!([])
            } else {
                corpus(api)["client_request"]["tools"].clone()
            };
        }
        Api::Messages => {
            for key in ["container", "diagnostics", "stop_details"] {
                value[key] = Value::Null;
            }
            for key in [
                "cache_creation",
                "inference_geo",
                "output_tokens_details",
                "server_tool_use",
                "service_tier",
            ] {
                value["usage"][key] = Value::Null;
            }
            for block in value["content"].as_array_mut().unwrap() {
                if block["type"] == "text" {
                    block["citations"] = Value::Null;
                }
            }
        }
    }
    value
}
fn frames(events: Vec<Value>) -> String {
    events
        .iter()
        .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap()))
        .collect()
}
pub(super) fn upstream(
    api: Api,
    profile: Profile,
    turn: usize,
    stream: bool,
    model: &str,
    incomplete: bool,
) -> String {
    let mut value = snapshot(api, profile, turn);
    value["model"] = json!(model);
    if incomplete {
        assert_eq!(api, Api::Responses);
        value["status"] = json!("incomplete");
        value["incomplete_details"] = json!({"reason":"max_output_tokens"});
        for item in value["output"].as_array_mut().unwrap() {
            if item["type"] == "reasoning" {
                item["status"] = json!("incomplete");
            }
        }
    }
    if !stream {
        return value.to_string();
    }
    match api {
        Api::Messages => {
            // Shared independent Messages wire generator.
            let wire = messages_wire::native_stream_body(&value);
            let events = wire
                .lines()
                .filter_map(|l| l.strip_prefix("data: "))
                .map(|line| {
                    let mut event: Value = serde_json::from_str(line).unwrap();
                    if event["type"] == "content_block_start"
                        && event["content_block"]["type"] == "text"
                    {
                        event["content_block"]["citations"] = Value::Null;
                    }
                    if event["type"] == "message_delta" {
                        for key in ["container", "stop_details", "stop_sequence"] {
                            event["delta"][key] = Value::Null;
                        }
                        for key in [
                            "input_tokens",
                            "cache_creation_input_tokens",
                            "cache_read_input_tokens",
                            "output_tokens_details",
                            "server_tool_use",
                        ] {
                            event["usage"][key] = Value::Null;
                        }
                    }
                    event
                })
                .collect();
            frames(events)
        }
        Api::Chat => {
            let message = &value["choices"][0]["message"];
            let mut delta = message.clone();
            if let Some(calls) = delta.get_mut("tool_calls").and_then(Value::as_array_mut) {
                for (i, call) in calls.iter_mut().enumerate() {
                    call["index"] = json!(i);
                }
            }
            let chunk = |delta: Value, finish: Value| json!({"id":value["id"],"object":"chat.completion.chunk","created":1,"model":value["model"],"choices":[{"index":0,"delta":delta,"finish_reason":finish,"logprobs":null}],"usage":null});
            let terminal = chunk(json!({}), value["choices"][0]["finish_reason"].clone());
            // Standard include_usage supplies one nonnull report, after the finish chunk.
            let usage = json!({"id":value["id"],"object":"chat.completion.chunk","created":1,"model":value["model"],"choices":[],"usage":value["usage"]});
            format!(
                "data: {}\n\ndata: {terminal}\n\ndata: {usage}\n\ndata: [DONE]\n\n",
                chunk(delta, Value::Null)
            )
        }
        Api::Responses => {
            let mut initial = value.clone();
            initial["status"] = json!("in_progress");
            initial["output"] = json!([]);
            initial["usage"] = Value::Null;
            initial["incomplete_details"] = Value::Null;
            let mut events = vec![json!({"type":"response.created","response":initial})];
            for (index, item) in value["output"].as_array().unwrap().iter().enumerate() {
                let id = &item["id"];
                let mut added = item.clone();
                added["status"] = json!("in_progress");
                match item["type"].as_str().unwrap() {
                    "message" => added["content"] = json!([]),
                    "reasoning" => {
                        added["summary"] = json!([]);
                        added.as_object_mut().unwrap().remove("encrypted_content");
                    }
                    "function_call" => added["arguments"] = json!(""),
                    other => panic!("source item {other}"),
                }
                events.push(
                    json!({"type":"response.output_item.added","output_index":index,"item":added}),
                );
                match item["type"].as_str().unwrap() {
                    "message" => {
                        let part = &item["content"][0];
                        events.push(json!({"type":"response.content_part.added","output_index":index,"item_id":id,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}));
                        events.push(json!({"type":"response.output_text.delta","output_index":index,"item_id":id,"content_index":0,"delta":part["text"],"logprobs":[]}));
                        events.push(json!({"type":"response.output_text.done","output_index":index,"item_id":id,"content_index":0,"text":part["text"],"logprobs":[]}));
                        events.push(json!({"type":"response.content_part.done","output_index":index,"item_id":id,"content_index":0,"part":part}));
                    }
                    "reasoning" => {
                        let part = &item["summary"][0];
                        events.push(json!({"type":"response.reasoning_summary_part.added","output_index":index,"item_id":id,"summary_index":0,"part":{"type":"summary_text","text":""}}));
                        events.push(json!({"type":"response.reasoning_summary_text.delta","output_index":index,"item_id":id,"summary_index":0,"delta":part["text"]}));
                        events.push(json!({"type":"response.reasoning_summary_text.done","output_index":index,"item_id":id,"summary_index":0,"text":part["text"]}));
                        events.push(json!({"type":"response.reasoning_summary_part.done","output_index":index,"item_id":id,"summary_index":0,"part":part}));
                    }
                    "function_call" => {
                        events.push(json!({"type":"response.function_call_arguments.delta","output_index":index,"item_id":id,"delta":item["arguments"]}));
                        events.push(json!({"type":"response.function_call_arguments.done","output_index":index,"item_id":id,"arguments":item["arguments"]}));
                    }
                    _ => unreachable!(),
                }
                events.push(
                    json!({"type":"response.output_item.done","output_index":index,"item":item}),
                );
            }
            events.push(json!({"type":if incomplete {"response.incomplete"}else{"response.completed"},"response":value}));
            for (i, event) in events.iter_mut().enumerate() {
                event["sequence_number"] = json!(i);
            }
            frames(events)
        }
    }
}

pub(super) fn delivered(api: Api, bytes: &[u8], stream: bool) -> Value {
    if !stream {
        return serde_json::from_slice(bytes).unwrap();
    }
    let wire = std::str::from_utf8(bytes).unwrap();
    let data: Vec<_> = wire
        .lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .collect();
    match api {
        Api::Responses => {
            let events: Vec<Value> = data
                .iter()
                .map(|s| serde_json::from_str(s).unwrap())
                .collect();
            assert_eq!(
                events
                    .iter()
                    .filter(|e| matches!(
                        e["type"].as_str(),
                        Some("response.completed" | "response.incomplete")
                    ))
                    .count(),
                1
            );
            let terminal = events.last().unwrap();
            assert_eq!(
                terminal["type"],
                format!(
                    "response.{}",
                    terminal["response"]["status"].as_str().unwrap()
                )
            );
            assert!(matches!(
                terminal["type"].as_str(),
                Some("response.completed" | "response.incomplete")
            ));
            let mut opened = std::collections::BTreeMap::new();
            let mut completed = Vec::new();
            for event in &events {
                if event["type"] == "response.output_item.added" {
                    assert!(
                        opened
                            .insert(
                                event["output_index"].as_u64().unwrap(),
                                event["item"].clone()
                            )
                            .is_none()
                    );
                }
                if event["type"] == "response.output_item.done" {
                    let added = opened
                        .remove(&event["output_index"].as_u64().unwrap())
                        .unwrap();
                    for key in ["id", "type", "call_id", "name"] {
                        assert_eq!(
                            added.get(key),
                            event["item"].get(key),
                            "opening identity {key}"
                        );
                    }
                    completed.push(event["item"].clone());
                }
            }
            assert!(opened.is_empty());
            let response = events.last().unwrap()["response"].clone();
            assert_eq!(
                json!(completed),
                response["output"],
                "terminal cannot replace item delivery"
            );
            response
        }
        Api::Messages => {
            assert_eq!(
                data.iter()
                    .filter(|s| serde_json::from_str::<Value>(s).unwrap()["type"] == "message_stop")
                    .count(),
                1
            );
            assert_eq!(
                serde_json::from_str::<Value>(data.last().unwrap()).unwrap()["type"],
                "message_stop"
            );
            messages_stream::messages_stream_body(wire)
        }
        Api::Chat => {
            assert_eq!(data.iter().filter(|s| **s == "[DONE]").count(), 1);
            assert_eq!(data.last(), Some(&"[DONE]"));
            let mut message = json!({"role":"assistant","content":""});
            let mut calls = std::collections::BTreeMap::<u64, Value>::new();
            let mut terminal = None;
            for data in data.into_iter().filter(|s| *s != "[DONE]") {
                let event: Value = serde_json::from_str(data).unwrap();
                for choice in event["choices"].as_array().unwrap() {
                    let delta = &choice["delta"];
                    if let Some(text) = delta["content"].as_str() {
                        message["content"] =
                            json!(format!("{}{text}", message["content"].as_str().unwrap()));
                    }
                    if let Some(values) = delta["tool_calls"].as_array() {
                        for value in values {
                            let call=calls.entry(value["index"].as_u64().unwrap()).or_insert_with(||json!({"type":"function","function":{"name":"","arguments":""}}));
                            if let Some(id) = value.get("id") {
                                call["id"] = id.clone();
                            }
                            for key in ["name", "arguments"] {
                                if let Some(s) = value["function"][key].as_str() {
                                    call["function"][key] = json!(format!(
                                        "{}{s}",
                                        call["function"][key].as_str().unwrap()
                                    ));
                                }
                            }
                        }
                    }
                    if choice["finish_reason"].is_string() {
                        assert!(terminal.replace(choice["finish_reason"].clone()).is_none());
                    }
                }
            }
            if !calls.is_empty() {
                message["tool_calls"] = json!(calls.into_values().collect::<Vec<_>>());
            }
            json!({"choices":[{"message":message,"finish_reason":terminal.unwrap()}]})
        }
    }
}

pub(super) fn assert_delivery(api: Api, profile: Profile, turn: usize, value: &Value) {
    if profile == Profile::Reasoning {
        // This is the accepted native Responses reasoning lane, not a tool-only control.
        assert_eq!(api, Api::Responses);
        let reasoning = value["output"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["type"] == "reasoning")
            .unwrap();
        assert_eq!(
            reasoning["summary"],
            json!([{"type":"summary_text","text":"Plan 🧪"}])
        );
        assert!(
            reasoning["encrypted_content"]
                .as_str()
                .unwrap()
                .starts_with("mc1.")
        );
    }
    if profile == Profile::Text {
        let text = match api {
            Api::Chat => value["choices"][0]["message"]["content"]
                .as_str()
                .unwrap()
                .to_owned(),
            Api::Responses => value["output"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|i| i["content"].as_array().into_iter().flatten())
                .filter_map(|p| p["text"].as_str())
                .collect(),
            Api::Messages => value["content"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|p| p["text"].as_str())
                .collect(),
        };
        let expected = snapshot(Api::Chat, profile, turn);
        assert_eq!(
            text,
            expected["choices"][0]["message"]["content"]
                .as_str()
                .unwrap()
        );
    } else {
        let ids: Vec<_> = match api {
            Api::Chat => value["choices"][0]["message"]["tool_calls"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| i["id"].as_str().unwrap())
                .collect(),
            Api::Responses => value["output"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|i| i["type"] == "function_call")
                .map(|i| i["call_id"].as_str().unwrap())
                .collect(),
            Api::Messages => value["content"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|i| i["type"] == "tool_use")
                .map(|i| i["id"].as_str().unwrap())
                .collect(),
        };
        assert_eq!(
            ids,
            if turn == 0 {
                vec!["call-z", "call-a"]
            } else {
                vec!["call-locate"]
            }
        );
        let calls: Vec<(&Value, Value)> = match api {
            Api::Chat => value["choices"][0]["message"]["tool_calls"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| {
                    (
                        &i["function"]["name"],
                        serde_json::from_str(i["function"]["arguments"].as_str().unwrap()).unwrap(),
                    )
                })
                .collect(),
            Api::Responses => value["output"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|i| i["type"] == "function_call")
                .map(|i| {
                    (
                        &i["name"],
                        serde_json::from_str(i["arguments"].as_str().unwrap()).unwrap(),
                    )
                })
                .collect(),
            Api::Messages => value["content"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|i| i["type"] == "tool_use")
                .map(|i| (&i["name"], i["input"].clone()))
                .collect(),
        };
        let expected = snapshot(Api::Chat, profile, turn);
        let expected: Vec<(&Value, Value)> = expected["choices"][0]["message"]["tool_calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| {
                (
                    &i["function"]["name"],
                    serde_json::from_str(i["function"]["arguments"].as_str().unwrap()).unwrap(),
                )
            })
            .collect();
        assert_eq!(
            calls, expected,
            "tool identity and arguments before client replay"
        );
    }
}

pub(super) fn append(api: Api, profile: Profile, turn: usize, body: &mut Value, value: Value) {
    let history = body[api.history()].as_array_mut().unwrap();
    match api {
        Api::Chat => {
            let mut message = value["choices"][0]["message"].clone();
            if message["content"] == "" {
                message["content"] = Value::Null;
            }
            if let Some(calls) = message.get_mut("tool_calls").and_then(Value::as_array_mut) {
                for call in calls {
                    call["function"]["arguments"] = json!(
                        pi_arguments(call["function"]["arguments"].as_str().unwrap()).to_string()
                    );
                }
            }
            history.push(message);
        }
        Api::Responses => {
            for item in value["output"].as_array().unwrap() {
                let mut item = item.clone();
                match item["type"].as_str().unwrap() {
                    "function_call" => {
                        item.as_object_mut().unwrap().remove("status");
                        if !item["id"].as_str().is_some_and(|id| id.starts_with("fc_")) {
                            item.as_object_mut().unwrap().remove("id");
                        }
                        item["arguments"] =
                            json!(pi_arguments(item["arguments"].as_str().unwrap()).to_string());
                    }
                    "message" => {
                        item["status"] = json!("completed");
                        for part in item["content"].as_array_mut().unwrap() {
                            part["annotations"] = json!([]);
                            part.as_object_mut().unwrap().remove("logprobs");
                        }
                    }
                    "reasoning" => {}
                    other => panic!("replay item {other}"),
                }
                history.push(item);
            }
        }
        Api::Messages => {
            let mut blocks = value["content"].as_array().unwrap().clone();
            for block in &mut blocks {
                if block["type"] == "text" {
                    *block = json!({"type":"text","text":block["text"]});
                }
                if block["type"] == "tool_use" {
                    block["input"] = pi_arguments(&block["input"].to_string());
                }
            }
            history.push(json!({"role":"assistant","content":blocks}));
        }
    }
    history.extend(
        corpus(api)["turns"][turn_index(profile, turn)]["client_append"]
            .as_array()
            .unwrap()
            .iter()
            .cloned(),
    );
    if api == Api::Responses {
        for item in history {
            if item["role"] == "user" {
                item.as_object_mut().unwrap().remove("type");
            }
        }
    }
    if api == Api::Messages {
        cache(body);
    }
}

fn pi_arguments(raw: &str) -> Value {
    assert!(!raw.is_empty());
    let mut value: Value = serde_json::from_str(raw).unwrap();
    // This corpus deliberately includes the first unsafe JS integer. pi parses tool
    // arguments with JSON.parse; the gateway must still preserve the original wire.
    fn normalize(value: &mut Value) {
        match value {
            Value::Array(a) => a.iter_mut().for_each(normalize),
            Value::Object(o) => o.values_mut().for_each(normalize),
            Value::Number(n) if n.as_u64() == Some(9_007_199_254_740_993) => {
                *value = json!(9_007_199_254_740_992u64)
            }
            _ => {}
        }
    }
    normalize(&mut value);
    value
}

pub(super) fn assert_failure(api: Api, stream: bool, status: u16, bytes: &[u8], aborted: bool) {
    if status >= 400 {
        assert!(!aborted);
        let error: Value = serde_json::from_slice(bytes).unwrap();
        assert!(error["error"].is_object(), "native error envelope: {error}");
        return;
    }
    assert_eq!(status, 200);
    assert!(stream, "JSON failure cannot fabricate success");
    let wire = std::str::from_utf8(bytes).unwrap();
    let mut error = false;
    for data in wire.lines().filter_map(|l| l.strip_prefix("data: ")) {
        assert_ne!(
            data, "[DONE]",
            "failed Chat stream cannot finish successfully"
        );
        let event: Value = serde_json::from_str(data).unwrap();
        assert!(!matches!(
            event["type"].as_str(),
            Some("response.completed" | "response.incomplete" | "message_stop")
        ));
        if api == Api::Chat {
            assert!(
                event["choices"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .all(|c| c["finish_reason"].is_null())
            );
        }
        error |= event["type"] == "error"
            || event["type"] == "response.failed"
            || event["error"].is_object();
    }
    assert!(
        aborted || error,
        "failure needs an observable abort or error"
    );
}

pub(super) fn assert_history(api: Api, profile: Profile, body: &Value) {
    let history = body[api.history()].as_array().unwrap();
    if profile == Profile::Text {
        let text = snapshot(Api::Chat, profile, 0)["choices"][0]["message"]["content"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(
            history
                .iter()
                .filter(|m| m["role"] == "assistant")
                .any(|m| m["content"] == text
                    || m["content"]
                        .as_array()
                        .is_some_and(|parts| parts.iter().any(|p| p["text"] == text))),
            "previous assistant text must reach the upstream"
        );
        return;
    }
    let mut calls = Vec::new();
    let mut results = Vec::new();
    for item in history {
        match api {
            Api::Chat => {
                if let Some(items) = item["tool_calls"].as_array() {
                    for call in items {
                        calls.push((
                            call["id"].clone(),
                            call["function"]["name"].clone(),
                            serde_json::from_str::<Value>(
                                call["function"]["arguments"].as_str().unwrap(),
                            )
                            .unwrap(),
                        ));
                    }
                }
                if item["role"] == "tool" {
                    results.push((item["tool_call_id"].clone(), item["content"].clone()));
                }
            }
            Api::Responses => {
                if item["type"] == "function_call" {
                    calls.push((
                        item["call_id"].clone(),
                        item["name"].clone(),
                        serde_json::from_str::<Value>(item["arguments"].as_str().unwrap()).unwrap(),
                    ));
                }
                if item["type"] == "function_call_output" {
                    results.push((item["call_id"].clone(), item["output"].clone()));
                }
            }
            Api::Messages => {
                for part in item["content"].as_array().into_iter().flatten() {
                    if part["type"] == "tool_use" {
                        calls.push((
                            part["id"].clone(),
                            part["name"].clone(),
                            part["input"].clone(),
                        ));
                    }
                    if part["type"] == "tool_result" {
                        let content = if part["content"].is_string() {
                            part["content"].clone()
                        } else {
                            json!(
                                part["content"]
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .map(|p| p["text"].as_str().unwrap())
                                    .collect::<String>()
                            )
                        };
                        results.push((part["tool_use_id"].clone(), content));
                    }
                }
            }
        }
    }
    assert_eq!(
        calls,
        vec![
            (
                json!("call-z"),
                json!("read"),
                json!({"path":"settings.json","serial":9_007_199_254_740_992u64,"label":"a"})
            ),
            (
                json!("call-a"),
                json!("read"),
                json!({"path":"missing.json"})
            ),
        ],
        "previous calls must reach upstream after pi-style argument serialization"
    );
    assert_eq!(
        results,
        vec![
            (json!("call-a"), json!("ENOENT: missing.json")),
            (json!("call-z"), json!("revision=9007199254740993; label=a")),
        ],
        "reverse-order feedback must reach upstream"
    );
    if profile == Profile::Reasoning {
        let reasoning = history.iter().find(|i| i["type"] == "reasoning").unwrap();
        assert_eq!(
            reasoning["encrypted_content"], "synthetic-reason-0",
            "same-issuer replay must open the delivered seal"
        );
        assert_eq!(
            reasoning["summary"],
            json!([{"type":"summary_text","text":"Plan 🧪"}])
        );
    }
}
