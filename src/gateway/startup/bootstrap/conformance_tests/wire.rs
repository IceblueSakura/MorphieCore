//! Source-derived pi 1.4.0 shapes; native outputs reuse the repair-and-recall corpus.
//! pi-ai serializers supply defaults/replay independently of MorphieCore codecs.
//! Bare API forms remain controls beside the resolved Go/Haiku built-in record.
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReasoningShape {
    Summary,
    Opaque,
    Readable,
    Signed,
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

pub(super) fn resolved_messages_headers(profile: Profile) -> Value {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../../../testdata/repair-and-recall/agent-defaults.json"
    ))
    .unwrap();
    let profile = match profile {
        Profile::Text => "text",
        Profile::Tools => "tools",
        Profile::Reasoning => "reasoning",
    };
    corpus["resolved_models"]["opencode_go_haiku"]["headers"][profile].clone()
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
    let source = if api == Api::Messages && model == "claude-haiku-5-5" {
        &corpus["resolved_models"]["opencode_go_haiku"]["requests"]
    } else {
        &corpus["requests"][api.name()]
    };
    let mut body = source[profile].clone();
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
            value["service_tier"] = json!("default");
            if profile == Profile::Reasoning {
                value["choices"][0]["message"]["reasoning_content"] = json!("Plan 🧪");
            }
        }
        Api::Responses => {
            // Required-nullable envelope members are present even without observations.
            for key in ["access_programs", "instructions", "temperature", "top_p"] {
                value[key] = Value::Null;
            }
            value["service_tier"] = json!("default");
            value["metadata"] = json!({"synthetic":"echo"});
            value["background"] = json!(false);
            value["store"] = json!(false);
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
    control: super::Control,
) -> String {
    let incomplete = matches!(control, super::Control::Incomplete { .. }) && turn == 0;
    let reasoning = control.reasoning();
    let mut value = snapshot(api, profile, turn);
    if control == super::Control::JsonOnlyReports {
        assert_eq!(api, Api::Chat);
        // Pinned OpenAI full responses have these fields; Chat chunks do not.
        value["metadata"] = json!({"synthetic":"json-only"});
        value["choices"][0]["message"]["annotations"] = json!([{
            "type":"url_citation","url_citation":{
                "start_index":0,"end_index":1,"title":"Synthetic source",
                "url":"https://example.invalid/source"
            }
        }]);
    }
    if matches!(
        reasoning,
        Some(ReasoningShape::Summary | ReasoningShape::Opaque)
    ) {
        assert_eq!(api, Api::Responses);
        let items = value["output"].as_array_mut().unwrap();
        items.retain(|i| i["type"] != "reasoning");
        let mut item = json!({
            "id":format!("reason-{turn}"),"type":"reasoning","status":"completed","summary":[]
        });
        if reasoning == Some(ReasoningShape::Summary) {
            item["summary"] = json!([{"type":"summary_text","text":"Plan 🧪"}]);
            // The documented Bailian output omits this optional lifecycle field.
            item.as_object_mut().unwrap().remove("status");
            // Isolate the documented summary-history carrier from phase support.
            for item in items.iter_mut() {
                item.as_object_mut().unwrap().remove("phase");
            }
        } else {
            item["encrypted_content"] = json!(format!("synthetic-reason-{turn}"));
        }
        items.insert(0, item);
    }
    value["model"] = json!(model);
    if let super::Control::ReportedFacts { refusal, .. } = control {
        assert_eq!(api, Api::Messages);
        value["usage"]["service_tier"] = json!("standard");
        if refusal {
            value["stop_reason"] = json!("refusal");
            value["stop_details"] =
                json!({"type":"refusal","category":"cyber","explanation":"synthetic category"});
        }
    }
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
                            event["delta"]
                                .as_object_mut()
                                .unwrap()
                                .entry(key)
                                .or_insert(Value::Null);
                        }
                        for key in [
                            "input_tokens",
                            "cache_creation_input_tokens",
                            "cache_read_input_tokens",
                            "output_tokens_details",
                            "server_tool_use",
                        ] {
                            event["usage"]
                                .as_object_mut()
                                .unwrap()
                                .entry(key)
                                .or_insert(Value::Null);
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
            delta.as_object_mut().unwrap().remove("annotations");
            if let Some(calls) = delta.get_mut("tool_calls").and_then(Value::as_array_mut) {
                for (i, call) in calls.iter_mut().enumerate() {
                    call["index"] = json!(i);
                }
            }
            let chunk = |delta: Value, finish: Value| json!({"id":value["id"],"object":"chat.completion.chunk","created":1,"model":value["model"],"service_tier":value["service_tier"],"choices":[{"index":0,"delta":delta,"finish_reason":finish,"logprobs":null}],"usage":null});
            let terminal = chunk(json!({}), value["choices"][0]["finish_reason"].clone());
            // Standard include_usage supplies one nonnull report, after the finish chunk.
            let usage = json!({"id":value["id"],"object":"chat.completion.chunk","created":1,"model":value["model"],"service_tier":value["service_tier"],"choices":[],"usage":value["usage"]});
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
                        for (summary_index, part) in
                            item["summary"].as_array().unwrap().iter().enumerate()
                        {
                            events.push(json!({"type":"response.reasoning_summary_part.added","output_index":index,"item_id":id,"summary_index":summary_index,"part":{"type":"summary_text","text":""}}));
                            events.push(json!({"type":"response.reasoning_summary_text.delta","output_index":index,"item_id":id,"summary_index":summary_index,"delta":part["text"]}));
                            events.push(json!({"type":"response.reasoning_summary_text.done","output_index":index,"item_id":id,"summary_index":summary_index,"text":part["text"]}));
                            events.push(json!({"type":"response.reasoning_summary_part.done","output_index":index,"item_id":id,"summary_index":summary_index,"part":part}));
                        }
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
            let mut fragments = std::collections::BTreeMap::<(u64, &str, u64), String>::new();
            let mut finished = std::collections::BTreeMap::new();
            for event in &events {
                let kind = match event["type"].as_str().unwrap() {
                    "response.output_text.delta" | "response.output_text.done" => {
                        Some(("text", "content_index", "text"))
                    }
                    "response.reasoning_summary_text.delta"
                    | "response.reasoning_summary_text.done" => {
                        Some(("summary", "summary_index", "text"))
                    }
                    "response.function_call_arguments.delta"
                    | "response.function_call_arguments.done" => {
                        Some(("arguments", "", "arguments"))
                    }
                    _ => None,
                };
                if let Some((kind, index, field)) = kind {
                    let key = (
                        event["output_index"].as_u64().unwrap(),
                        kind,
                        if index.is_empty() {
                            0
                        } else {
                            event[index].as_u64().unwrap()
                        },
                    );
                    if event["type"].as_str().unwrap().ends_with(".delta") {
                        fragments
                            .entry(key)
                            .or_default()
                            .push_str(event["delta"].as_str().unwrap());
                    } else {
                        let text = fragments.remove(&key).unwrap_or_default();
                        assert_eq!(text, event[field].as_str().unwrap(), "incremental {kind}");
                        assert!(
                            finished.insert(key, text).is_none(),
                            "duplicate value completion"
                        );
                    }
                }
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
                    let item = &event["item"];
                    let index = event["output_index"].as_u64().unwrap();
                    match item["type"].as_str().unwrap() {
                        "function_call" => assert_eq!(
                            finished.remove(&(index, "arguments", 0)).unwrap(),
                            item["arguments"].as_str().unwrap()
                        ),
                        "message" | "reasoning" => {
                            let (field, kind) = if item["type"] == "message" {
                                ("content", "text")
                            } else {
                                ("summary", "summary")
                            };
                            for (part, value) in item[field].as_array().unwrap().iter().enumerate()
                            {
                                assert_eq!(
                                    finished.remove(&(index, kind, part as u64)).unwrap(),
                                    value["text"].as_str().unwrap(),
                                    "completed item cannot replace incremental {kind}"
                                );
                            }
                        }
                        other => panic!("delivered item {other}"),
                    }
                    completed.push(event["item"].clone());
                }
            }
            assert!(opened.is_empty());
            assert!(fragments.is_empty() && finished.is_empty());
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
            let mut reports = serde_json::Map::new();
            let mut calls = std::collections::BTreeMap::<u64, Value>::new();
            let mut terminal = None;
            let mut usage = Value::Null;
            for data in data.into_iter().filter(|s| *s != "[DONE]") {
                let event: Value = serde_json::from_str(data).unwrap();
                for key in ["service_tier", "system_fingerprint", "metadata"] {
                    if let Some(value) = event.get(key)
                        && let Some(previous) = reports.insert(key.into(), value.clone())
                    {
                        assert_eq!(previous, *value, "consistent Chat report");
                    }
                }
                if !event["usage"].is_null() {
                    assert!(usage.is_null(), "one final Chat usage observation");
                    usage = event["usage"].clone();
                }
                for choice in event["choices"].as_array().unwrap() {
                    let delta = &choice["delta"];
                    if let Some(text) = delta["content"].as_str() {
                        message["content"] =
                            json!(format!("{}{text}", message["content"].as_str().unwrap()));
                    }
                    if let Some(text) = delta["reasoning_content"].as_str() {
                        let prefix = message["reasoning_content"].as_str().unwrap_or("");
                        message["reasoning_content"] = json!(format!("{prefix}{text}"));
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
            let mut value = json!({"choices":[{"message":message,"finish_reason":terminal.unwrap()}],"usage":usage});
            value.as_object_mut().unwrap().extend(reports);
            value
        }
    }
}

/// Compare content, accounting and terminals across transports. Request-specific
/// IDs/timestamps and issuer-bound seals differ between isolated loopback gateways.
pub(super) fn delivery_view(api: Api, value: &Value) -> Value {
    let mut view = match api {
        Api::Chat => json!({
            "message":value["choices"][0]["message"],
            "terminal":value["choices"][0]["finish_reason"],"usage":value["usage"]
        }),
        Api::Responses => json!({
            "output":value["output"],"terminal":value["status"],
            "incomplete_details":value["incomplete_details"],"usage":value["usage"]
        }),
        Api::Messages => json!({
            "content":value["content"],"terminal":value["stop_reason"],
            "stop_sequence":value["stop_sequence"],"stop_details":value["stop_details"],
            "usage":value["usage"]
        }),
    };
    // Keep absence, null and concrete report values distinct in the comparison.
    view["reports"] = Value::Object(
        [
            "service_tier",
            "system_fingerprint",
            "metadata",
            "background",
            "store",
        ]
        .into_iter()
        .filter_map(|key| value.get(key).map(|value| (key.into(), value.clone())))
        .collect(),
    );
    match api {
        Api::Chat => {
            let message = view["message"].as_object_mut().unwrap();
            if message
                .get("content")
                .is_none_or(|v| v.is_null() || v == "")
            {
                message.insert("content".into(), Value::Null);
            }
            // JSON has a required nullable refusal; chunks have no absent-value update.
            if message.get("refusal").is_some_and(Value::is_null) {
                message.remove("refusal");
            }
        }
        Api::Responses => {
            for (index, item) in view["output"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                if item["id"].is_string() {
                    item["id"] = json!(format!("owner-{index}"));
                }
                if item["encrypted_content"].is_string() {
                    item["encrypted_content"] = json!("<sealed>");
                }
            }
        }
        Api::Messages => {
            for block in view["content"].as_array_mut().unwrap() {
                for key in ["signature", "data"] {
                    if block[key].is_string() {
                        block[key] = json!("<sealed>");
                    }
                }
            }
        }
    }
    view
}

/// Compare the entire JSON-client envelope, including all settings and reports.
/// Reuse only the documented identity/seal and Chat-shell normalizations above.
pub(super) fn intake_view(api: Api, value: &Value) -> Value {
    let semantic = delivery_view(api, value);
    let mut full = value.clone();
    match api {
        Api::Chat => full["choices"][0]["message"] = semantic["message"].clone(),
        Api::Responses => full["output"] = semantic["output"].clone(),
        Api::Messages => full["content"] = semantic["content"].clone(),
    }
    for key in ["id", "created", "created_at", "completed_at", "morphiecore"] {
        full.as_object_mut().unwrap().remove(key);
    }
    if full["previous_response_id"].is_string() {
        full["previous_response_id"] = json!("<previous-response>");
    }
    full
}

pub(super) fn assert_delivery(
    api: Api,
    profile: Profile,
    turn: usize,
    value: &Value,
    shape: Option<ReasoningShape>,
) {
    if let Some(shape) = shape {
        assert_reasoning(api, shape, value, false, turn == 0);
    } else if profile == Profile::Reasoning {
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

pub(super) fn assert_history(
    api: Api,
    profile: Profile,
    body: &Value,
    shape: Option<ReasoningShape>,
) {
    let history = body[api.history()].as_array().unwrap();
    if let Some(shape) = shape {
        assert_reasoning(api, shape, body, true, true);
    }
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
    if profile == Profile::Reasoning && shape.is_none() {
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

fn assert_reasoning(api: Api, shape: ReasoningShape, value: &Value, history: bool, first: bool) {
    let items = if history {
        value[api.history()].as_array().unwrap().clone()
    } else {
        match api {
            Api::Responses => value["output"].as_array().unwrap().clone(),
            Api::Messages => vec![json!({"content":value["content"]})],
            Api::Chat => vec![value["choices"][0]["message"].clone()],
        }
    };
    match shape {
        ReasoningShape::Summary | ReasoningShape::Opaque => {
            assert_eq!(api, Api::Responses);
            let item = items.iter().find(|i| i["type"] == "reasoning").unwrap();
            if shape == ReasoningShape::Summary {
                assert_eq!(item["id"], if first { "reason-0" } else { "reason-1" });
                if !history {
                    assert_eq!(item["status"], "completed");
                }
                assert_eq!(
                    item["summary"],
                    json!([{"type":"summary_text","text":"Plan 🧪"}])
                );
                assert!(item.get("encrypted_content").is_none());
            } else {
                assert_eq!(item["summary"], json!([]));
                let token = item["encrypted_content"].as_str().unwrap();
                if history {
                    assert_eq!(token, "synthetic-reason-0");
                } else {
                    assert!(token.starts_with("mc1."));
                }
            }
        }
        ReasoningShape::Readable => {
            assert_eq!(api, Api::Chat);
            let item = items
                .iter()
                .find(|i| i["reasoning_content"].is_string())
                .unwrap();
            assert_eq!(item["reasoning_content"], "Plan 🧪");
        }
        ReasoningShape::Signed => {
            assert_eq!(api, Api::Messages);
            let blocks: Vec<_> = items
                .iter()
                .flat_map(|i| i["content"].as_array().into_iter().flatten())
                .collect();
            let thinking = blocks.iter().find(|b| b["type"] == "thinking").unwrap();
            assert_eq!(thinking["thinking"], "Plan 🧪");
            let mut tokens = vec![(thinking, "signature", "synthetic-thinking-0")];
            if first {
                let redacted = blocks
                    .iter()
                    .find(|b| b["type"] == "redacted_thinking")
                    .unwrap();
                tokens.push((redacted, "data", "synthetic-redacted"));
            }
            for (block, key, expected) in tokens {
                let token = block[key].as_str().unwrap();
                if history {
                    assert_eq!(token, expected);
                } else {
                    assert!(token.starts_with("mc1."));
                }
            }
        }
    }
}
