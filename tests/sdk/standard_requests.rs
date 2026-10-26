//! Actual standard request encodings consumed by pinned upstream SDK parameter types.
use crate::child_process;
use morphiecore::protocol::{
    anthropic::messages::{self, Dialect as MessagesDialect, Profile},
    native::ReplayTarget,
    openai::{Dialect, GenerationApi, GenerationCodec, TargetContract},
};
use serde_json::json;
use std::time::Duration;

#[tokio::test]
#[ignore = "requires the locked Python SDK environment"]
async fn standard_request_encodings_match_upstream_parameter_unions() {
    let mut cases = vec![];
    for (api, source) in [
        (
            GenerationApi::Chat,
            json!({"model":"m","store":false,"messages":[{"role":"assistant","name":"alice","content":"A"}]}),
        ),
        (
            GenerationApi::Responses,
            json!({"model":"m","input":[{"role":"assistant","content":"A"}]}),
        ),
        (
            GenerationApi::Responses,
            json!({"model":"m","input":[{"type":"message","role":"developer","status":"incomplete","content":[{"type":"input_text","text":"P"}]}]}),
        ),
    ] {
        let codec = GenerationCodec::new(api, Dialect::Standard, None);
        let request = codec
            .decode_client_request(source.to_string().as_bytes())
            .unwrap();
        let wire = codec
            .encode_request(&request, "m", &TargetContract::full())
            .unwrap();
        cases.push(
            json!({"api":if api==GenerationApi::Chat {"chat"} else {"responses"},"wire":wire}),
        );
    }
    let request = messages::client::decode_client_request(br#"{"model":"m","max_tokens":2048,"thinking":{"type":"enabled","budget_tokens":1024},"messages":[{"role":"user","content":"go"}]}"#).unwrap();
    let wire = messages::TargetContract::new(Profile::AdaptiveTextTools, MessagesDialect::Standard)
        .encode_request(&request, &ReplayTarget::new("synthetic", "m").unwrap())
        .unwrap()
        .value;
    cases.push(json!({"api":"messages","wire":wire}));
    let home = tempfile::tempdir().unwrap();
    let mut command = super::isolated_sdk_python(home.path());
    command.arg("tests/sdk/standard_requests.py");
    let output = child_process::run(
        &mut command,
        &serde_json::to_vec(&cases).unwrap(),
        Duration::from_secs(30),
        16 << 10,
    )
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "SDK parameter validation: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "standard request parameter unions verified"
    );
}
