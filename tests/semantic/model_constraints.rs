//! Model identity does not authorize a Provider to erase unsupported media.
use morphiecore::protocol::openai::{Adapter, Dialect, Profile};
use serde_json::json;
#[test]
fn native_audio_binding_is_explicit_chat_input_with_text_only_output() {
    use morphiecore::topology::{ProtocolProfile, catalog::API_KEY_BINDINGS};
    let binding = API_KEY_BINDINGS
        .iter()
        .find(|b| b.model == "gpt-audio-mini")
        .unwrap();
    assert_eq!(binding.upstream, "openai/gpt-audio-mini");
    assert_eq!(binding.protocols, &[ProtocolProfile::OpenAiChat]);
    let endpoint = binding.endpoint(ProtocolProfile::OpenAiChat).unwrap();
    assert!(endpoint.representation.semantics().audio_input);
    assert!(!endpoint.representation.semantics().audio_output);
    assert!(!endpoint.representation.semantics().audio_history);
    assert!(!endpoint.representation.semantics().reasoning);
    let client = Adapter::new(Profile::Chat, Dialect::Standard, None);
    let body = json!({"model":"gpt-audio-mini","modalities":["text"],"messages":[
        {"role":"user","content":[{"type":"input_audio","input_audio":{"format":"wav","data":"AQID"}}]}]});
    let input = client
        .decode_request(&body.to_string().into_bytes())
        .unwrap();
    assert!(
        input
            .check_semantic(&binding.public_model().contract)
            .is_ok()
    );
    let upstream = Adapter::new(Profile::Chat, binding.dialect, None);
    let sent = upstream
        .encode_request(
            &input,
            binding.upstream,
            endpoint.representation.openai().unwrap(),
        )
        .unwrap();
    assert_eq!(sent["messages"], body["messages"]);
    assert_eq!(sent["modalities"], json!(["text"]));
    let unrelated = API_KEY_BINDINGS
        .iter()
        .find(|b| b.model == "gpt-6-luna")
        .unwrap();
    assert!(
        input
            .check_semantic(&unrelated.public_model().contract)
            .is_err()
    );
}
