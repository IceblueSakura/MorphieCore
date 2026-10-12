//! Schema order, dialects and supplied documents.
mod schema_order {
    //! Schema order is asserted independently of Value equality, which ignores object order.
    use crate::wire;
    use morphiecore::{
        protocol::openai::{
            Profile, TargetCaps as Contract,
            lowering::lower_request,
            responses::{self, envelope},
        },
        semantic::{
            task::generation::*,
            value::{Presence, Text},
        },
    };
    use serde_json::{Value, json};

    #[test]
    fn exact_numeric_schema_checks_never_round_values_or_expand_large_exponents() {
        for source in [
            r#"{"enum":[9007199254740993.0,9007199254740992]}"#,
            r#"{"multipleOf":1e-9999}"#,
            r#"{"minLength":1e9999,"maxLength":2e9999}"#,
        ] {
            assert!(
                admit(serde_json::from_str(source).unwrap(), false).is_ok(),
                "{source}"
            );
        }
        for source in [
            r#"{"enum":[123.4500,1.2345e2]}"#,
            r#"{"minLength":1.0000000000000000000001}"#,
            r#"{"minLength":2e9999,"maxLength":1e9999}"#,
            r#"{"multipleOf":-1e-9999}"#,
        ] {
            assert!(
                admit(serde_json::from_str(source).unwrap(), false).is_err(),
                "{source}"
            );
        }
    }

    fn admit(
        schema: Value,
        strict: bool,
    ) -> Result<
        morphiecore::protocol::openai::DecodedRequest,
        morphiecore::protocol::openai::CodecError,
    > {
        responses::decode_generation(
            &json!({"input":"hello","text":{"format":{"type":"json_schema","name":"answer","schema":schema,"strict":strict}}}),
        )
    }
    fn closed_property(value: Value) -> Value {
        json!({"type":"object","properties":{"value":value},"required":["value"],"additionalProperties":false})
    }

    #[test]
    fn schema_keywords_are_checked_before_ir_or_reported_settings_admission() {
        for bad in [
            json!({"type":"unknown"}),
            json!({"type":[]}),
            json!({"type":["string","string"]}),
            json!({"properties":[]}),
            json!({"properties":{"x":1}}),
            json!({"required":"x"}),
            json!({"required":["x","x"]}),
            json!({"items":[]}),
            json!({"anyOf":[]}),
            json!({"anyOf":[1]}),
            json!({"$defs":[]}),
            json!({"minimum":"0"}),
            json!({"minItems":-1}),
            json!({"minLength":0.5}),
            json!({"multipleOf":0}),
            json!({"minLength":5,"maxLength":2}),
            json!({"enum":[]}),
            json!({"enum":[1,1.0]}),
            json!({"title":1}),
            json!({"unknown_keyword":true}),
        ] {
            assert!(admit(bad.clone(), false).is_err(), "{bad}");
            let d = responses::decode_generation(&json!({"input":"hello"})).unwrap();
            let r = d.semantic.with_output(OutputConstraint::JsonSchema {
                name: Text::new("answer", "test", 64).unwrap(),
                description: None,
                schema: bad.clone().into(),
                strict: Some(false),
            });
            assert_eq!(r.unwrap_err(), GenerationError::InvalidSchema);
            let mut response = wire::response(2);
            response["text"]["format"]["schema"] = bad;
            response["text"]["format"]["strict"] = json!(false);
            assert!(envelope::decode_response(&response).is_err());
        }
        for schema in [
            json!({}),
            json!({"allOf":[{"type":"object"},{"properties":{"x":{"type":"integer"}}}]}),
            json!({"enum":[9007199254740992u64,9007199254740993u64]}),
            json!({"const":{"data":{"not_a_schema":1}}}),
        ] {
            assert!(admit(schema, false).is_ok());
        }
        let duplicate = json!({"enum":[{"a":1,"b":2},{"b":2,"a":1}]});
        assert!(admit(duplicate, false).is_err());
    }

    #[test]
    fn explicit_strict_rejects_open_incomplete_or_unsupported_shapes() {
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
            json!({"$ref":"#"}),
        ] {
            assert!(admit(bad.clone(), true).is_err(), "{bad}");
        }
        let valid = closed_property(
            json!({"anyOf":[{"type":["string","null"],"minLength":1},{"type":"array","items":{"type":"integer","minimum":0}}]}),
        );
        assert!(admit(valid, true).is_ok());
    }

    #[test]
    fn local_reference_graphs_allow_recursion_but_not_dangling_or_non_schema_targets() {
        let recursive = json!({"type":"object","properties":{"next":{"anyOf":[{"$ref":"#"},{"type":"null"}]}},"required":["next"],"additionalProperties":false});
        assert!(admit(recursive, true).is_ok());
        let recursive_defs = json!({"$ref":"#/$defs/Node","$defs":{"Node":{"type":"object","properties":{"next":{"anyOf":[{"$ref":"#/$defs/Node"},{"type":"null"}]}},"required":["next"],"additionalProperties":false}}});
        assert!(admit(recursive_defs, true).is_ok());
        let mut value = closed_property(json!({"$ref":"#/$defs/a~1b~0%20c"}));
        value["$defs"] = json!({"a/b~ c":{"type":"string"}});
        value["properties"]["value"]["default"] = json!("sample");
        value["properties"]["value"]["examples"] = json!(["sample"]);
        assert!(admit(value.clone(), true).is_ok());
        for target in [
            "https://example.test/schema",
            "other.json#/x",
            "#/$defs/missing",
            "#/properties",
            "#/default",
            "#/$defs/a~2b",
            "#/%xx",
        ] {
            let mut bad = value.clone();
            bad["properties"]["value"]["$ref"] = json!(target);
            bad["default"] = json!({"type":"string"});
            assert!(admit(bad, true).is_err(), "{target}");
        }
        let mut d = admit(value, true).unwrap();
        let mut settings = d.semantic.settings().clone();
        let Presence::Value(OutputConstraint::JsonSchema { schema, .. }) =
            &mut settings.text.format
        else {
            panic!()
        };
        schema
            .root_mut()
            .as_object_mut()
            .unwrap()
            .shift_remove("$defs");
        assert!(d.semantic.clone().with_settings(settings.clone()).is_err());
        let Presence::Value(OutputConstraint::JsonSchema { schema, .. }) =
            &mut settings.text.format
        else {
            panic!()
        };
        schema.root_mut()["properties"]["value"] = json!({"type":"boolean"});
        d.semantic = d.semantic.with_settings(settings).unwrap();
        let out = responses::encode_generation(
            &lower_request(
                &d.semantic,
                &d.fidelity,
                Profile::Responses,
                Contract::full(),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(out["text"]["format"]["schema"].get("$defs").is_none());
        assert_eq!(
            out["text"]["format"]["schema"]["properties"]["value"],
            json!({"type":"boolean"})
        );
    }

    #[test]
    fn schema_resource_bounds_count_physical_shapes_without_expanding_refs() {
        for (count, accepted) in [(5000, true), (5001, false)] {
            let props: serde_json::Map<String, Value> = (0..count)
                .map(|i| (format!("p{i}"), json!({"type":"boolean"})))
                .collect();
            let required: Vec<_> = props.keys().cloned().collect();
            let value = json!({"type":"object","properties":props,"required":required,"additionalProperties":false});
            assert_eq!(admit(value, true).is_ok(), accepted, "properties {count}");
        }
        for (depth, accepted) in [(10, true), (11, false)] {
            let mut value = json!({"type":"string"});
            for _ in 0..depth {
                value = closed_property(value);
            }
            assert_eq!(admit(value, true).is_ok(), accepted, "depth {depth}");
        }
        for (count, accepted) in [(1000, true), (1001, false)] {
            assert_eq!(
                admit(
                    closed_property(
                        json!({"type":"integer","enum":(0..count).collect::<Vec<_>>()})
                    ),
                    true
                )
                .is_ok(),
                accepted
            );
        }
        for (chars, accepted) in [(119_995, true), (119_996, false)] {
            assert_eq!(
                admit(
                    closed_property(json!({"type":"string","const":"x".repeat(chars)})),
                    true
                )
                .is_ok(),
                accepted
            );
        }
        for (edges, accepted) in [(8192, true), (8193, false)] {
            let value = json!({"anyOf":vec![json!({"$ref":"#/$defs/Leaf"});edges],"$defs":{"Leaf":{"type":"string"}}});
            assert_eq!(admit(value, false).is_ok(), accepted, "edges {edges}");
        }
        for (children, accepted) in [(16_383, true), (16_384, false)] {
            assert_eq!(
                admit(json!({"allOf":vec![json!({});children]}), false).is_ok(),
                accepted,
                "schema nodes"
            );
        }
        for (values, accepted) in [(65_533, true), (65_534, false)] {
            assert_eq!(
                admit(json!({"default":vec![0;values]}), false).is_ok(),
                accepted,
                "raw nodes"
            );
        }
        for (wrappers, accepted) in [(63, true), (64, false)] {
            let mut deep = json!({});
            for _ in 0..wrappers {
                deep = json!({"default":deep});
            }
            assert_eq!(admit(deep, false).is_ok(), accepted, "raw depth");
        }
        for (length, accepted) in [(1000, true), (5000, false)] {
            let children: serde_json::Map<String, Value> = (0..1000)
                .map(|i| (format!("p{i}"), json!({"type":"string"})))
                .collect();
            let mut properties = serde_json::Map::new();
            properties.insert(
                "x".repeat(length),
                json!({"type":"object","properties":children}),
            );
            assert_eq!(
                admit(json!({"type":"object","properties":properties}), false).is_ok(),
                accepted,
                "pointer path amplification"
            );
        }
        assert!(admit(json!({"default":"x".repeat(MAX_TEXT_BYTES)}), false).is_err());
    }

    #[test]
    fn regex_unicode_syntax_rejects_legacy_escapes_unknown_properties_and_invalid_references() {
        for pattern in [
            r"\!",
            r"\p{NotAProperty}",
            r"\k<missing>",
            r"\2(a)",
            "(?<1bad>a)",
            "(?i:a)",
            r"\A",
            r"\z",
            r"\Z",
        ] {
            assert!(
                admit(
                    closed_property(json!({"type":"string","pattern":pattern})),
                    false
                )
                .is_err(),
                "{pattern}"
            );
        }
    }
    #[test]
    fn format_names_come_from_the_fixed_registry() {
        for good in [
            "date",
            "time",
            "date-time",
            "duration",
            "email",
            "idn-email",
            "hostname",
            "idn-hostname",
            "ipv4",
            "ipv6",
            "uri",
            "uri-reference",
            "uri-template",
            "iri",
            "iri-reference",
            "uuid",
            "json-pointer",
            "relative-json-pointer",
            "regex",
        ] {
            for strict in [true, false] {
                let schema = closed_property(json!({"type":"string","format":good}));
                assert!(admit(schema, strict).is_ok(), "{good} must pass");
            }
        }
        for bad in [
            "",
            "date_time",
            "DateTime",
            "byte",
            "custom-thing",
            "uri_template",
        ] {
            for strict in [true, false] {
                let schema = closed_property(json!({"type":"string","format":bad}));
                assert!(admit(schema, strict).is_err(), "{bad} must fail");
            }
        }
    }

    #[test]
    fn pattern_property_keys_are_checked_like_patterns() {
        let ok = json!({"type":"object","patternProperties":{"^[a-z]+":{"type":"integer"}},"additionalProperties":false});
        assert!(admit(ok, false).is_ok());
        let bad = json!({"type":"object","patternProperties":{"(": {"type":"integer"}},"additionalProperties":false});
        assert!(admit(bad, false).is_err());
    }
}
mod schema_documents {
    //! Declared dialects and supplied reference graphs; no resolver I/O or schema rewriting.
    use morphiecore::{
        protocol::{
            fidelity::FidelityRecords,
            openai::{Profile, TargetCaps, lowering::lower_request},
        },
        semantic::{
            task::generation::*,
            value::{Presence, Text},
        },
    };
    use serde_json::json;
    fn text(v: &str) -> Text {
        Text::new(v, "synthetic", MAX_TEXT_BYTES).unwrap()
    }
    fn resource(id: &str, value: serde_json::Value) -> SchemaResource {
        SchemaResource {
            id: text(id),
            value,
        }
    }
    #[test]
    fn dialect_declarations_reference_positions_and_duplicate_resources_fail_closed() {
        let dialect = SchemaDialect::Draft202012;
        for (root, resources) in [
            (
                json!({"$ref":"https://not-provided.invalid/schema"}),
                vec![],
            ),
            (
                json!({"$schema":"https://json-schema.org/draft/2019-09/schema","type":"string"}),
                vec![],
            ),
            (
                json!({"$id":"https://rebase.invalid/","type":"string"}),
                vec![],
            ),
            (json!({"$ref":"#/$defs/missing"}), vec![]),
            (
                json!({"default":{"type":"string"},"$ref":"#/default"}),
                vec![],
            ),
            (
                json!({"$ref":"urn:synthetic:one"}),
                vec![
                    resource("urn:synthetic:one", json!({"type":"string"})),
                    resource("urn:synthetic:one", json!({"type":"integer"})),
                ],
            ),
        ] {
            assert!(SchemaDocument::new(dialect, root, resources).is_err());
        }
        SchemaDocument::new(dialect, json!(true), vec![]).unwrap();
        assert!(SchemaDocument::new(SchemaDialect::Unspecified, json!(true), vec![]).is_err());
        assert!(
            SchemaDocument::new(
                SchemaDialect::Unspecified,
                json!({"$schema":"https://json-schema.org/draft/2020-12/schema","type":"string"}),
                vec![]
            )
            .is_err()
        );
    }
    #[test]
    fn physical_reference_and_resource_budgets_are_independent_of_data_depth() {
        for count in [
            MAX_SCHEMA_REFERENCES - 1,
            MAX_SCHEMA_REFERENCES,
            MAX_SCHEMA_REFERENCES + 1,
        ] {
            let properties: serde_json::Map<String, serde_json::Value> = (0..count)
                .map(|n| (format!("p{n}"), json!({"$ref":"#"})))
                .collect();
            let result = SchemaDocument::new(
                SchemaDialect::Draft202012,
                json!({"type":"object","properties":properties}),
                vec![],
            );
            assert_eq!(result.is_ok(), count <= MAX_SCHEMA_REFERENCES);
        }
        for count in [
            MAX_SCHEMA_RESOURCES - 1,
            MAX_SCHEMA_RESOURCES,
            MAX_SCHEMA_RESOURCES + 1,
        ] {
            let resources = (0..count)
                .map(|n| {
                    resource(
                        &format!("urn:synthetic:{n}"),
                        json!({"$ref":format!("urn:synthetic:{}", (n+1)%count)}),
                    )
                })
                .collect();
            let result = SchemaDocument::new(
                SchemaDialect::Draft202012,
                json!({"type":"string"}),
                resources,
            );
            assert_eq!(result.is_ok(), count <= MAX_SCHEMA_RESOURCES);
        }
        let mut deep = json!(0);
        for _ in 0..65 {
            deep = json!([deep]);
        }
        assert_eq!(
            SchemaDocument::new(SchemaDialect::Draft202012, json!({"default":deep}), vec![])
                .unwrap_err(),
            GenerationError::Limit
        );
    }

    #[test]
    fn function_parameter_and_output_dialects_cannot_be_erased_by_echo_or_event_codecs() {
        use morphiecore::protocol::openai::{events::EventEncoder, lowering::lower_response};
        for output in [false, true] {
            let schema =
                SchemaDocument::new(SchemaDialect::Draft202012, json!({"type":"object"}), vec![])
                    .unwrap();
            let settings = GenerationSettings {
                instructions: Presence::Value(text("hello")),
                tools: Some(vec![ToolDefinition::Function(FunctionTool {
                    name: text("lookup"),
                    description: None,
                    parameters: (!output).then(|| schema.clone()),
                    output_schema: output.then_some(schema),
                    strict: FunctionStrictness::Explicit(false),
                    dispatch: Default::default(),
                })]),
                ..Default::default()
            };
            let request = GenerationRequest::from_settings(vec![], settings.clone()).unwrap();
            let fidelity = FidelityRecords::default();
            let mut metadata = crate::events_support::metadata();
            metadata.context.settings = Some(settings);
            let response = GenerationResponse::new(vec![], Outcome::Completed).unwrap();
            assert!(
                lower_request(&request, &fidelity, Profile::Responses, TargetCaps::full()).is_err()
            );
            assert!(
                lower_response(
                    &response,
                    &fidelity,
                    &metadata,
                    Profile::Responses,
                    TargetCaps::full()
                )
                .is_err()
            );
            let mut encoder = EventEncoder::new(Profile::Responses, metadata).unwrap();
            assert!(encoder.encode(&StreamEvent::Started, &fidelity).is_err());
        }
    }
}
