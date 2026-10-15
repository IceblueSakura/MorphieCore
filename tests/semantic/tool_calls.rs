//! Tool definitions, calls, results and continuation.
mod tools {
    //! Independent oracles for the function-tool migration, without providers or production routing.
    use crate::events_support::{
        apply, close, created, encode, envelope, metadata, start, terminal,
    };
    use morphiecore::{
        lowering::generation::RepresentationError,
        protocol::{
            fidelity::FidelityRecords,
            openai::{
                DecodedRequest, Profile, TargetCaps as Contract, chat, events,
                lowering::{lower_request, lower_response},
                responses,
            },
        },
        semantic::{task::generation::*, value::Text},
    };
    use serde_json::{Value, json};

    fn text(s: &str) -> Text {
        Text::new(s, "fixture", 1024).unwrap()
    }
    fn chat_history() -> Value {
        json!({"messages":[
        {"role":"user","content":"weather?"},
        {"role":"assistant","content":"Checking.","tool_calls":[
            {"id":"call_a","type":"function","function":{"name":"weather","arguments":"{ \"city\": \"Paris\" }"}},
            {"id":"call_b","type":"function","function":{"name":"weather","arguments":"{\"city\":\"Rome\"}"}}
        ]},
        {"role":"tool","tool_call_id":"call_b","content":""},
        {"role":"tool","tool_call_id":"call_a","content":"sunny"}
    ],"tools":[{"type":"function","function":{"name":"weather","description":"","parameters":{"type":"object","properties":{"city":{"type":"string"}}},"strict":false}}],
    "tool_choice":{"type":"function","function":{"name":"weather"}},"parallel_tool_calls":false})
    }
    fn request_wire(d: &DecodedRequest, profile: Profile) -> Value {
        let t = lower_request(&d.semantic, &d.fidelity, profile, Contract::full()).unwrap();
        match profile {
            Profile::Chat => chat::encode_generation(&t).unwrap(),
            Profile::Responses => responses::encode_generation(&t).unwrap(),
        }
    }
    fn expected_responses_history() -> Value {
        json!({"input":[
        {"type":"message","role":"user","content":[{"type":"input_text","text":"weather?"}]},
        {"type":"message","role":"assistant","content":[{"type":"output_text","text":"Checking.","annotations":[]}]},
        {"type":"function_call","call_id":"call_a","name":"weather","arguments":"{ \"city\": \"Paris\" }"},
        {"type":"function_call","call_id":"call_b","name":"weather","arguments":"{\"city\":\"Rome\"}"},
        {"type":"function_call_output","call_id":"call_b","output":""},
        {"type":"function_call_output","call_id":"call_a","output":"sunny"}
    ],"tools":[{"type":"function","name":"weather","description":"","parameters":{"type":"object","properties":{"city":{"type":"string"}}},"strict":false}],
    "tool_choice":{"type":"function","name":"weather"},"parallel_tool_calls":false})
    }
    #[test]
    fn chat_tool_text_arrays_preserve_parts_and_project_only_final_ir() {
        for parts in [
            json!([]),
            json!([{"type":"text","text":""}]),
            json!([{"type":"text","text":"sunny"},{"type":"text","text":""}]),
        ] {
            let mut c = chat_history();
            c["messages"][3]["content"] = parts.clone();
            let mut r = expected_responses_history();
            r["input"][5]["output"] = json!(
                parts
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| json!({"type":"input_text","text":p["text"]}))
                    .collect::<Vec<_>>()
            );
            let mut decoded = chat::decode_generation(&c).unwrap();
            let Item::ToolResult(result) = &decoded.semantic.items()[5].1 else {
                panic!("result");
            };
            let ToolOutput::Parts(values) = &result.output else {
                panic!("parts");
            };
            assert_eq!(result.required_call_id().unwrap(), "call_a");
            assert_eq!(values.len(), parts.as_array().unwrap().len());
            // The same result facts can be reported independently in a response.
            assert!(
                GenerationResponse::new(
                    vec![decoded.semantic.items()[5].clone()],
                    Outcome::Completed
                )
                .is_ok()
            );
            if !values.is_empty() {
                let mut cached = r.clone();
                cached["input"][5]["output"][0]["prompt_cache_breakpoint"] =
                    json!({"mode":"explicit"});
                let cached = responses::decode_generation(&cached).unwrap();
                assert!(
                    lower_request(
                        &cached.semantic,
                        &cached.fidelity,
                        Profile::Chat,
                        Contract::full()
                    )
                    .is_err()
                );
            }
            assert_eq!(request_wire(&decoded, Profile::Chat), c);
            assert_eq!(request_wire(&decoded, Profile::Responses), r);
            let independent = responses::decode_generation(&r).unwrap();
            assert_eq!(request_wire(&independent, Profile::Responses), r);
            // Responses call items have no Chat carrier-message association.
            let projected = request_wire(&responses::decode_generation(&r).unwrap(), Profile::Chat);
            let result = projected["messages"].as_array().unwrap().last().unwrap();
            assert_eq!(result["tool_call_id"], "call_a");
            assert_eq!(result["content"], parts);
            let mut items = decoded.semantic.items().to_vec();
            let Item::ToolResult(result) = &mut items[5].1 else {
                unreachable!()
            };
            result.output = ToolOutput::Parts(vec![
                (PartId::new(900), InputPart::Text(text("replacement"))),
                (PartId::new(901), InputPart::Text(text("inserted"))),
            ]);
            decoded.semantic = decoded.semantic.with_items(items).unwrap();
            assert_eq!(
                request_wire(&decoded, Profile::Chat)["messages"][3]["content"],
                json!([{"type":"text","text":"replacement"},{"type":"text","text":"inserted"}])
            );
            let mut items = decoded.semantic.items().to_vec();
            let Item::ToolResult(result) = &mut items[5].1 else {
                unreachable!()
            };
            result.output = ToolOutput::Parts(vec![]);
            decoded.semantic = decoded.semantic.with_items(items).unwrap();
            assert_eq!(
                request_wire(&decoded, Profile::Chat)["messages"][3]["content"],
                json!([])
            );
        }
        for bad in [
            Value::Null,
            json!([{"type":"image_url","image_url":{"url":"https://example.test/a"}}]),
            json!([{"type":"text","text":1}]),
            json!([{"type":"text","text":"x","extra":true}]),
        ] {
            let mut c = chat_history();
            c["messages"][3]["content"] = bad;
            assert!(chat::decode_generation(&c).is_err());
        }
    }

    #[test]
    fn chat_allowed_functions_use_their_own_shell_and_keep_selection_authoritative() {
        for mode in ["auto", "required"] {
            let mut c = chat_history();
            c["messages"] = json!([{"role":"user","content":"weather?"}]);
            c["tool_choice"] = json!({"type":"allowed_tools","allowed_tools":{"mode":mode,"tools":[{"type":"function","function":{"name":"weather"}}]}});
            let mut r = expected_responses_history();
            r["input"].as_array_mut().unwrap().truncate(1);
            r["tool_choice"] = json!({"type":"allowed_tools","mode":mode,"tools":[{"type":"function","name":"weather"}]});
            let mut d = chat::decode_generation(&c).unwrap();
            assert_eq!(
                d.semantic.tool_choice(),
                Some(&ToolChoice::Allowed {
                    required: mode == "required",
                    tools: vec![ToolReference {
                        kind: ToolKind::Function,
                        name: text("weather"),
                        namespace: None,
                    }]
                })
            );
            assert_eq!(request_wire(&d, Profile::Chat), c);
            assert_eq!(request_wire(&d, Profile::Responses), r);
            assert_eq!(
                request_wire(&responses::decode_generation(&r).unwrap(), Profile::Chat)["tool_choice"],
                c["tool_choice"]
            );
            let mut s = d.semantic.settings().clone();
            s.tool_choice = Some(ToolChoice::None);
            d.semantic = d.semantic.with_settings(s).unwrap();
            assert_eq!(request_wire(&d, Profile::Chat)["tool_choice"], "none");
            let mut s = d.semantic.settings().clone();
            s.tool_choice = None;
            d.semantic = d.semantic.with_settings(s).unwrap();
            assert!(request_wire(&d, Profile::Chat).get("tool_choice").is_none());
            for refs in [
                json!([]),
                json!([{"type":"function","function":{"name":"absent"}}]),
                json!([{"type":"function","function":{"name":"weather"}},{"type":"function","function":{"name":"weather"}}]),
                json!([{"type":"custom","name":"weather"}]),
            ] {
                let mut bad = c.clone();
                bad["tool_choice"]["allowed_tools"]["tools"] = refs;
                assert!(chat::decode_generation(&bad).is_err());
            }
        }
    }

    #[test]
    fn bounded_values_fidelity_collisions_and_codec_profile_mismatch_fail() {
        let mut wire = chat_history();
        wire["messages"][1]["tool_calls"][0]["function"]["arguments"] =
            json!("x".repeat(MAX_TEXT_BYTES + 1));
        assert!(chat::decode_generation(&wire).is_err());
        let mut f = FidelityRecords::default();
        f.record_response_item_id(ItemId::new(1), "same").unwrap();
        assert!(f.record_response_item_id(ItemId::new(2), "same").is_err());
        let d = chat::decode_generation(&chat_history()).unwrap();
        let t = lower_request(&d.semantic, &d.fidelity, Profile::Chat, Contract::full()).unwrap();
        assert!(responses::encode_generation(&t).is_err());
    }

    #[test]
    fn semantic_construction_cannot_bypass_identifier_or_control_validation() {
        let empty = Text::allowing_empty("", "fixture", 1).unwrap();
        let call = Item::ToolCall(ToolCall {
            call_id: empty.clone(),
            name: text("f"),
            arguments: "{}".into(),
            status: ItemLifecycle::Completed,
            context: CallContext::default(),
        });
        assert!(
            GenerationRequest::new(vec![(ItemId::new(1), call)], GenerationControls::default())
                .is_err()
        );
        let d = chat::decode_generation(&chat_history()).unwrap();
        let tool = ToolDefinition::Function(FunctionTool {
            name: empty,
            output_schema: None,
            description: None,
            parameters: None,
            strict: FunctionStrictness::Explicit(false),
            dispatch: ToolDispatch::default(),
        });
        assert!(
            d.semantic
                .clone()
                .with_tools(vec![tool], ToolChoice::Auto)
                .is_err()
        );
        let mut controls = GenerationControls::default();
        controls.max_output_tokens = Some(0);
        assert!(GenerationRequest::new(d.semantic.items().to_vec(), controls).is_err());
    }
    #[test]
    fn active_tool_dispatch_and_instruction_lifecycle_round_trip_without_chat_projection() {
        use morphiecore::protocol::openai::{Adapter, Dialect};
        let adapter = Adapter::new(Profile::Chat, Dialect::MorphieCore, None);
        let decode = |value: &Value| adapter.decode_client_request(value.to_string().as_bytes());
        let mut ignored = chat_history();
        ignored["model"] = json!("m");
        let original = decode(&ignored).unwrap();
        ignored["tools"][0]["function"]["output_schema"] = json!({"type":"string"});
        ignored["messages"][1]["tool_calls"][0]["status"] = json!("in_progress");
        assert_eq!(decode(&ignored).unwrap().task, original.task);
        for key in ["async", "defer_loading", "allowed_callers"] {
            let mut active = ignored.clone();
            active["tools"][0]["function"][key] = if key == "allowed_callers" {
                json!(["programmatic"])
            } else {
                json!(true)
            };
            assert!(decode(&active).is_err(), "{key}");
        }
        let mut legacy = ignored.clone();
        legacy["messages"][1]["function_call"] = json!({"name":"lookup","arguments":"{}"});
        assert!(decode(&legacy).is_err());
        let mut caller = ignored.clone();
        caller["messages"][1]["tool_calls"][0]["caller"] =
            json!({"type":"program","caller_id":"p"});
        assert!(decode(&caller).is_err());
        let mut audio = ignored;
        audio["messages"][0]["audio"] = json!({"id":"a"});
        assert!(decode(&audio).is_err());
        let source = json!({"input":[
        {"type":"message","id":"ins","role":"developer","status":"in_progress","content":"hold"},
        {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}","namespace":"pkg","async":true,"caller":{"type":"program","caller_id":"prog"}}
    ],"tools":[{"type":"function","name":"lookup","async":true,"defer_loading":true,"allowed_callers":["programmatic"]}]});
        let decoded = responses::decode_generation(&source).unwrap();
        let Item::Instruction(instruction) = &decoded.semantic.items()[0].1 else {
            panic!("instruction")
        };
        assert_eq!(instruction.status, Some(ItemLifecycle::InProgress));
        let Item::ToolCall(call) = &decoded.semantic.items()[1].1 else {
            panic!("call")
        };
        assert_eq!(
            call.context.namespace.as_ref().map(|v| v.as_str()),
            Some("pkg")
        );
        assert!(call.context.async_call);
        assert!(matches!(
            call.context.caller,
            Some(CallOrigin::Program { .. })
        ));
        let ToolDefinition::Function(tool) = &decoded.semantic.tools()[0] else {
            panic!("tool")
        };
        assert!(tool.dispatch.async_call && tool.dispatch.defer_loading);
        assert_eq!(
            tool.dispatch.allowed_callers,
            Some(vec![CallerMode::Programmatic])
        );
        let encoded = request_wire(&decoded, Profile::Responses);
        assert_eq!(encoded["input"][0]["status"], json!("in_progress"));
        assert_eq!(encoded["input"][0]["id"], json!("ins"));
        assert_eq!(encoded["input"][1]["namespace"], json!("pkg"));
        assert_eq!(encoded["input"][1]["caller"]["type"], json!("program"));
        assert_eq!(encoded["tools"][0]["defer_loading"], json!(true));
        assert!(
            lower_request(
                &decoded.semantic,
                &decoded.fidelity,
                Profile::Chat,
                Contract::full()
            )
            .is_err()
        );
        let completed =
            responses::decode_generation(&json!({"input":[{"role":"developer","content":"done"}]}))
                .unwrap();
        let Item::Instruction(instruction) = &completed.semantic.items()[0].1 else {
            panic!("complete instruction")
        };
        assert_eq!(instruction.status, None);
        assert!(
            request_wire(&completed, Profile::Responses)["input"][0]
                .get("status")
                .is_none()
        );
    }
    fn program_history() -> Value {
        json!({"input":[
            {"type":"message","role":"user","content":[{"type":"input_text","text":"run"}]},
            {"type":"program","id":"pg_1","call_id":"call_p","code":"const x = 1; return x;","fingerprint":"fp-Ω-1"},
            {"type":"program_output","id":"pg_2","call_id":"call_p","result":"1","status":"completed"}
        ]})
    }
    fn program_items() -> Vec<(ItemId, Item)> {
        vec![
            (
                ItemId::new(1),
                Item::Program(Program {
                    call_id: text("call_p"),
                    code: "const x = 1; return x;".into(),
                    fingerprint: "fp-Ω-1".into(),
                }),
            ),
            (
                ItemId::new(2),
                Item::ProgramOutput(ProgramOutput {
                    call_id: text("call_p"),
                    result: "1".into(),
                    status: ItemLifecycle::Completed,
                }),
            ),
        ]
    }
    #[test]
    fn program_output_requires_terminal_status_and_complete_opaque_fields() {
        for (index, key) in [(1, "fingerprint"), (1, "id"), (2, "status"), (2, "result")] {
            let mut wire = program_history();
            wire["input"][index].as_object_mut().unwrap().remove(key);
            assert!(responses::decode_generation(&wire).is_err());
        }
        for status in ["in_progress", "failed", "queued"] {
            let mut wire = program_history();
            wire["input"][2]["status"] = json!(status);
            assert!(responses::decode_generation(&wire).is_err());
        }
        let mut wire = program_history();
        wire["input"][1]["fingerprint"] = Value::Null;
        assert!(responses::decode_generation(&wire).is_err());
        let mut items = program_items();
        if let Item::ProgramOutput(o) = &mut items[1].1 {
            o.status = ItemLifecycle::InProgress;
        }
        assert!(matches!(
            GenerationRequest::new(items, GenerationControls::default()),
            Err(GenerationError::InvalidProgramOutput)
        ));
    }
    #[test]
    fn program_items_have_no_chat_projection() {
        let d = responses::decode_generation(&program_history()).unwrap();
        assert!(matches!(
            lower_request(&d.semantic, &d.fidelity, Profile::Chat, Contract::full()),
            Err(RepresentationError::Tools)
        ));
        let r = GenerationResponse::new(
            vec![(ItemId::new(1), program_items()[0].1.clone())],
            Outcome::Completed,
        )
        .unwrap();
        assert!(matches!(
            lower_response(
                &r,
                &FidelityRecords::default(),
                &metadata(),
                Profile::Chat,
                Contract::full()
            ),
            Err(RepresentationError::Tools)
        ));
        let mut e = events::EventEncoder::new(Profile::Chat, metadata()).unwrap();
        assert!(
            e.encode(&start(1, program_kind()), &FidelityRecords::default())
                .is_err()
        );
    }
    fn program_kind() -> ItemKind {
        ItemKind::Program {
            call_id: text("call_p"),
            code: "c()".into(),
            fingerprint: "fp-1".into(),
        }
    }
    #[test]
    fn program_items_are_atomic_event_items_with_owner_bound_snapshots() {
        let mut e = vec![StreamEvent::Started, start(1, program_kind())];
        e.push(close(1, ItemLifecycle::Completed));
        e.push(start(
            2,
            ItemKind::ProgramOutput {
                call_id: text("call_p"),
                result: "1".into(),
            },
        ));
        e.push(close(2, ItemLifecycle::Completed));
        e.push(terminal(StreamTerminal::Completed));
        let wire = encode(&e, Profile::Responses, &FidelityRecords::default());
        let added: Vec<_> = wire
            .iter()
            .filter(|v| v["type"] == "response.output_item.added")
            .collect();
        assert_eq!(
            added[0]["item"],
            json!({"id":"item_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"})
        );
        assert_eq!(
            added[1]["item"],
            json!({"id":"item_2","type":"program_output","call_id":"call_p","result":"1"})
        );
        let done: Vec<_> = wire
            .iter()
            .filter(|v| v["type"] == "response.output_item.done")
            .collect();
        assert_eq!(done[1]["item"]["status"], json!("completed"));
        assert_eq!(done[0]["item"].get("status"), None);
        assert_eq!(
            wire.last().unwrap()["response"]["output"][0]["type"],
            json!("program")
        );
        assert!(apply(&e).is_ok());
        let mut dup = e.clone();
        dup.insert(
            2,
            start(
                3,
                ItemKind::Program {
                    call_id: text("call_p"),
                    code: "x".into(),
                    fingerprint: "fp-2".into(),
                },
            ),
        );
        assert!(apply(&dup).is_err());
    }
    #[test]
    fn program_event_stream_decodes_and_changed_or_unfinished_snapshots_fail() {
        let mut d = events::EventDecoder::new(Profile::Responses);
        d.push(&created()).unwrap();
        d.push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"}})).unwrap();
        d.push(&json!({"type":"response.output_item.done","output_index":0,"item":{"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"}})).unwrap();
        d.push(&json!({"type":"response.output_item.added","output_index":1,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1"}})).unwrap();
        d.push(&json!({"type":"response.output_item.done","output_index":1,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1","status":"completed"}})).unwrap();
        let out = json!([
            {"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"},
            {"id":"pg_2","type":"program_output","call_id":"call_p","result":"1","status":"completed"}
        ]);
        d.push(&json!({"type":"response.completed","response":envelope("completed",out)}))
            .unwrap();
        let r = d.materialize().unwrap().semantic;
        assert_eq!(r.outcome(), Outcome::Completed);
        assert_eq!(r.continuation(), Continuation::Unreported);
        assert_eq!(r.items()[1].1, program_items()[1].1);
        let mut d = events::EventDecoder::new(Profile::Responses);
        d.push(&created()).unwrap();
        d.push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"}})).unwrap();
        assert!(d
        .push(&json!({"type":"response.output_item.done","output_index":0,"item":{"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-OTHER"}}))
        .is_err());
        let mut d = events::EventDecoder::new(Profile::Responses);
        d.push(&created()).unwrap();
        d.push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1"}})).unwrap();
        assert!(d
        .push(&json!({"type":"response.output_item.done","output_index":0,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1"}}))
        .is_err());
        let mut d = events::EventDecoder::new(Profile::Responses);
        d.push(&created()).unwrap();
        assert!(d
        .push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1","status":"completed"}}))
        .is_err());
    }
}
mod tool_results {
    //! Tool result kind, artifact lifecycle and wire representability are independent.
    use crate::events_support::text;
    use crate::resources_support as resources;
    use morphiecore::semantic::task::generation::*;
    use serde_json::json;
    fn call() -> (ItemId, Item) {
        (
            ItemId::new(1),
            Item::ToolCall(ToolCall {
                call_id: text("c"),
                name: text("lookup"),
                arguments: "{}".into(),
                status: ItemLifecycle::Completed,
                context: CallContext::default(),
            }),
        )
    }
    struct Payload {
        value: ToolOutput,
        resources: Vec<(u64, Resource)>,
    }
    impl From<ToolOutput> for Payload {
        fn from(value: ToolOutput) -> Self {
            Self {
                value,
                resources: vec![],
            }
        }
    }
    impl From<(ToolOutput, Vec<(u64, Resource)>)> for Payload {
        fn from((value, resources): (ToolOutput, Vec<(u64, Resource)>)) -> Self {
            Self { value, resources }
        }
    }
    fn request(
        output: impl Into<Payload>,
        status: Option<ItemLifecycle>,
    ) -> Result<GenerationRequest, GenerationError> {
        request_with_execution(output, status, None)
    }
    fn request_with_execution(
        output: impl Into<Payload>,
        status: Option<ItemLifecycle>,
        execution: Option<ToolExecution>,
    ) -> Result<GenerationRequest, GenerationError> {
        let output = output.into();
        let table = ResourceTable::new(
            output
                .resources
                .into_iter()
                .map(|(id, resource)| (ResourceId::new(id), resource.location))
                .collect(),
        )?;
        GenerationRequest::from_resources(
            vec![
                call(),
                (
                    ItemId::new(2),
                    Item::ToolResult(ToolResult {
                        is_error: None,
                        execution,
                        call_id: Some(text("c")),
                        name: None,
                        output: output.value,
                        status,
                        context: CallContext::default(),
                    }),
                ),
            ],
            GenerationSettings::default(),
            table,
        )
    }
    fn image(url: &str) -> Resource {
        Resource {
            location: ResourceLocation::Url(text(url)),
            description: ResourceDescription::Image { detail: None },
        }
    }
    fn media(url: &str) -> Payload {
        (
            ToolOutput::Parts(vec![
                (PartId::new(10), InputPart::Text(text("caption"))),
                (PartId::new(11), InputPart::Resource(resources::image(1))),
            ]),
            vec![(1, image(url))],
        )
            .into()
    }
    #[test]
    fn execution_error_is_a_terminal_result_not_a_generation_failure() {
        let error = ToolExecution::Failed {
            code: Some(text("not_found")),
        };
        let payload = ToolOutput::Text("synthetic failure".into());
        assert!(
            request_with_execution(
                payload.clone(),
                Some(ItemLifecycle::InProgress),
                Some(error.clone())
            )
            .is_err()
        );
        let history =
            request_with_execution(payload, Some(ItemLifecycle::Completed), Some(error)).unwrap();
        let response = GenerationResponse::new(vec![call()], Outcome::Completed).unwrap();
        let exchange = ResponseContinuation::new(
            ResponseRelation::new(TurnId::new(1), ResponseId::new(1)),
            &response,
        );
        assert_eq!(
            exchange.inspect(&history).unwrap(),
            ResultReadiness::ResultsComplete
        );
        assert_eq!(response.outcome(), Outcome::Completed);
        assert!(GenerationRequirements::derive(&history).tool_execution_reports);
    }
    #[test]
    fn ordered_media_has_unique_parts_and_distinct_requirements() {
        use morphiecore::protocol::{
            fidelity::FidelityRecords,
            openai::{Profile, TargetCaps, lowering::lower_request, responses},
        };
        let oracle = json!({"input":[
            {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"},
            {"type":"function_call_output","call_id":"c","output":[
                {"type":"input_text","text":"caption"},
                {"type":"input_image","image_url":"https://example.invalid/a.png"}]}]});
        let decoded = responses::decode_generation(&oracle).unwrap();
        let Item::ToolResult(result) = &decoded.semantic.items()[1].1 else {
            panic!("tool result")
        };
        let ToolOutput::Parts(parts) = &result.output else {
            panic!("parts")
        };
        assert_eq!(result.required_call_id().unwrap(), "c");
        assert!(matches!(&parts[0].1, InputPart::Text(t) if t.as_str() == "caption"));
        let InputPart::Resource(resource) = &parts[1].1 else {
            panic!("image")
        };
        let view = resource.media(decoded.semantic.resources()).unwrap();
        assert_eq!(view.kind(), ResourceKind::Image);
        assert!(
            matches!(view.location, ResourceLocation::Url(u) if u.as_str() == "https://example.invalid/a.png")
        );
        // Encode independently built IR, not the decoded request.
        let outbound = request(media("https://example.invalid/a.png"), None).unwrap();
        let fidelity = FidelityRecords::default();
        let target =
            lower_request(&outbound, &fidelity, Profile::Responses, TargetCaps::full()).unwrap();
        assert_eq!(
            responses::encode_generation(&target).unwrap()["input"],
            oracle["input"]
        );
        let history = request(
            media("https://example.invalid/a.png"),
            Some(ItemLifecycle::Incomplete),
        )
        .unwrap();
        let requirements = GenerationRequirements::derive(&history);
        assert_eq!(requirements.tool_result_media, 1);
        assert_eq!(requirements.image_inputs, 1);
        let mut text_only = GenerationSemanticContract::full();
        text_only.image_input = false;
        assert_eq!(
            text_only.check(&history),
            Err(GenerationFeature::ImageInput)
        );
        assert!(
            request(
                ToolOutput::Parts(vec![
                    (PartId::new(10), InputPart::Text(text("one"))),
                    (PartId::new(10), InputPart::Text(text("two")))
                ]),
                None
            )
            .is_err()
        );
        assert!(request(media("https://user:secret@example.invalid/a.png"), None).is_err());
        let mut opaque = image("https://example.invalid/a.png");
        opaque.location = ResourceLocation::OpaqueReference(text("issuer-file"));
        assert!(
            request(
                (
                    ToolOutput::Parts(vec![(
                        PartId::new(10),
                        InputPart::Resource(resources::image(1))
                    )]),
                    vec![(1, opaque)]
                ),
                None
            )
            .is_err()
        );
    }

    #[test]
    fn tool_image_decode_rejects_wrong_branches_and_unsafe_resources() {
        use morphiecore::protocol::openai::responses;
        for part in [
            json!({"type":"input_image","image_url":"https://example.invalid/a.png","text":"cross-kind"}),
            json!({"type":"input_text","text":"caption","image_url":"https://example.invalid/a.png"}),
            json!({"type":"input_image","file_id":"issuer-file"}),
            json!({"type":"input_file","file_id":"issuer-file"}),
            json!({"type":"input_image","image_url":"https://example.invalid/a.png","detail":null}),
            json!({"type":"input_image","image_url":"https://user:secret@example.invalid/a.png"}),
            json!({"type":"input_image","image_url":"file:///etc/passwd"}),
            json!({"type":"input_image","image_url":"data:image/png;base64,???"}),
        ] {
            let wire = json!({"input":[
                {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"},
                {"type":"function_call_output","call_id":"c","output":[part]}
            ]});
            assert!(responses::decode_generation(&wire).is_err());
        }
    }
}
mod tool_result_errors {
    //! Error reports and execution facts have independent presence and admission.
    use morphiecore::{
        protocol::openai::{Profile, chat::envelope as chat_envelope, responses::envelope},
        semantic::{task::generation::*, value::Text},
    };

    fn text(s: &str) -> Text {
        Text::new(s, "synthetic", 256).unwrap()
    }
    fn history(
        is_error: Option<bool>,
        execution: Option<ToolExecution>,
    ) -> Result<GenerationRequest, GenerationError> {
        GenerationRequest::new(
            vec![
                (
                    ItemId::new(1),
                    Item::ToolCall(ToolCall {
                        call_id: text("c"),
                        name: text("lookup"),
                        arguments: "{}".into(),
                        status: ItemLifecycle::Completed,
                        context: Default::default(),
                    }),
                ),
                (
                    ItemId::new(2),
                    Item::ToolResult(ToolResult {
                        call_id: Some(text("c")),
                        name: None,
                        output: "actual feedback".into(),
                        is_error,
                        execution,
                        status: None,
                        context: Default::default(),
                    }),
                ),
            ],
            GenerationControls::default(),
        )
    }
    #[test]
    fn not_executed_and_execution_failure_remain_distinct_with_the_same_feedback() {
        for fact in [
            ToolExecution::NotExecuted,
            ToolExecution::Failed { code: None },
        ] {
            let request = history(Some(true), Some(fact.clone())).unwrap();
            let Item::ToolResult(result) = &request.items()[1].1 else {
                panic!()
            };
            assert_eq!(result.execution, Some(fact));
            assert_eq!(result.output, ToolOutput::Text("actual feedback".into()));
        }
        assert!(history(Some(true), Some(ToolExecution::Succeeded)).is_err());
        assert!(history(Some(false), Some(ToolExecution::Failed { code: None })).is_err());
        assert!(history(None, Some(ToolExecution::Failed { code: None })).is_ok());
    }
    #[test]
    fn standard_codecs_cannot_silently_omit_an_error_report() {
        for profile in [Profile::Chat, Profile::Responses] {
            let body = match profile {
            Profile::Chat => br#"{"model":"synthetic","messages":[{"role":"assistant","tool_calls":[{"id":"c","type":"function","function":{"name":"lookup","arguments":"{}"}}]},{"role":"tool","tool_call_id":"c","content":"feedback"}]}"#.as_slice(),
            Profile::Responses => br#"{"model":"synthetic","input":[{"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"},{"type":"function_call_output","call_id":"c","output":"feedback"}]}"#.as_slice(),
        };
            let mut decoded = match profile {
                Profile::Chat => chat_envelope::decode_request_bytes(body).unwrap().task,
                Profile::Responses => envelope::decode_request_bytes(body).unwrap().task,
            };
            let mut items = decoded.semantic.items().to_vec();
            let result = items
                .iter_mut()
                .find_map(|(_, item)| match item {
                    Item::ToolResult(result) => Some(result),
                    _ => None,
                })
                .unwrap();
            result.is_error = Some(false);
            decoded.semantic = decoded.semantic.with_items(items).unwrap();
            assert!(
                morphiecore::protocol::openai::lowering::lower_request(
                    &decoded.semantic,
                    &decoded.fidelity,
                    profile,
                    morphiecore::protocol::openai::TargetCaps::full(),
                )
                .is_err()
            );
        }
    }
}
mod tool_namespaces {
    //! Namespace expectations are independent of the encoder and provider catalog.
    use morphiecore::{
        protocol::openai::{Profile, TargetCaps as Contract, lowering::lower_request, responses},
        semantic::{task::generation::*, value::Text},
    };
    use serde_json::json;

    #[test]
    fn grouped_definitions_and_qualified_selection_are_not_flattened() {
        let wire = json!({"input":[{"role":"user","content":"check"}],
        "tools":[
            {"type":"namespace","name":"left","description":"first","tools":[
                {"type":"function","name":"status","strict":false}]},
            {"type":"namespace","name":"right","description":"second","tools":[
                {"type":"function","name":"status","strict":false}]}],
        "tool_choice":"required"});
        let decoded = responses::decode_generation(&wire).expect("standard namespace");
        let representation = lower_request(
            &decoded.semantic,
            &decoded.fidelity,
            Profile::Responses,
            Contract::full(),
        )
        .unwrap();
        let encoded = responses::encode_generation(&representation).unwrap();
        assert_eq!(encoded["tools"], wire["tools"]);
        assert_eq!(encoded["tool_choice"], wire["tool_choice"]);
        assert!(
            lower_request(
                &decoded.semantic,
                &decoded.fidelity,
                Profile::Chat,
                Contract::full(),
            )
            .is_err()
        );
        let mut settings = decoded.semantic.settings().clone();
        settings.tool_choice = Some(ToolChoice::Qualified(ToolReference {
            kind: ToolKind::Function,
            name: Text::new("status", "name", 128).unwrap(),
            namespace: Some(Text::new("right", "namespace", 128).unwrap()),
        }));
        let selected = decoded
            .semantic
            .clone()
            .with_settings(settings.clone())
            .unwrap();
        assert!(
            lower_request(
                &selected,
                &decoded.fidelity,
                Profile::Responses,
                Contract::full()
            )
            .is_err()
        );
        settings.tools.as_mut().unwrap().pop();
        assert!(decoded.semantic.with_settings(settings).is_err());
    }

    #[test]
    fn namespace_rejects_duplicates_nested_groups_and_unqualified_selection() {
        let group = json!({"type":"namespace","name":"ops","description":"operations",
        "tools":[{"type":"function","name":"status","strict":false}]});
        let wire = json!({"input":"check","tools":[group.clone()], "tool_choice":"auto"});
        assert!(responses::decode_generation(&wire).is_ok());
        let mut duplicate = wire.clone();
        duplicate["tools"][0]["tools"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type":"function","name":"status","strict":false}));
        assert!(responses::decode_generation(&duplicate).is_err());
        let mut nested = wire.clone();
        nested["tools"][0]["tools"] = json!([group]);
        assert!(responses::decode_generation(&nested).is_err());
        let mut missing = wire;
        missing["tool_choice"] = json!({"type":"function","name":"status"});
        assert!(responses::decode_generation(&missing).is_err());
        missing["tool_choice"]["namespace"] = json!("ops");
        assert!(responses::decode_generation(&missing).is_err());
    }
}
mod function_events {
    //! Function argument, mixed-item identity, snapshot and terminal conformance.
    use crate::events_support::*;
    use morphiecore::{
        protocol::{
            fidelity::FidelityRecords,
            openai::{
                Profile,
                events::{EventDecoder, EventEncoder},
            },
        },
        semantic::task::generation::*,
    };
    use serde_json::json;

    #[test]
    fn function_done_name_echo_must_match_its_existing_call_owner() {
        for name in [json!("lookup"), json!("different"), json!(null)] {
            let mut decoder = EventDecoder::new(Profile::Responses);
            decoder.push(&created()).unwrap();
            decoder.push(&json!({"type":"response.output_item.added","output_index":0,"item":call_item("fc","c","{}","in_progress")})).unwrap();
            let result=decoder.push(&json!({"type":"response.function_call_arguments.done","output_index":0,"item_id":"fc","name":name,"arguments":"{}"}));
            assert_eq!(result.is_ok(), name == json!("lookup"));
            if result.is_err() {
                assert!(decoder.finish().is_err());
            }
        }
    }

    #[test]
    fn function_done_supplies_missing_argument_suffix_but_never_rewrites() {
        // Some Responses servers stream a call's arguments partly or not at all.
        for (deltas, suffix) in [
            (vec![], Some(r#"{"a":1}"#)),
            (vec![r#"{"a":"#], Some("1}")),
            (vec![r#"{"a":1}"#], None),
            (vec!["{}"], None),
        ] {
            let mut decoder = EventDecoder::new(Profile::Responses);
            decoder.push(&created()).unwrap();
            decoder.push(&json!({"type":"response.output_item.added","output_index":0,"item":call_item("fc","c","","in_progress")})).unwrap();
            for delta in &deltas {
                decoder.push(&json!({"type":"response.function_call_arguments.delta","output_index":0,"item_id":"fc","delta":delta})).unwrap();
            }
            let done = decoder.push(&json!({"type":"response.function_call_arguments.done","output_index":0,"item_id":"fc","arguments":r#"{"a":1}"#}));
            if deltas == ["{}"] {
                assert!(done.is_err(), "a conflicting done is rejected");
                continue;
            }
            let emitted: Vec<_> = done
                .unwrap()
                .into_iter()
                .filter_map(|event| match event {
                    StreamEvent::Delta { fragment, .. } => Some(fragment),
                    _ => None,
                })
                .collect();
            assert_eq!(
                emitted,
                suffix.into_iter().map(String::from).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn non_success_terminals_keep_partial_output_and_error_is_not_materializable() {
        for t in [
            StreamTerminal::Incomplete,
            StreamTerminal::Failed,
            StreamTerminal::Cancelled,
        ] {
            let events = vec![
                StreamEvent::Started,
                start(
                    1,
                    ItemKind::ToolCall {
                        format: ArgumentFormat::Raw,
                        call_id: text("a"),
                        name: text("f"),
                        context: CallContext::default(),
                    },
                ),
                StreamEvent::PartStarted {
                    item: ItemId::new(1),
                    part: PartId::new(1),
                    kind: PartKind::Arguments,
                },
                StreamEvent::Delta {
                    item: ItemId::new(1),
                    part: PartId::new(1),
                    fragment: "{".into(),
                    logprobs: vec![],
                },
                terminal(t),
            ];
            if t == StreamTerminal::Cancelled {
                // Cancellation still materializes partial typed output, but cannot
                // invent a standard wire event to deliver that snapshot.
                let response = materialize(&apply(&events).unwrap()).unwrap();
                assert_eq!(response.outcome(), Outcome::Cancelled);
                let Item::ToolCall(call) = &response.items()[0].1 else {
                    panic!("partial call")
                };
                assert_eq!(call.arguments.as_raw(), Some("{"));
                assert_eq!(response.continuation(), Continuation::Unreported);
                let mut encoder = EventEncoder::new(Profile::Responses, metadata()).unwrap();
                let source = FidelityRecords::default();
                for event in &events[..events.len() - 1] {
                    encoder.encode(event, &source).unwrap();
                }
                assert!(encoder.encode(events.last().unwrap(), &source).is_err());
                assert!(encoder.finish().is_err());
                continue;
            }
            let wire = encode(&events, Profile::Responses, &FidelityRecords::default());
            assert_eq!(
                wire.last().unwrap()["response"]["output"][0]["arguments"],
                "{"
            );
            assert_eq!(
                wire.last().unwrap()["response"]["output"][0]["status"],
                "in_progress"
            );
            let mut d = EventDecoder::new(Profile::Responses);
            for v in wire {
                d.push(&v).unwrap();
            }
            let response = d.materialize().unwrap().semantic;
            assert_ne!(response.outcome(), Outcome::Completed);
            assert_eq!(response.continuation(), Continuation::Unreported);
        }
        let mut d = EventDecoder::new(Profile::Responses);
        d.push(
            &json!({"type":"error","code":"server_error","message":"synthetic error","param":null}),
        )
        .unwrap();
        assert!(d.finish().is_ok());
        assert!(d.materialize().is_err());
    }
    #[test]
    fn event_encoder_rejects_duplicate_terminal_and_unrepresentable_chat_grouping() {
        let mut e = EventEncoder::new(Profile::Responses, metadata()).unwrap();
        let f = FidelityRecords::default();
        e.encode(&StreamEvent::Started, &f).unwrap();
        e.encode(&terminal(StreamTerminal::Completed), &f).unwrap();
        assert!(e.encode(&terminal(StreamTerminal::Completed), &f).is_err());
        let mut e = EventEncoder::new(Profile::Chat, metadata()).unwrap();
        e.encode(&StreamEvent::Started, &f).unwrap();
        e.encode(&start(1, ItemKind::Message { phase: None }), &f)
            .unwrap();
        assert!(
            e.encode(&start(2, ItemKind::Message { phase: None }), &f)
                .is_err()
        );
    }
}
mod structured_events {
    //! Structured builders become one JSON authority, never a repaired string at terminal.
    use crate::events_support::{self as events, text};
    use morphiecore::semantic::task::generation::*;
    fn opening() -> Vec<StreamEvent> {
        vec![
            StreamEvent::Started,
            events::start(
                1,
                ItemKind::ToolCall {
                    format: ArgumentFormat::Json,
                    call_id: text("c"),
                    name: text("lookup"),
                    context: CallContext::default(),
                },
            ),
            StreamEvent::PartStarted {
                item: ItemId::new(1),
                part: PartId::new(2),
                kind: PartKind::StructuredArguments,
            },
        ]
    }
    fn delta(fragment: &str) -> StreamEvent {
        StreamEvent::Delta {
            item: ItemId::new(1),
            part: PartId::new(2),
            fragment: fragment.into(),
            logprobs: vec![],
        }
    }
    #[test]
    fn incomplete_or_invalid_json_never_becomes_a_completed_argument() {
        for fragment in ["{", "{\"x\":1,\"x\":2}"] {
            let mut input = opening();
            input.push(delta(fragment));
            let state = events::apply(&input).unwrap();
            assert!(
                reduce(
                    state.clone(),
                    StreamEvent::ValueFinished {
                        item: ItemId::new(1),
                        part: PartId::new(2)
                    }
                )
                .is_err()
            );
            assert!(reduce(state, events::terminal(StreamTerminal::Completed)).is_err());
        }
        let mut input = opening();
        input.push(delta("{"));
        input.push(events::close(1, ItemLifecycle::Incomplete));
        input.push(events::terminal(StreamTerminal::Incomplete));
        let response = materialize(&events::apply(&input).unwrap()).unwrap();
        assert!(
            matches!(&response.items()[0].1,Item::ToolCall(c) if matches!(&c.arguments,ToolArguments::StructuredPartial(s) if s=="{"))
        );
        assert_eq!(response.continuation(), Continuation::Unreported);
    }
}
mod continuation {
    //! Response closure and pending tool results are independent; neither executes tools.
    use crate::events_support::{call_item, envelope};
    use morphiecore::{
        protocol::openai::{Adapter, Dialect, Profile, TargetCaps as Contract},
        semantic::task::generation::*,
    };
    use serde_json::{Value, json};

    fn adapter() -> Adapter {
        Adapter::new(Profile::Responses, Dialect::Standard, None)
    }
    fn decode(status: &str, output: Value) -> morphiecore::protocol::openai::DecodedResponse {
        adapter()
            .decode_response(envelope(status, output).to_string().as_bytes())
            .unwrap()
    }
    fn calls(response: &GenerationResponse) -> Vec<(ItemId, &str)> {
        match response.continuation() {
            Continuation::Unreported => vec![],
            Continuation::ToolResults(calls) => calls
                .into_iter()
                .map(|call| (call.item, call.call_id))
                .collect(),
        }
    }
    fn program_pair() -> Value {
        json!([
            {"id":"p","type":"program","call_id":"pc","code":"opaque code","fingerprint":"opaque fingerprint"},
            {"id":"po","type":"program_output","call_id":"pc","result":"done","status":"completed"}
        ])
    }

    #[test]
    fn partial_or_failed_responses_do_not_offer_completed_tool_continuation() {
        for status in ["incomplete", "failed", "cancelled"] {
            let mut wire = envelope(status, json!([call_item("a", "ca", "{", "incomplete")]));
            if status == "incomplete" {
                wire["incomplete_details"] = json!({"reason":"max_output_tokens"});
            }
            if status == "failed" {
                wire["error"] = json!({"code":"synthetic_failure","message":"failed"});
            }
            wire["usage"] = json!({"input_tokens":3,"output_tokens":2,"total_tokens":5});
            let mut decoded = adapter()
                .decode_response(wire.to_string().as_bytes())
                .unwrap();
            let usage = decoded.semantic.usage();
            let outcome = decoded.semantic.outcome();
            let details = decoded.semantic.details().clone();
            assert_eq!(decoded.semantic.continuation(), Continuation::Unreported);
            decoded.semantic = decoded.semantic.with_items(vec![]).unwrap();
            assert_eq!(decoded.semantic.outcome(), outcome);
            assert_eq!(decoded.semantic.details(), &details);
            assert_eq!(decoded.semantic.usage(), usage);
            assert_eq!(decoded.semantic.continuation(), Continuation::Unreported);
        }
        let call = decode(
            "incomplete",
            json!([call_item("a", "ca", "{", "incomplete")]),
        );
        assert!(
            GenerationResponse::new(call.semantic.items().to_vec(), Outcome::Completed).is_err()
        );
    }

    #[test]
    fn program_results_resolve_by_identity_and_edits_recompute_dependencies() {
        let mut output = program_pair().as_array().unwrap().clone();
        output.insert(
            1,
            json!({"id":"q","type":"program","call_id":"qc","code":"second","fingerprint":"fp"}),
        );
        let mut decoded = decode("completed", json!(output));
        assert_eq!(calls(&decoded.semantic), [(ItemId::new(2), "qc")]);
        let mut items = decoded.semantic.items().to_vec();
        let Item::ProgramOutput(result) = &mut items[2].1 else {
            panic!("program output")
        };
        result.call_id = crate::events_support::text("qc");
        decoded.semantic = decoded.semantic.with_items(items).unwrap();
        assert_eq!(calls(&decoded.semantic), [(ItemId::new(1), "pc")]);
        let items = decoded.semantic.items()[..2].to_vec();
        decoded.semantic = decoded.semantic.with_items(items).unwrap();
        assert_eq!(
            calls(&decoded.semantic),
            [(ItemId::new(1), "pc"), (ItemId::new(2), "qc")]
        );
    }

    #[test]
    fn chat_retains_its_wire_finish_consistency_after_outcome_separation() {
        let chat = Adapter::new(Profile::Chat, Dialect::Standard, None);
        for (finish, message) in [
            (
                "stop",
                json!({"role":"assistant","content":null,"tool_calls":[{"id":"c","type":"function","function":{"name":"lookup","arguments":"{}"}}]}),
            ),
            ("tool_calls", json!({"role":"assistant","content":"done"})),
        ] {
            let wire = json!({"id":"r","object":"chat.completion","created":0,"model":"synthetic","choices":[{"index":0,"message":message,"finish_reason":finish}]});
            assert!(chat.decode_response(wire.to_string().as_bytes()).is_err());
        }
        let decoded = decode(
            "completed",
            json!([call_item("a", "ca", "{}", "completed")]),
        );
        let encoded = chat.encode_response(&decoded, &Contract::full()).unwrap();
        assert_eq!(encoded["choices"][0]["finish_reason"], "tool_calls");
    }
}
mod turn_continuation {
    //! Explicit local relations never manufacture wire identity or execution permission.
    use crate::events_support::text;
    use morphiecore::semantic::task::generation::*;

    fn call(id: u64, name: &str) -> (ItemId, Item) {
        (
            ItemId::new(id),
            Item::ToolCall(ToolCall {
                call_id: text(name),
                name: text("lookup"),
                arguments: "{}".into(),
                status: ItemLifecycle::Completed,
                context: CallContext::default(),
            }),
        )
    }
    fn result(id: u64, call_id: &str, status: ItemLifecycle) -> (ItemId, Item) {
        (
            ItemId::new(id),
            Item::ToolResult(ToolResult {
                is_error: None,
                execution: None,
                call_id: Some(text(call_id)),
                name: None,
                output: "done".into(),
                status: Some(status),
                context: CallContext::default(),
            }),
        )
    }
    fn history(items: Vec<(ItemId, Item)>) -> GenerationRequest {
        GenerationRequest::new(items, GenerationControls::default()).unwrap()
    }
    fn relation() -> ResponseRelation {
        ResponseRelation::new(TurnId::new(7), ResponseId::new(10))
    }

    #[test]
    fn parallel_exchange_distinguishes_pending_partial_and_complete_results() {
        let response =
            GenerationResponse::new(vec![call(1, "a"), call(2, "b")], Outcome::Completed).unwrap();
        let exchange = ResponseContinuation::new(relation(), &response);
        let request = history(response.items().to_vec());
        assert!(
            matches!(exchange.inspect(&request).unwrap(), ResultReadiness::Awaiting { ref missing, ref in_progress } if missing.len() == 2 && in_progress.is_empty())
        );
        let mut items = request.items().to_vec();
        items.push(result(3, "b", ItemLifecycle::InProgress));
        let partial = history(items.clone());
        assert!(matches!(
            exchange.inspect(&partial).unwrap(),
            ResultReadiness::Awaiting { ref missing, ref in_progress }
                if missing[0].call_id == "a" && in_progress[0].call_id == "b"
        ));
        assert!(
            exchange
                .advance(&partial, ResponseId::new(11), &response)
                .is_err()
        );
        let Item::ToolResult(r) = &mut items[2].1 else {
            panic!("result")
        };
        r.status = Some(ItemLifecycle::Incomplete);
        let request = history(items.clone());
        assert!(
            matches!(exchange.inspect(&request).unwrap(), ResultReadiness::Awaiting { ref missing, ref in_progress } if missing[0].call_id == "a" && in_progress.is_empty())
        );
        items.push(result(4, "a", ItemLifecycle::Completed));
        let request = history(items);
        assert_eq!(
            exchange.inspect(&request).unwrap(),
            ResultReadiness::ResultsComplete
        );
        let next_response =
            GenerationResponse::new(vec![call(5, "c")], Outcome::Completed).unwrap();
        let next = exchange
            .advance(&request, ResponseId::new(11), &next_response)
            .unwrap();
        assert_eq!(next.relation().turn(), TurnId::new(7));
        assert_eq!(next.relation().previous(), Some(ResponseId::new(10)));
        assert_eq!(response.outcome(), Outcome::Completed);
        assert!(
            matches!(response.continuation(), Continuation::ToolResults(ref calls) if calls.len() == 2)
        );
        let mut items = request.items().to_vec();
        items.extend_from_slice(next_response.items());
        assert!(
            matches!(next.inspect(&history(items)).unwrap(), ResultReadiness::Awaiting { ref missing, .. } if missing[0].call_id == "c")
        );
        assert!(
            next.advance(&request, ResponseId::new(10), &next_response)
                .is_err()
        );
    }
}

mod reported_tool_results {
    use crate::events_support::{
        apply, close, created, encode, envelope, metadata, start, terminal, text,
    };
    use morphiecore::{
        protocol::{
            anthropic::{client::encode_client_response, client_events::ClientEventEncoder},
            fidelity::FidelityRecords,
            openai::{
                Profile, TargetCaps,
                events::{EventDecoder, EventEncoder},
                lowering::lower_response,
                responses,
            },
        },
        semantic::task::generation::*,
    };
    use serde_json::{Value, json};

    fn result() -> Value {
        json!({"id":"out_1","type":"function_call_output","call_id":"c",
            "name":"lookup","output":"42","status":"completed"})
    }
    fn custom_result() -> Value {
        json!({"id":"out_2","type":"custom_tool_call_output","call_id":"custom",
            "output":[{"type":"input_text","text":"caption"},
                {"type":"input_image","image_url":"https://example.invalid/result.png"}],
            "status":"completed"})
    }
    fn semantic() -> GenerationResponse {
        let resources = ResourceTable::default()
            .insert(
                ResourceId::new(1),
                ResourceLocation::Url(text("https://example.invalid/result.png")),
            )
            .unwrap();
        GenerationResponse::from_resources(
            vec![
                (
                    ItemId::new(1),
                    Item::ToolResult(ToolResult {
                        call_id: Some(text("c")),
                        name: Some(text("lookup")),
                        output: "42".into(),
                        status: Some(ItemLifecycle::Completed),
                        is_error: None,
                        execution: None,
                        context: Default::default(),
                    }),
                ),
                (
                    ItemId::new(2),
                    Item::CustomResult(ToolResult {
                        call_id: Some(text("custom")),
                        name: None,
                        output: ToolOutput::Parts(vec![
                            (PartId::new(1), InputPart::Text(text("caption"))),
                            (
                                PartId::new(2),
                                InputPart::Resource(crate::resources_support::image(1)),
                            ),
                        ]),
                        status: Some(ItemLifecycle::Completed),
                        is_error: None,
                        execution: None,
                        context: Default::default(),
                    }),
                ),
            ],
            Outcome::Completed,
            resources,
        )
        .unwrap()
    }
    fn fidelity() -> FidelityRecords {
        let mut f = FidelityRecords::default();
        f.record_response_item_id(ItemId::new(1), "out_1").unwrap();
        f.record_response_item_id(ItemId::new(2), "out_2").unwrap();
        f
    }

    #[test]
    fn reported_results_preserve_facts_and_reject_missing_downstream_carriers() {
        let output = json!([result(), custom_result()]);
        let decoded = responses::decode_response(&envelope("completed", output.clone())).unwrap();
        assert_eq!(decoded.semantic, semantic());
        // Encode independent IR, not the decoded response.
        let expected = semantic();
        let f = fidelity();
        let m = metadata();
        let target =
            lower_response(&expected, &f, &m, Profile::Responses, TargetCaps::full()).unwrap();
        assert_eq!(
            responses::encode_response(&target).unwrap()["output"],
            output
        );
        let mut no_media = TargetCaps::full();
        no_media.semantics.tool_result_media = false;
        assert!(lower_response(&expected, &f, &m, Profile::Responses, no_media).is_err());

        // Even a plain function result has no assistant-output carrier in these protocols.
        let decoded =
            responses::decode_response(&envelope("completed", json!([result()]))).unwrap();
        assert!(
            lower_response(
                &decoded.semantic,
                &decoded.fidelity,
                &decoded.metadata,
                Profile::Chat,
                TargetCaps::full()
            )
            .is_err()
        );
        assert!(encode_client_response(&decoded, "synthetic").is_err());

        let mut unlinked = result();
        unlinked.as_object_mut().unwrap().remove("call_id");
        let reported =
            responses::decode_response(&envelope("completed", json!([unlinked.clone()]))).unwrap();
        let Item::ToolResult(value) = &reported.semantic.items()[0].1 else {
            panic!("result")
        };
        assert_eq!(value.call_id, None);
        assert_eq!(value.name.as_ref().map(|n| n.as_str()), Some("lookup"));
        assert!(responses::decode_generation(&json!({"input":[unlinked.clone()]})).is_err());
        unlinked["status"] = json!("in_progress");
        assert!(responses::decode_response(&envelope("completed", json!([unlinked]))).is_err());

        let mut history = vec![
            (
                ItemId::new(3),
                Item::ToolCall(ToolCall {
                    call_id: text("c"),
                    name: text("lookup"),
                    arguments: "{}".into(),
                    status: ItemLifecycle::Completed,
                    context: Default::default(),
                }),
            ),
            (
                ItemId::new(4),
                Item::CustomCall(CustomCall {
                    call_id: text("custom"),
                    name: text("run"),
                    input: "raw".into(),
                    context: Default::default(),
                }),
            ),
        ];
        history.extend_from_slice(expected.items());
        let request = GenerationRequest::from_resources(
            history,
            GenerationSettings::default(),
            expected.resources().clone(),
        )
        .unwrap();
        let target = morphiecore::protocol::openai::lowering::lower_request(
            &request,
            &f,
            Profile::Responses,
            TargetCaps::full(),
        )
        .unwrap();
        let replay = responses::encode_generation(&target).unwrap();
        assert_eq!(
            &replay["input"].as_array().unwrap()[2..],
            output.as_array().unwrap()
        );
        assert!(responses::decode_generation(&replay).is_ok());
    }

    #[test]
    fn result_stream_uses_item_snapshots_and_requires_honest_closure() {
        let final_item = result();
        let mut added = final_item.clone();
        added["status"] = json!("in_progress");
        added["output"] = json!("");
        let open = json!({"type":"response.output_item.added","output_index":0,"item":added});
        let done = json!({"type":"response.output_item.done","output_index":0,"item":final_item});
        let final_event = json!({"type":"response.completed",
            "response":envelope("completed", json!([result()]))});
        let mut decoder = EventDecoder::new(Profile::Responses);
        for event in [created(), open.clone(), done.clone(), final_event.clone()] {
            decoder.push(&event).unwrap();
        }
        let decoded = decoder.materialize().unwrap();
        let Item::ToolResult(reported) = &decoded.semantic.items()[0].1 else {
            panic!("result");
        };
        assert_eq!(reported.output, ToolOutput::Text("42".into()));
        assert_eq!(reported.status, Some(ItemLifecycle::Completed));

        let mut decoder = EventDecoder::new(Profile::Responses);
        decoder.push(&created()).unwrap();
        decoder.push(&open).unwrap();
        assert!(decoder.push(&final_event).is_err());
        let mut decoder = EventDecoder::new(Profile::Responses);
        decoder.push(&created()).unwrap();
        decoder.push(&open).unwrap();
        let mut changed = done;
        changed["item"]["call_id"] = json!("other");
        assert!(decoder.push(&changed).is_err());

        let expected = semantic();
        let mut events = vec![StreamEvent::Started];
        for (id, item) in expected.items() {
            let (Item::ToolResult(value) | Item::CustomResult(value)) = item else {
                unreachable!()
            };
            let mut initial = value.clone();
            initial.status = Some(ItemLifecycle::InProgress);
            let kind = if matches!(item, Item::CustomResult(_)) {
                events.push(StreamEvent::ResourceDeclared {
                    id: ResourceId::new(1),
                    resource: ResourceLocation::Url(text("https://example.invalid/result.png")),
                });
                ItemKind::CustomResult(initial)
            } else {
                initial.output = "".into();
                ItemKind::ToolResult(initial)
            };
            events.push(StreamEvent::ItemStarted {
                item: *id,
                kind,
                replay: None,
            });
            events.push(StreamEvent::ToolResultOutput {
                item: *id,
                output: value.output.clone(),
            });
            events.push(close(id.get(), ItemLifecycle::Completed));
        }
        events.push(terminal(StreamTerminal::Completed));
        assert_eq!(materialize(&apply(&events).unwrap()).unwrap(), expected);
        let wire = encode(&events, Profile::Responses, &fidelity());
        let finished: Vec<_> = wire
            .iter()
            .filter(|v| v["type"] == "response.output_item.done")
            .map(|v| v["item"].clone())
            .collect();
        assert_eq!(finished, vec![result(), custom_result()]);
        assert!(
            !wire
                .iter()
                .any(|v| v["type"].as_str().unwrap().ends_with(".delta"))
        );
        let mut decoder = EventDecoder::new(Profile::Responses);
        for event in wire {
            decoder.push(&event).unwrap();
        }
        assert_eq!(decoder.materialize().unwrap().semantic, expected);

        let starter = events[1].clone();
        let mut chat = EventEncoder::new(Profile::Chat, metadata()).unwrap();
        chat.encode(&StreamEvent::Started, &fidelity()).unwrap();
        assert!(chat.encode(&starter, &fidelity()).is_err());
        let mut messages = ClientEventEncoder::new("r", "synthetic");
        messages.encode(&StreamEvent::Started).unwrap();
        assert!(messages.encode(&starter).is_err());
        let closed = apply(&events[..events.len() - 1]).unwrap();
        assert!(
            reduce(
                closed.clone(),
                StreamEvent::ToolResultOutput {
                    item: ItemId::new(1),
                    output: "late".into(),
                }
            )
            .is_err()
        );
        let mut initial = match &expected.items()[0].1 {
            Item::ToolResult(r) => r.clone(),
            _ => unreachable!(),
        };
        initial.status = Some(ItemLifecycle::InProgress);
        assert!(reduce(closed, start(3, ItemKind::ToolResult(initial))).is_err());
    }
}
