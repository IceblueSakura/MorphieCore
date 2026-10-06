//! Cache affinity is Provider functionality, not gateway sessions or route selection.
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract as Contract,
    protocol::openai::Profile,
};
use serde_json::{Value, json};
fn client(p: Profile) -> Adapter {
    Adapter::new(p, Dialect::MorphieCore, None)
}
fn request(p: Profile) -> Value {
    match p {
        Profile::Responses => {
            json!({"model":"public","input":[{"role":"user","content":[{"type":"input_text","text":"stable prefix"},{"type":"input_image","image_url":"https://example.invalid/image","detail":"auto"}]}]})
        }
        Profile::Chat => {
            json!({"model":"public","messages":[{"role":"user","content":[{"type":"text","text":"stable prefix"},{"type":"image_url","image_url":{"url":"https://example.invalid/image","detail":"auto"}}]}]})
        }
    }
}
#[test]
fn cache_omission_never_erases_independent_identity_fields() {
    for p in [Profile::Chat, Profile::Responses] {
        let mut wire = request(p);
        wire["user"] = json!("synthetic-user");
        wire["safety_identifier"] = json!("synthetic-safety");
        wire["prompt_cache_key"] = json!("affinity-key");
        let req = client(p)
            .decode_request(&serde_json::to_vec(&wire).unwrap())
            .unwrap();
        let mut target = Contract::full();
        target.cache = Default::default();
        let out = client(p).encode_request(&req, "upstream", &target).unwrap();
        assert_eq!(out["user"], "synthetic-user");
        assert_eq!(out["safety_identifier"], "synthetic-safety");
        assert!(out.get("prompt_cache_key").is_none());
        target.identity_hints = false;
        assert!(client(p).encode_request(&req, "upstream", &target).is_err());
    }
}
#[test]
fn append_only_turns_preserve_target_prefix_tool_and_schema_order() {
    let raw=br#"{"model":"public","prompt_cache_key":"stable-key","messages":[{"role":"system","content":"fixed instruction"},{"role":"user","content":[{"type":"text","text":"stable prefix"},{"type":"image_url","image_url":{"url":"https://example.invalid/image","detail":"auto"}}]}],"tools":[{"type":"function","function":{"name":"lookup","strict":false,"parameters":{"type":"object","properties":{"zeta":{"type":"string"},"alpha":{"type":"string"}}}}}]}"#;
    let first = client(Profile::Chat).decode_request(raw).unwrap();
    let mut next: Value = serde_json::from_slice(raw).unwrap();
    next["messages"].as_array_mut().unwrap().extend([
        json!({"role":"assistant","content":"previous answer"}),
        json!({"role":"user","content":"next question"}),
    ]);
    let second = client(Profile::Chat)
        .decode_request(&serde_json::to_vec(&next).unwrap())
        .unwrap();
    for p in [Profile::Chat, Profile::Responses] {
        let target = Adapter::new(p, Dialect::OpenRouter, None);
        let one = target
            .encode_request(&first, "upstream", &Contract::full())
            .unwrap();
        let two = target
            .encode_request(&second, "upstream", &Contract::full())
            .unwrap();
        let field = if p == Profile::Chat {
            "messages"
        } else {
            "input"
        };
        assert_eq!(one[field].as_array().unwrap().len(), 2);
        assert_eq!(
            &two[field].as_array().unwrap()[..2],
            one[field].as_array().unwrap()
        );
        assert_eq!(
            serde_json::to_vec(&two["tools"]).unwrap(),
            serde_json::to_vec(&one["tools"]).unwrap()
        );
        let tool = if p == Profile::Chat {
            &two["tools"][0]["function"]
        } else {
            &two["tools"][0]
        };
        assert_eq!(
            tool["parameters"]["properties"]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["zeta", "alpha"]
        );
        assert_eq!(two["prompt_cache_key"], "stable-key");
        let prefix = serde_json::to_string(&one[field]).unwrap();
        assert!(prefix.contains("fixed instruction"));
        assert!(prefix.contains("stable prefix"));
        assert!(prefix.contains("https://example.invalid/image"));
    }
    let mut active: Value = serde_json::from_slice(raw).unwrap();
    active["prompt_cache_options"] = json!({"mode":"explicit","ttl":"30m"});
    let active = client(Profile::Chat)
        .decode_request(&serde_json::to_vec(&active).unwrap())
        .unwrap();
    assert!(
        Adapter::new(Profile::Chat, Dialect::DeepSeek, None)
            .encode_request(&active, "upstream", &Contract::full())
            .is_err()
    );
}
#[test]
fn scoped_session_and_standard_cache_key_remain_independent_on_both_wires() {
    for p in [Profile::Chat, Profile::Responses] {
        let mut wire = request(p);
        wire["prompt_cache_key"] = json!("key-a");
        wire["session_id"] = json!("session-b");
        let req = client(p)
            .decode_request(&serde_json::to_vec(&wire).unwrap())
            .unwrap();
        let target = Adapter::new(p, Dialect::OpenRouter, None);
        let out = target
            .encode_request(&req, "upstream", &Contract::full())
            .unwrap();
        assert_eq!(out["session_id"], "session-b");
        assert_eq!(out["prompt_cache_key"], "key-a");
        let mut edited = req.clone();
        edited.cache_session = None;
        edited.context.cache.prompt_cache_key = morphiecore::semantic::value::Presence::Absent;
        let erased = target
            .encode_request(&edited, "upstream", &Contract::full())
            .unwrap();
        assert!(erased.get("session_id").is_none());
        assert!(erased.get("prompt_cache_key").is_none());
        assert!(
            Adapter::new(p, Dialect::Standard, None)
                .decode_request(&serde_json::to_vec(&wire).unwrap())
                .is_err()
        );
        assert!(
            Adapter::new(p, Dialect::DeepSeek, None)
                .encode_request(&req, "upstream", &Contract::full())
                .is_err()
        );
        for invalid in [
            Value::Null,
            json!(""),
            json!("x".repeat(257)),
            json!({"token":"synthetic"}),
        ] {
            wire["session_id"] = invalid;
            assert!(
                client(p)
                    .decode_request(&serde_json::to_vec(&wire).unwrap())
                    .is_err()
            );
        }
    }
}
