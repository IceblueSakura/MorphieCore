//! Model restrictions apply after projection, not to protocol-wide carriers.
use morphiecore::{
    execution::{
        plan::{RejectionReason, representable},
        prepare,
    },
    protocol::{
        CodecError, ConversionFailure, GenerationRequestEnvelope as Request,
        model_limits::ModelLimits,
        openai::{Dialect, GenerationApi, GenerationCodec, TargetContract},
        reasoning::{
            ReasoningConversion, ReasoningEffortMapping, ReasoningRequestPolicy, SHARED_BUDGETS,
        },
    },
    provider::SecretMaterial,
    semantic::task::generation::*,
    topology::{Endpoint, catalog::GOOGLE_BINDINGS},
};
use serde_json::{Value, json};

fn source(value: Value) -> Request {
    GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None)
        .decode_request(value.to_string().as_bytes())
        .unwrap()
}

fn messages(value: Value) -> Request {
    morphiecore::adapter::ClientAdapter::Messages
        .decode_client_request(value.to_string().as_bytes())
        .unwrap()
}

#[test]
fn service_output_ceilings_bound_prepared_requests_without_filling_unknown_services() {
    use morphiecore::{
        semantic::context::ConversationContext,
        topology::{
            GenerationProtocol,
            catalog::{API_KEY_BINDINGS, MESSAGES_BINDINGS},
        },
    };
    let secret = SecretMaterial::new("synthetic").unwrap();
    for (prefix, protocol, key, maximum) in [
        (
            "deepseek",
            GenerationProtocol::OpenAiChat,
            "max_tokens",
            393216,
        ),
        (
            "deepseek",
            GenerationProtocol::OpenAiResponses,
            "max_output_tokens",
            393216,
        ),
        (
            "opencode-go",
            GenerationProtocol::OpenAiChat,
            "max_tokens",
            65536,
        ),
        (
            "opencode-go-step",
            GenerationProtocol::OpenAiChat,
            "max_tokens",
            65536,
        ),
        (
            "opencode-go-mimo-pro",
            GenerationProtocol::OpenAiChat,
            "max_tokens",
            131072,
        ),
        (
            "openrouter",
            GenerationProtocol::OpenAiResponses,
            "max_output_tokens",
            128000,
        ),
        (
            "openai-terra",
            GenerationProtocol::OpenAiResponses,
            "max_output_tokens",
            128000,
        ),
    ] {
        let binding = API_KEY_BINDINGS
            .iter()
            .find(|b| b.endpoint_prefix == prefix)
            .unwrap();
        let endpoint = binding.endpoint(protocol).unwrap();
        let mut request = source(json!({"model":"public","input":"go","max_output_tokens":500000}));
        request.conversation =
            Some(ConversationContext::independent_request("synthetic-output").unwrap());
        let before = request.clone();
        assert_eq!(representable(&endpoint, &request), Ok(()));
        let (_, rules) = endpoint.encode_request_with_rules(&request).unwrap();
        assert!(rules.contains(&"generation.clamp-output-to-model-maximum.v1"));
        let prepared = prepare(&endpoint, &(binding.provider)(), &secret, &request).unwrap();
        let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
        assert_eq!(wire[key], maximum, "{prefix}");
        assert_eq!(request, before);
    }
    // A native Xiaomi binding must not inherit the Go ceiling for the same model.
    let native = API_KEY_BINDINGS
        .iter()
        .find(|b| b.endpoint_prefix == "xiaomi")
        .unwrap()
        .endpoint(GenerationProtocol::OpenAiResponses)
        .unwrap();
    let request = source(json!({"model":"public","input":"go","max_output_tokens":500000}));
    assert_eq!(
        native.encode_request(&request).unwrap()["max_output_tokens"],
        500000
    );
    let deepseek = API_KEY_BINDINGS
        .iter()
        .find(|b| b.endpoint_prefix == "deepseek")
        .unwrap()
        .endpoint(GenerationProtocol::OpenAiResponses)
        .unwrap();
    for cap in [None, Some(64)] {
        let mut body = json!({"model":"public","input":"go"});
        if let Some(cap) = cap {
            body["max_output_tokens"] = json!(cap);
        }
        let (wire, rules) = deepseek.encode_request_with_rules(&source(body)).unwrap();
        assert_eq!(
            wire.get("max_output_tokens"),
            cap.map(|n| json!(n)).as_ref()
        );
        assert!(rules.is_empty());
    }
    let haiku = &MESSAGES_BINDINGS[0];
    let endpoint = haiku.endpoint();
    for (cap, expected) in [(Some(500000), 128000), (None, 32000)] {
        let mut body = json!({"model":"public","input":"go"});
        if let Some(cap) = cap {
            body["max_output_tokens"] = json!(cap);
        }
        let mut request = source(body);
        request.conversation =
            Some(ConversationContext::independent_request("synthetic-output").unwrap());
        let prepared = prepare(&endpoint, &haiku.provider(), &secret, &request).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&prepared.body).unwrap()["max_tokens"],
            expected
        );
    }
}

#[test]
fn google_response_effort_is_a_selected_direction_not_native_support() {
    use morphiecore::{execution::admit, topology::*};
    let binding = &GOOGLE_BINDINGS[0];
    let rule = "google.gemini-3.8-flash.response-effort-to-thinking.v1";
    for mut endpoint in [binding.endpoint(), binding.interactions_endpoint()] {
        assert!(
            !endpoint
                .representation
                .dispositions()
                .native(morphiecore::semantic::task::generation::Feature::ResponseEffort)
        );
        let mut public = binding.public_model();
        // RequireAll checks the selected input contract, not a fabricated native bit.
        public.contract = endpoint.representation.dispositions().clone();
        public.contract.set_native(
            morphiecore::semantic::task::generation::Feature::ResponseEffort,
            true,
        );
        let mut route = binding.route();
        route.endpoints = vec![endpoint.id.clone()];
        route.policy.candidates = CandidatePolicy::RequireAll;
        let compile_one = |endpoint: Endpoint| {
            compile(
                vec![binding.provider()],
                vec![endpoint],
                vec![route.clone()],
                vec![public.clone()],
                vec![binding.canonical_model()],
            )
        };
        assert!(compile_one(endpoint.clone()).is_ok());

        for (level, expected) in [
            ("low", EffortLevel::Low),
            ("medium", EffortLevel::Medium),
            ("high", EffortLevel::High),
        ] {
            let request = messages(json!({"model":binding.model,"max_tokens":2048,
                "thinking":{"type":"adaptive"},"output_config":{"effort":level},
                "messages":[{"role":"user","content":"go"}]}));
            assert_eq!(
                request.task.semantic.settings().effort.as_ref(),
                Some(&EffortControl {
                    scope: EffortScope::Response,
                    level: expected
                })
            );
            assert_eq!(
                request.task.semantic.reasoning().mode.value(),
                Some(&ReasoningMode::Adaptive)
            );
            let before = request.clone();
            assert!(admit(&public, &request).is_ok());
            assert_eq!(representable(&endpoint, &request), Ok(()));
            let (wire, rules) = endpoint.encode_request_with_rules(&request).unwrap();
            assert_eq!(rules.iter().filter(|r| **r == rule).count(), 1);
            let prepared = prepare(
                &endpoint,
                &binding.provider(),
                &SecretMaterial::new("synthetic").unwrap(),
                &request,
            )
            .unwrap();
            assert_eq!(
                serde_json::from_slice::<Value>(&prepared.body).unwrap(),
                wire
            );
            if endpoint.protocol == GenerationProtocol::GoogleGenerateContent {
                assert_eq!(
                    wire["generationConfig"],
                    json!({"maxOutputTokens":2048,
                    "thinkingConfig":{"thinkingLevel":level.to_uppercase()}})
                );
            } else {
                assert_eq!(
                    wire["generation_config"],
                    json!({"max_output_tokens":2048,"thinking_level":level})
                );
            }
            assert_eq!(request, before);
        }
        // A response-wide effort does not by itself insert thinking activation.
        let request = messages(json!({"model":binding.model,"max_tokens":2048,
            "output_config":{"effort":"low"},"messages":[{"role":"user","content":"go"}]}));
        assert!(request.task.semantic.reasoning().activation.is_none());
        let before = request.clone();
        assert!(endpoint.encode_request(&request).is_ok());
        assert_eq!(request, before);
        for request in [
            source(json!({"model":binding.model,"input":"go","reasoning":{"effort":"low"}})),
            GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
                .decode_request(
                    json!({"model":binding.model,"messages":[{"role":"user","content":"go"}],
                    "reasoning_effort":"low"})
                    .to_string()
                    .as_bytes(),
                )
                .unwrap(),
            source(json!({"model":binding.model,"input":"go"})),
        ] {
            let (wire, rules) = endpoint.encode_request_with_rules(&request).unwrap();
            assert!(
                !rules.contains(&rule),
                "already native or absent controls stay unchanged"
            );
            if request.task.semantic.settings().effort.is_none() {
                assert!(wire.pointer("/generationConfig/thinkingConfig").is_none());
                assert!(wire.pointer("/generation_config/thinking_level").is_none());
            }
        }
        for fabricated_native_support in [false, true] {
            let mut invalid = endpoint.clone();
            let (semantics, policy) = match &mut invalid.representation {
                morphiecore::lowering::target::TargetContract::GoogleGenerateContent(c) => {
                    (&mut c.dispositions, &mut c.reasoning_policy)
                }
                morphiecore::lowering::target::TargetContract::GoogleInteractions(c) => {
                    (&mut c.dispositions, &mut c.reasoning_policy)
                }
                _ => unreachable!(),
            };
            if fabricated_native_support {
                semantics.set_native(
                    morphiecore::semantic::task::generation::Feature::ResponseEffort,
                    true,
                );
            } else {
                policy.as_mut().unwrap().conversion = ReasoningConversion::Scope {
                    from: EffortScope::Response,
                    to: EffortScope::Reasoning,
                    levels: &[],
                };
            }
            assert_eq!(
                compile_one(invalid),
                Err(TopologyError::ContractUnsatisfiable)
            );
        }
        match &mut endpoint.representation {
            morphiecore::lowering::target::TargetContract::GoogleGenerateContent(c) => {
                c.reasoning_policy = None
            }
            morphiecore::lowering::target::TargetContract::GoogleInteractions(c) => {
                c.reasoning_policy = None
            }
            _ => unreachable!(),
        }
        let (wire, rules) = projected(&endpoint, &request);
        assert_eq!(rules, ["generation.project-effort.v1"]);
        if endpoint.protocol == GenerationProtocol::GoogleGenerateContent {
            assert_eq!(
                wire["generationConfig"],
                json!({"maxOutputTokens":2048,
                "thinkingConfig":{"thinkingLevel":"LOW"}})
            );
        } else {
            assert_eq!(
                wire["generation_config"],
                json!({"max_output_tokens":2048,"thinking_level":"low"})
            );
        }
        assert!(compile_one(endpoint).is_ok());
    }
}

#[test]
fn google_scope_mapping_approximates_levels_without_weakening_activation_or_history() {
    let binding = &GOOGLE_BINDINGS[0];
    for endpoint in [binding.endpoint(), binding.interactions_endpoint()] {
        let base = json!({"model":binding.model,"max_tokens":2048,
            "output_config":{"effort":"low"},"messages":[{"role":"user","content":"go"}]});
        for level in ["xhigh", "max"] {
            let mut body = base.clone();
            body["output_config"]["effort"] = json!(level);
            let request = messages(body);
            assert!(morphiecore::execution::admit(&binding.public_model(), &request).is_ok());
            let (wire, rules) = projected(&endpoint, &request);
            assert_eq!(
                rules,
                ["google.gemini-3.8-flash.response-effort-to-thinking.v1"]
            );
            if endpoint.protocol == morphiecore::topology::GenerationProtocol::GoogleGenerateContent
            {
                assert_eq!(
                    wire["generationConfig"],
                    json!({"maxOutputTokens":2048,
                    "thinkingConfig":{"thinkingLevel":"HIGH"}})
                );
            } else {
                assert_eq!(
                    wire["generation_config"],
                    json!({"max_output_tokens":2048,"thinking_level":"high"})
                );
            }
        }
        for thinking in [
            json!({"type":"disabled"}),
            json!({"type":"enabled","budget_tokens":1024}),
        ] {
            let mut body = base.clone();
            body["thinking"] = thinking;
            let request = messages(body);
            let before = request.clone();
            assert!(morphiecore::execution::admit(&binding.public_model(), &request).is_ok());
            if request.task.semantic.reasoning().activation.as_ref()
                == Some(&ReasoningActivation::Enabled)
            {
                let (wire, rules) = projected(&endpoint, &request);
                assert_eq!(
                    rules,
                    [
                        "generation.adaptive-preserve-explicit-effort.v1",
                        "google.gemini-3.8-flash.response-effort-to-thinking.v1"
                    ]
                );
                if endpoint.protocol
                    == morphiecore::topology::GenerationProtocol::GoogleGenerateContent
                {
                    assert_eq!(
                        wire["generationConfig"],
                        json!({"maxOutputTokens":2048,
                        "thinkingConfig":{"thinkingLevel":"LOW"}})
                    );
                } else {
                    assert_eq!(
                        wire["generation_config"],
                        json!({"max_output_tokens":2048,"thinking_level":"low"})
                    );
                }
                continue;
            }
            assert_eq!(
                representable(&endpoint, &request),
                Err(RejectionReason::Semantic(Feature::ReasoningActivation))
            );
            assert!(
                prepare(
                    &endpoint,
                    &binding.provider(),
                    &SecretMaterial::new("synthetic").unwrap(),
                    &request
                )
                .is_err()
            );
            assert_eq!(request, before);
        }
        let body = json!({"model":binding.model,"max_tokens":2048,
            "messages":[{"role":"system","content":"","output_config":{"effort":"low"}},
                {"role":"user","content":"go"}]});
        assert!(
            morphiecore::adapter::ClientAdapter::Messages
                .decode_client_request(body.to_string().as_bytes())
                .is_ok()
        );
        // Source-timed controls collapse only on the candidate copy.
        let decoded = morphiecore::protocol::anthropic::messages::Profile::AdaptiveTextTools
            .decode_generation_request(
                body.to_string().as_bytes(),
                LocalScope::ROOT,
                &morphiecore::protocol::native::ReplayTarget::new("synthetic", binding.model)
                    .unwrap(),
            )
            .unwrap();
        let request = Request::from_generation(decoded, binding.model).unwrap();
        assert!(
            request
                .task
                .semantic
                .items()
                .iter()
                .any(|(_, item)| matches!(item, Item::ConfigurationUpdate(_)))
        );
        assert!(morphiecore::execution::admit(&binding.public_model(), &request).is_ok());
        let (wire, rules) = projected(&endpoint, &request);
        assert!(rules.contains(&"generation.collapse-configuration.v1"));
        assert!(rules.contains(&"generation.configuration-prefix-change.v1"));
        if endpoint.protocol == morphiecore::topology::GenerationProtocol::GoogleGenerateContent {
            assert_eq!(
                wire["generationConfig"]["thinkingConfig"]["thinkingLevel"],
                "LOW"
            );
        } else {
            assert_eq!(wire["generation_config"]["thinking_level"], "low");
        }
    }
}

#[test]
fn google_ordered_effort_uses_source_activation_not_the_last_marker() {
    let binding = &GOOGLE_BINDINGS[0];
    let body = json!({"model":binding.model,"max_tokens":2048,
    "output_config":{"effort":"low"},
    "tools":[{"name":"read","input_schema":{"type":"object"}}],
    "messages":[
        {"role":"user","content":"read"},
        {"role":"system","content":"","output_config":{"effort":"medium"}},
        {"role":"assistant","content":[{"type":"tool_use","id":"call","name":"read","input":{}}]},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":"call","content":"result"}]},
        {"role":"system","content":"","output_config":{"effort":"high"}}
    ]});
    for next_input in [false, true] {
        let mut body = body.clone();
        if next_input {
            body["messages"].as_array_mut().unwrap().extend([
                json!({"role":"assistant","content":"done"}),
                json!({"role":"user","content":"next"}),
            ]);
        }
        let request = messages(body);
        for endpoint in [binding.endpoint(), binding.interactions_endpoint()] {
            let (wire, rules) = projected(&endpoint, &request);
            assert!(rules.contains(&"generation.collapse-configuration.v1"));
            assert!(rules.contains(&"generation.configuration-prefix-change.v1"));
            if endpoint.protocol == morphiecore::topology::GenerationProtocol::GoogleGenerateContent
            {
                assert_eq!(
                    wire["generationConfig"]["thinkingConfig"]["thinkingLevel"],
                    if next_input { "HIGH" } else { "MEDIUM" }
                );
                assert_eq!(
                    wire["contents"].as_array().unwrap().len(),
                    if next_input { 5 } else { 3 }
                );
            } else {
                assert_eq!(
                    wire["generation_config"]["thinking_level"],
                    if next_input { "high" } else { "medium" }
                );
                assert_eq!(
                    wire["input"].as_array().unwrap().len(),
                    if next_input { 5 } else { 3 }
                );
            }
        }
    }
}

#[test]
fn google_effective_tools_preserve_old_calls_and_explicit_guarantees() {
    let binding = &GOOGLE_BINDINGS[0];
    let old = json!({"type":"object","properties":{"old":{"type":"integer"}},
        "required":["old"],"additionalProperties":false});
    let current = json!({"type":"object","properties":{"new":{"type":"string"}},
        "required":["new"],"additionalProperties":false});
    let body = json!({"model":binding.model,"max_tokens":2048,
    "tools":[{"name":"f","input_schema":old}],
    "messages":[
        {"role":"user","content":"go"},
        {"role":"assistant","content":[{"type":"tool_use","id":"call","name":"f","input":{"old":1}}]},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":"call","content":"ok"}]},
        {"role":"assistant","content":"done"},
        {"role":"user","content":"again"},
        {"role":"system","content":[{"type":"tool_addition","tool":{"type":"tool_definition",
            "definition":{"name":"f","input_schema":current}}}]}
    ]});
    for endpoint in [binding.endpoint(), binding.interactions_endpoint()] {
        let (wire, rules) = projected(&endpoint, &messages(body.clone()));
        assert!(rules.contains(&"generation.collapse-tool-set.v1"));
        if endpoint.protocol == morphiecore::topology::GenerationProtocol::GoogleGenerateContent {
            assert_eq!(
                wire["tools"],
                json!([{"functionDeclarations":[{"name":"f","parametersJsonSchema":current}]}])
            );
            assert_eq!(
                wire["contents"][1]["parts"][0]["functionCall"]["args"],
                json!({"old":1})
            );
            assert_eq!(
                wire["contents"][2]["parts"][0]["functionResponse"]["name"],
                "f"
            );
        } else {
            assert_eq!(
                wire["tools"],
                json!([{"type":"function","name":"f","parameters":current}])
            );
            assert_eq!(wire["input"][1]["arguments"], json!({"old":1}));
            assert_eq!(wire["input"][2]["call_id"], "call");
        }
        let mut strict = body.clone();
        strict["messages"][5]["content"][0]["tool"]["definition"]["strict"] = json!(true);
        let (strict_wire, _) = projected(&endpoint, &messages(strict.clone()));
        if endpoint.protocol == morphiecore::topology::GenerationProtocol::GoogleGenerateContent {
            assert_eq!(
                strict_wire["toolConfig"]["functionCallingConfig"]["mode"],
                "VALIDATED"
            );
        } else {
            assert_eq!(strict_wire["generation_config"]["tool_choice"], "validated");
        }
        strict["messages"][5]["content"][0]["tool"]["definition"]["input_schema"]["properties"]["new"]
            ["minLength"] = json!(5);
        assert!(representable(&endpoint, &messages(strict)).is_err());
        let mut serial = body.clone();
        serial["tool_choice"] = json!({"type":"auto","disable_parallel_tool_use":true});
        assert!(representable(&endpoint, &messages(serial)).is_err());
    }

    let deferred = messages(json!({"model":binding.model,"max_tokens":2048,
        "tool_choice":{"type":"tool","name":"ls"},
        "tools":[{"name":"read","input_schema":{"type":"object"}},
            {"name":"ls","input_schema":{"type":"object"},"defer_loading":true}],
        "messages":[{"role":"user","content":"go"},
            {"role":"system","content":[
                {"type":"tool_removal","tool":{"type":"tool_reference","name":"read"}},
                {"type":"tool_addition","tool":{"type":"tool_reference","name":"ls"}}]}]}));
    for endpoint in [binding.endpoint(), binding.interactions_endpoint()] {
        let (wire, _) = projected(&endpoint, &deferred);
        if endpoint.protocol == morphiecore::topology::GenerationProtocol::GoogleGenerateContent {
            assert_eq!(
                wire["tools"],
                json!([{"functionDeclarations":[{"name":"ls","parametersJsonSchema":{"type":"object"}}]}])
            );
            assert_eq!(
                wire["toolConfig"]["functionCallingConfig"],
                json!({"mode":"ANY","allowedFunctionNames":["ls"]})
            );
        } else {
            assert_eq!(
                wire["tools"],
                json!([{"type":"function","name":"ls","parameters":{"type":"object"}}])
            );
            assert_eq!(
                wire["generation_config"]["tool_choice"],
                json!({"allowed_tools":{"mode":"any","tools":["ls"]}})
            );
        }
    }
}

fn projected(endpoint: &Endpoint, request: &Request) -> (Value, Vec<&'static str>) {
    let before = request.clone();
    let (wire, rules) = endpoint.encode_request_with_rules(request).unwrap();
    assert_eq!(representable(endpoint, request), Ok(()));
    let prepared = prepare(
        endpoint,
        &GOOGLE_BINDINGS[0].provider(),
        &SecretMaterial::new("synthetic").unwrap(),
        request,
    )
    .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&prepared.body).unwrap(),
        wire
    );
    assert_eq!(request, &before);
    (wire, rules)
}

#[test]
fn google_no_call_admission_agrees_with_preparation_without_relaxing_other_choices() {
    let parameters = json!({"type":"object","properties":{
        "label":{"type":"string","minLength":5}},"required":["label"],"additionalProperties":false});
    for endpoint in [
        GOOGLE_BINDINGS[0].endpoint(),
        GOOGLE_BINDINGS[0].interactions_endpoint(),
    ] {
        for strict in [Some(true), None] {
            let mut body = json!({"model":GOOGLE_BINDINGS[0].model,"input":"go",
                "tool_choice":"none","parallel_tool_calls":false,
                "tools":[{"type":"function","name":"lookup","parameters":parameters}]});
            if let Some(strict) = strict {
                body["tools"][0]["strict"] = json!(strict);
            }
            let request = source(body.clone());
            let before = request.clone();
            assert_eq!(representable(&endpoint, &request), Ok(()));
            let (wire, rules) = projected(&endpoint, &request);
            assert!(rules.is_empty());
            if endpoint.protocol == morphiecore::topology::GenerationProtocol::GoogleGenerateContent
            {
                assert_eq!(
                    wire["tools"][0]["functionDeclarations"][0]["parametersJsonSchema"],
                    parameters
                );
                assert_eq!(
                    wire["toolConfig"]["functionCallingConfig"],
                    json!({"mode":"NONE"})
                );
            } else {
                assert_eq!(wire["tools"][0]["parameters"], parameters);
                assert_eq!(wire["generation_config"]["tool_choice"], "none");
            }
            assert_eq!(request, before);
            for choice in ["auto", "required"] {
                body["tool_choice"] = json!(choice);
                // Isolate schema enforcement from serial enforcement.
                body["parallel_tool_calls"] = json!(true);
                body["tools"][0]["strict"] = json!(true);
                assert!(representable(&endpoint, &source(body.clone())).is_err());
                body["tools"][0]["parameters"] = json!({"type":"object",
                    "properties":{},"additionalProperties":false});
                body["parallel_tool_calls"] = json!(false);
                assert!(representable(&endpoint, &source(body.clone())).is_err());
                body["tools"][0]["parameters"] = parameters.clone();
            }
        }
    }
}

#[test]
fn google_conditional_strictness_accounts_for_the_entire_validated_inventory() {
    let closed = json!({"type":"object","properties":{"minLength":{"type":"integer","enum":[7]}},
        "required":["minLength"],"additionalProperties":false});
    let closed = json!({"$ref":"#/$defs/Args","$defs":{"Args":closed}});
    for endpoint in [
        GOOGLE_BINDINGS[0].endpoint(),
        GOOGLE_BINDINGS[0].interactions_endpoint(),
    ] {
        let body = json!({"model":GOOGLE_BINDINGS[0].model,"input":"go",
            "tools":[{"type":"function","name":"lookup","parameters":closed},
                {"type":"function","name":"other","strict":false,"parameters":{"type":"object"}}]});
        let (wire, rules) = projected(&endpoint, &source(body.clone()));
        assert!(rules.contains(&"openai.omit-conditional-tool-strictness.v1"));
        assert!(wire.get("toolConfig").is_none());
        assert!(wire["generation_config"].get("tool_choice").is_none());
        let mut explicit = body.clone();
        explicit["tools"][0]["strict"] = json!(true);
        assert!(representable(&endpoint, &source(explicit.clone())).is_err());
        explicit["tools"][1]["parameters"] = closed.clone();
        let (wire, _) = projected(&endpoint, &source(explicit));
        let mut conditional = body;
        conditional["tools"][1]["parameters"] = closed.clone();
        let (_, rules) = projected(&endpoint, &source(conditional));
        assert!(!rules.contains(&"openai.omit-conditional-tool-strictness.v1"));
        if endpoint.protocol == morphiecore::topology::GenerationProtocol::GoogleGenerateContent {
            assert_eq!(
                wire["tools"][0]["functionDeclarations"][0]["parametersJsonSchema"],
                closed
            );
            assert_eq!(
                wire["toolConfig"]["functionCallingConfig"]["mode"],
                "VALIDATED"
            );
        } else {
            assert_eq!(wire["tools"][0]["parameters"], closed);
            assert_eq!(wire["generation_config"]["tool_choice"], "validated");
        }
    }
}

#[test]
fn gemini_model_values_are_checked_by_both_preflight_and_preparation() {
    let binding = &GOOGLE_BINDINGS[0];
    for mut endpoint in [binding.endpoint(), binding.interactions_endpoint()] {
        let request = source(json!({"model":binding.model,"input":"go",
            "reasoning":{"effort":"high"},"max_output_tokens":65536}));
        let before = request.clone();
        assert_eq!(representable(&endpoint, &request), Ok(()));
        let prepared = prepare(
            &endpoint,
            &binding.provider(),
            &SecretMaterial::new("synthetic").unwrap(),
            &request,
        )
        .unwrap();
        let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
        if endpoint.protocol == morphiecore::topology::GenerationProtocol::GoogleGenerateContent {
            assert_eq!(
                wire["generationConfig"],
                json!({"maxOutputTokens":65536,"thinkingConfig":{"thinkingLevel":"HIGH"}})
            );
        } else {
            assert_eq!(
                wire["generation_config"],
                json!({"max_output_tokens":65536,"thinking_level":"high"})
            );
        }
        assert_eq!(request, before);
        let minimal = source(json!({"model":binding.model,"input":"go",
            "reasoning":{"effort":"minimal"},"max_output_tokens":4096}));
        let (wire, rules) = projected(&endpoint, &minimal);
        assert_eq!(rules, ["generation.project-effort.v1"]);
        if endpoint.protocol == morphiecore::topology::GenerationProtocol::GoogleGenerateContent {
            assert_eq!(
                wire["generationConfig"],
                json!({"maxOutputTokens":4096,
                "thinkingConfig":{"thinkingLevel":"LOW"}})
            );
        } else {
            assert_eq!(
                wire["generation_config"],
                json!({"max_output_tokens":4096,"thinking_level":"low"})
            );
        }
        let excessive =
            source(json!({"model":binding.model,"input":"go","max_output_tokens":65537}));
        let (wire, rules) = projected(&endpoint, &excessive);
        assert_eq!(rules, ["generation.clamp-output-to-model-maximum.v1"]);
        assert_eq!(
            wire[if endpoint.protocol
                == morphiecore::topology::GenerationProtocol::GoogleGenerateContent
            {
                "generationConfig"
            } else {
                "generation_config"
            }],
            if endpoint.protocol == morphiecore::topology::GenerationProtocol::GoogleGenerateContent
            {
                json!({"maxOutputTokens":65536})
            } else {
                json!({"max_output_tokens":65536})
            }
        );

        // Restrictions belong to this binding, not the generic Google carriers.
        *endpoint.representation.model_limits_mut() = ModelLimits::default();
        let wire = endpoint.encode_request(&minimal).unwrap();
        if endpoint.protocol == morphiecore::topology::GenerationProtocol::GoogleGenerateContent {
            assert_eq!(
                wire["generationConfig"]["thinkingConfig"]["thinkingLevel"],
                "MINIMAL"
            );
        } else {
            assert_eq!(wire["generation_config"]["thinking_level"], "minimal");
        }
        assert!(endpoint.encode_request(&excessive).is_ok());
    }
}

#[test]
fn gemini_preserves_native_budgets_and_checks_fallback_levels_without_requantizing() {
    let binding = &GOOGLE_BINDINGS[0];
    let plain = source(json!({"model":binding.model,"input":"go","max_output_tokens":4096}));
    let mut settings = plain.task.semantic.settings().clone();
    settings.reasoning = ReasoningRequest::present(None);
    settings.reasoning.budget_tokens = Some(1024);
    let mut request = plain.clone();
    request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
    let before = request.clone();
    for mut endpoint in [binding.endpoint(), binding.interactions_endpoint()] {
        let policy = Some(ReasoningRequestPolicy {
            canonical_model: binding.model.into(),
            rule: "synthetic.budget-to-effort.v1",
            conversion: ReasoningConversion::BudgetToEffort {
                scope: EffortScope::Reasoning,
                table: SHARED_BUDGETS.clone(),
            },
        });
        match &mut endpoint.representation {
            morphiecore::lowering::target::TargetContract::GoogleGenerateContent(c) => {
                c.reasoning_policy = policy
            }
            morphiecore::lowering::target::TargetContract::GoogleInteractions(c) => {
                c.reasoning_policy = policy
            }
            _ => unreachable!(),
        }
        let (wire, rules) = projected(&endpoint, &request);
        assert_eq!(
            rules,
            [
                "generation.project-effort.v1",
                "synthetic.budget-to-effort.v1"
            ]
        );
        if endpoint.protocol == morphiecore::topology::GenerationProtocol::GoogleGenerateContent {
            assert_eq!(
                wire["generationConfig"],
                json!({"maxOutputTokens":4096,
                "thinkingConfig":{"thinkingLevel":"LOW"}})
            );
        } else {
            assert_eq!(
                wire["generation_config"],
                json!({"max_output_tokens":4096,"thinking_level":"low"})
            );
        }
        // A wire field alone is not a declaration of native model support.
        if let morphiecore::lowering::target::TargetContract::GoogleGenerateContent(c) =
            &mut endpoint.representation
        {
            c.dispositions.set_native(
                morphiecore::semantic::task::generation::Feature::ReasoningBudget,
                true,
            );
        }
        let native_budget =
            endpoint.protocol == morphiecore::topology::GenerationProtocol::GoogleGenerateContent;
        if native_budget {
            let (wire, rules) = endpoint.encode_request_with_rules(&request).unwrap();
            assert_eq!(
                wire["generationConfig"]["thinkingConfig"]["thinkingBudget"],
                1024
            );
            assert!(!rules.contains(&"synthetic.budget-to-effort.v1"));
            assert_eq!(representable(&endpoint, &request), Ok(()));
        } else {
            let (wire, rules) = projected(&endpoint, &request);
            assert_eq!(
                wire["generation_config"],
                json!({"max_output_tokens":4096,"thinking_level":"low"})
            );
            assert_eq!(
                rules,
                [
                    "generation.project-effort.v1",
                    "synthetic.budget-to-effort.v1"
                ]
            );
        }
        let mut settings = request.task.semantic.settings().clone();
        settings.reasoning.budget_tokens = Some(2048);
        let mut low = request.clone();
        low.task.semantic = low.task.semantic.with_settings(settings).unwrap();
        let (wire, rules) = endpoint.encode_request_with_rules(&low).unwrap();
        assert_eq!(
            rules
                .iter()
                .filter(|r| **r == "synthetic.budget-to-effort.v1")
                .count(),
            usize::from(!native_budget)
        );
        if native_budget {
            assert_eq!(
                wire["generationConfig"]["thinkingConfig"]["thinkingBudget"],
                2048
            );
        } else {
            assert_eq!(wire["generation_config"]["thinking_level"], "low");
        }
        assert!(endpoint.encode_request(&plain).is_ok());
    }
    assert_eq!(request, before);
}

#[test]
fn openai_limits_check_calibrated_levels_and_do_not_insert_caps() {
    let request = source(json!({"model":"public","input":"go","reasoning":{"effort":"medium"}}));
    let before = request.clone();
    for profile in [GenerationApi::Chat, GenerationApi::Responses] {
        let mut adapter = GenerationCodec::new(profile, Dialect::Standard, None);
        adapter.adaptation.effort_mapping = Some(&ReasoningEffortMapping {
            canonical_model: "synthetic",
            rule: "synthetic.effort.v1",
            scope: EffortScope::Reasoning,
            native_levels: &[EffortLevel::High],
            encode: &[
                (EffortLevel::Medium, EffortLevel::High),
                (EffortLevel::High, EffortLevel::High),
            ],
            decode: &[(EffortLevel::High, EffortLevel::Medium)],
        });
        let mut caps = TargetContract::full();
        caps.model_limits = ModelLimits {
            allowed_effort: Some(vec![EffortLevel::High].into()),
            maximum_output_tokens: Some(32),
            ..Default::default()
        };
        let (wire, rules) = adapter
            .encode_request_with_rules(&request, "synthetic", &caps)
            .unwrap();
        assert_eq!(
            rules
                .iter()
                .filter(|r| **r == "synthetic.effort.v1")
                .count(),
            1
        );
        if profile == GenerationApi::Chat {
            assert_eq!(wire["reasoning_effort"], "high");
            assert!(wire.get("max_completion_tokens").is_none());
        } else {
            assert_eq!(wire["reasoning"]["effort"], "high");
            assert!(wire.get("max_output_tokens").is_none());
        }
        caps.model_limits.allowed_effort = Some(vec![EffortLevel::Medium].into());
        assert!(matches!(
            adapter.encode_request(&request, "synthetic", &caps),
            Err(ConversionFailure {
                cause: morphiecore::protocol::ConversionCause::Codec(CodecError::Invalid(
                    morphiecore::protocol::failure::ShapeReason::InvalidValue
                )),
                ..
            })
        ));
        caps.model_limits.allowed_effort = Some(vec![EffortLevel::High].into());
        let mut capped = request.clone();
        let mut settings = capped.task.semantic.settings().clone();
        settings.controls.max_output_tokens = Some(33);
        capped.task.semantic = capped.task.semantic.with_settings(settings).unwrap();
        let (wire, rules) = adapter
            .encode_request_with_rules(&capped, "synthetic", &caps)
            .unwrap();
        assert_eq!(
            wire[if profile == GenerationApi::Chat {
                "max_completion_tokens"
            } else {
                "max_output_tokens"
            }],
            32
        );
        assert_eq!(
            rules,
            [
                "generation.clamp-output-to-model-maximum.v1",
                "synthetic.effort.v1"
            ]
        );
        let mut settings = capped.task.semantic.settings().clone();
        settings.controls.max_output_tokens = Some(32);
        capped.task.semantic = capped.task.semantic.with_settings(settings).unwrap();
        let wire = adapter.encode_request(&capped, "synthetic", &caps).unwrap();
        assert_eq!(
            wire[if profile == GenerationApi::Chat {
                "max_completion_tokens"
            } else {
                "max_output_tokens"
            }],
            32
        );
    }
    assert_eq!(request, before);
}

#[test]
fn google_sampling_omission_is_binding_selected_and_preparation_matches_preflight() {
    use morphiecore::protocol::{
        google::{self, SamplingPolicy},
        native::ReplayTarget,
    };
    let binding = &GOOGLE_BINDINGS[0];
    let request = source(
        json!({"model":binding.model,"input":"go","max_output_tokens":64,
        "temperature":0.25,"top_p":0.8,"store":false}),
    );
    let original = request.clone();
    for mut endpoint in [binding.endpoint(), binding.interactions_endpoint()] {
        let interactions = matches!(
            endpoint.representation,
            morphiecore::lowering::target::TargetContract::GoogleInteractions(_)
        );
        let omission_rules = ["generation.omit-temperature.v1", "generation.omit-top-p.v1"];
        assert!(
            !endpoint
                .representation
                .dispositions()
                .native(morphiecore::semantic::task::generation::Feature::Temperature)
        );
        assert!(
            !endpoint
                .representation
                .dispositions()
                .native(morphiecore::semantic::task::generation::Feature::TopP)
        );
        representable(&endpoint, &request).unwrap();
        let (wire, rules) = endpoint.encode_request_with_rules(&request).unwrap();
        assert_eq!(rules, omission_rules);
        let config = &wire[if interactions {
            "generation_config"
        } else {
            "generationConfig"
        }];
        for field in ["temperature", "top_p", "topP", "top_k", "topK"] {
            assert!(config.get(field).is_none());
        }
        let prepared = prepare(
            &endpoint,
            &binding.provider(),
            &SecretMaterial::new("synthetic-key").unwrap(),
            &request,
        )
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&prepared.body).unwrap(),
            wire
        );
        let mut settings = request.task.semantic.settings().clone();
        settings.controls = settings.controls.without_sampling();
        let mut absent = request.clone();
        absent.task.semantic = request
            .task
            .semantic
            .clone()
            .with_settings(settings)
            .unwrap();
        let (_, rules) = endpoint.encode_request_with_rules(&absent).unwrap();
        assert!(rules.is_empty());
        if interactions {
            let morphiecore::lowering::target::TargetContract::GoogleInteractions(caps) =
                &mut endpoint.representation
            else {
                unreachable!()
            };
            caps.sampling = SamplingPolicy::Reject;
            let (projected_wire, rules) = endpoint.encode_request_with_rules(&request).unwrap();
            assert_eq!(projected_wire, wire);
            assert_eq!(rules, omission_rules);
            assert!(
                google::interactions::encode_generation_request(
                    &request.task.semantic,
                    &request.task.fidelity,
                    &ReplayTarget::new("synthetic", "model").unwrap()
                )
                .is_err()
            );
        }
    }
    let (native, rules) = google::generate_content::encode_generation_request(
        &request.task.semantic,
        &request.task.fidelity,
        &ReplayTarget::new("synthetic", "model").unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(native["generationConfig"]["temperature"], 0.25);
    assert_eq!(native["generationConfig"]["topP"], 0.8);
    assert!(!rules.contains(&google::generate_content::RequestRule::OmitSampling));
    for patch in [json!({"temperature":-1}), json!({"top_p":2})] {
        let mut value = json!({"model":binding.model,"input":"go"});
        value
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(
            GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None)
                .decode_request(value.to_string().as_bytes())
                .is_err()
        );
    }
    assert_eq!(request, original);
}
#[test]
fn hydrogen_xhigh_uses_both_native_carriers_without_reactivating_disabled_thinking() {
    let binding = GOOGLE_BINDINGS
        .iter()
        .find(|b| b.model == "gemini-4-hydrogen")
        .unwrap();
    let request = source(json!({"model":binding.model,"input":"go",
        "reasoning":{"effort":"xhigh"},"temperature":0.4,"top_p":0.8}));
    let before = request.clone();
    let generated = binding.endpoint().encode_request(&request).unwrap();
    assert_eq!(
        generated["generationConfig"]["thinkingConfig"]["thinkingLevel"],
        "XHIGH"
    );
    assert!(generated["generationConfig"].get("temperature").is_none());
    assert!(generated["generationConfig"].get("topP").is_none());
    let interaction = binding
        .interactions_endpoint()
        .encode_request(&request)
        .unwrap();
    assert_eq!(interaction["generation_config"]["thinking_level"], "xhigh");
    assert!(
        interaction["generation_config"]
            .get("temperature")
            .is_none()
    );
    assert!(interaction["generation_config"].get("top_p").is_none());
    assert_eq!(request, before);
    let off = source(json!({"model":binding.model,"input":"go","reasoning":{"effort":"none"}}));
    assert!(representable(&binding.endpoint(), &off).is_err());
    assert!(representable(&binding.interactions_endpoint(), &off).is_err());
}

#[test]
fn terra_sampling_is_available_only_with_explicit_thinking_off() {
    use morphiecore::topology::{GenerationProtocol, catalog::API_KEY_BINDINGS};
    let binding = API_KEY_BINDINGS
        .iter()
        .find(|b| b.endpoint_prefix == "openai-terra")
        .unwrap();
    let endpoint = binding
        .endpoint(GenerationProtocol::OpenAiResponses)
        .unwrap();
    for effort in ["none", "low"] {
        let request = source(json!({"model":binding.model,"input":"go",
            "reasoning":{"effort":effort},"temperature":0.4,"top_p":0.8}));
        let original = request.clone();
        let wire = endpoint.encode_request(&request).unwrap();
        assert_eq!(wire["reasoning"]["effort"], effort);
        if effort == "none" {
            assert_eq!(wire["temperature"], 0.4);
            assert_eq!(wire["top_p"], 0.8);
        } else {
            assert!(wire.get("temperature").is_none());
            assert!(wire.get("top_p").is_none());
        }
        assert_eq!(request, original);
    }
    for (root, update, effective_off) in [("none", "high", false), ("low", "none", true)] {
        let request = source(json!({"model":binding.model,"temperature":0.4,"top_p":0.8,
            "reasoning":{"effort":root},"input":[{"role":"user","content":"go"},
                {"type":"configuration_update","reasoning":{"effort":update}}]}));
        let original = request.clone();
        let wire = endpoint.encode_request(&request).unwrap();
        assert_eq!(wire.get("temperature").is_some(), effective_off);
        assert_eq!(wire.get("top_p").is_some(), effective_off);
        assert!(
            wire["input"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["type"] == "configuration_update")
        );
        assert_eq!(request, original);
    }
}
