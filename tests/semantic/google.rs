//! Independent Google Interactions semantic/schema/history contracts migrated from shared Google coverage.
use morphiecore::{
    protocol::{
        CodecError, GenerationRequestEnvelope as Request,
        failure::CarrierReason,
        google::interactions::{
            RequestRule, encode_generation_request, representation_contract, semantic_contract,
        },
        native::ReplayTarget,
        openai::{Dialect, GenerationApi, GenerationCodec},
    },
    semantic::{task::generation::*, value::Text},
};
use serde_json::{Value, json};

fn target() -> ReplayTarget {
    ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap()
}

fn client(profile: GenerationApi, body: Value) -> Request {
    GenerationCodec::new(profile, Dialect::OpenRouter, None)
        .decode_request(body.to_string().as_bytes())
        .unwrap()
}

#[test]
fn structured_tool_results_reject_arrays_scalars_and_execution_reports() {
    let text = |s: &str| Text::allowing_empty(s, "fixture", 64).unwrap();
    let request = |output, is_error| {
        GenerationRequest::from_resources(
            vec![
                (
                    ItemId::new(1),
                    Item::ToolCall(ToolCall {
                        call_id: text("c"),
                        name: text("shot"),
                        arguments: "{}".into(),
                        replay: None,
                        cache: None,
                        status: ItemLifecycle::Completed,
                        context: Default::default(),
                    }),
                ),
                (
                    ItemId::new(2),
                    Item::ToolResult(ToolResult {
                        call_id: Some(text("c")),
                        name: None,
                        output,
                        is_error,
                        execution: None,
                        status: None,
                        context: Default::default(),
                        cache: None,
                    }),
                ),
            ],
            GenerationSettings::default(),
            ResourceTable::new(vec![(
                ResourceId::new(1),
                ResourceLocation::Inline {
                    media_type: text("image/png"),
                    data_base64: text("AQID"),
                },
            )])
            .unwrap(),
        )
        .unwrap()
    };
    assert!(representation_contract().native(Feature::StructuredToolResult));
    assert!(!semantic_contract().native(Feature::StructuredToolResult));
    for value in [
        json!({}),
        json!({"n":9007199254740993_u64,"nested":[null,{"$ref":"literal"}]}),
        json!([1, 2]),
        json!(false),
        json!(null),
    ] {
        for is_error in [None, Some(false), Some(true)] {
            let source = request(
                ToolOutput::Structured(StructuredValue::new(value.clone()).unwrap()),
                is_error,
            );
            let original = source.clone();
            let interaction = encode_generation_request(&source, &Default::default(), &target());
            if value.is_object() {
                let (wire, rules) = interaction.unwrap();
                let mut expected =
                    json!({"type":"function_result","call_id":"c","name":"shot","result":value});
                if let Some(error) = is_error {
                    expected["is_error"] = json!(error);
                }
                assert_eq!(wire["input"][1], expected);
                assert!(rules.is_empty());
            } else {
                assert!(
                    interaction.is_err(),
                    "only native object results, not arbitrary array/scalar coercion"
                );
            }
            let mut items = source.items().to_vec();
            let Item::ToolResult(result) = &mut items[1].1 else {
                panic!("result")
            };
            result.execution = Some(ToolExecution::Unknown);
            let reported = source.clone().with_items(items).unwrap();
            assert!(encode_generation_request(&reported, &Default::default(), &target()).is_err());
            assert_eq!(source, original);
        }
    }
}

#[test]
fn image_sizes_remain_strict_in_user_and_tool_result_uses() {
    let messages = |oversized: &str, tool: bool| {
        let image = json!({"type":"image","source":{"type":"base64","media_type":"image/png","data":"AQID"},
            "transformations":{"oversized_image":oversized}});
        let history = if tool {
            json!([
                {"role":"assistant","content":[{"type":"tool_use","id":"c","name":"shot","input":{}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"c","content":[image]}]}
            ])
        } else {
            json!([{"role":"user","content":[image]}])
        };
        let raw = json!({"model":"gemini","max_tokens":8,"messages":history});
        morphiecore::protocol::anthropic::messages::client::decode_client_request(
            raw.to_string().as_bytes(),
        )
        .unwrap()
    };
    for tool in [false, true] {
        for oversized in ["downsize", "error"] {
            let request = messages(oversized, tool);
            let original = request.clone();
            // Fresh fidelity cannot erase an image-use guarantee.
            let interactions =
                encode_generation_request(&request.task.semantic, &Default::default(), &target());
            if oversized == "downsize" {
                interactions.unwrap();
            } else {
                assert_eq!(
                    interactions.unwrap_err(),
                    CodecError::Unsupported(CarrierReason::Unavailable)
                );
            }
            assert_eq!(request, original);
        }
    }
}

#[test]
fn referenced_function_parameters_reach_interactions_without_schema_rewriting() {
    let parameters = json!({"$ref":"#/$defs/Args","$defs":{"Args":{
        "type":"object","properties":{"value":{"type":"integer","enum":[7]}},
        "required":["value"],"additionalProperties":false}}});
    let source = client(
        GenerationApi::Responses,
        json!({"model":"gemini","input":"go",
        "tools":[{"type":"function","name":"lookup","strict":false,"parameters":parameters}]}),
    );
    let before = source.clone();
    let (wire, rules) =
        encode_generation_request(&source.task.semantic, &source.task.fidelity, &target()).unwrap();
    assert_eq!(
        wire,
        json!({
            "model":"gemini-synthetic","store":false,"stream":true,
            "input":[{"type":"user_input","content":[{"type":"text","text":"go"}]}],
            "tools":[{"type":"function","name":"lookup","parameters":parameters}]
        })
    );
    assert!(rules.is_empty());
    assert_eq!(source, before);
}

#[test]
fn no_call_keeps_strict_schemas_and_makes_serial_intent_vacuous() {
    let parameters = json!({"type":"object","properties":{
        "label":{"type":"string","minLength":5,"pattern":"^[a-z]+$"}},
        "required":["label"],"additionalProperties":false});
    let source = client(
        GenerationApi::Responses,
        json!({"model":"gemini","input":"go","tool_choice":"none",
            "parallel_tool_calls":false,
            "tools":[{"type":"function","name":"lookup","strict":true,"parameters":parameters}]}),
    );
    let before = source.clone();
    let (interactions, rules) =
        encode_generation_request(&source.task.semantic, &source.task.fidelity, &target()).unwrap();
    assert_eq!(
        interactions,
        json!({
            "model":"gemini-synthetic","store":false,"stream":true,
            "input":[{"type":"user_input","content":[{"type":"text","text":"go"}]}],
            "tools":[{"type":"function","name":"lookup","parameters":parameters}],
            "generation_config":{"tool_choice":"none"}
        })
    );
    assert!(rules.is_empty());
    assert_eq!(source, before);
}

#[test]
fn strict_local_references_keep_the_original_document_in_interactions() {
    for parameters in [
        json!({"$ref":"#/$defs/Args","$defs":{
        "Args":{"type":"object","properties":{
            "label":{"anyOf":[{"$ref":"#/$defs/Label~1~0"},{"type":"null"}]}},
            "required":["label"],"additionalProperties":false},
        "Label/~":{"type":"string","enum":["cedar"]}}}),
        json!({"type":"object","properties":{
            "label":{"anyOf":[{"type":"string","enum":["cedar"]},{"type":"null"}]}},
            "required":["label"],"additionalProperties":false}),
    ] {
        let source = client(
            GenerationApi::Responses,
            json!({"model":"gemini","input":"go",
            "tools":[{"type":"function","name":"lookup","strict":true,"parameters":parameters}]}),
        );
        let before = source.clone();
        let (interactions, rules) =
            encode_generation_request(&source.task.semantic, &source.task.fidelity, &target())
                .unwrap();
        assert_eq!(
            interactions["tools"],
            json!([{"type":"function","name":"lookup","parameters":parameters}])
        );
        assert_eq!(
            interactions["generation_config"],
            json!({"tool_choice":"validated"})
        );
        assert!(rules.is_empty());
        assert_eq!(source, before);
    }
}

#[test]
fn strict_local_references_cannot_hide_constraints_scopes_or_cycles() {
    let closed = json!({"type":"object","properties":{},"additionalProperties":false});
    let mut cases = vec![
        json!({"$ref":"#/$defs/Missing","$defs":{"Args":closed}}),
        json!({"$ref":"https://example.invalid/schema","$defs":{"Args":closed}}),
        json!({"$ref":"#/$defs/Args~2","$defs":{"Args~2":closed}}),
        json!({"$ref":"#/$defs/%41rgs","$defs":{"Args":closed}}),
        json!({"$ref":"#/$defs/Args","enum":[{}],"$defs":{"Args":closed}}),
        json!({"$ref":"#/$defs/Args","$id":"scope","$defs":{"Args":closed}}),
        json!({"$ref":"#/$defs/Args","$defs":{"Args":{"type":"string"}}}),
        json!({"$ref":"#/$defs/Args","$defs":{"Args":{"$ref":"#/$defs/Other"},
            "Other":{"$ref":"#/$defs/Args"}}}),
    ];
    for property in [
        json!({"type":"string","minLength":5}),
        json!({"$ref":"#/$defs/Label","enum":["cedar"]}),
        json!({"$ref":"#/$defs/Args"}),
        json!({"type":"array","items":{"$ref":"#/$defs/Args"}}),
        json!({"$defs":{"Local":{"type":"string"}},"$ref":"#/$defs/Local"}),
    ] {
        cases.push(json!({"$ref":"#/$defs/Args","$defs":{
            "Args":{"type":"object","properties":{"label":property},
                "required":["label"],"additionalProperties":false},
            "Label":{"type":"string"}}}));
    }
    let mut deep = json!({"$ref":"#/$defs/0","$defs":{}});
    for i in 0..70 {
        deep["$defs"][i.to_string()] = if i == 69 {
            closed.clone()
        } else {
            json!({"$ref":format!("#/$defs/{}", i + 1)})
        };
    }
    cases.push(deep);
    for parameters in cases {
        let source = client(
            GenerationApi::Responses,
            json!({"model":"gemini","input":"go",
            "tools":[{"type":"function","name":"lookup","strict":true,"parameters":parameters}]}),
        );
        assert!(
            encode_generation_request(&source.task.semantic, &source.task.fidelity, &target())
                .is_err(),
            "Interactions admitted {parameters}"
        );
    }
}

#[test]
fn strict_shared_reference_graphs_remain_bounded_without_schema_expansion() {
    let mut parameters = json!({"$ref":"#/$defs/20","$defs":{
        "0":{"type":"object","properties":{},"additionalProperties":false}}});
    for i in 1..=20 {
        let reference = json!({"$ref":format!("#/$defs/{}", i - 1)});
        parameters["$defs"][i.to_string()] = json!({
            "type":"object","properties":{"left":reference,"right":reference},
            "required":["left","right"],"additionalProperties":false});
    }
    let source = client(
        GenerationApi::Responses,
        json!({"model":"gemini","input":"go",
        "tools":[{"type":"function","name":"lookup","strict":true,"parameters":parameters}]}),
    );
    let (wire, _) =
        encode_generation_request(&source.task.semantic, &source.task.fidelity, &target()).unwrap();
    assert_eq!(wire["tools"][0]["parameters"], parameters);
    // Cached subtrees must still count when reached through a longer path.
    for i in 21..50 {
        parameters["$defs"][i.to_string()] = json!({"$ref":format!("#/$defs/{}", i - 1)});
    }
    parameters["$ref"] = json!("#/$defs/49");
    let source = client(
        GenerationApi::Responses,
        json!({"model":"gemini","input":"go",
        "tools":[{"type":"function","name":"lookup","strict":true,"parameters":parameters}]}),
    );
    assert!(
        encode_generation_request(&source.task.semantic, &source.task.fidelity, &target()).is_err()
    );
}

#[test]
fn strict_function_arguments_use_validated_modes_without_rewriting_schemas() {
    let parameters = json!({"type":"object","properties":{
        "value":{"type":"integer","enum":[7]},
        "ok":{"type":"boolean"},
        "label":{"type":"string","enum":["cedar"]},
        "offset":{"anyOf":[{"type":"integer"},{"type":"null"}]},
        "rows":{"type":"array","items":{"type":"object","properties":{
            "amount":{"type":"number","enum":[1.25]}},
            "required":["amount"],"additionalProperties":false}}},
        "required":["value","ok","label","offset"],"additionalProperties":false});
    for (choice, interaction_mode) in [
        (None, "validated"),
        (Some(json!("auto")), "validated"),
        (Some(json!("required")), "any"),
        (Some(json!("none")), "none"),
        (
            Some(json!({"type":"allowed_tools","mode":"auto",
            "tools":[{"type":"function","name":"lookup"}]})),
            "validated",
        ),
    ] {
        let mut input = json!({"model":"gemini","input":"go",
            "tools":[{"type":"function","name":"lookup","strict":true,"parameters":parameters}]});
        if let Some(choice) = &choice {
            input["tool_choice"] = choice.clone();
        }
        let source = client(GenerationApi::Responses, input);
        let before = source.clone();
        let (interactions, rules) =
            encode_generation_request(&source.task.semantic, &source.task.fidelity, &target())
                .unwrap();
        assert_eq!(
            interactions["tools"],
            json!([{"type":"function","name":"lookup","parameters":parameters}])
        );
        if choice.as_ref().is_some_and(Value::is_object) {
            assert_eq!(
                interactions["generation_config"]["tool_choice"],
                json!({"allowed_tools":{"mode":interaction_mode,"tools":["lookup"]}})
            );
        } else {
            assert_eq!(
                interactions["generation_config"]["tool_choice"],
                interaction_mode
            );
        }
        assert!(rules.is_empty());
        assert_eq!(source, before);
    }
    // Explicit guarantees with unqualified vocabulary stay rejected, including
    // nested properties named like keywords: traverse schema positions only.
    for property in [
        json!({"type":"string","minLength":5}),
        json!({"type":"string","pattern":"^[a-z]+$"}),
        json!({"anyOf":[{"type":"string"},{"type":"integer"}]}),
        json!({"anyOf":[{"type":"string","minLength":5},{"type":"null"}]}),
        json!({"anyOf":[{"type":"integer"},{"type":"null"}],"maxLength":5}),
        json!({"$ref":"#/$defs/Label"}),
        json!({"type":"boolean","enum":[true]}),
        json!({"type":"integer","enum":[9007199254740993u64]}),
        json!({"type":"number","enum":[9007199254740993u64]}),
        serde_json::from_str(r#"{"type":"number","enum":[0.10000000000000001]}"#).unwrap(),
        json!({"type":"object","properties":{"value":{"type":"integer"}}}),
    ] {
        let source = client(
            GenerationApi::Responses,
            json!({"model":"gemini","input":"go",
            "tools":[{"type":"function","name":"lookup","strict":true,"parameters":{
                "type":"object","properties":{"label":property},"additionalProperties":false}}]}),
        );
        assert!(
            encode_generation_request(&source.task.semantic, &source.task.fidelity, &target())
                .is_err()
        );
    }
}

#[test]
fn google_composes_only_the_leading_instruction_prefix_without_hoisting_history() {
    let request = client(
        GenerationApi::Responses,
        json!({"model":"gemini","instructions":"root",
        "input":[{"role":"system","content":[{"type":"input_text","text":"A"},{"type":"input_text","text":"B"}]},
            {"role":"developer","content":"C"},{"role":"user","content":"go"}]}),
    );
    let original = request.clone();
    let (wire, rules) =
        encode_generation_request(&request.task.semantic, &request.task.fidelity, &target())
            .unwrap();
    assert_eq!(wire["system_instruction"], "root\n\nA\n\nB\n\nC");
    assert_eq!(
        wire["input"],
        json!([{"type":"user_input","content":[{"type":"text","text":"go"}]}])
    );
    assert!(rules.contains(&RequestRule::JoinInstructionParts));

    let mut items = request.task.semantic.items().to_vec();
    items.swap(1, 2);
    let late = GenerationRequest::from_resources(
        items,
        request.task.semantic.settings().clone(),
        request.task.semantic.resources().clone(),
    )
    .unwrap();
    assert!(encode_generation_request(&late, &request.task.fidelity, &target()).is_err());
    assert_eq!(request, original);
}
