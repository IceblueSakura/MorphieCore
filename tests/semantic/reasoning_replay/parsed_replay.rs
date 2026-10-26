//! SDK-derived views are unregistered wire fields, never request authority.
use morphiecore::{
    protocol::openai::{Dialect, GenerationApi, GenerationCodec},
    semantic::task::generation::Item,
};
use serde_json::{Value, json};

#[test]
fn sdk_views_never_validate_or_rewrite_raw_arguments() {
    for raw in [r#"{"n":0.123456789012345678901}"#, "{n:1}"] {
        for view in [Value::Null, json!({"n":2}), json!([1]), json!(true)] {
            for api in [GenerationApi::Chat, GenerationApi::Responses] {
                let body = match api {
                    GenerationApi::Chat => {
                        json!({"model":"m","messages":[{"role":"assistant","tool_calls":[{"id":"c","type":"function","function":{"name":"f","arguments":raw,"parsed_arguments":view}}]}]})
                    }
                    GenerationApi::Responses => {
                        json!({"model":"m","input":[{"type":"function_call","call_id":"c","name":"f","arguments":raw,"parsed_arguments":view}]})
                    }
                };
                let codec = GenerationCodec::new(api, Dialect::Standard, None);
                let bytes = body.to_string();
                let request = codec.decode_client_request(bytes.as_bytes()).unwrap();
                assert!(codec.decode_request(bytes.as_bytes()).is_err());
                let arguments = request
                    .task
                    .semantic
                    .items()
                    .iter()
                    .find_map(|(_, item)| match item {
                        Item::ToolCall(call) => call.arguments.as_raw(),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(arguments, raw);
            }
        }
    }
}
