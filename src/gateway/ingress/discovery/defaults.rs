//! Public, synthetic installed-client shapes shared by discovery and its wire gate.
use crate::protocol::ClientProtocol;
use serde_json::{Value, json};
use std::sync::OnceLock;

fn corpus() -> &'static Value {
    static CORPUS: OnceLock<Value> = OnceLock::new();
    CORPUS.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../testdata/repair-and-recall/agent-defaults.json"
        ))
        .expect("checked public client-default corpus")
    })
}

pub(crate) fn source() -> &'static str {
    corpus()["source"].as_str().expect("checked corpus source")
}

pub(crate) fn request(
    protocol: ClientProtocol,
    profile: &str,
    model: &str,
    streaming: bool,
) -> Value {
    let api = match protocol {
        ClientProtocol::Chat => "chat",
        ClientProtocol::Responses => "responses",
        ClientProtocol::Messages => "messages",
    };
    let source = if protocol == ClientProtocol::Messages && model == "claude-haiku-5-5" {
        &corpus()["resolved_models"]["opencode_go_haiku"]["requests"]
    } else {
        &corpus()["requests"][api]
    };
    let mut body = source[profile].clone();
    assert!(body.is_object(), "checked client-default profile");
    body["model"] = json!(model);
    body["stream"] = json!(streaming);
    // The recorded clients stream; JSON controls remove stream-only options.
    if !streaming {
        body.as_object_mut()
            .expect("checked object")
            .remove("stream_options");
    }
    body
}
