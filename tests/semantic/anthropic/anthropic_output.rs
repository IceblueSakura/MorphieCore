//! Messages structured output: one IR owner with a native and an OpenAI carrier.

use morphiecore::{
    protocol::{
        GenerationRequestEnvelope as Request,
        anthropic::messages::client::decode_client_request,
        openai::{Dialect, GenerationApi, GenerationCodec, TargetContract},
    },
    semantic::{context::ConversationContext, task::generation::*},
    topology::catalog,
};
use serde_json::{Value, json};

fn schema() -> Value {
    json!({"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false})
}
fn grouped(mut request: Request) -> Request {
    // The Go binding needs a session grouping.
    request.conversation = Some(ConversationContext::conversation("synthetic").unwrap());
    request
}
fn messages(config: Value) -> Result<Request, morphiecore::protocol::CodecError> {
    let raw = json!({"model":"m","max_tokens":8,"output_config":config,
            "messages":[{"role":"user","content":"go"}]});
    decode_client_request(raw.to_string().as_bytes()).map(grouped)
}
fn chat(format: Value) -> Request {
    let raw = json!({"model":"m","response_format":format,
            "messages":[{"role":"user","content":"go"}]});
    grouped(
        GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
            .decode_client_request(raw.to_string().as_bytes())
            .unwrap(),
    )
}

#[test]
fn messages_format_is_an_unnamed_strict_schema_with_native_and_openai_carriers() {
    let format = json!({"type":"json_schema","schema":schema()});
    let source = messages(json!({"format":format})).unwrap();
    let OutputConstraint::JsonSchema {
        name,
        description,
        schema: decoded,
        strict,
    } = source.task.semantic.output()
    else {
        panic!("structured output")
    };
    assert_eq!((name, description, strict), (&None, &None, &Some(true)));
    assert_eq!(decoded.root().to_string(), schema().to_string());
    let endpoint = catalog::MESSAGES_BINDINGS[0].endpoint();
    let (wire, rules) = endpoint.encode_request_with_rules(&source).unwrap();
    assert_eq!(wire["output_config"], json!({"format":format}));
    assert_eq!(rules, ["messages.default-automatic-cache.v1"]);
    let (wire, rules) = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
        .encode_request_with_rules(&source, "synthetic", &TargetContract::full())
        .unwrap();
    assert_eq!(
        wire["response_format"],
        json!({"type":"json_schema","json_schema":{"name":"response","schema":schema(),"strict":true}})
    );
    assert!(rules.contains(&"openai.default-schema-name.v1"));

    let responses = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    let raw = json!({"model":"m","input":"go","max_output_tokens":8,
            "text":{"verbosity":"low","format":{"type":"json_schema","name":"answer","schema":schema(),"strict":true}},
            "reasoning":{"effort":"none"},
            "tools":[{"type":"function","name":"lookup","parameters":{"type":"object"},"strict":false}]});
    let source = grouped(
        responses
            .decode_client_request(raw.to_string().as_bytes())
            .unwrap(),
    );
    let saved = source.clone();
    assert_eq!(
        source.task.semantic.text_options().verbosity,
        morphiecore::semantic::value::Presence::Value(Verbosity::Low)
    );
    let (wire, rules) = endpoint.encode_request_with_rules(&source).unwrap();
    assert!(wire.get("text").is_none() && wire.get("verbosity").is_none());
    assert_eq!(wire["max_tokens"], 8);
    assert_eq!(wire["thinking"], json!({"type":"disabled"}));
    assert_eq!(
        wire["tools"],
        json!([{"name":"lookup","input_schema":{"type":"object"},"strict":false}])
    );
    assert_eq!(wire["output_config"], json!({"format":format}));
    assert!(rules.contains(&"messages.omit-verbosity.v1"));
    use morphiecore::{
        protocol::conversion::generation::Effect,
        protocol::{anthropic, native::ReplayTarget},
        semantic::value::Presence,
    };
    let caps = anthropic::messages::TargetContract::new(
        anthropic::messages::Profile::AdaptiveTextTools,
        anthropic::messages::Dialect::Standard,
    );
    assert!(!caps.semantics.verbosity);
    assert!(GenerationRequirements::derive(&source.task.semantic).verbosity);
    assert!(caps.semantics.check(&source.task.semantic).is_err());
    let target = ReplayTarget::new("synthetic", "synthetic").unwrap();
    let projected = caps.encode_request(&source, &target).unwrap();
    assert_eq!(
        projected.evidence.iter().cloned().collect::<Vec<_>>(),
        [
            Effect::new(
                "messages.omit-schema-name.v1",
                morphiecore::protocol::conversion::generation::Field::SchemaName,
                morphiecore::protocol::conversion::generation::ConversionBoundary::RequestProject,
                morphiecore::protocol::conversion::generation::Change::Omit
            ),
            Effect::MessagesVerbosity {
                input: Presence::Value(Verbosity::Low),
                output: Presence::Absent,
                owner: morphiecore::protocol::conversion::generation::Owner::Root,
            }
        ]
    );
    let receipt = &projected.evidence[1];
    assert_eq!(receipt.rule(), "messages.omit-verbosity.v1");
    assert_eq!(receipt.field(), "generation.text.verbosity");
    assert_eq!(
        (receipt.boundary().as_str(), receipt.change().as_str()),
        ("request.project", "omit")
    );
    let mut constrained = caps;
    constrained.semantics.structured_output = false;
    assert!(constrained.encode_request(&source, &target).is_err());
    assert_eq!(source, saved);
    assert_eq!(
        responses
            .encode_request(&source, "m", &TargetContract::full())
            .unwrap()["text"],
        raw["text"]
    );
    let mut invalid = raw;
    invalid["text"]["verbosity"] = json!("loud");
    assert!(
        responses
            .decode_client_request(invalid.to_string().as_bytes())
            .is_err()
    );
}

#[test]
fn unknown_and_natively_unrepresentable_formats_fail() {
    assert!(messages(json!({"format":{"type":"json_object"}})).is_err());
    assert!(messages(json!({"format":{"type":"json_schema","schema":true}})).is_err());
    let endpoint = catalog::MESSAGES_BINDINGS[0].endpoint();
    // Messages has no JSON-object mode and no model-visible schema description.
    for format in [
        json!({"type":"json_object"}),
        json!({"type":"json_schema","json_schema":{"name":"answer","description":"pick","schema":schema()}}),
    ] {
        assert!(endpoint.encode_request_with_rules(&chat(format)).is_err());
    }
    let named =
        chat(json!({"type":"json_schema","json_schema":{"name":"answer","schema":schema()}}));
    let (wire, rules) = endpoint.encode_request_with_rules(&named).unwrap();
    assert_eq!(
        wire["output_config"],
        json!({"format":{"type":"json_schema","schema":schema()}})
    );
    assert_eq!(
        rules,
        [
            "messages.default-automatic-cache.v1",
            "messages.default-max-tokens.v1",
            "messages.omit-schema-name.v1"
        ]
    );
}
