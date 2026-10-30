//! Schema admission: the IR keeps any bounded JSON-object schema; target subsets are
//! checked at their protocol boundary.
mod schema_admission {
    use morphiecore::{
        protocol::{
            ConversionCause,
            anthropic::messages::client::decode_client_request as decode_messages,
            conversion::generation::RepresentationError,
            openai::{
                Dialect, GenerationApi, GenerationCodec, TargetContract, lowering::lower_request,
                responses,
            },
        },
        semantic::task::generation::*,
    };
    use serde_json::{Value, json};

    #[test]
    fn messages_conditional_tools_preserve_source_and_native_strict_guarantees() {
        use morphiecore::protocol::{
            anthropic::messages::{self, Carriers, Profile},
            native::ReplayTarget,
        };
        let source = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
        let profile = Profile::AdaptiveTextTools;
        let mut target = messages::TargetContract::new(profile, messages::Dialect::Standard);
        target.semantics = profile.representation_contract();
        target.carriers = Carriers::all();
        let replay = ReplayTarget::new("synthetic", "m").unwrap();
        let loss = "openai.omit-conditional-tool-strictness.v1";
        for (property, closed, strict, rules) in [
            (json!({"type":"string"}), true, true, vec![]),
            (json!({"type":"string"}), false, false, vec![loss]),
            (
                json!({"type":"string","enum":["A","B"]}),
                true,
                false,
                vec![loss],
            ),
            (json!({"type":"integer","enum":[1,2]}), true, true, vec![]),
            (
                json!({"oneOf":[{"type":"string"},{"type":"integer"}]}),
                true,
                false,
                vec![],
            ),
        ] {
            let mut schema = json!({"type":"object","properties":{"x":property}});
            if closed {
                schema["required"] = json!(["x"]);
                schema["additionalProperties"] = json!(false);
            }
            let body = json!({"model":"m","max_output_tokens":64,"input":"go","tools":[
                {"type":"function","name":"lookup","parameters":schema}]});
            let request = source
                .decode_client_request(body.to_string().as_bytes())
                .unwrap();
            for inline in [false, true] {
                let mut request = request.clone();
                if inline {
                    let ToolDefinition::Function(tool) = request.task.semantic.tools()[0].clone()
                    else {
                        panic!("function")
                    };
                    let mut settings = request.task.semantic.settings().clone();
                    settings.tools = None;
                    let mut items = request.task.semantic.items().to_vec();
                    items.push((
                        ItemId::new(2),
                        Item::ToolChange(ToolChange {
                            action: ToolChangeAction::AddDefinition(Box::new(tool)),
                            cache: None,
                        }),
                    ));
                    request.task.semantic = request
                        .task
                        .semantic
                        .with_settings(settings)
                        .unwrap()
                        .with_items(items)
                        .unwrap();
                }
                let original = request.clone();
                let projection = target.encode_request(&request, &replay).unwrap();
                let tool = if inline {
                    &projection.value["messages"][1]["content"][0]["tool"]["definition"]
                } else {
                    &projection.value["tools"][0]
                };
                assert_eq!(tool["strict"], strict, "{schema}, inline={inline}");
                assert_eq!(tool["input_schema"], schema);
                assert_eq!(projection.evidence.rule_names(), rules);
                assert_eq!(request, original);
                if strict {
                    let mut without_strict = target.clone();
                    without_strict.semantics.strict_tools = false;
                    let projection = without_strict.encode_request(&request, &replay).unwrap();
                    let tool = if inline {
                        &projection.value["messages"][1]["content"][0]["tool"]["definition"]
                    } else {
                        &projection.value["tools"][0]
                    };
                    assert_eq!(tool["strict"], false);
                    assert_eq!(tool["input_schema"], schema);
                    assert_eq!(projection.evidence.rule_names(), [loss]);
                }
            }
        }
        let schema = json!({"type":"object","properties":{},"additionalProperties":false});
        for n in [20, 21] {
            let tools: Vec<_> = (0..n)
                .map(|i| {
                    json!({
                        "type":"function","name":format!("t{i}"),"parameters":schema
                    })
                })
                .collect();
            let request = source
                .decode_client_request(
                    json!({
                        "model":"m","max_output_tokens":64,"input":"go","tools":tools
                    })
                    .to_string()
                    .as_bytes(),
                )
                .unwrap();
            let original = request.clone();
            let result = target.encode_request(&request, &replay);
            if n == 20 {
                let projection = result.unwrap();
                assert!(
                    projection.value["tools"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|t| t["strict"] == true)
                );
                assert!(projection.evidence.is_empty());
            } else {
                assert!(matches!(
                    result.unwrap_err().cause,
                    ConversionCause::Codec(morphiecore::protocol::CodecError::Unsupported(
                        morphiecore::protocol::failure::CarrierReason::Schema
                    ))
                ));
            }
            assert_eq!(request, original);
        }
        // The output schema reserves the same native union budget as tools.
        let names: Vec<_> = (0..16).map(|n| format!("u{n}")).collect();
        let properties: serde_json::Map<_, _> = names
            .iter()
            .map(|name| (name.clone(), json!({"type":["string","null"]})))
            .collect();
        let request = source
            .decode_client_request(
                json!({
                    "model":"m","max_output_tokens":64,"input":"go",
                    "text":{"format":{"type":"json_schema","name":"answer","strict":true,
                        "schema":{"type":"object","properties":properties,
                            "required":names,"additionalProperties":false}}},
                    "tools":[{"type":"function","name":"lookup","parameters":{
                        "type":"object","properties":{"x":{"type":["string","null"]}},
                        "required":["x"],"additionalProperties":false}}]
                })
                .to_string()
                .as_bytes(),
            )
            .unwrap();
        assert!(matches!(
            target.encode_request(&request, &replay).unwrap_err().cause,
            ConversionCause::Codec(morphiecore::protocol::CodecError::Unsupported(
                morphiecore::protocol::failure::CarrierReason::Schema
            ))
        ));
        for (schema, explicit) in [
            (json!({"type":"object","x-unknown":true}), false),
            (
                json!({"type":"object","properties":{"x":{"type":"string","enum":["A"]}},"required":["x"],"additionalProperties":false}),
                true,
            ),
        ] {
            let expected = if explicit {
                ConversionCause::Codec(morphiecore::protocol::CodecError::Unsupported(
                    morphiecore::protocol::failure::CarrierReason::Schema,
                ))
            } else {
                ConversionCause::Representation(RepresentationError::StrictDefault)
            };
            let mut body = json!({"model":"m","max_output_tokens":64,"input":"go","tools":[
                {"type":"function","name":"lookup","parameters":schema}]});
            if explicit {
                body["tools"][0]["strict"] = json!(true);
            }
            let request = source
                .decode_client_request(body.to_string().as_bytes())
                .unwrap();
            assert_eq!(
                target.encode_request(&request, &replay).unwrap_err().cause,
                expected
            );
        }
    }

    #[test]
    fn conditional_tools_resolve_without_rewriting_schemas_or_source() {
        let source = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
        let target = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None);
        let loss = "openai.omit-conditional-tool-strictness.v1";
        for (schema, strict, rules) in [
            (
                json!({"type":"object","properties":{"x":{"type":"string"}}}),
                false,
                vec![loss],
            ),
            (
                json!({"type":"object","properties":{"x":{"type":"string"}},"required":["x"],"additionalProperties":false}),
                true,
                vec![],
            ),
            (
                json!({"type":"object","properties":{"x":{"oneOf":[{"type":"string"},{"type":"integer"}]}}}),
                false,
                vec![],
            ),
            (
                json!({"type":"object","properties":{"mode":{"type":"string","enum":["a","b"]}},"required":["mode"],"additionalProperties":false}),
                true,
                vec![],
            ),
            (
                json!({"type":"object","properties":{"mode":{"type":"string","const":"a"}}}),
                false,
                vec![loss],
            ),
            (
                json!({"type":"object","properties":{"xs":{"type":"array","items":{"type":"string"},"uniqueItems":true}}}),
                false,
                vec![],
            ),
        ] {
            let body = json!({"model":"m","input":"go","tools":[
                {"type":"function","name":"lookup","description":"lookup","parameters":schema}]});
            let request = source
                .decode_client_request(body.to_string().as_bytes())
                .unwrap();
            let original = request.clone();
            let (wire, applied) = target
                .encode_request_with_rules(&request, "m", &TargetContract::full())
                .unwrap();
            assert_eq!(wire["tools"][0]["function"]["strict"], strict);
            assert_eq!(wire["tools"][0]["function"]["parameters"], schema);
            assert_eq!(applied, rules);
            let native = source
                .encode_request(&request, "m", &TargetContract::full())
                .unwrap();
            assert!(native["tools"][0].get("strict").is_none());
            assert_eq!(native["tools"][0]["parameters"], schema);
            if strict {
                let mut non_strict = TargetContract::full();
                non_strict.semantics.strict_tools = false;
                let (wire, applied) = target
                    .encode_request_with_rules(&request, "m", &non_strict)
                    .unwrap();
                assert_eq!(wire["tools"][0]["function"]["strict"], false);
                assert_eq!(applied, [loss]);
            }
            assert_eq!(request, original);
        }
        for schema in [
            json!({"type":"object","x-unknown":true}),
            json!({"type":"object","required":"x"}),
            json!({"type":"object","properties":{"x":{"type":"string","minLength":"bad"}}}),
            json!({"type":"object","properties":{"x":{"$ref":"#/$defs/X"}},"$defs":{"X":{"type":"string"}}}),
            json!({"type":"object","properties":{"x":{"type":"string","enum":["a","a"]}}}),
            json!({"type":"object","properties":{"x":{"type":["string","integer"]}}}),
            json!({"type":"object","properties":{"x":{"type":"array","items":{"type":"string"},"uniqueItems":"yes"}}}),
            json!({"type":"object","properties":{"x":{"type":"array","items":{"type":"string"},"uniqueItems":true,"x-unknown":true}}}),
        ] {
            let body = json!({"model":"m","input":"go","tools":[
                {"type":"function","name":"lookup","parameters":schema}]});
            let request = source
                .decode_client_request(body.to_string().as_bytes())
                .unwrap();
            assert_eq!(
                target
                    .encode_request(&request, "m", &TargetContract::full())
                    .unwrap_err()
                    .cause,
                ConversionCause::Representation(RepresentationError::StrictDefault)
            );
            assert!(
                source
                    .encode_request(&request, "m", &TargetContract::full())
                    .is_ok()
            );
        }
        let body = json!({"model":"m","input":"go","tools":[{"type":"function","name":"lookup"}]});
        let request = source
            .decode_client_request(body.to_string().as_bytes())
            .unwrap();
        assert_eq!(
            target
                .encode_request(&request, "m", &TargetContract::full())
                .unwrap_err()
                .cause,
            ConversionCause::Representation(RepresentationError::StrictDefault)
        );
    }

    /// An MCP server schema in draft-07 form, as opencode forwards it unchanged.
    fn mcp_schema() -> Value {
        json!({"$schema":"http://json-schema.org/draft-07/schema#","$id":"urn:synthetic:lookup",
            "type":"object","properties":{"key":{"$ref":"#/definitions/Key"},"mode":{"type":["string","number"]}},
            "definitions":{"Key":{"type":"string","x-label":"synthetic","deprecated":true}},
            "required":["key"],"additionalProperties":false})
    }

    #[test]
    fn every_client_protocol_admits_an_mcp_tool_schema_unchanged() {
        let schema = mcp_schema();
        let chat = json!({"model":"m","messages":[{"role":"user","content":"go"}],
            "tools":[{"type":"function","function":{"name":"lookup","parameters":schema}}]});
        let responses = json!({"model":"m","input":"go",
            "tools":[{"type":"function","name":"lookup","parameters":schema,"strict":false}]});
        let parameters = |request: &GenerationRequest| match &request.tools()[0] {
            ToolDefinition::Function(tool) => tool.parameters.clone().unwrap(),
            _ => panic!(),
        };
        for (profile, body) in [
            (GenerationApi::Chat, chat),
            (GenerationApi::Responses, responses),
        ] {
            let request = GenerationCodec::new(profile, Dialect::Standard, None)
                .decode_client_request(body.to_string().as_bytes())
                .unwrap();
            assert_eq!(
                parameters(&request.task.semantic).root().to_string(),
                schema.to_string()
            );
        }
        let messages = json!({"model":"m","max_tokens":8,"messages":[{"role":"user","content":"go"}],
            "tools":[{"name":"lookup","input_schema":schema}]});
        let request = decode_messages(messages.to_string().as_bytes()).unwrap();
        assert_eq!(
            parameters(&request.task.semantic).root().to_string(),
            schema.to_string()
        );
    }

    #[test]
    fn schemas_must_be_bounded_json_objects() {
        let admit = |schema: Value| {
            responses::decode_generation(
                &json!({"input":"go","tools":[{"type":"function","name":"lookup","parameters":schema,"strict":false}]}),
            )
        };
        for bad in [json!(true), json!([]), json!("object")] {
            assert!(admit(bad.clone()).is_err(), "{bad}");
        }
        for (wrappers, accepted) in [(63, true), (64, false)] {
            let mut deep = json!({});
            for _ in 0..wrappers {
                deep = json!({"default":deep});
            }
            assert_eq!(admit(deep).is_ok(), accepted, "raw depth");
        }
        for (values, accepted) in [(65_533, true), (65_534, false)] {
            assert_eq!(
                admit(json!({"default":vec![0;values]})).is_ok(),
                accepted,
                "raw nodes"
            );
        }
        assert!(admit(json!({"default":"x".repeat(MAX_TEXT_BYTES)})).is_err());
    }

    fn decode(schema: Value, strict: bool) -> morphiecore::protocol::openai::DecodedRequest {
        responses::decode_generation(
            &json!({"input":"hello","text":{"format":{"type":"json_schema","name":"answer","schema":schema,"strict":strict}}}),
        )
        .unwrap()
    }
    /// Strict schemas are admitted to the IR; only an OpenAI target checks its subset.
    fn lowers(schema: Value, strict: bool) -> bool {
        let d = decode(schema, strict);
        let responses = lower_request(
            &d.semantic,
            &d.fidelity,
            GenerationApi::Responses,
            TargetContract::full(),
        );
        let chat = lower_request(
            &d.semantic,
            &d.fidelity,
            GenerationApi::Chat,
            TargetContract::full(),
        );
        assert_eq!(responses.is_ok(), chat.is_ok());
        responses.is_ok()
    }
    fn closed_property(value: Value) -> Value {
        json!({"type":"object","properties":{"value":value},"required":["value"],"additionalProperties":false})
    }

    #[test]
    fn openai_targets_reject_strict_schemas_outside_their_subset() {
        for bad in [
            json!({"type":"string"}),
            json!({"anyOf":[{"type":"object"}]}),
            json!({"type":"object","properties":{"x":{"type":"string"}},"additionalProperties":false}),
            json!({"type":"object","properties":{},"required":["missing"],"additionalProperties":false}),
            json!({"type":"object","additionalProperties":true}),
            closed_property(json!({"type":"object"})),
            closed_property(json!({"type":"array"})),
            closed_property(json!({"allOf":[{"type":"string"}]})),
            closed_property(json!({"type":"string","not":{"const":"x"}})),
            closed_property(json!({"type":["string","integer"]})),
            closed_property(json!({"$ref":"#/$defs/missing"})),
            closed_property(json!({"$ref":"https://example.test/schema"})),
            json!({"$ref":"#"}),
        ] {
            assert!(!lowers(bad.clone(), true), "{bad}");
            assert!(lowers(bad.clone(), false), "{bad}");
        }
        let valid = closed_property(
            json!({"anyOf":[{"type":["string","null"],"minLength":1},{"type":"array","items":{"type":"integer","minimum":0}}]}),
        );
        assert!(lowers(valid, true));
        // Omission delegates normalization and best-effort fallback to Responses.
        let tool = |strict: Option<bool>| {
            let mut tool = json!({"type":"function","name":"lookup","parameters":
                {"type":"object","properties":{"x":{"oneOf":[{"type":"string"}]}}}});
            if let Some(strict) = strict {
                tool["strict"] = json!(strict);
            }
            let d = responses::decode_generation(&json!({"input":"go","tools":[tool]})).unwrap();
            lower_request(
                &d.semantic,
                &d.fidelity,
                GenerationApi::Responses,
                TargetContract::full(),
            )
            .is_ok()
        };
        assert!(tool(None));
        assert!(!tool(Some(true)));
        assert!(tool(Some(false)));

        let normalized = responses::decode_generation(
            &json!({"input":"go","tools":[{"type":"function","name":"lookup","parameters":{"type":"object"}}]}),
        )
        .unwrap();
        let mut native = morphiecore::protocol::google::generate_content::semantic_contract();
        native.strict_tools = true;
        assert_eq!(
            native.check(&normalized.semantic),
            Err(GenerationFeature::ConditionalToolStrictness)
        );
        let requirements = GenerationRequirements::derive(&normalized.semantic);
        assert!(requirements.conditional_tool_defaults);
        assert!(!requirements.strict_function_tools);
        let mut contract = GenerationFeatureSet::full();
        contract.strict_tools = false;
        assert!(contract.check(&normalized.semantic).is_ok());
        let mut without_normalization = contract.clone();
        without_normalization.conditional_tool_defaults = false;
        assert!(!without_normalization.supports(&contract));
        assert!(contract.supports(&without_normalization));
        let mut settings = normalized.semantic.settings().clone();
        let ToolDefinition::Function(tool) = &mut settings.tools.as_mut().unwrap()[0] else {
            panic!("function tool")
        };
        tool.strict = FunctionStrictness::Explicit(false);
        let non_strict = normalized.semantic.with_settings(settings).unwrap();
        assert!(native.check(&non_strict).is_ok());
    }

    #[test]
    fn strict_reference_graphs_allow_recursion_and_keep_schema_order() {
        let recursive = json!({"type":"object","properties":{"next":{"anyOf":[{"$ref":"#"},{"type":"null"}]}},"required":["next"],"additionalProperties":false});
        assert!(lowers(recursive, true));
        let recursive_defs = json!({"$ref":"#/$defs/Node","$defs":{"Node":{"type":"object","properties":{"next":{"anyOf":[{"$ref":"#/$defs/Node"},{"type":"null"}]}},"required":["next"],"additionalProperties":false}}});
        assert!(lowers(recursive_defs, true));
        let mut value = closed_property(json!({"$ref":"#/$defs/a~1b~0%20c"}));
        value["$defs"] = json!({"a/b~ c":{"type":"string"}});
        value["properties"]["value"]["default"] = json!("sample");
        assert!(lowers(value.clone(), true));
        for target in ["#/properties", "#/default", "#/$defs/a~2b", "#/%xx"] {
            let mut bad = value.clone();
            bad["properties"]["value"]["$ref"] = json!(target);
            bad["default"] = json!({"type":"string"});
            assert!(!lowers(bad, true), "{target}");
        }
        let d = decode(value.clone(), true);
        let out = responses::encode_generation(
            &lower_request(
                &d.semantic,
                &d.fidelity,
                GenerationApi::Responses,
                TargetContract::full(),
            )
            .unwrap(),
        )
        .unwrap();
        // Value equality ignores object order; compare serialized text instead.
        assert_eq!(
            out["text"]["format"]["schema"].to_string(),
            value.to_string()
        );
    }

    #[test]
    fn strict_limits_count_physical_shapes() {
        for (count, accepted) in [(5000, true), (5001, false)] {
            let props: serde_json::Map<String, Value> = (0..count)
                .map(|i| (format!("p{i}"), json!({"type":"boolean"})))
                .collect();
            let required: Vec<_> = props.keys().cloned().collect();
            let value = json!({"type":"object","properties":props,"required":required,"additionalProperties":false});
            assert_eq!(lowers(value, true), accepted, "properties {count}");
        }
        for (depth, accepted) in [(10, true), (11, false)] {
            let mut value = json!({"type":"string"});
            for _ in 0..depth {
                value = closed_property(value);
            }
            assert_eq!(lowers(value, true), accepted, "depth {depth}");
        }
        for (count, accepted) in [(1000, true), (1001, false)] {
            let value =
                closed_property(json!({"type":"integer","enum":(0..count).collect::<Vec<_>>()}));
            assert_eq!(lowers(value, true), accepted, "enums {count}");
        }
        for (chars, accepted) in [(119_995, true), (119_996, false)] {
            let value = closed_property(json!({"type":"string","const":"x".repeat(chars)}));
            assert_eq!(lowers(value, true), accepted, "chars {chars}");
        }
    }
}
