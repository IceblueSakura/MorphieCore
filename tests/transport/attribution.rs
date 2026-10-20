//! Google label admission agrees between preflight and prepared wire bytes.
use morphiecore::{
    adapter::AdapterError,
    execution::{
        AttemptError,
        plan::{RejectionReason, representable},
        prepare,
    },
    protocol::{
        CodecError,
        openai::{Adapter, Dialect, Profile},
    },
    provider::SecretMaterial,
    topology::catalog::GOOGLE_BINDINGS,
};
use serde_json::{Value, json};

#[test]
fn google_safety_labels_are_bounded_unicode_without_repair_or_fallback() {
    let binding = &GOOGLE_BINDINGS[0];
    let endpoint = binding.endpoint();
    for (value, valid) in [
        (String::new(), true),
        ("user_123-abc".into(), true),
        ("用".repeat(63), true),
        ("éⅳ١".into(), true),
        ("用".repeat(64), false),
        ("Uppercase".into(), false),
        ("a b".into(), false),
        ("a.b".into(), false),
        ("e\u{301}".into(), false),
        ("ǅ".into(), false),
        ("🧪".into(), false),
    ] {
        let request = Adapter::new(Profile::Responses, Dialect::Standard, None)
            .decode_request(
                json!({"model":binding.model,"input":"go","max_output_tokens":4096,
                "user":"valid_fallback","safety_identifier":value})
                .to_string()
                .as_bytes(),
            )
            .unwrap();
        let before = request.clone();
        let prepared = prepare(
            &endpoint,
            &binding.provider(),
            &SecretMaterial::new("synthetic").unwrap(),
            &request,
        );
        if valid {
            representable(&endpoint, &request).unwrap();
            let prepared = prepared.unwrap();
            let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
            assert_eq!(wire["labels"], json!({"safety_identifier":value}));
            assert_eq!(wire["generationConfig"]["maxOutputTokens"], 4096);
        } else {
            assert_eq!(
                representable(&endpoint, &request),
                Err(RejectionReason::Representation)
            );
            assert!(matches!(
                endpoint.encode_request(&request),
                Err(AdapterError::Codec(CodecError::Invalid(
                    "Google safety label"
                )))
            ));
            assert!(matches!(
                prepared,
                Err(AttemptError::Adapter(AdapterError::Codec(
                    CodecError::Invalid("Google safety label")
                )))
            ));
        }
        assert_eq!(request, before);
    }
}
