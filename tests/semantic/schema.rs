//! Schema admission: the IR keeps any bounded JSON-object schema; target subsets are
//! checked at their protocol boundary.
mod schema_admission {
    use morphiecore::{
        protocol::{
            anthropic::messages::client::decode_client_request as decode_messages,
            openai::{
                Adapter, Dialect, Profile, TargetCaps,
                lowering::lower_request,
                responses::{self},
            },
        },
        semantic::task::generation::*,
    };
    use serde_json::{Value, json};

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
        for (profile, body) in [(Profile::Chat, chat), (Profile::Responses, responses)] {
            let request = Adapter::new(profile, Dialect::Standard, None)
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
            Profile::Responses,
            TargetCaps::full(),
        );
        let chat = lower_request(&d.semantic, &d.fidelity, Profile::Chat, TargetCaps::full());
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
                Profile::Responses,
                TargetCaps::full(),
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
            Err(GenerationFeature::ToolSchemaNormalization)
        );
        let requirements = GenerationRequirements::derive(&normalized.semantic);
        assert!(requirements.normalization_default_tools);
        assert!(!requirements.strict_function_tools);
        let mut contract = GenerationSemanticContract::full();
        contract.strict_tools = false;
        assert!(contract.check(&normalized.semantic).is_ok());
        let mut without_normalization = contract.clone();
        without_normalization.normalization_default_tools = false;
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
                Profile::Responses,
                TargetCaps::full(),
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
