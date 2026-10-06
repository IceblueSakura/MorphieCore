//! Schema order is asserted independently of Value equality, which ignores object order.
use crate::wire;
use morphiecore::{
    lowering::generation::{
        GenerationRepresentationContract as Contract, lower_request, lower_response,
    },
    protocol::openai::{
        Profile, chat, envelope, responses,
        sse::{Obfuscation, ResponsesSseDecoder, ResponsesSseEncoder, SseLimits, encode_frame},
    },
    semantic::{
        task::generation::*,
        value::{Presence, Text},
    },
};
use serde_json::{Value, json};

const SCHEMA: &str = r#"{"type":"object","properties":{"zeta":{"type":"object","properties":{"omega":{"type":"string"},"beta":{"type":"integer"}},"required":["omega","beta"],"additionalProperties":false},"middle":{"type":"boolean"},"alpha":{"type":"string"}},"required":["zeta","middle","alpha"],"additionalProperties":false}"#;

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

fn assert_order(schema: &Value, expected: &[&str]) {
    assert_eq!(
        schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        expected
    );
}
fn assert_original(schema: &Value) {
    assert_order(schema, &["zeta", "middle", "alpha"]);
    assert_order(&schema["properties"]["zeta"], &["omega", "beta"]);
    assert_eq!(schema.to_string(), SCHEMA);
}
fn request() -> envelope::DecodedResponsesRequest {
    let bytes = format!(
        r#"{{"model":"fixture-model","input":"hello","text":{{"format":{{"type":"json_schema","name":"answer","strict":true,"schema":{SCHEMA}}}}},"tools":[{{"type":"function","name":"f","strict":true,"parameters":{SCHEMA},"output_schema":{SCHEMA}}}]}}"#
    );
    envelope::decode_request_bytes(bytes.as_bytes()).unwrap()
}

fn admit(
    schema: Value,
    strict: bool,
) -> Result<morphiecore::protocol::openai::DecodedRequest, morphiecore::protocol::openai::CodecError>
{
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
            schema: bad.clone(),
            strict: Some(false),
        });
        assert!(lower_request(&r, &d.fidelity, Profile::Responses, Contract::full()).is_err());
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
    let Presence::Value(OutputConstraint::JsonSchema { schema, .. }) = &mut settings.text.format
    else {
        panic!()
    };
    schema.as_object_mut().unwrap().shift_remove("$defs");
    assert!(d.semantic.clone().with_settings(settings.clone()).is_err());
    let Presence::Value(OutputConstraint::JsonSchema { schema, .. }) = &mut settings.text.format
    else {
        panic!()
    };
    schema["properties"]["value"] = json!({"type":"boolean"});
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
fn function_strict_defaults_and_output_schemas_have_separate_admission() {
    let open = json!({"type":"object","properties":{"x":{"type":"string"}}});
    for strict in [None, Some(false), Some(true)] {
        let mut tool = json!({"type":"function","name":"f","parameters":open});
        if let Some(strict) = strict {
            tool["strict"] = json!(strict);
        }
        let result = responses::decode_generation(&json!({"input":"x","tools":[tool]}));
        assert_eq!(result.is_ok(), strict != Some(true));
        if let Ok(d) = result {
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
            assert_eq!(out["tools"][0]["parameters"], open);
            assert_eq!(
                out["tools"][0].get("strict"),
                strict.map(|v| json!(v)).as_ref()
            );
        }
    }
    let closed = closed_property(json!({"type":"integer"}));
    let d=responses::decode_generation(&json!({"input":"x","tools":[{"type":"function","name":"f","parameters":closed,"strict":true,"output_schema":{"type":"string"}}]})).unwrap();
    assert!(
        lower_request(
            &d.semantic,
            &d.fidelity,
            Profile::Responses,
            Contract::full()
        )
        .is_ok()
    );
    let mut tool =
        json!({"type":"function","name":"f","parameters":{"type":"object","allOf":[{}]}});
    assert!(responses::decode_generation(&json!({"input":"x","tools":[tool.clone()]})).is_err());
    tool["strict"] = json!(false);
    assert!(responses::decode_generation(&json!({"input":"x","tools":[tool]})).is_ok());
    // Opening the output-constraint mapping relaxes no Chat tool admission.
    let d=responses::decode_generation(&json!({"input":"x","text":{"format":{"type":"json_object"}},"tools":[{"type":"function","name":"f","parameters":closed}]})).unwrap();
    assert!(lower_request(&d.semantic, &d.fidelity, Profile::Chat, Contract::full()).is_err());
    let d=responses::decode_generation(&json!({"input":"x","text":{"format":{"type":"json_object"}},"tools":[{"type":"function","name":"f","parameters":closed,"strict":false,"output_schema":{"type":"string"}}]})).unwrap();
    assert!(lower_request(&d.semantic, &d.fidelity, Profile::Chat, Contract::full()).is_err());
    let d=responses::decode_generation(&json!({"input":"x","text":{"format":{"type":"json_object"}},"tools":[{"type":"function","name":"f","parameters":closed,"strict":false}]})).unwrap();
    assert!(lower_request(&d.semantic, &d.fidelity, Profile::Chat, Contract::full()).is_ok());
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
                closed_property(json!({"type":"integer","enum":(0..count).collect::<Vec<_>>()})),
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
fn invalid_schema_metadata_poisons_stream_decode_and_encode_updates() {
    let invalid = closed_property(json!({"$ref":"#/$defs/missing"}));
    let mut payload = wire::events(2)[0].clone();
    payload["response"]["text"]["format"]["schema"] = invalid.clone();
    let mut decoder =
        ResponsesSseDecoder::new(200, "text/event-stream", SseLimits::default(), None).unwrap();
    let bad = encode_frame(&payload, SseLimits::default().max_event_bytes).unwrap();
    assert!(decoder.consume(&bad).is_err());
    assert!(
        decoder
            .consume(
                &encode_frame(&wire::events(2)[0], SseLimits::default().max_event_bytes).unwrap()
            )
            .is_err()
    );
    assert!(decoder.finish().is_err());
    assert!(decoder.materialize().is_err());
    let mut metadata = envelope::decode_response(&wire::response(2))
        .unwrap()
        .metadata;
    let mut encoder = ResponsesSseEncoder::new(
        metadata.clone(),
        Contract::full(),
        SseLimits::default(),
        Obfuscation::Disabled,
    )
    .unwrap();
    let Presence::Value(OutputConstraint::JsonSchema { schema, .. }) =
        &mut metadata.context.settings.as_mut().unwrap().text.format
    else {
        panic!()
    };
    *schema = invalid;
    assert!(encoder.update_metadata(metadata).is_err());
    assert!(
        encoder
            .encode(&StreamEvent::Started, &Default::default())
            .is_err()
    );
    assert!(encoder.finish().is_err());
}

#[test]
fn raw_schema_order_survives_ir_and_independent_protocol_projections() {
    let mut d = request();
    let OutputConstraint::JsonSchema { schema, .. } = d.task.semantic.output() else {
        panic!()
    };
    assert_original(schema);
    let ToolDefinition::Function(tool) = &d.task.semantic.tools()[0] else {
        panic!()
    };
    assert_original(tool.parameters.as_ref().unwrap());
    assert_original(tool.output_schema.as_ref().unwrap());
    let encoded = envelope::encode_request(
        &lower_request(
            &d.task.semantic,
            &d.task.fidelity,
            Profile::Responses,
            Contract::full(),
        )
        .unwrap(),
        &d.context,
    )
    .unwrap();
    assert_original(&encoded["text"]["format"]["schema"]);
    assert_original(&encoded["tools"][0]["parameters"]);
    assert_original(&encoded["tools"][0]["output_schema"]);

    // Chat still rejects function output_schema even though text.format now projects.
    assert!(
        lower_request(
            &d.task.semantic,
            &d.task.fidelity,
            Profile::Chat,
            Contract::full()
        )
        .is_err()
    );
    let mut settings = d.task.semantic.settings().clone();
    let ToolDefinition::Function(tool) = &mut settings.tools.as_mut().unwrap()[0] else {
        panic!()
    };
    tool.output_schema = None;
    d.task.semantic = d.task.semantic.with_settings(settings).unwrap();
    let encoded = chat::encode_generation(
        &lower_request(
            &d.task.semantic,
            &d.task.fidelity,
            Profile::Chat,
            Contract::full(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_original(&encoded["response_format"]["json_schema"]["schema"]);
    assert_original(&encoded["tools"][0]["function"]["parameters"]);

    // Deleting the output constraint does not resurrect the old schema on the Chat shell.
    let mut settings = d.task.semantic.settings().clone();
    settings.text = Default::default();
    d.task.semantic = d.task.semantic.with_settings(settings).unwrap();
    let encoded = chat::encode_generation(
        &lower_request(
            &d.task.semantic,
            &d.task.fidelity,
            Profile::Chat,
            Contract::full(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(encoded.get("response_format").is_none());
}

#[test]
fn switching_or_deleting_the_output_constraint_cannot_resurrect_a_schema() {
    let mut d = request();
    let mut settings = d.task.semantic.settings().clone();
    settings.tools = None;
    let replacement = closed_property(json!({"type":"boolean"}));
    settings.text.format = Presence::Value(OutputConstraint::JsonSchema {
        name: Text::new("answer", "schema name", 64).unwrap(),
        description: None,
        schema: replacement.clone(),
        strict: Some(true),
    });
    d.task.semantic = d.task.semantic.with_settings(settings.clone()).unwrap();
    assert!(GenerationRequirements::derive(&d.task.semantic).structured_output);
    let encoded = chat::encode_generation(
        &lower_request(
            &d.task.semantic,
            &d.task.fidelity,
            Profile::Chat,
            Contract::full(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_order(
        &encoded["response_format"]["json_schema"]["schema"],
        &["value"],
    );
    assert_eq!(
        encoded["response_format"]["json_schema"]["schema"],
        replacement
    );
    // Switching the constraint drops the old schema instead of keeping it in source metadata.
    settings.text.format = Presence::Value(OutputConstraint::JsonObject);
    d.task.semantic = d.task.semantic.with_settings(settings.clone()).unwrap();
    assert!(GenerationRequirements::derive(&d.task.semantic).structured_output);
    let encoded = chat::encode_generation(
        &lower_request(
            &d.task.semantic,
            &d.task.fidelity,
            Profile::Chat,
            Contract::full(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(encoded["response_format"], json!({"type":"json_object"}));
    assert!(!encoded.to_string().contains("value"));
    settings.text = Default::default();
    d.task.semantic = d.task.semantic.with_settings(settings).unwrap();
    assert!(!GenerationRequirements::derive(&d.task.semantic).structured_output);
    let encoded = chat::encode_generation(
        &lower_request(
            &d.task.semantic,
            &d.task.fidelity,
            Profile::Chat,
            Contract::full(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(encoded.get("response_format").is_none());
    assert!(!encoded.to_string().contains("value"));
}

#[test]
fn final_schema_edits_own_order_and_deleted_constraints_do_not_return() {
    let mut d = request();
    let mut settings = d.task.semantic.settings().clone();
    let Presence::Value(OutputConstraint::JsonSchema { schema, .. }) = &mut settings.text.format
    else {
        panic!()
    };
    let properties = schema["properties"].as_object_mut().unwrap();
    properties.insert("middle".into(), json!({"type":"integer"}));
    properties.insert("new_first".into(), json!({"type":"string"}));
    // Retain surviving properties in their declared order.
    properties.retain(|key, _| key != "zeta");
    schema["required"] = json!(["middle", "alpha", "new_first"]);
    assert_order(schema, &["middle", "alpha", "new_first"]);
    let replacement = schema.clone();
    let ToolDefinition::Function(tool) = &mut settings.tools.as_mut().unwrap()[0] else {
        panic!()
    };
    tool.parameters = Some(replacement.clone());
    tool.output_schema = Some(replacement);
    d.task.semantic = d.task.semantic.with_settings(settings).unwrap();
    let encoded = responses::encode_generation(
        &lower_request(
            &d.task.semantic,
            &d.task.fidelity,
            Profile::Responses,
            Contract::full(),
        )
        .unwrap(),
    )
    .unwrap();
    for schema in [
        &encoded["text"]["format"]["schema"],
        &encoded["tools"][0]["parameters"],
        &encoded["tools"][0]["output_schema"],
    ] {
        assert_order(schema, &["middle", "alpha", "new_first"]);
        assert_eq!(schema["properties"]["middle"], json!({"type":"integer"}));
        assert!(schema["properties"].get("zeta").is_none());
    }
    let mut settings = d.task.semantic.settings().clone();
    settings.text = Default::default();
    settings.tools = None;
    d.task.semantic = d.task.semantic.with_settings(settings).unwrap();
    let encoded = responses::encode_generation(
        &lower_request(
            &d.task.semantic,
            &d.task.fidelity,
            Profile::Responses,
            Contract::full(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(encoded.get("text").is_none());
    assert!(encoded.get("tools").is_none());
}

#[test]
fn reported_schema_order_closes_across_static_and_sse() {
    let schema: Value = serde_json::from_str(SCHEMA).unwrap();
    let mut response = wire::response(2);
    response["text"]["format"]["schema"] = schema.clone();
    response["tools"][0]["parameters"] = schema.clone();
    let d = envelope::decode_response_bytes(&serde_json::to_vec(&response).unwrap()).unwrap();
    let encoded = envelope::encode_response(
        &lower_response(
            &d.semantic,
            &d.fidelity,
            &d.metadata,
            Profile::Responses,
            Contract::full(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_original(&encoded["text"]["format"]["schema"]);
    assert_original(&encoded["tools"][0]["parameters"]);

    let mut decoder =
        ResponsesSseDecoder::new(200, "text/event-stream", SseLimits::default(), None).unwrap();
    let mut events = vec![];
    for mut payload in wire::events(2) {
        if let Some(response) = payload.get_mut("response") {
            response["text"]["format"]["schema"] = schema.clone();
            response["tools"][0]["parameters"] = schema.clone();
        }
        let bytes = encode_frame(&payload, SseLimits::default().max_event_bytes).unwrap();
        for byte in bytes.iter() {
            let (used, next) = decoder.consume(std::slice::from_ref(byte)).unwrap();
            assert_eq!(used, 1);
            events.extend(next);
        }
    }
    decoder.finish().unwrap();
    let d = decoder.materialize().unwrap();
    let OutputConstraint::JsonSchema { schema, .. } =
        d.metadata.context.settings.as_ref().unwrap().output()
    else {
        panic!()
    };
    assert_original(schema);
    let mut encoder = ResponsesSseEncoder::new(
        d.metadata,
        Contract::full(),
        SseLimits::default(),
        Obfuscation::Disabled,
    )
    .unwrap();
    let mut terminal = None;
    for event in &events {
        for frame in encoder.encode(event, &d.fidelity).unwrap() {
            // Inspect serialized output independently of the Responses decoder/materializer.
            let text = std::str::from_utf8(&frame).unwrap();
            let data = text
                .lines()
                .find_map(|line| line.strip_prefix("data: "))
                .unwrap();
            let value: Value = serde_json::from_str(data).unwrap();
            if value["type"] == "response.completed" {
                terminal = Some(value);
            }
        }
    }
    encoder.finish().unwrap();
    let terminal = terminal.unwrap();
    assert_original(&terminal["response"]["text"]["format"]["schema"]);
    assert_original(&terminal["response"]["tools"][0]["parameters"]);
}

#[test]
fn pattern_syntax_stays_inside_the_admitted_ecmascript_subset() {
    for good in [
        "",
        r"^\d{4}-\d{2}-\d{2}$",
        r"^\+?[1-9]\d{1,14}$",
        "^[a-z]+$",
        "a|b|",
        "(?:ab)+c",
        "(?=x)y",
        "(?<!x)y",
        r"(?<year>\d{4})-\k<year>",
        r"[\-a-z]",
        r"[^\]]",
        "a{2,3}?",
        r"\p{L}+",
        r"\p{Script=Greek}+",
        r"\u{1F600}",
        r"a\{b\}",
    ] {
        for strict in [true, false] {
            let schema = closed_property(json!({"type":"string","pattern":good}));
            assert!(
                admit(schema, strict).is_ok(),
                "{good} strict={strict} must pass"
            );
        }
    }
    for bad in [
        "(", "a)", "[a", "a{2,1}", "*", "a**", "(x)*+", r"\q", r"\x{", r"\p{", r"\p{}", "[z-a]",
        "{", "a{,3}", "a{2", "(?<", r"[\d-a]", "(?i)a",
    ] {
        for strict in [true, false] {
            let schema = closed_property(json!({"type":"string","pattern":bad}));
            assert!(
                admit(schema, strict).is_err(),
                "{bad} strict={strict} must fail"
            );
        }
    }
}

#[test]
fn regex_work_is_bounded_before_recursive_syntax_parsing() {
    for pattern in [
        format!("{}a{}", "(".repeat(65), ")".repeat(65)),
        "a".repeat(65_537),
    ] {
        for strict in [true, false] {
            let error = admit(
                closed_property(json!({"type":"string","pattern":pattern})),
                strict,
            )
            .unwrap_err();
            assert!(matches!(
                error,
                morphiecore::protocol::openai::CodecError::Semantic(GenerationError::Limit)
            ));
        }
        let error = admit(
            json!({"patternProperties":{pattern:{"type":"string"}}}),
            false,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            morphiecore::protocol::openai::CodecError::Semantic(GenerationError::Limit)
        ));
    }
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
