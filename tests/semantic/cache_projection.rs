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
fn public_responses_cache_key_preserves_presence_without_creating_session_identity() {
    use morphiecore::semantic::value::Presence;
    let client = Adapter::new(Profile::Responses, Dialect::Standard, None);
    for dialect in [Dialect::Siwc, Dialect::Grok] {
        let target = Adapter::new(Profile::Responses, dialect, None);
        for key in [None, Some(Value::Null), Some(json!("shared-workspace"))] {
            let mut wire = json!({"model":"public","input":"hello"});
            if let Some(key) = &key {
                wire["prompt_cache_key"] = key.clone();
            }
            let mut request = client.decode_request(wire.to_string().as_bytes()).unwrap();
            let expected = match &key {
                None => Presence::Absent,
                Some(Value::Null) => Presence::Null,
                Some(_) => Presence::Value("shared-workspace".into()),
            };
            assert_eq!(request.context.cache.prompt_cache_key, expected);
            assert!(request.cache_session.is_none());
            let original = request.clone();
            let out = target
                .encode_request(&request, "upstream", &Contract::full())
                .unwrap();
            assert_eq!(out.get("prompt_cache_key"), key.as_ref(), "{dialect:?}");
            assert!(out.get("session_id").is_none());
            assert_eq!(request, original);
            request.context.cache.prompt_cache_key = Presence::Absent;
            let out = target
                .encode_request(&request, "upstream", &Contract::full())
                .unwrap();
            assert!(out.get("prompt_cache_key").is_none());
        }
    }
    let request = client
        .decode_request(
            br#"{"model":"public","input":"hello","prompt_cache_key":"shared-workspace"}"#,
        )
        .unwrap();
    // The Responses carrier does not authorize an undocumented Grok Chat body field.
    let out = Adapter::new(Profile::Chat, Dialect::Grok, None)
        .encode_request(&request, "upstream", &Contract::full())
        .unwrap();
    assert!(out.get("prompt_cache_key").is_none());
}

#[test]
fn neutral_conversation_defaults_cache_without_overriding_explicit_intent() {
    use morphiecore::semantic::{context::ConversationContext, value::Presence};
    let client = Adapter::new(Profile::Responses, Dialect::Standard, None);
    let mut request = client
        .decode_request(br#"{"model":"public","input":"hello"}"#)
        .unwrap();
    request.conversation = Some(ConversationContext::conversation("conversation-one").unwrap());
    let before = request.clone();
    for dialect in [Dialect::Siwc, Dialect::Grok] {
        let target = Adapter::new(Profile::Responses, dialect, None);
        let out = target
            .encode_request(&request, "upstream", &Contract::full())
            .unwrap();
        assert_eq!(
            out["prompt_cache_key"],
            "2b9fca8fab555e6f158d416aad2a034e943010b3721ca44a5fb1c3e77d9668af"
        );
        for key in [Presence::Null, Presence::Value("shared-workspace".into())] {
            let mut explicit = request.clone();
            explicit.context.cache.prompt_cache_key = key.clone();
            let out = target
                .encode_request(&explicit, "upstream", &Contract::full())
                .unwrap();
            assert_eq!(
                out["prompt_cache_key"],
                match key {
                    Presence::Null => Value::Null,
                    Presence::Value(s) => json!(s),
                    _ => unreachable!(),
                }
            );
        }
        let mut independent = request.clone();
        independent.conversation =
            Some(ConversationContext::independent_request("conversation-one").unwrap());
        assert!(
            target
                .encode_request(&independent, "upstream", &Contract::full())
                .unwrap()
                .get("prompt_cache_key")
                .is_none()
        );
    }
    assert_eq!(request, before);
    assert!(request.cache_session.is_none());
    assert!(!format!("{:?}", request.conversation).contains("conversation-one"));
    for id in ["", " ", "x,y", "bad\r\nheader", "中文", &"x".repeat(257)] {
        assert!(ConversationContext::conversation(id).is_err());
    }
}

#[test]
fn go_grouping_uses_neutral_context_and_never_infers_it_from_cache_keys() {
    use morphiecore::{
        protocol::cache::CacheSession,
        semantic::{context::ConversationContext, value::Presence},
    };
    let client = Adapter::new(Profile::Responses, Dialect::Standard, None);
    let target = Adapter::new(Profile::Chat, Dialect::OpenCodeGo, None);
    let mut request = client
        .decode_request(
            br#"{"model":"public","input":"hello","prompt_cache_key":"shared-workspace"}"#,
        )
        .unwrap();
    assert!(
        target
            .encode_request(&request, "upstream", &Contract::full())
            .is_err()
    );
    request.conversation = Some(ConversationContext::conversation("conversation-one").unwrap());
    let before = request.clone();
    let out = target
        .encode_request(&request, "upstream", &Contract::full())
        .unwrap();
    assert!(out.get("prompt_cache_key").is_none());
    assert!(out.get("session_id").is_none());
    assert_eq!(request, before);
    assert!(request.cache_session.is_none());
    let mut boundary = request.clone();
    boundary.context.cache.prompt_cache_key = Presence::Value("x".repeat(256));
    target
        .encode_request(&boundary, "upstream", &Contract::full())
        .unwrap();
    for key in [
        Presence::Absent,
        Presence::Null,
        Presence::Value(String::new()),
        Presence::Value("bad\r\nheader".into()),
        Presence::Value("中文".into()),
    ] {
        let mut edited = request.clone();
        edited.context.cache.prompt_cache_key = key;
        target
            .encode_request(&edited, "upstream", &Contract::full())
            .unwrap();
    }
    let mut oversized = request.clone();
    oversized.context.cache.prompt_cache_key = Presence::Value("x".repeat(257));
    assert!(
        target
            .encode_request(&oversized, "upstream", &Contract::full())
            .is_err()
    );
    let mut conflicting = request.clone();
    conflicting.cache_session = Some(CacheSession::new("legacy-provider-group").unwrap());
    assert!(
        target
            .encode_request(&conflicting, "upstream", &Contract::full())
            .is_err()
    );
    let mut no_carrier = Contract::full();
    no_carrier.cache.session_id = false;
    assert!(
        target
            .encode_request(&request, "upstream", &no_carrier)
            .is_err()
    );
    assert!(
        target
            .encode_request(&conflicting, "upstream", &no_carrier)
            .is_err()
    );
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
