//! Chat media, probabilities, citations, files and client carriers.
#[path = "chat/chat_logprobs.rs"]
mod chat_logprobs;
#[path = "chat/chat_media.rs"]
mod chat_media;
#[path = "chat/citation_bindings.rs"]
mod citation_bindings;
#[path = "chat/client_carrier.rs"]
mod client_carrier;
#[path = "chat/files.rs"]
mod files;
#[path = "chat/images.rs"]
mod images;

#[test]
fn null_output_caps_are_absent_and_two_caps_still_conflict() {
    use morphiecore::protocol::openai::chat::envelope::decode_request;
    use serde_json::json;
    let cap = |fields: serde_json::Value| {
        let mut wire = json!({"model":"m","messages":[{"role":"user","content":"go"}]});
        wire.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        decode_request(&wire).map(|decoded| decoded.task.semantic.controls().max_output_tokens)
    };
    assert_eq!(
        cap(json!({"max_tokens":null,"max_completion_tokens":7})).unwrap(),
        Some(7)
    );
    assert_eq!(
        cap(json!({"max_tokens":7,"max_completion_tokens":null})).unwrap(),
        Some(7)
    );
    assert_eq!(cap(json!({"max_tokens":7})).unwrap(), Some(7));
    assert_eq!(cap(json!({"max_tokens":null})).unwrap(), None);
    assert!(cap(json!({"max_tokens":7,"max_completion_tokens":7})).is_err());
}
