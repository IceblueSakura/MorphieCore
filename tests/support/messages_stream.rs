//! Independent downstream Messages SSE fold; never uses production decoding.
use serde_json::{Value, json};

/// Independent fold of Anthropic SSE into the message it streams.
pub fn messages_stream_body(wire: &str) -> Value {
    let mut message = Value::Null;
    let mut partial = std::collections::BTreeMap::<u64, String>::new();
    let mut open = None;
    for data in wire.lines().filter_map(|line| line.strip_prefix("data: ")) {
        let event: Value = serde_json::from_str(data).unwrap();
        let index = event["index"].as_u64().unwrap_or(0);
        match event["type"].as_str().unwrap() {
            "message_start" => message = event["message"].clone(),
            "content_block_start" => {
                assert_eq!(open.replace(index), None, "overlapping Messages blocks");
                assert_eq!(index as usize, message["content"].as_array().unwrap().len());
                message["content"]
                    .as_array_mut()
                    .unwrap()
                    .push(event["content_block"].clone());
            }
            "content_block_delta" => {
                assert_eq!(open, Some(index), "delta owner");
                let block = &mut message["content"][index as usize];
                let delta = &event["delta"];
                match delta["type"].as_str().unwrap() {
                    "text_delta" => {
                        block["text"] = json!(format!(
                            "{}{}",
                            block["text"].as_str().unwrap(),
                            delta["text"].as_str().unwrap()
                        ))
                    }
                    "thinking_delta" => {
                        block["thinking"] = json!(format!(
                            "{}{}",
                            block["thinking"].as_str().unwrap(),
                            delta["thinking"].as_str().unwrap()
                        ))
                    }
                    "signature_delta" => block["signature"] = delta["signature"].clone(),
                    "input_json_delta" => partial
                        .entry(index)
                        .or_default()
                        .push_str(delta["partial_json"].as_str().unwrap()),
                    other => panic!("delta {other}"),
                }
            }
            "content_block_stop" => {
                assert_eq!(open.take(), Some(index), "stop owner");
                if let Some(json) = partial.remove(&index) {
                    message["content"][index as usize]["input"] =
                        serde_json::from_str(&json).unwrap();
                }
            }
            "message_delta" => {
                assert_eq!(open, None);
                message["stop_reason"] = event["delta"]["stop_reason"].clone();
                for (key, value) in event["usage"].as_object().unwrap() {
                    if !value.is_null() {
                        message["usage"][key] = value.clone();
                    }
                }
            }
            "message_stop" => {
                assert_eq!(open, None);
                return message;
            }
            other => panic!("event {other}"),
        }
    }
    panic!("no message_stop")
}
