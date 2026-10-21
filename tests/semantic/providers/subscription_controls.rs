use crate::events_support::text;
use morphiecore::{
    adapter::Request,
    protocol::{
        DecodedRequest, DecodedResponse,
        fidelity::FidelityRecords,
        openai::{Adapter, Dialect, Profile, TargetCaps},
    },
    semantic::{
        task::generation::*,
        value::{Presence, ReplayOrigin},
    },
    topology::catalog::{API_KEY_BINDINGS, SUBSCRIPTION_BINDINGS},
};
use serde_json::{Value, json};

fn adapter(dialect: Dialect) -> Adapter {
    Adapter::new(
        Profile::Responses,
        dialect,
        Some(ReplayOrigin::new("subscription-scope").unwrap()),
    )
}
fn message(role: MessageRole, id: u64, value: &str) -> Item {
    Item::Message(Message {
        role,
        status: ItemLifecycle::Completed,
        phase: None,
        parts: vec![Part::new(
            PartId::new(id),
            ContentPart::Text(text(value).into()),
        )],
    })
}
fn tools(grouped: bool) -> Vec<ToolDefinition> {
    let leaf = ToolDefinition::Function(FunctionTool {
        name: text("lookup"),
        description: None,
        parameters: Some(json!({"type":"object","properties":{"n":{"type":"integer"}}}).into()),
        strict: FunctionStrictness::Explicit(false),
        output_schema: None,
        input_examples: None,
        dispatch: Default::default(),
        cache: None,
        eager_input: None,
    });
    if grouped {
        vec![ToolDefinition::Namespace(ToolNamespace {
            name: text("ops"),
            description: "operations".into(),
            tools: vec![leaf],
        })]
    } else {
        vec![leaf]
    }
}
fn request(grouped: bool, stream: bool) -> Request {
    let semantic = GenerationRequest::from_settings(
        vec![
            (
                ItemId::new(1),
                Item::Instruction(Instruction {
                    authority: InstructionAuthority::Developer,
                    parts: vec![Part::new(PartId::new(1), text("ordered"))],
                    status: None,
                }),
            ),
            (ItemId::new(2), message(MessageRole::User, 2, "go")),
        ],
        GenerationSettings {
            instructions: Presence::Value(text("policy")),
            tools: Some(tools(grouped)),
            parallel_tool_calls: Some(false),
            reasoning: ReasoningRequest::absent().with_encrypted_output(true),
            ..Default::default()
        },
    )
    .unwrap();
    let mut request = Request::from_generation(
        DecodedRequest {
            semantic,
            fidelity: FidelityRecords::default(),
        },
        "client-model",
    )
    .unwrap();
    request.delivery.stream = Presence::Value(stream);
    request.context.cache.prompt_cache_key = Presence::Value("affinity".into());
    request
}
fn request_wire(grouped: bool, stream: bool) -> Value {
    let function = json!({"type":"function","name":"lookup","strict":false,
            "parameters":{"type":"object","properties":{"n":{"type":"integer"}}}});
    json!({"model":"client-model","instructions":"policy",
            "input":[{"type":"message","role":"developer","content":[{"type":"input_text","text":"ordered"}]},
                {"type":"message","role":"user","content":[{"type":"input_text","text":"go"}]}],
            "tools":if grouped{json!([{"type":"namespace","name":"ops","description":"operations","tools":[function]}])}else{json!([function])},
            "parallel_tool_calls":false,"include":["reasoning.encrypted_content"],
            "prompt_cache_key":"affinity","stream":stream,"store":false})
}
#[test]
fn native_request_directions_use_actual_subscription_bindings() {
    for binding in SUBSCRIPTION_BINDINGS {
        let grouped = binding.profile == "openai";
        for stream in [false, true] {
            let wire = request_wire(grouped, stream);
            let source = adapter(binding.dialect)
                .decode_client_request(wire.to_string().as_bytes())
                .unwrap();
            let independent = request(grouped, stream);
            assert_eq!(source.task.semantic, independent.task.semantic);
            assert_eq!(
                source.context.cache.prompt_cache_key,
                Presence::Value("affinity".into())
            );
            assert_eq!(source.delivery.stream, Presence::Value(stream));
            for input in [source, independent] {
                let before = input.clone();
                let (actual, rules) = binding
                    .endpoint()
                    .encode_request_with_rules(&input)
                    .unwrap();
                let mut expected = wire.clone();
                expected["model"] = json!(binding.upstream);
                expected["stream"] = json!(stream || grouped);
                expected["input"][0] = json!({"role":"developer","content":"ordered"});
                assert_eq!(actual, expected, "{}", binding.identity);
                assert!(rules.is_empty());
                assert_eq!(input, before);
            }
        }
    }
}
fn response(grouped: bool, streamed: bool) -> DecodedResponse {
    let reasoning = ReasoningItem {
        status: ItemLifecycle::Completed,
        parts: vec![
            (PartId::new(1), ReasoningContent::Summary(text("Plan 🧪"))),
            (PartId::new(2), ReasoningContent::Text(text("Check"))),
        ],
        replay: Some(ReplayValue::final_value(
            ReplayFormat::ResponsesEncrypted,
            text("synthetic-final-token"),
        )),
    };
    let mut fidelity = FidelityRecords::default();
    fidelity
        .record_replay(
            ItemId::new(1),
            &reasoning,
            Some(ReplayOrigin::new("subscription-scope").unwrap()),
        )
        .unwrap();
    for (id, alias) in [(1, "reason"), (2, "call-item"), (3, "comment")] {
        fidelity
            .record_response_item_id(ItemId::new(id), alias)
            .unwrap();
    }
    let mut context = CallContext::default();
    if grouped {
        context.namespace = Some(text("ops"));
    }
    let semantic = GenerationResponse::new(
        vec![
            (ItemId::new(1), Item::Reasoning(reasoning)),
            (
                ItemId::new(2),
                Item::ToolCall(ToolCall {
                    call_id: text("c"),
                    name: text("lookup"),
                    arguments: ToolArguments::Raw("{ \"n\":9007199254740993 }".into()),
                    status: ItemLifecycle::Completed,
                    context,
                    cache: None,
                    replay: None,
                }),
            ),
            (
                ItemId::new(3),
                Item::Message(Message {
                    role: MessageRole::Assistant,
                    status: ItemLifecycle::Completed,
                    phase: Some(Phase::Commentary),
                    parts: vec![Part::new(
                        // Stream arguments own a part ID; static arguments do not.
                        PartId::new(if streamed { 4 } else { 3 }),
                        ContentPart::Text(
                            TextContent::from(text("checking"))
                                .with_logprobs(Presence::Value(vec![]))
                                .unwrap(),
                        ),
                    )],
                }),
            ),
        ],
        Outcome::Completed,
    )
    .unwrap()
    .with_usage(Some({
        let mut usage = Usage::operation(3, 5, 8);
        usage.cached_input_tokens = Some(1);
        usage.input_cache_write_tokens = Some(0);
        usage.reasoning_tokens = Some(2);
        usage
    }))
    .unwrap();
    let mut metadata = crate::events_support::metadata();
    metadata.id = Some("subscription".into());
    metadata.model = "fixture-model".into();
    metadata.created = Some(1.into());
    DecodedResponse {
        semantic,
        fidelity,
        metadata,
    }
}
#[test]
fn native_static_directions_preserve_reasoning_phase_calls_and_usage() {
    for (dialect, grouped) in [(Dialect::Grok, false), (Dialect::Siwc, true)] {
        let adapter = adapter(dialect);
        let wire = crate::wire::subscription_response(grouped);
        let decoded = adapter
            .decode_response(wire.to_string().as_bytes())
            .unwrap();
        let independent = response(grouped, false);
        assert_eq!(decoded.semantic, independent.semantic);
        assert!(matches!(&decoded.semantic.items()[1].1,Item::ToolCall(call)
                if call.arguments.as_raw()==Some("{ \"n\":9007199254740993 }")
                    && call.context.namespace.as_ref().map(|n|n.as_str())==grouped.then_some("ops")));
        assert_eq!(
            decoded.fidelity.replay_origin(ItemId::new(1)),
            adapter.adaptation.scope.as_ref()
        );
        let contract = TargetCaps {
            replay_origin: adapter.adaptation.scope.clone(),
            ..TargetCaps::full()
        };
        assert_eq!(
            adapter.encode_response(&independent, &contract).unwrap(),
            wire
        );
        let mut decoder = adapter.event_decoder();
        for event in crate::wire::response_events(wire.clone()) {
            decoder.push(&event).unwrap();
        }
        decoder.finish().unwrap();
        let streamed = decoder.materialize().unwrap();
        assert_eq!(streamed.semantic, response(grouped, true).semantic);
        assert_eq!(adapter.encode_response(&streamed, &contract).unwrap(), wire);
    }
}
#[test]
fn independent_semantic_events_encode_native_subscription_sse() {
    use crate::events_support::{close, part, start, terminal};
    use morphiecore::protocol::openai::events::EventEncoder;
    let mut events = vec![
        StreamEvent::Started,
        start(
            1,
            ItemKind::Message {
                phase: Some(Phase::FinalAnswer),
            },
        ),
    ];
    events.extend(part(1, 1, PartKind::Text, "done 🧪"));
    events.extend([
        close(1, ItemLifecycle::Completed),
        StreamEvent::Usage(Usage::operation(3, 2, 5)),
        terminal(StreamTerminal::Completed),
    ]);
    for dialect in [Dialect::Grok, Dialect::Siwc] {
        let independent = response(dialect == Dialect::Siwc, false);
        let contract = adapter(dialect).contract(&TargetCaps::full());
        let mut encoder = EventEncoder::new(Profile::Responses, independent.metadata)
            .unwrap()
            .with_contract(contract);
        let values = events
            .iter()
            .flat_map(|event| encoder.encode(event, &FidelityRecords::default()).unwrap())
            .collect::<Vec<_>>();
        encoder.finish().unwrap();
        let expected = json!({"id":"item_1","type":"message","role":"assistant","phase":"final_answer",
                "status":"completed","content":[{"type":"output_text","text":"done 🧪","annotations":[]}]});
        let delta = values
            .iter()
            .find(|e| e["type"] == "response.output_text.delta")
            .unwrap();
        assert_eq!(delta["delta"], "done 🧪");
        assert_eq!(delta["item_id"], "item_1");
        let terminal = &values.last().unwrap()["response"];
        assert_eq!(terminal["output"], json!([expected]));
        assert_eq!(terminal["status"], "completed");
        assert_eq!(
            terminal["usage"],
            json!({"input_tokens":3,"output_tokens":2,"total_tokens":5})
        );
    }
}
#[test]
fn subscription_identity_omission_is_named_and_retention_stays_closed() {
    for binding in SUBSCRIPTION_BINDINGS {
        let mut input = request(binding.profile == "openai", false);
        input.context.identity.user = Presence::Value("opaque-user".into());
        let before = input.clone();
        let (wire, rules) = binding
            .endpoint()
            .encode_request_with_rules(&input)
            .unwrap();
        assert!(wire.get("user").is_none());
        assert_eq!(rules, ["generation.omit-identity-hints.v1"]);
        assert_eq!(input, before);
        input.context.cache.prompt_cache_retention =
            Presence::Value(morphiecore::semantic::context::CacheRetention::Day);
        assert!(
            binding
                .endpoint()
                .encode_request_with_rules(&input)
                .is_err()
        );
    }
}
#[test]
fn billing_plans_share_one_dialect_but_keep_authority_and_replay_isolated() {
    use morphiecore::topology::ProtocolProfile;
    let dash = API_KEY_BINDINGS
        .iter()
        .find(|b| b.endpoint_prefix == "dashscope-flash")
        .unwrap();
    let plan = API_KEY_BINDINGS
        .iter()
        .find(|b| b.endpoint_prefix == "aliyun-tokenplan-cn")
        .unwrap();
    for binding in API_KEY_BINDINGS.iter().filter(|b| {
        matches!(
            (b.provider)().id.as_str(),
            "aliyun-dashscope-cn" | "aliyun-tokenplan-cn"
        )
    }) {
        assert_eq!(binding.dialect, Dialect::Bailian);
    }
    for profile in [
        ProtocolProfile::OpenAiChat,
        ProtocolProfile::OpenAiResponses,
    ] {
        let a = dash.endpoint(profile).unwrap();
        let b = plan.endpoint(profile).unwrap();
        let (
            morphiecore::adapter::UpstreamAdapter::OpenAi(left),
            morphiecore::adapter::UpstreamAdapter::OpenAi(right),
        ) = (a.adapter().unwrap(), b.adapter().unwrap())
        else {
            panic!("OpenAI wire")
        };
        assert_eq!(left.adaptation.rules, right.adaptation.rules);
        assert_eq!(left.adaptation.profile_id, right.adaptation.profile_id);
        assert_ne!(a.target.origin, b.target.origin);
        assert_ne!(a.credential, b.credential);
        assert_ne!(a.replay_origin(), b.replay_origin());
        if profile == ProtocolProfile::OpenAiChat {
            let client = Adapter::new(Profile::Chat, Dialect::MorphieCore, None);
            let input=client.decode_client_request(br#"{"model":"m","max_completion_tokens":64,"preserve_thinking":true,"messages":[{"role":"user","content":"q"}]}"#).unwrap();
            let (da, dr) = a.encode_request_with_rules(&input).unwrap();
            let (pa, pr) = b.encode_request_with_rules(&input).unwrap();
            assert_eq!(da, pa);
            assert_eq!(pa["preserve_thinking"], true);
            assert_eq!(pa["max_completion_tokens"], 64);
            assert!(pa.get("max_tokens").is_none());
            assert!(dr.is_empty() && pr.is_empty());
        }
    }
}
