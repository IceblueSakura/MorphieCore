//! Native Messages wire shapes and semantic mapping.
mod anthropic_profile {
    //! Independent native wire shapes; no registry, credentials or semantic round-trip oracle.
    use morphiecore::protocol::{CodecError, anthropic::Profile};
    use serde_json::{Value, json};

    fn request() -> Value {
        json!({"model":"synthetic-model","max_tokens":512,"system":"Use lookup.",
        "tools":[{"name":"lookup","input_schema":{"type":"object",
            "properties":{"key":{"type":"string","enum":["A","B"]}},
            "required":["key"],"additionalProperties":false},"strict":false}],
        "tool_choice":{"type":"auto"},"messages":[{"role":"user","content":"Look up A."}]})
    }
    /// Strict tools follow the native structured-output subset and request-wide grammar
    /// budget; nothing is simplified to fit.
    #[test]
    fn strict_tool_schemas_admit_only_the_native_subset_and_budget() {
        let p = Profile::AdaptiveTextTools;
        let encode = |tools: Vec<Value>, output: Option<Value>| {
            let mut body = request();
            body["tools"] = json!(tools);
            if let Some(schema) = output {
                body["output_config"] = json!({"format":{"type":"json_schema","schema":schema}});
            }
            let decoded = p.decode_request(body.to_string().as_bytes()).unwrap();
            p.encode_request(&decoded)
        };
        let tool = |schema: Value| json!({"name":"lookup","input_schema":schema,"strict":true});
        let object = |properties: Value, required: Vec<String>| json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
        let admitted = |result: Result<Value, CodecError>| result.is_ok();
        let rejected = |result: Result<Value, CodecError>| matches!(result, Err(CodecError::Unsupported(r)) if r == "Anthropic strict tool schema");
        let rich = object(
            json!({
                "email":{"type":["string","null"],"format":"email"},
                "count":{"anyOf":[{"type":"integer"},{"type":"null"}]},
                "tags":{"type":"array","items":{"type":"number"},"minItems":1}
            }),
            vec!["email".into(), "count".into(), "tags".into()],
        );
        assert!(admitted(encode(vec![tool(rich.clone())], None)));
        for (path, value) in [
            ("/properties/email/format", json!("binary")),
            ("/properties/email/type", json!([])),
            ("/properties/email/pattern", json!("^a")),
            ("/properties/email/enum", json!(["a"])),
            ("/properties/count/anyOf", json!([])),
            ("/properties/tags/minItems", json!(2)),
            ("/properties/tags/items", json!({"$ref":"#/$defs/n"})),
            ("/additionalProperties", json!(true)),
        ] {
            let mut schema = rich.clone();
            let (parent, key) = path.rsplit_once('/').unwrap();
            let target = if parent.is_empty() {
                &mut schema
            } else {
                schema.pointer_mut(parent).unwrap()
            };
            target[key] = value;
            assert!(rejected(encode(vec![tool(schema)], None)), "{path}");
        }

        let optional = |n: usize| {
            object(
                (0..n)
                    .map(|i| (format!("p{i}"), json!({"type":"string"})))
                    .collect::<serde_json::Map<_, _>>()
                    .into(),
                vec![],
            )
        };
        assert!(admitted(encode(vec![tool(optional(24))], None)));
        assert!(rejected(encode(vec![tool(optional(25))], None)));
        // The budget is request-wide: output schemas count beside strict tools.
        assert!(rejected(encode(
            vec![tool(optional(20))],
            Some(optional(5))
        )));
        let unions = |n: usize| {
            let names: Vec<String> = (0..n).map(|i| format!("u{i}")).collect();
            object(
                names
                    .iter()
                    .map(|name| (name.clone(), json!({"type":["string","null"]})))
                    .collect::<serde_json::Map<_, _>>()
                    .into(),
                names,
            )
        };
        assert!(admitted(encode(vec![tool(unions(16))], None)));
        assert!(rejected(encode(vec![tool(unions(17))], None)));
        let many = |n: usize| {
            (0..n)
                .map(|i| {
                    json!({"name":format!("t{i}"),"input_schema":object(json!({}), vec![]),"strict":true})
                })
                .collect::<Vec<_>>()
        };
        assert!(admitted(encode(many(20), None)));
        assert!(rejected(encode(many(21), None)));
    }
    #[test]
    fn audit_consecutive_messages_share_tool_turn_validation_without_reordering() {
        let p = Profile::AdaptiveTextTools;
        let mut body = request();
        body["messages"] = json!([
            {"role":"user","content":"go"},
            {"role":"assistant","content":[{"type":"tool_use","id":"c1","name":"lookup","input":{}}]},
            {"role":"assistant","content":[{"type":"tool_use","id":"c2","name":"lookup","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"c1","content":"one"}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"c2","content":"two"},{"type":"text","text":"Continue."}]}
        ]);
        let decoded = p.decode_request(body.to_string().as_bytes()).unwrap();
        assert_eq!(
            p.encode_request(&decoded).unwrap()["messages"],
            json!([
                {"role":"user","content":"go"},
                {"role":"assistant","content":[{"type":"tool_use","id":"c1","name":"lookup","input":{}}]},
                {"role":"assistant","content":[{"type":"tool_use","id":"c2","name":"lookup","input":{}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"c1","content":"one"}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"c2","content":"two"},{"type":"text","text":"Continue."}]}
            ])
        );
        let mut missing = body.clone();
        missing["messages"].as_array_mut().unwrap().remove(4);
        assert!(matches!(
            p.decode_request(missing.to_string().as_bytes()),
            Err(CodecError::Invalid("Anthropic pending results"))
        ));
        body["messages"][3]["content"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type":"text","text":"Too early."}));
        assert!(matches!(
            p.decode_request(body.to_string().as_bytes()),
            Err(CodecError::Invalid("Anthropic result reference"))
        ));
    }
    #[test]
    fn audit_protected_client_controls_are_not_discarded() {
        use morphiecore::protocol::anthropic::client::decode_client_request;
        let base = json!({"model":"m","max_tokens":8,"messages":[{"role":"user","content":"go"}]});
        assert!(decode_client_request(base.to_string().as_bytes()).is_ok());
        for (key, value) in [
            ("inference_geo", json!("us")),
            ("speed", json!("fast")),
            ("fallbacks", json!([{"model":"other"}])),
            ("fallback_credit_token", json!("synthetic")),
        ] {
            let mut body = base.clone();
            body[key] = value;
            assert!(matches!(decode_client_request(body.to_string().as_bytes()),
                Err(CodecError::Unsupported(field)) if field == key));
        }
        let mut body = base;
        body["thinking"] = json!({"type":"adaptive"});
        let plain = decode_client_request(body.to_string().as_bytes()).unwrap();
        // Tolerant dropping is omitted with a receipt; enforcement has no carrier.
        body["thinking"]["block_binding"] = json!({"prefix_mismatch_behavior":"drop_block"});
        let dropped = decode_client_request(body.to_string().as_bytes()).unwrap();
        assert_eq!(dropped.task.semantic, plain.task.semantic);
        assert!(dropped.task.fidelity.normalizations().contains(
            &morphiecore::protocol::fidelity::Normalization::Omitted {
                rule: "messages.omit-block-binding.v1",
                field: "generation.reasoning.block_binding",
            }
        ));
        assert!(plain.task.fidelity.normalizations().is_empty());
        for binding in [json!({"prefix_mismatch_behavior":"error"}), json!({})] {
            body["thinking"]["block_binding"] = binding;
            assert!(matches!(decode_client_request(body.to_string().as_bytes()),
                Err(CodecError::Unsupported(field)) if field == "thinking.block_binding"));
        }
    }
    fn message() -> Value {
        json!({"id":"msg_synthetic","type":"message","role":"assistant","model":"synthetic-model",
        "content":[{"type":"thinking","thinking":"","signature":"synthetic-signature"},
            {"type":"redacted_thinking","data":"synthetic-redacted"},
            {"type":"tool_use","id":"toolu_1","name":"lookup","input":{"key":"A"}}],
        "stop_reason":"tool_use","stop_sequence":null,
        "usage":{"input_tokens":11,"output_tokens":7}})
    }
    #[test]
    fn typed_native_shapes_preserve_schema_presence_and_signature_only() {
        let p = Profile::AdaptiveTextTools;
        let decoded = p
            .decode_request(&request().to_string().into_bytes())
            .unwrap();
        assert_eq!(p.encode_request(&decoded).unwrap(), request());
        let mut selected = request();
        selected["service_tier"] = json!("standard_only");
        let decoded = p.decode_request(selected.to_string().as_bytes()).unwrap();
        assert_eq!(p.encode_request(&decoded).unwrap(), selected);
        selected["service_tier"] = json!("priority");
        assert!(p.decode_request(selected.to_string().as_bytes()).is_err());
        let decoded = p
            .decode_response(&message().to_string().into_bytes())
            .unwrap();
        assert_eq!(p.encode_response(&decoded).unwrap(), message());
        assert!(!format!("{decoded:?}").contains("synthetic-signature"));
        assert!(!format!("{decoded:?}").contains("synthetic-redacted"));
        for data in [json!(""), json!(7), Value::Null] {
            let mut bad = message();
            bad["content"][1]["data"] = data;
            assert!(p.decode_response(bad.to_string().as_bytes()).is_err());
        }
        // Unknown upstream fields are ignored; known ones are re-encoded unchanged.
        let mut unknown = message();
        unknown["created_at"] = json!(0);
        unknown["usage"]["service_tier"] = json!("standard");
        unknown["usage"]["synthetic_future_usage"] = json!({"count":0});
        unknown["content"][2]["extra"] = json!(null);
        let decoded = p.decode_response(unknown.to_string().as_bytes()).unwrap();
        let mut expected = message();
        expected["usage"]["service_tier"] = json!("standard");
        assert_eq!(p.encode_response(&decoded).unwrap(), expected);
        unknown["usage"]["service_tier"] = json!("auto");
        assert!(p.decode_response(unknown.to_string().as_bytes()).is_err());
        let mut edited = p.decode_request(request().to_string().as_bytes()).unwrap();
        edited.max_tokens = 0;
        assert!(p.encode_request(&edited).is_err());
    }
    #[test]
    fn native_non_strict_tools_do_not_rewrite_or_drop_constraints() {
        let p = Profile::AdaptiveTextTools;
        for (key, value) in [
            ("temperature", json!(0.5)),
            ("thinking", json!({"type":"enabled","budget_tokens":1024})),
            ("unknown", json!(false)),
            ("max_tokens", Value::Null),
            ("stream", Value::Null),
        ] {
            let mut body = request();
            body[key] = value;
            assert!(
                p.decode_request(body.to_string().as_bytes()).is_err(),
                "{key}"
            );
        }
        let mut body = request();
        body["tools"][0]["strict"] = Value::Null;
        assert!(p.decode_request(body.to_string().as_bytes()).is_err());
        body["tools"][0]["strict"] = json!(true);
        let decoded = p.decode_request(body.to_string().as_bytes()).unwrap();
        // Native strict string enums have a documented capitalization loss.
        assert!(p.encode_request(&decoded).is_err());
        body["tools"][0]["input_schema"]["properties"]["key"] =
            json!({"type":"integer","enum":[1,2]});
        let decoded = p.decode_request(body.to_string().as_bytes()).unwrap();
        assert_eq!(p.encode_request(&decoded).unwrap(), body);
        let mut too_many = body.clone();
        too_many["tools"] = json!(
            (0..21)
                .map(|n| {
                    let mut tool = body["tools"][0].clone();
                    tool["name"] = json!(format!("tool_{n}"));
                    tool
                })
                .collect::<Vec<_>>()
        );
        let decoded = p.decode_request(too_many.to_string().as_bytes()).unwrap();
        assert!(p.encode_request(&decoded).is_err());
        let mut complex = body.clone();
        let properties = (0..25)
            .map(|n| (format!("p{n}"), json!({"type":"boolean"})))
            .collect::<serde_json::Map<_, _>>();
        complex["output_config"] = json!({"format":{"type":"json_schema","schema":{
            "type":"object","properties":properties,"additionalProperties":false
        }}});
        let decoded = p.decode_request(complex.to_string().as_bytes()).unwrap();
        assert!(p.encode_request(&decoded).is_err());
        let mut output_only = complex.clone();
        output_only.as_object_mut().unwrap().remove("tools");
        let decoded = p
            .decode_request(output_only.to_string().as_bytes())
            .unwrap();
        assert!(p.encode_request(&decoded).is_err());
        let properties = (0..17)
            .map(|n| (format!("p{n}"), json!({"type":["boolean","null"]})))
            .collect::<serde_json::Map<_, _>>();
        complex["output_config"]["format"]["schema"]["properties"] = json!(properties);
        let decoded = p.decode_request(complex.to_string().as_bytes()).unwrap();
        assert!(p.encode_request(&decoded).is_err());
        output_only["output_config"] = complex["output_config"].clone();
        let decoded = p
            .decode_request(output_only.to_string().as_bytes())
            .unwrap();
        assert!(p.encode_request(&decoded).is_err());
        output_only["output_config"]["format"]["schema"] = json!({
            "type":"object","properties":{"ok":{"type":"boolean"}},
            "required":["ok"],"additionalProperties":false
        });
        let decoded = p
            .decode_request(output_only.to_string().as_bytes())
            .unwrap();
        assert_eq!(p.encode_request(&decoded).unwrap(), output_only);
        body["tools"][0]["input_schema"]["properties"]["key"]["minimum"] = json!(2);
        let decoded = p.decode_request(body.to_string().as_bytes()).unwrap();
        assert!(p.encode_request(&decoded).is_err());
        let mut body = request();
        body["tools"][0]
            .as_object_mut()
            .unwrap()
            .remove("input_schema");
        assert!(p.decode_request(body.to_string().as_bytes()).is_err());
        let mut body = request();
        body["thinking"] = json!({"type":"adaptive","display":"omitted"});
        body["output_config"] = json!({"effort":"medium"});
        body["stream"] = json!(false);
        assert_eq!(
            p.encode_request(&p.decode_request(body.to_string().as_bytes()).unwrap())
                .unwrap(),
            body
        );
        for content in [json!(""), json!([]), json!([{"type":"text","text":""}])] {
            let mut body = request();
            body["messages"][0]["content"] = content;
            assert!(p.decode_request(body.to_string().as_bytes()).is_err());
        }
    }
    #[test]
    fn native_wire_history_preserves_error_presence_and_rejects_bad_arrangements() {
        let p = Profile::AdaptiveTextTools;
        for marker in [None, Some(json!(false)), Some(json!(true))] {
            let mut body = request();
            let mut result = json!({"type":"tool_result","tool_use_id":"toolu_1","content":""});
            if let Some(marker) = marker {
                result["is_error"] = marker;
            }
            body["messages"] = json!([
            {"role":"user","content":"Look up A."},
            {"role":"assistant","content":message()["content"]},
            {"role":"user","content":[result]}]);
            assert_eq!(
                p.encode_request(&p.decode_request(body.to_string().as_bytes()).unwrap())
                    .unwrap(),
                body
            );
            body["messages"][2]["content"][0]["tool_use_id"] = json!("other");
            assert!(p.decode_request(body.to_string().as_bytes()).is_err());
        }
        let mut body = request();
        body["messages"]
            .as_array_mut()
            .unwrap()
            .push(json!({"role":"assistant","content":message()["content"]}));
        assert!(p.decode_request(body.to_string().as_bytes()).is_err());
    }
    #[test]
    fn individual_native_event_shapes_do_not_claim_cross_event_closure() {
        let p = Profile::AdaptiveTextTools;
        let mut opening = message();
        opening["content"] = json!([]);
        opening["stop_reason"] = Value::Null;
        let events = [
            json!({"type":"message_start","message":opening}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"c","name":"lookup","input":{}}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"key\":"}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"signature_delta","signature":"synthetic-fragment"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{"output_tokens":9}}),
            json!({"type":"message_delta","delta":{"stop_reason":"stop_sequence","stop_sequence":"END"},"usage":{"output_tokens":9}}),
            json!({"type":"message_stop"}),
            json!({"type":"ping"}),
            json!({"type":"error","error":{"type":"overloaded_error","message":"synthetic failure"}}),
        ];
        for wire in events {
            let name = wire["type"].as_str().unwrap();
            let event = p.decode_event(name, wire.to_string().as_bytes()).unwrap();
            assert_eq!(p.encode_event(&event).unwrap(), wire);
            assert!(
                p.decode_event("not-the-event", wire.to_string().as_bytes())
                    .is_err()
            );
        }
        let extra = json!({"type":"message_stop","extra":false});
        let event = p
            .decode_event("message_stop", extra.to_string().as_bytes())
            .unwrap();
        assert_eq!(
            p.encode_event(&event).unwrap(),
            json!({"type":"message_stop"})
        );
        let billing =
            json!({"type":"error","error":{"type":"billing_error","message":"synthetic"}});
        p.decode_event("error", billing.to_string().as_bytes())
            .unwrap();
        for wire in [
            json!({"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":"END"},"usage":{"output_tokens":9}}),
            json!({"type":"content_block_delta","index":morphiecore::semantic::task::generation::MAX_ITEMS,"delta":{"type":"text_delta","text":"x"}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"citations_delta","citation":{}}}),
        ] {
            assert!(
                p.decode_event(wire["type"].as_str().unwrap(), wire.to_string().as_bytes())
                    .is_err()
            );
        }
        let mut bad = message();
        bad["stop_reason"] = json!("pause_turn");
        assert!(p.decode_response(bad.to_string().as_bytes()).is_err());
    }
    #[test]
    fn native_typed_edits_cannot_escape_local_resource_limits() {
        use morphiecore::{
            protocol::anthropic::Block,
            semantic::{task::generation::StructuredValue, value::Presence},
        };
        let p = Profile::AdaptiveTextTools;
        let mut req = p.decode_request(request().to_string().as_bytes()).unwrap();
        req.messages =
            vec![req.messages[0].clone(); morphiecore::semantic::task::generation::MAX_ITEMS + 1];
        assert!(matches!(p.encode_request(&req), Err(CodecError::Limit)));
        let mut req = p.decode_request(request().to_string().as_bytes()).unwrap();
        req.system = morphiecore::semantic::value::Presence::Value(
            morphiecore::protocol::anthropic::TextContent::Text(
                "x".repeat(morphiecore::semantic::task::generation::MAX_TEXT_BYTES + 1),
            ),
        );
        assert!(matches!(p.encode_request(&req), Err(CodecError::Limit)));
        let mut response = p.decode_response(message().to_string().as_bytes()).unwrap();
        let input = StructuredValue::new(json!({"data":vec![Value::Null;4096]})).unwrap();
        let blocks = morphiecore::semantic::value::JsonLimits::ENVELOPE.nodes / 4096 + 1;
        response.content = (0..blocks)
            .map(|index| Block::ToolUse {
                id: format!("c_{index}"),
                name: "lookup".into(),
                input: input.clone(),
                caller: Presence::Absent,
            })
            .collect();
        assert!(matches!(
            p.encode_response(&response),
            Err(CodecError::Limit)
        ));
    }
}
mod anthropic_semantics {
    //! Independent semantic mapping and append-only native continuation expectations.
    use morphiecore::{
        protocol::{DecodedRequest, anthropic::Profile, native::ReplayTarget},
        semantic::{
            task::generation::*,
            value::{Presence, Text},
        },
    };
    use serde_json::{Value, json};

    fn target() -> ReplayTarget {
        ReplayTarget::new("synthetic-provider", "synthetic-model").unwrap()
    }
    fn request() -> &'static [u8] {
        br#"{"model":"synthetic-model","max_tokens":512,"system":"Use lookup.","tools":[{"name":"lookup","strict":false,"input_schema":{"type":"object","properties":{"key":{"type":"string"}}}}],"messages":[{"role":"user","content":"Look up A."}]}"#
    }
    fn response() -> &'static [u8] {
        br#"{"id":"msg_synthetic","type":"message","role":"assistant","model":"synthetic-model","content":[{"type":"thinking","thinking":"","signature":"synthetic-signature"},{"type":"tool_use","id":"c","name":"lookup","input":{"key":"A"}}],"stop_reason":"tool_use","stop_sequence":null,"usage":{"input_tokens":11,"output_tokens":7}}"#
    }
    fn feedback(
        source: &DecodedRequest,
        observed: &morphiecore::protocol::DecodedResponse,
        execution: ToolExecution,
    ) -> GenerationRequest {
        let id = ItemId::scoped(LocalScope::new(3), 1);
        let mut request = ClientManaged::new(source.semantic.settings().clone())
            .unwrap()
            .select_request(&source.semantic)
            .unwrap()
            .select_response(&observed.semantic)
            .unwrap()
            .append_items(vec![(
                id,
                Item::ToolResult(ToolResult {
                    call_id: Some(Text::new("c", "call", 64).unwrap()),
                    cache: None,
                    name: None,
                    output: "actual feedback".into(),
                    is_error: if matches!(&execution, ToolExecution::Succeeded) {
                        None
                    } else {
                        Some(true)
                    },
                    execution: Some(execution),
                    status: None,
                    context: Default::default(),
                }),
            )])
            .unwrap()
            .build()
            .unwrap();
        let mut groups = request.message_envelopes().to_vec();
        groups.push(
            MessageEnvelope::new(
                GroupId::new(LocalScope::new(3), 1),
                MessageEnvelopeRole::User,
                vec![id],
            )
            .unwrap(),
        );
        request = request.with_message_envelopes(groups).unwrap();
        request
    }
    #[test]
    fn native_intake_keeps_independent_error_execution_progress_and_signature_owners() {
        let p = Profile::AdaptiveTextTools;
        let source = p
            .decode_generation_request(request(), LocalScope::new(1), &target())
            .unwrap();
        let observed = p
            .decode_generation_response(response(), LocalScope::new(2), &target())
            .unwrap();
        assert_eq!(observed.metadata.created, None);
        assert_eq!(
            observed.semantic.progress(),
            InteractionProgress::AwaitingToolResults
        );
        assert_eq!(observed.semantic.outcome(), Outcome::Completed);
        assert!(
            matches!(&observed.semantic.items()[0].1, Item::Reasoning(r) if r.parts.is_empty() && r.replay.as_ref().unwrap().format() == ReplayFormat::AnthropicMessagesThinking)
        );
        assert!(
            matches!(&observed.semantic.items()[1].1, Item::ToolCall(c) if c.arguments.as_structured().unwrap().value() == &json!({"key":"A"}))
        );
        assert_eq!(observed.semantic.usage().unwrap().total_tokens, None);
        assert_eq!(
            observed.semantic.usage().unwrap().input_relation,
            InputTokenRelation::ExcludesCacheReadAndWrite
        );
        assert_eq!(
            p.encode_generation_response(&observed, &source, &target())
                .unwrap(),
            serde_json::from_slice::<Value>(response()).unwrap()
        );
        use morphiecore::semantic::context::ServiceClass;
        assert_eq!(observed.metadata.context.service_class, Presence::Absent);
        let mut reported: Value = serde_json::from_slice(response()).unwrap();
        reported["usage"]["service_tier"] = json!("priority");
        let decoded = p
            .decode_generation_response(
                reported.to_string().as_bytes(),
                LocalScope::new(2),
                &target(),
            )
            .unwrap();
        assert_eq!(
            decoded.metadata.context.service_class,
            Presence::Value(ServiceClass::PriorityCapacity)
        );
        let mut manual = observed.clone();
        manual.metadata.context.service_class = Presence::Value(ServiceClass::PriorityCapacity);
        assert_eq!(
            p.encode_generation_response(&manual, &source, &target())
                .unwrap(),
            reported
        );
        manual.metadata.context.service_class = Presence::Value(ServiceClass::PriorityProcessing);
        assert!(
            p.encode_generation_response(&manual, &source, &target())
                .is_err()
        );
    }
    #[test]
    fn native_stop_reasons_have_independent_terminal_owners() {
        let p = Profile::AdaptiveTextTools;
        let decode = |stop: &str, sequence: Value| {
            let wire = json!({"id":"msg_synthetic","type":"message","role":"assistant","model":"synthetic-model",
                "content":[{"type":"text","text":"partial"}],"stop_reason":stop,"stop_sequence":sequence,
                "stop_details":{"category":"cyber","explanation":"synthetic"},
                "usage":{"input_tokens":11,"output_tokens":7}});
            p.decode_generation_response(wire.to_string().as_bytes(), LocalScope::new(2), &target())
        };
        for (stop, sequence, outcome, progress, incomplete) in [
            (
                "stop_sequence",
                json!("END"),
                Outcome::Completed,
                InteractionProgress::TurnFinished,
                None,
            ),
            (
                "model_context_window_exceeded",
                Value::Null,
                Outcome::Incomplete,
                InteractionProgress::Unreported,
                Some(IncompleteReason::ContextWindowExceeded),
            ),
            (
                "refusal",
                Value::Null,
                Outcome::Incomplete,
                InteractionProgress::Unreported,
                Some(IncompleteReason::ContentFilter),
            ),
        ] {
            let observed = decode(stop, sequence).unwrap();
            assert_eq!(observed.semantic.outcome(), outcome, "{stop}");
            assert_eq!(observed.semantic.progress(), progress, "{stop}");
            assert_eq!(observed.semantic.details().incomplete, incomplete, "{stop}");
            if stop == "stop_sequence" {
                let source = p
                    .decode_generation_request(request(), LocalScope::new(1), &target())
                    .unwrap();
                let wire = p
                    .encode_generation_response(&observed, &source, &target())
                    .unwrap();
                assert_eq!(wire["stop_reason"], "stop_sequence");
                assert_eq!(wire["stop_sequence"], "END");
            }
        }
        assert!(decode("end_turn", json!("END")).is_err());
        assert!(decode("stop_sequence", Value::Null).is_err());
        assert_eq!(
            decode("end_turn", Value::Null)
                .unwrap()
                .semantic
                .details()
                .completion,
            Some(CompletionCause::Natural)
        );
        let mut unreported = decode("end_turn", Value::Null).unwrap();
        unreported.semantic = unreported
            .semantic
            .with_details(TerminalDetails::default())
            .unwrap();
        let source = p
            .decode_generation_request(request(), LocalScope::new(1), &target())
            .unwrap();
        assert!(
            p.encode_generation_response(&unreported, &source, &target())
                .is_err()
        );
        let expected = decode("stop_sequence", json!("END")).unwrap();
        assert_eq!(
            expected.semantic.details().completion,
            Some(CompletionCause::StopSequence(
                Text::new("END", "fixture", 64).unwrap()
            ))
        );
        // Responses has no completion-cause carrier: the cause is omitted under a named rule.
        // Native progress is removed by the separate native client projection.
        let delivered = GenerationResponse::new(
            expected.semantic.items().to_vec(),
            expected.semantic.outcome(),
        )
        .unwrap()
        .with_details(expected.semantic.details().clone())
        .unwrap();
        let (fidelity, metadata) = (Default::default(), crate::events_support::metadata());
        let projected = morphiecore::protocol::openai::lowering::lower_response(
            &delivered,
            &fidelity,
            &metadata,
            morphiecore::protocol::openai::Profile::Responses,
            morphiecore::protocol::openai::TargetCaps::full(),
        )
        .unwrap();
        assert!(projected.projection().iter().any(|stage| {
            stage
                .loss
                .is_some_and(|rule| rule.name() == "responses.omit-completion-cause.v1")
        }));
        let responses =
            morphiecore::protocol::openai::responses::encode_response(&projected).unwrap();
        assert_eq!(responses["status"], "completed");
        assert!(responses["incomplete_details"].is_null());
        let chat = morphiecore::protocol::openai::lowering::lower_response(
            &delivered,
            &fidelity,
            &metadata,
            morphiecore::protocol::openai::Profile::Chat,
            morphiecore::protocol::openai::TargetCaps::full(),
        )
        .unwrap();
        assert!(chat.projection().iter().any(|stage| {
            stage
                .loss
                .is_some_and(|rule| rule.name() == "chat.coarsen-completion-cause.v1")
        }));
        assert_eq!(
            morphiecore::protocol::openai::chat::encode_response(&chat).unwrap()["choices"][0]["finish_reason"],
            "stop"
        );
        // The reported thinking subset of output tokens is kept, never derived.
        let thinking = json!({"id":"msg_synthetic","type":"message","role":"assistant","model":"synthetic-model",
            "content":[{"type":"text","text":"partial"}],"stop_reason":"end_turn","stop_sequence":null,
            "usage":{"input_tokens":11,"output_tokens":7,"output_tokens_details":{"thinking_tokens":5}}});
        let thinking = p
            .decode_generation_response(
                thinking.to_string().as_bytes(),
                LocalScope::new(2),
                &target(),
            )
            .unwrap();
        assert_eq!(thinking.semantic.usage().unwrap().reasoning_tokens, Some(5));
        let window = decode("model_context_window_exceeded", Value::Null).unwrap();
        let window_wire =
            morphiecore::protocol::anthropic::client::encode_client_response(&window, "m").unwrap();
        assert_eq!(window_wire["stop_reason"], "model_context_window_exceeded");
        // OpenAI has no context-window reason: Chat reports `length` and Responses an
        // unspecified incomplete reason, each under a named rule; neither becomes success.
        use morphiecore::protocol::openai::Profile as OpenAi;
        // As after the native client projection: no native usage, items follow the outcome.
        let mut items = window.semantic.items().to_vec();
        let Item::Message(message) = &mut items[0].1 else {
            unreachable!()
        };
        message.status = ItemLifecycle::Incomplete;
        let stopped = GenerationResponse::new(items, window.semantic.outcome())
            .unwrap()
            .with_details(window.semantic.details().clone())
            .unwrap();
        for (profile, rule) in [
            (OpenAi::Chat, "chat.context-window-as-length.v1"),
            (OpenAi::Responses, "responses.omit-context-window-reason.v1"),
        ] {
            let projected = morphiecore::protocol::openai::lowering::lower_response(
                &stopped,
                &fidelity,
                &metadata,
                profile,
                morphiecore::protocol::openai::TargetCaps::full(),
            )
            .unwrap();
            let named = |stages: &[morphiecore::protocol::openai::lowering::projection::ProjectionStage]| {
                stages
                    .iter()
                    .any(|stage| stage.loss.is_some_and(|loss| loss.name() == rule))
            };
            assert!(named(projected.projection()), "{rule}");
            let events = [
                vec![
                    StreamEvent::Started,
                    crate::events_support::start(1, ItemKind::Message { phase: None }),
                ],
                crate::events_support::part(1, 1, PartKind::Text, "partial"),
                vec![
                    crate::events_support::close(1, ItemLifecycle::Incomplete),
                    StreamEvent::Terminal {
                        terminal: StreamTerminal::Incomplete,
                        details: stopped.details().clone(),
                    },
                ],
            ]
            .concat();
            let mut encoder =
                morphiecore::protocol::openai::events::EventEncoder::new(profile, metadata.clone())
                    .unwrap();
            let frames: Vec<_> = events
                .iter()
                .flat_map(|event| encoder.encode(event, &fidelity).unwrap())
                .collect();
            encoder.finish().unwrap();
            assert!(named(encoder.projection()), "{rule}");
            let last = frames.last().unwrap();
            if profile == OpenAi::Chat {
                let json =
                    morphiecore::protocol::openai::chat::encode_response(&projected).unwrap();
                assert_eq!(json["choices"][0]["finish_reason"], "length");
                assert_eq!(last["choices"][0]["finish_reason"], "length");
            } else {
                let json =
                    morphiecore::protocol::openai::responses::encode_response(&projected).unwrap();
                for response in [&json, &last["response"]] {
                    assert_eq!(response["status"], "incomplete");
                    assert_eq!(response["incomplete_details"], json!({"reason": null}));
                }
                assert_eq!(last["type"], "response.incomplete");
            }
        }
        let frames = vec![
            json!({"type":"message_start","message":{"id":"msg_synthetic","type":"message","role":"assistant","model":"synthetic-model","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":11,"output_tokens":0}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"partial"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"message_delta","delta":{"stop_reason":"stop_sequence","stop_sequence":"END"},"usage":{"output_tokens":7}}),
            json!({"type":"message_stop"}),
        ];
        let bytes = frames
            .iter()
            .map(|v| format!("event: {}\ndata: {v}\n\n", v["type"].as_str().unwrap()))
            .collect::<String>();
        let mut decoder = morphiecore::protocol::anthropic::GenerationSseDecoder::new(
            200,
            "text/event-stream",
            Default::default(),
            LocalScope::new(2),
            target(),
        )
        .unwrap();
        let mut state = StreamState::new();
        let mut client =
            morphiecore::protocol::anthropic::client_events::ClientEventEncoder::new("r", "m");
        let mut output = Vec::new();
        let mut rest = bytes.as_bytes();
        while !rest.is_empty() {
            let (used, events) = decoder.consume(rest).unwrap();
            assert!(used > 0);
            rest = &rest[used..];
            for event in events {
                state = reduce(state, event.clone()).unwrap();
                output.extend(client.encode(&event).unwrap().into_iter().flatten());
            }
        }
        decoder.finish().unwrap();
        client.finish().unwrap();
        assert_eq!(materialize(&state).unwrap(), expected.semantic);
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("\"stop_reason\":\"stop_sequence\""));
        assert!(output.contains("\"stop_sequence\":\"END\""));
        let static_wire =
            morphiecore::protocol::anthropic::client::encode_client_response(&expected, "m")
                .unwrap();
        assert_eq!(static_wire["stop_sequence"], "END");
        let mut conflict = frames;
        conflict.insert(4, json!({"type":"message_delta","delta":{"stop_reason":"stop_sequence","stop_sequence":"DIFFERENT"},"usage":{"output_tokens":7}}));
        let bytes = conflict
            .iter()
            .map(|v| format!("event: {}\ndata: {v}\n\n", v["type"].as_str().unwrap()))
            .collect::<String>();
        let mut decoder = morphiecore::protocol::anthropic::MessageSseDecoder::new(
            200,
            "text/event-stream",
            Default::default(),
        )
        .unwrap();
        let mut rest = bytes.as_bytes();
        let rejected = loop {
            match decoder.consume(rest) {
                Ok(used) if used > 0 => {
                    rest = &rest[used..];
                    if rest.is_empty() {
                        break false;
                    }
                }
                _ => break true,
            }
        };
        assert!(rejected);
        assert!(decoder.finish().is_err());
    }
    #[test]
    fn saved_observation_and_appended_error_encode_the_independent_native_history() {
        let p = Profile::AdaptiveTextTools;
        let source = p
            .decode_generation_request(request(), LocalScope::new(1), &target())
            .unwrap();
        let observed = p
            .decode_generation_response(response(), LocalScope::new(2), &target())
            .unwrap();
        for (execution, marker) in [
            (ToolExecution::NotExecuted, Some(true)),
            (ToolExecution::Failed { code: None }, Some(true)),
            (ToolExecution::Succeeded, None),
        ] {
            let mut next = feedback(&source, &observed, execution);
            let mut items = next.items().to_vec();
            let Item::ToolResult(r) = &mut items.last_mut().unwrap().1 else {
                panic!()
            };
            r.is_error = marker;
            next = next.with_items(items).unwrap();
            let encoded = p
                .encode_generation_request(&next, &observed.fidelity, &target())
                .unwrap();
            let mut result =
                json!({"type":"tool_result","tool_use_id":"c","content":"actual feedback"});
            if let Some(marker) = marker {
                result["is_error"] = json!(marker);
            }
            assert_eq!(
                encoded.value["messages"],
                json!([
                    {"role":"user","content":[{"type":"text","text":"Look up A."}]},
                    {"role":"assistant","content":[{"type":"thinking","thinking":"","signature":"synthetic-signature"},{"type":"tool_use","id":"c","name":"lookup","input":{"key":"A"}}]},
                    {"role":"user","content":[result]}
                ])
            );
            assert_eq!(
                encoded.omitted_execution,
                vec![ItemId::scoped(LocalScope::new(3), 1)]
            );
            let intake = p
                .decode_generation_request(
                    &serde_json::to_vec(&encoded.value).unwrap(),
                    LocalScope::new(4),
                    &target(),
                )
                .unwrap();
            let Item::ToolResult(result) = &intake.semantic.items().last().unwrap().1 else {
                panic!()
            };
            assert_eq!(result.is_error, marker);
            assert_eq!(result.execution, None);
        }
    }
    #[test]
    fn ordered_system_blocks_and_disabled_controls_have_independent_semantic_owners() {
        let p = Profile::AdaptiveTextTools;
        let content_wire = br#"{"model":"synthetic-model","max_tokens":32,"system":[{"type":"text","text":"stable","cache_control":{"type":"ephemeral","ttl":"1h"}},{"type":"text","text":"changing"}],"cache_control":{"type":"ephemeral"},"messages":[{"role":"user","content":[{"type":"text","text":"go","cache_control":{"type":"ephemeral"}}]}]}"#;
        let content = p
            .decode_generation_request(content_wire, LocalScope::new(1), &target())
            .unwrap();
        let content_client =
            morphiecore::protocol::anthropic::client::decode_client_request(content_wire).unwrap();
        assert_eq!(
            content.semantic.settings(),
            content_client.task.semantic.settings()
        );
        let encoded = p
            .encode_generation_request(&content.semantic, &content.fidelity, &target())
            .unwrap();
        assert_eq!(
            encoded.value["system"],
            json!([
                {"type":"text","text":"stable","cache_control":{"type":"ephemeral","ttl":"1h"}},
                {"type":"text","text":"changing"}
            ])
        );
        assert_eq!(
            encoded.value["messages"],
            json!([
                {"role":"user","content":[{"type":"text","text":"go","cache_control":{"type":"ephemeral"}}]}
            ])
        );
        assert_eq!(encoded.value["cache_control"], json!({"type":"ephemeral"}));
        let Item::Instruction(instruction) = &content.semantic.items()[0].1 else {
            panic!("system owner")
        };
        assert_eq!(instruction.parts[0].cache, Some(PrefixCacheTtl::OneHour));
        assert_eq!(instruction.parts[1].cache, None);
        assert_eq!(
            content.semantic.settings().automatic_prefix_cache,
            Some(PrefixCacheTtl::FiveMinutes)
        );
        let text = |value| Text::new(value, "fixture", 64).unwrap();
        let mut settings = GenerationSettings {
            automatic_prefix_cache: Some(PrefixCacheTtl::FiveMinutes),
            ..Default::default()
        };
        settings.controls.max_output_tokens = Some(32);
        let manual = GenerationRequest::from_settings(
            vec![
                (
                    ItemId::new(1),
                    Item::Instruction(Instruction {
                        authority: InstructionAuthority::System,
                        status: None,
                        parts: vec![
                            Part {
                                replay: None,
                                id: PartId::new(1),
                                content: text("stable"),
                                cache: Some(PrefixCacheTtl::OneHour),
                            },
                            Part::new(PartId::new(2), text("changing")),
                        ],
                    }),
                ),
                (
                    ItemId::new(2),
                    Item::Message(Message {
                        role: MessageRole::User,
                        status: ItemLifecycle::Completed,
                        phase: None,
                        parts: vec![Part {
                            replay: None,
                            id: PartId::new(3),
                            content: ContentPart::Text(text("go").into()),
                            cache: Some(PrefixCacheTtl::FiveMinutes),
                        }],
                    }),
                ),
            ],
            settings,
        )
        .unwrap()
        .with_message_envelopes(vec![
            MessageEnvelope::new(
                GroupId::new(LocalScope::ROOT, 1),
                MessageEnvelopeRole::User,
                vec![ItemId::new(2)],
            )
            .unwrap(),
        ])
        .unwrap();
        assert_eq!(
            p.encode_generation_request(&manual, &Default::default(), &target())
                .unwrap()
                .value,
            serde_json::from_slice::<Value>(content_wire).unwrap()
        );
        assert!(
            morphiecore::protocol::openai::lowering::lower_request(
                &manual,
                &Default::default(),
                morphiecore::protocol::openai::Profile::Chat,
                morphiecore::protocol::openai::TargetCaps::full()
            )
            .is_err()
        );
        let mut conflict: Value = serde_json::from_slice(content_wire).unwrap();
        conflict["cache_control"]["ttl"] = json!("1h");
        assert_eq!(
            p.decode_request(conflict.to_string().as_bytes())
                .err()
                .unwrap(),
            morphiecore::protocol::CodecError::Invalid("Anthropic automatic cache TTL conflict")
        );
        let mut full: Value = serde_json::from_slice(content_wire).unwrap();
        full["tools"] = json!([
            {"name":"first","input_schema":{"type":"object"},"cache_control":{"type":"ephemeral","ttl":"1h"}},
            {"name":"second","input_schema":{"type":"object"},"cache_control":{"type":"ephemeral","ttl":"1h"}}
        ]);
        assert_eq!(
            p.decode_request(full.to_string().as_bytes()).err(),
            Some(morphiecore::protocol::CodecError::Limit)
        );
        let mut items = content.semantic.items().to_vec();
        let Item::Instruction(instruction) = &mut items[0].1 else {
            unreachable!()
        };
        instruction.parts[0].cache = None;
        let edited = content.semantic.clone().with_items(items).unwrap();
        let encoded = p
            .encode_generation_request(&edited, &content.fidelity, &target())
            .unwrap();
        assert!(encoded.value["system"][0].get("cache_control").is_none());
        assert!(GenerationRequirements::derive(&edited).explicit_cache);
        let nested_wire = br#"{"model":"synthetic-model","max_tokens":32,"messages":[{"role":"user","content":"lookup"},{"role":"assistant","content":[{"type":"tool_use","id":"c","name":"lookup","input":{},"cache_control":{"type":"ephemeral","ttl":"1h"}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"c","cache_control":{"type":"ephemeral"},"content":[{"type":"text","text":"stable","cache_control":{"type":"ephemeral","ttl":"1h"}},{"type":"text","text":"tail"}]}]}]}"#;
        let nested = p
            .decode_generation_request(nested_wire, LocalScope::new(1), &target())
            .unwrap();
        let Item::ToolCall(call) = &nested.semantic.items()[1].1 else {
            panic!("call owner")
        };
        assert_eq!(call.cache, Some(PrefixCacheTtl::OneHour));
        let Item::ToolResult(result) = &nested.semantic.items()[2].1 else {
            panic!("result owner")
        };
        assert_eq!(result.cache, Some(PrefixCacheTtl::FiveMinutes));
        let ToolOutput::Parts(parts) = &result.output else {
            panic!("inner owners")
        };
        assert_eq!(parts[0].cache, Some(PrefixCacheTtl::OneHour));
        assert_eq!(parts[1].cache, None);
        let encoded = p
            .encode_generation_request(&nested.semantic, &nested.fidelity, &target())
            .unwrap();
        assert_eq!(
            encoded.value["messages"],
            json!([
                {"role":"user","content":[{"type":"text","text":"lookup"}]},
                {"role":"assistant","content":[{"type":"tool_use","id":"c","name":"lookup","input":{},"cache_control":{"type":"ephemeral","ttl":"1h"}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"c","cache_control":{"type":"ephemeral"},"content":[
                    {"type":"text","text":"stable","cache_control":{"type":"ephemeral","ttl":"1h"}},
                    {"type":"text","text":"tail"}
                ]}]}
            ])
        );
        assert!(p.decode_request(br#"{"model":"synthetic-model","max_tokens":32,"messages":[{"role":"user","content":[{"type":"text","text":"","cache_control":{"type":"ephemeral"}}]}]}"#).is_err());
        let empty_wire = json!({
            "model":"synthetic-model","max_tokens":32,
            "system":[{"type":"text","text":"stable"},{"type":"text","text":""}],
            "cache_control":{"type":"ephemeral","ttl":"1h"},
            "messages":[{"role":"user","content":[
                {"type":"text","text":"go","cache_control":{"type":"ephemeral","ttl":"1h"}},
                {"type":"text","text":""}
            ]}]
        });
        let empty = p
            .decode_generation_request(
                empty_wire.to_string().as_bytes(),
                LocalScope::new(1),
                &target(),
            )
            .unwrap();
        assert!(
            morphiecore::protocol::anthropic::client::decode_client_request(
                empty_wire.to_string().as_bytes()
            )
            .is_ok()
        );
        let encoded = p
            .encode_generation_request(&empty.semantic, &empty.fidelity, &target())
            .unwrap();
        assert_eq!(encoded.value, empty_wire);
        let reject_empty_cache = |wire: &Value| {
            let bytes = wire.to_string();
            let error =
                morphiecore::protocol::CodecError::Invalid("Anthropic empty text cache owner");
            assert_eq!(
                p.decode_request(bytes.as_bytes()).err(),
                Some(error.clone())
            );
            assert_eq!(
                morphiecore::protocol::anthropic::client::decode_client_request(bytes.as_bytes())
                    .err(),
                Some(error)
            );
        };
        let mut cached_empty = empty_wire.clone();
        cached_empty["messages"][0]["content"][1]["cache_control"] =
            json!({"type":"ephemeral","ttl":"1h"});
        reject_empty_cache(&cached_empty);
        let mut cached_system = empty_wire.clone();
        cached_system["system"][1]["cache_control"] = json!({"type":"ephemeral","ttl":"1h"});
        reject_empty_cache(&cached_system);
        let mut cached_nested: Value = serde_json::from_slice(nested_wire).unwrap();
        cached_nested["messages"][2]["content"][0]["content"][0]["text"] = json!("");
        reject_empty_cache(&cached_nested);
        cached_nested["messages"][2]["content"][0]["content"][0]
            .as_object_mut()
            .unwrap()
            .remove("cache_control");
        assert!(
            p.decode_generation_request(
                cached_nested.to_string().as_bytes(),
                LocalScope::new(1),
                &target()
            )
            .is_ok()
        );
        let mut items = manual.items().to_vec();
        let Item::Message(message) = &mut items[1].1 else {
            panic!("message owner")
        };
        message.parts.push(Part {
            replay: None,
            id: PartId::new(4),
            content: ContentPart::Text(Text::allowing_empty("", "fixture", 64).unwrap().into()),
            cache: Some(PrefixCacheTtl::FiveMinutes),
        });
        let cached_edit = manual.clone().with_items(items).unwrap();
        assert_eq!(
            p.encode_generation_request(&cached_edit, &Default::default(), &target())
                .err(),
            Some(morphiecore::protocol::CodecError::Invalid(
                "Anthropic empty text cache owner"
            ))
        );
        let mut automatic_conflict = empty_wire.clone();
        automatic_conflict["cache_control"]["ttl"] = json!("5m");
        let conflict_error =
            morphiecore::protocol::CodecError::Invalid("Anthropic automatic cache TTL conflict");
        assert_eq!(
            p.decode_request(automatic_conflict.to_string().as_bytes())
                .err(),
            Some(conflict_error.clone())
        );
        assert_eq!(
            morphiecore::protocol::anthropic::client::decode_client_request(
                automatic_conflict.to_string().as_bytes()
            )
            .err(),
            Some(conflict_error.clone())
        );
        let mut settings = empty.semantic.settings().clone();
        settings.automatic_prefix_cache = Some(PrefixCacheTtl::FiveMinutes);
        let conflict_edit = empty.semantic.clone().with_settings(settings).unwrap();
        assert_eq!(
            p.encode_generation_request(&conflict_edit, &empty.fidelity, &target())
                .err(),
            Some(conflict_error)
        );
        let cached_wire = br#"{"model":"synthetic-model","max_tokens":32,"tools":[{"name":"lookup","input_schema":{"type":"object"},"cache_control":{"type":"ephemeral","ttl":"1h"}}],"messages":[{"role":"user","content":"go"}]}"#;
        let cached = p
            .decode_generation_request(cached_wire, LocalScope::new(1), &target())
            .unwrap();
        assert!(
            matches!(&cached.semantic.tools()[0], ToolDefinition::Function(tool)
            if tool.cache == Some(PrefixCacheTtl::OneHour)
                && tool.strict == FunctionStrictness::Omitted(StrictDefault::NonStrict)
                && tool.parameters.as_ref().unwrap().root() == &json!({"type":"object"}))
        );
        let client =
            morphiecore::protocol::anthropic::client::decode_client_request(cached_wire).unwrap();
        assert_eq!(client.task.semantic.settings(), cached.semantic.settings());
        assert!(GenerationRequirements::derive(&client.task.semantic).explicit_cache);
        let encoded = p
            .encode_generation_request(&cached.semantic, &cached.fidelity, &target())
            .unwrap();
        assert_eq!(
            encoded.value["tools"],
            json!([{"name":"lookup","input_schema":{"type":"object"},"cache_control":{"type":"ephemeral","ttl":"1h"}}])
        );
        let mut settings = cached.semantic.settings().clone();
        let ToolDefinition::Function(tool) = &mut settings.tools.as_mut().unwrap()[0] else {
            panic!("function tool")
        };
        tool.cache = Some(PrefixCacheTtl::FiveMinutes);
        let short = cached
            .semantic
            .clone()
            .with_settings(settings.clone())
            .unwrap();
        let encoded = p
            .encode_generation_request(&short, &cached.fidelity, &target())
            .unwrap();
        assert_eq!(
            encoded.value["tools"],
            json!([{"name":"lookup","input_schema":{"type":"object"},"cache_control":{"type":"ephemeral"}}])
        );
        let ToolDefinition::Function(tool) = &mut settings.tools.as_mut().unwrap()[0] else {
            panic!("function tool")
        };
        tool.cache = None;
        let removed = cached.semantic.clone().with_settings(settings).unwrap();
        let encoded = p
            .encode_generation_request(&removed, &cached.fidelity, &target())
            .unwrap();
        assert!(encoded.value["tools"][0].get("cache_control").is_none());
        assert!(!GenerationRequirements::derive(&removed).explicit_cache);
        for profile in [
            morphiecore::protocol::openai::Profile::Chat,
            morphiecore::protocol::openai::Profile::Responses,
        ] {
            assert_eq!(
                morphiecore::protocol::openai::lowering::lower_request(
                    &client.task.semantic,
                    &client.task.fidelity,
                    profile,
                    morphiecore::protocol::openai::TargetCaps::full(),
                )
                .err()
                .unwrap(),
                morphiecore::lowering::generation::RepresentationError::Controls
            );
        }
        assert!(
            morphiecore::protocol::google::encode_generation_request(
                &client.task.semantic,
                &client.task.fidelity,
                &target(),
                None,
            )
            .is_err()
        );
        assert!(
            morphiecore::protocol::interactions::encode_generation_request(
                &client.task.semantic,
                &client.task.fidelity,
                &target()
            )
            .is_err()
        );
        let invalid = std::str::from_utf8(cached_wire)
            .unwrap()
            .replace("\"1h\"", "\"1d\"");
        assert!(
            morphiecore::protocol::anthropic::client::decode_client_request(invalid.as_bytes())
                .is_err()
        );
        let mut bounded: Value = serde_json::from_slice(cached_wire).unwrap();
        bounded["tools"] = json!(
            (0..4)
                .map(|index| json!({
                    "name":format!("lookup_{index}"),"input_schema":{"type":"object"},
                    "cache_control":{"type":"ephemeral","ttl":"1h"}
                }))
                .collect::<Vec<_>>()
        );
        let four = p
            .decode_generation_request(
                bounded.to_string().as_bytes(),
                LocalScope::new(1),
                &target(),
            )
            .unwrap();
        assert!(
            p.encode_generation_request(&four.semantic, &four.fidelity, &target())
                .is_err()
        );
        bounded["tools"].as_array_mut().unwrap().push(json!({
            "name":"extra","input_schema":{"type":"object"},
            "cache_control":{"type":"ephemeral"}
        }));
        assert!(
            p.decode_generation_request(
                bounded.to_string().as_bytes(),
                LocalScope::new(1),
                &target()
            )
            .is_err()
        );
        bounded["tools"].as_array_mut().unwrap().truncate(2);
        bounded["tools"][0]["cache_control"]["ttl"] = json!("5m");
        assert!(
            morphiecore::protocol::anthropic::client::decode_client_request(
                bounded.to_string().as_bytes()
            )
            .is_err()
        );
        let raw = br#"{"model":"synthetic-model","max_tokens":32,"system":[{"type":"text","text":"one"},{"type":"text","text":"two"}],"thinking":{"type":"disabled"},"output_config":{"effort":"high"},"messages":[{"role":"user","content":"go"}]}"#;
        let decoded = p
            .decode_generation_request(raw, LocalScope::new(1), &target())
            .unwrap();
        assert!(decoded.semantic.instructions().is_absent());
        let Item::Instruction(i) = &decoded.semantic.items()[0].1 else {
            panic!()
        };
        assert_eq!(i.authority, InstructionAuthority::System);
        assert_eq!(
            i.texts().map(|(_, t)| t).collect::<Vec<_>>(),
            ["one", "two"]
        );
        let encoded = p
            .encode_generation_request(&decoded.semantic, &decoded.fidelity, &target())
            .unwrap();
        assert_eq!(
            encoded.value["system"],
            json!([{"type":"text","text":"one"},{"type":"text","text":"two"}])
        );
        assert_eq!(encoded.value["thinking"], json!({"type":"disabled"}));
        assert_eq!(encoded.value["output_config"], json!({"effort":"high"}));
        assert_eq!(encoded.value["cache_control"], json!({"type":"ephemeral"}));
        assert_eq!(encoded.value["max_tokens"], 32);
        let mut stopped: Value = serde_json::from_slice(raw).unwrap();
        stopped["stop_sequences"] = json!(["END"]);
        let stopped = p
            .decode_generation_request(
                stopped.to_string().as_bytes(),
                LocalScope::new(1),
                &target(),
            )
            .unwrap();
        assert_eq!(
            stopped
                .semantic
                .controls()
                .stop_sequences
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
            ["END"]
        );
        let encoded = p
            .encode_generation_request(&stopped.semantic, &stopped.fidelity, &target())
            .unwrap();
        assert_eq!(encoded.value["stop_sequences"], json!(["END"]));
        assert!(
            morphiecore::protocol::openai::lowering::lower_request(
                &stopped.semantic,
                &stopped.fidelity,
                morphiecore::protocol::openai::Profile::Responses,
                morphiecore::protocol::openai::TargetCaps::full()
            )
            .is_err()
        );
        let mut automatic: Value = serde_json::from_slice(raw).unwrap();
        automatic["cache_control"] = json!({"type":"ephemeral"});
        assert!(
            p.decode_generation_request(
                automatic.to_string().as_bytes(),
                LocalScope::new(1),
                &target()
            )
            .is_ok()
        );
        assert!(
            morphiecore::protocol::anthropic::client::decode_client_request(
                automatic.to_string().as_bytes()
            )
            .is_ok()
        );
        let client = morphiecore::protocol::anthropic::client::decode_client_request(raw).unwrap();
        assert_eq!(
            client.task.semantic.reasoning().activation,
            Presence::Value(ReasoningActivation::Disabled)
        );
        let invalid = std::str::from_utf8(raw)
            .unwrap()
            .replace("\"disabled\"", "7");
        assert!(
            morphiecore::protocol::anthropic::client::decode_client_request(invalid.as_bytes())
                .is_err()
        );
        // Messages requires max_tokens; a source without a cap gets the fixed default.
        let mut settings = decoded.semantic.settings().clone();
        settings.controls.max_output_tokens = None;
        let uncapped = decoded.semantic.clone().with_settings(settings).unwrap();
        let encoded = p
            .encode_generation_request(&uncapped, &decoded.fidelity, &target())
            .unwrap();
        assert_eq!(encoded.value["max_tokens"], 32_000);
        automatic["cache_control"]["ttl"] = json!("1h");
        let long = p
            .decode_generation_request(
                automatic.to_string().as_bytes(),
                LocalScope::new(1),
                &target(),
            )
            .unwrap();
        assert_eq!(
            long.semantic.settings().automatic_prefix_cache,
            Some(PrefixCacheTtl::OneHour)
        );
        let encoded = p
            .encode_generation_request(&long.semantic, &long.fidelity, &target())
            .unwrap();
        assert_eq!(
            encoded.value["cache_control"],
            json!({"type":"ephemeral","ttl":"1h"})
        );
        automatic["cache_control"]["ttl"] = json!("1d");
        assert!(
            p.decode_generation_request(
                automatic.to_string().as_bytes(),
                LocalScope::new(1),
                &target()
            )
            .is_err()
        );
    }
}
mod anthropic_media {
    //! Independent native image/document oracles; inert sources, no fetch or file API.
    #[test]
    fn native_image_count_includes_tool_result_images() {
        use morphiecore::protocol::anthropic::Profile;
        use serde_json::json;
        let image = json!({"type":"image","source":{
            "type":"base64","media_type":"image/png","data":"AQID"}});
        let mut wire = json!({"model":"synthetic","max_tokens":32,"messages":[
            {"role":"user","content":vec![image.clone();599]},
            {"role":"assistant","content":[
                {"type":"tool_use","id":"c","name":"read","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"c",
                "content":[image.clone()]}]}]});
        let profile = Profile::AdaptiveTextTools;
        let request = profile.decode_request(wire.to_string().as_bytes()).unwrap();
        assert!(profile.encode_request(&request).is_ok());
        wire["messages"][2]["content"][0]["content"]
            .as_array_mut()
            .unwrap()
            .push(image);
        assert!(profile.decode_request(wire.to_string().as_bytes()).is_err());
    }

    #[test]
    fn native_image_size_counts_encoded_base64_bytes() {
        use morphiecore::protocol::anthropic::{Block, Media, MessageContent, Profile, Source};
        use serde_json::json;
        let profile = Profile::AdaptiveTextTools;
        let mut request = profile
            .decode_request(
                json!({"model":"synthetic","max_tokens":32,"messages":[
                {"role":"user","content":[{"type":"image","source":{
                    "type":"base64","media_type":"image/png","data":"AQID"}}]}]})
                .to_string()
                .as_bytes(),
            )
            .unwrap();
        let MessageContent::Blocks(blocks) = &mut request.messages[0].content else {
            panic!("image blocks")
        };
        blocks[0].content = Block::Media(Media::Image {
            source: Source::Base64 {
                media_type: "image/png".into(),
                data: "AAAA".repeat(10_000_000 / 4),
            },
            reject_oversized: false,
        });
        assert!(profile.encode_request(&request).is_ok());
        let MessageContent::Blocks(blocks) = &mut request.messages[0].content else {
            unreachable!()
        };
        let Block::Media(Media::Image {
            source: Source::Base64 { data, .. },
            ..
        }) = &mut blocks[0].content
        else {
            unreachable!()
        };
        data.push_str("AAAA");
        assert!(profile.encode_request(&request).is_err());
    }
    use morphiecore::{
        protocol::{anthropic::Profile, native::ReplayTarget},
        semantic::task::generation::*,
        topology::catalog,
    };
    use serde_json::{Value, json};

    fn target() -> ReplayTarget {
        ReplayTarget::new("synthetic-provider", "synthetic-model").unwrap()
    }
    fn messages() -> Value {
        json!([
            {"role":"user","content":[
                {"type":"image","source":{"type":"base64","media_type":"image/png","data":"AQID"},
                    "transformations":{"oversized_image":"error"}},
                {"type":"image","source":{"type":"url","url":"https://example.invalid/a.jpg"}},
                {"type":"document","source":{"type":"base64","media_type":"application/pdf","data":"BAUG"},"title":"Report"},
                {"type":"document","source":{"type":"text","media_type":"text/plain","data":"notes"}},
                {"type":"text","text":"Compare."}]},
            {"role":"assistant","content":[
                {"type":"tool_use","id":"toolu_1","name":"shot","input":{}}]},
            {"role":"user","content":[
                {"type":"tool_result","tool_use_id":"toolu_1","content":[
                    {"type":"text","text":"screen"},
                    {"type":"image","source":{"type":"base64","media_type":"image/webp","data":"BwgJ"},
                        "transformations":{"oversized_image":"error"}}]}]}
        ])
    }
    fn view<'a>(request: &'a GenerationRequest, resource: &'a ResourceUse) -> ResourceView<'a> {
        resource.media(request.resources()).unwrap()
    }

    #[test]
    fn images_and_documents_map_to_inert_resources_and_back() {
        let raw = json!({"model":"synthetic-model","max_tokens":64,"messages":messages()});
        let p = Profile::AdaptiveTextTools;
        let decoded = p
            .decode_generation_request(
                &serde_json::to_vec(&raw).unwrap(),
                LocalScope::new(1),
                &target(),
            )
            .unwrap();
        let semantic = &decoded.semantic;
        let needs = GenerationRequirements::derive(semantic);
        assert_eq!(
            (
                needs.image_inputs,
                needs.file_inputs,
                needs.tool_result_media
            ),
            (3, 2, 1)
        );
        let inputs: Vec<_> = semantic
            .items()
            .iter()
            .filter_map(|(_, item)| match item {
                Item::Message(m) => match &m.parts[0].content {
                    ContentPart::Resource(r) => Some(view(semantic, r)),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        let inline = |media: &str, data: &str| -> bool {
            inputs.iter().any(|v| {
                matches!(v.location, ResourceLocation::Inline { media_type, data_base64 }
                    if media_type.as_str() == media && data_base64.as_str() == data)
            })
        };
        assert_eq!(inputs.len(), 4);
        assert_eq!(inputs[0].kind(), ResourceKind::Image);
        assert!(inline("image/png", "AQID"));
        assert!(
            matches!(inputs[1].location, ResourceLocation::Url(u) if u.as_str() == "https://example.invalid/a.jpg")
        );
        assert_eq!(inputs[2].kind(), ResourceKind::File);
        // A document title is the model-visible file name.
        assert!(
            matches!(inputs[2].description, ResourceDescription::File(FileDescription {
            filename: Some(name), detail: None }) if name.as_str() == "Report")
        );
        assert!(inline("application/pdf", "BAUG"));
        // A text document is an inline text/plain file: base64("notes").
        assert!(inline("text/plain", "bm90ZXM="));
        let Item::ToolResult(result) = &semantic.items()[6].1 else {
            panic!("tool result")
        };
        let ToolOutput::Parts(parts) = &result.output else {
            panic!("parts")
        };
        let InputPart::Resource(shot) = &parts[1].content else {
            panic!("tool image")
        };
        assert_eq!(view(semantic, shot).kind(), ResourceKind::Image);
        let encoded = p
            .encode_generation_request(semantic, &decoded.fidelity, &target())
            .unwrap();
        assert_eq!(encoded.value["messages"], messages());
        assert!(encoded.rules.is_empty());
    }

    #[test]
    fn unrepresentable_sources_and_uncalibrated_options_fail_without_losing_filenames() {
        for block in [
            json!({"type":"image","source":{"type":"file","file_id":"file_synthetic"}}),
            json!({"type":"document","source":{"type":"content","content":"x"}}),
            json!({"type":"document","context":"private","source":{"type":"url","url":"https://example.invalid/a.pdf"}}),
            json!({"type":"image","source":{"type":"base64","media_type":"image/bmp","data":"AQID"}}),
            // Citations are deferred; dropping one would lose a known field silently.
            json!({"type":"text","text":"x","citations":[{"type":"char_location","cited_text":"x",
                "document_index":0,"document_title":null,"start_char_index":0,"end_char_index":1}]}),
        ] {
            let raw =
                json!({"model":"m","max_tokens":8,"messages":[{"role":"user","content":[block]}]});
            assert!(
                morphiecore::protocol::anthropic::client::decode_client_request(
                    &serde_json::to_vec(&raw).unwrap()
                )
                .is_err()
            );
        }
        let uncited = json!({"model":"m","max_tokens":8,"messages":[{"role":"user","content":[
            {"type":"text","text":"x","citations":null}]}]});
        assert!(
            morphiecore::protocol::anthropic::client::decode_client_request(
                &serde_json::to_vec(&uncited).unwrap()
            )
            .is_ok()
        );
        use morphiecore::protocol::openai::{Adapter, Dialect, Profile as Client};
        let endpoint = catalog::MESSAGES_BINDINGS[0].endpoint();
        let chat = Adapter::new(Client::Chat, Dialect::Standard, None);
        let decode = |content: Value| {
            let mut request = chat
                .decode_client_request(
                    &serde_json::to_vec(
                        &json!({"model":"m","messages":[{"role":"user","content":content}]}),
                    )
                    .unwrap(),
                )
                .unwrap();
            // The Go binding needs a session grouping.
            request.conversation = Some(
                morphiecore::semantic::context::ConversationContext::conversation("synthetic")
                    .unwrap(),
            );
            request
        };
        let request = decode(json!([
            {"type":"image_url","image_url":{"url":"https://example.invalid/a.png","detail":"low"}}]));
        assert!(endpoint.encode_request_with_rules(&request).is_err());
        let mut context = decode(json!("hello"));
        let mut settings = context.task.semantic.settings().clone();
        settings.reasoning =
            morphiecore::semantic::task::generation::ReasoningRequest::present(None);
        settings.reasoning.context = morphiecore::semantic::value::Presence::Value(
            morphiecore::semantic::task::generation::ReasoningContext::CurrentTurn,
        );
        context.task.semantic = context.task.semantic.with_settings(settings).unwrap();
        assert!(endpoint.encode_request_with_rules(&context).is_err());
        let named = decode(json!([{"type":"file","file":{
            "file_data":"data:application/pdf;base64,AQID","filename":"report.pdf"}}]));
        let (wire, rules) = endpoint.encode_request_with_rules(&named).unwrap();
        assert_eq!(
            wire["messages"][0]["content"],
            json!([{"type":"document","source":{"type":"base64","media_type":"application/pdf","data":"AQID"},"title":"report.pdf"}])
        );
        // The Chat source has no cap; Messages requires one and records the default.
        assert_eq!(wire["max_tokens"], 32_000);
        assert_eq!(rules, ["messages.default-max-tokens.v1"]);
        // A later developer message becomes a mid-conversation system message once the
        // binding's representation declares it; default admission stays closed.
        let ordered = |messages: Value| {
            let mut request = chat
                .decode_client_request(
                    &serde_json::to_vec(&json!({"model":"m","messages":messages})).unwrap(),
                )
                .unwrap();
            request.conversation = Some(
                morphiecore::semantic::context::ConversationContext::conversation("synthetic")
                    .unwrap(),
            );
            request
        };
        let later = ordered(json!([{"role":"user","content":"go"},
            {"role":"developer","content":"Be brief."}]));
        assert!(endpoint.encode_request_with_rules(&later).is_err());
        let mut widened = endpoint.clone();
        let mut caps = morphiecore::protocol::anthropic::TargetCaps::new(
            Profile::AdaptiveTextTools,
            morphiecore::protocol::anthropic::Dialect::OpenCodeGo,
        );
        caps.semantics = Profile::AdaptiveTextTools.representation_contract();
        widened.representation = caps.into();
        let (wire, rules) = widened.encode_request_with_rules(&later).unwrap();
        assert_eq!(
            wire["messages"],
            json!([{"role":"user","content":[{"type":"text","text":"go"}]},
                {"role":"system","content":[{"type":"text","text":"Be brief."}]}])
        );
        assert!(rules.contains(&"messages.developer-as-system.v1"));
        let between = ordered(json!([{"role":"user","content":"go"},
            {"role":"developer","content":"Be brief."},{"role":"user","content":"again"}]));
        assert!(widened.encode_request_with_rules(&between).is_err());
        // Messages has no audio or video carrier, and its system prompt is text.
        let audio =
            decode(json!([{"type":"input_audio","input_audio":{"data":"AQID","format":"wav"}}]));
        assert!(endpoint.encode_request_with_rules(&audio).is_err());
        let video =
            decode(json!([{"type":"video_url","video_url":{"url":"data:video/mp4;base64,AQID"}}]));
        assert!(endpoint.encode_request_with_rules(&video).is_err());
        assert_eq!(
            Adapter::new(Client::Responses, Dialect::Standard, None)
                .decode_client_request(
                    json!({"model":"m","input":[
                    {"role":"developer","content":[{"type":"input_image","image_url":"data:image/png;base64,AQID"}]},
                    {"role":"user","content":"Describe."}]})
                    .to_string()
                    .as_bytes(),
                )
                .unwrap_err(),
            morphiecore::protocol::CodecError::Unsupported("instruction content".into())
        );
        // `downsize` is the default; `error` survives only on a Messages target.
        let messages = |oversized: &str| {
            let raw = json!({"model":"m","max_tokens":8,"messages":[{"role":"user","content":[
                {"type":"image","source":{"type":"url","url":"https://example.invalid/a.png"},
                    "transformations":{"oversized_image":oversized}}]}]});
            let mut request = morphiecore::protocol::anthropic::client::decode_client_request(
                &serde_json::to_vec(&raw).unwrap(),
            )
            .unwrap();
            request.conversation = Some(
                morphiecore::semantic::context::ConversationContext::conversation("synthetic")
                    .unwrap(),
            );
            request
        };
        let image =
            json!({"type":"image","source":{"type":"url","url":"https://example.invalid/a.png"}});
        let (wire, _) = endpoint
            .encode_request_with_rules(&messages("downsize"))
            .unwrap();
        assert_eq!(wire["messages"][0]["content"], json!([image]));
        let full = morphiecore::protocol::openai::TargetCaps::full();
        assert!(
            chat.encode_request(&messages("downsize"), "synthetic", &full)
                .is_ok()
        );
        let strict = messages("error");
        let (wire, _) = endpoint.encode_request_with_rules(&strict).unwrap();
        let mut expected = image;
        expected["transformations"] = json!({"oversized_image":"error"});
        assert_eq!(wire["messages"][0]["content"], json!([expected]));
        assert!(chat.encode_request(&strict, "synthetic", &full).is_err());
    }
}
mod anthropic_output {
    //! Messages structured output: one IR owner with a native and an OpenAI carrier.

    use morphiecore::{
        adapter::Request,
        protocol::{
            anthropic::client::decode_client_request,
            openai::{Adapter, Dialect, Profile, TargetCaps},
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
            Adapter::new(Profile::Chat, Dialect::Standard, None)
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
        assert!(rules.is_empty());
        let (wire, rules) = Adapter::new(Profile::Chat, Dialect::Standard, None)
            .encode_request_with_rules(&source, "synthetic", &TargetCaps::full())
            .unwrap();
        assert_eq!(
            wire["response_format"],
            json!({"type":"json_schema","json_schema":{"name":"response","schema":schema(),"strict":true}})
        );
        assert!(rules.contains(&"openai.default-schema-name.v1"));

        let responses = Adapter::new(Profile::Responses, Dialect::Standard, None);
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
            lowering::generation::RequestControlLoss,
            protocol::{anthropic, native::ReplayTarget},
            semantic::value::Presence,
        };
        let caps = anthropic::TargetCaps::new(
            anthropic::Profile::AdaptiveTextTools,
            anthropic::Dialect::Standard,
        );
        assert!(!caps.semantics.verbosity);
        assert!(GenerationRequirements::derive(&source.task.semantic).verbosity);
        assert!(caps.semantics.check(&source.task.semantic).is_err());
        let target = ReplayTarget::new("synthetic", "synthetic").unwrap();
        let projected = caps.encode_request(&source, &target).unwrap();
        assert_eq!(
            projected.control_losses,
            [RequestControlLoss::MessagesVerbosity {
                input: Presence::Value(Verbosity::Low),
                output: Presence::Absent,
            }]
        );
        let receipt = &projected.control_losses[0];
        assert_eq!(receipt.rule(), "messages.omit-verbosity.v1");
        assert_eq!(receipt.field(), "generation.text.verbosity");
        assert_eq!(
            (receipt.direction(), receipt.action()),
            ("request.encode", "omit")
        );
        let mut constrained = caps;
        constrained.semantics.structured_output = false;
        assert!(constrained.encode_request(&source, &target).is_err());
        assert_eq!(source, saved);
        assert_eq!(
            responses
                .encode_request(&source, "m", &TargetCaps::full())
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
                "messages.omit-schema-name.v1",
                "messages.default-max-tokens.v1"
            ]
        );
    }
}
mod client_fidelity {
    use morphiecore::{
        protocol::{
            CodecError, DecodedResponse,
            anthropic::{
                client::{decode_client_request, encode_client_response},
                client_events::ClientEventEncoder,
            },
            openai::{Adapter, Dialect, Profile},
        },
        semantic::{task::generation::*, value::Presence},
    };
    use serde_json::json;

    fn response() -> DecodedResponse {
        // Orthogonal content/usage fixtures carry an explicit, representable terminal.
        DecodedResponse {
            semantic: GenerationResponse::new(
                vec![(
                    ItemId::new(1),
                    Item::Message(Message {
                        role: MessageRole::Assistant,
                        status: ItemLifecycle::Completed,
                        phase: None,
                        parts: vec![Part::new(
                            PartId::new(1),
                            ContentPart::Text(
                                morphiecore::semantic::value::Text::new("answer", "fixture", 64)
                                    .unwrap()
                                    .into(),
                            ),
                        )],
                    }),
                )],
                Outcome::Completed,
            )
            .unwrap()
            .with_progress(InteractionProgress::TurnFinished)
            .unwrap()
            .with_details(TerminalDetails {
                completion: Some(CompletionCause::Natural),
                ..Default::default()
            })
            .unwrap(),
            fidelity: Default::default(),
            metadata: morphiecore::protocol::ResponseMetadata {
                id: Some("r".into()),
                model: "synthetic".into(),
                storage: None,
                created: None,
                context: Default::default(),
                instruction_fidelity: Default::default(),
            },
        }
    }
    fn terminal() -> StreamEvent {
        StreamEvent::Terminal {
            terminal: StreamTerminal::Completed,
            details: TerminalDetails {
                completion: Some(CompletionCause::Natural),
                ..Default::default()
            },
        }
    }
    #[test]
    fn messages_final_usage_requires_reported_counts_and_unambiguous_cache_categories() {
        let mut response = response();
        assert!(encode_client_response(&response, "m").is_err());
        let mut stream = ClientEventEncoder::new("r", "m");
        stream.encode(&StreamEvent::Started).unwrap();
        stream
            .encode(&StreamEvent::Progress(InteractionProgress::TurnFinished))
            .unwrap();
        assert!(stream.encode(&terminal()).is_err());
        assert!(stream.finish().is_err());

        let mut usage = Usage::operation(10, 3, 13);
        usage.cached_input_tokens = Some(2);
        usage.input_cache_write_tokens = Some(0);
        response.semantic = response.semantic.with_usage(Some(usage)).unwrap();
        let mut unknown = response.clone();
        unknown.semantic = unknown
            .semantic
            .with_details(TerminalDetails::default())
            .unwrap()
            .with_progress(InteractionProgress::Unreported)
            .unwrap();
        assert!(matches!(
            encode_client_response(&unknown, "m"),
            Err(morphiecore::protocol::CodecError::Unsupported(reason))
                if reason == "Messages terminal"
        ));
        let mut unknown_stream = ClientEventEncoder::new("r", "m");
        unknown_stream.encode(&StreamEvent::Usage(usage)).unwrap();
        assert!(matches!(
            unknown_stream.encode(&StreamEvent::Terminal {
                terminal: StreamTerminal::Completed,
                details: TerminalDetails::default(),
            }),
            Err(morphiecore::protocol::CodecError::Unsupported(reason))
                if reason == "Messages terminal"
        ));
        unknown.semantic = unknown
            .semantic
            .with_progress(InteractionProgress::NeedsContinuation)
            .unwrap();
        assert!(
            morphiecore::adapter::ClientAdapter::Messages
                .snapshot(unknown)
                .is_err()
        );
        let mut continuation_stream = ClientEventEncoder::new("r", "m");
        continuation_stream
            .encode(&StreamEvent::Usage(usage))
            .unwrap();
        continuation_stream
            .encode(&StreamEvent::Progress(
                InteractionProgress::NeedsContinuation,
            ))
            .unwrap();
        assert!(matches!(
            continuation_stream.encode(&terminal()),
            Err(morphiecore::protocol::CodecError::Unsupported(reason))
                if reason == "Messages continuation"
        ));
        assert_eq!(
            encode_client_response(&response, "m").unwrap()["usage"],
            json!({"input_tokens":8,"output_tokens":3,
                "cache_read_input_tokens":2,"cache_creation_input_tokens":0})
        );
        use morphiecore::semantic::context::ServiceClass;
        response.metadata.context.service_class = Presence::Value(ServiceClass::StandardCapacity);
        assert_eq!(
            encode_client_response(&response, "m").unwrap()["usage"]["service_tier"],
            "standard"
        );
        let mut wrong_service = response.clone();
        wrong_service.metadata.context.service_class =
            Presence::Value(ServiceClass::StandardProcessing);
        assert!(encode_client_response(&wrong_service, "m").is_err());
        let mut late_service = ClientEventEncoder::new("r", "m");
        late_service.encode(&StreamEvent::Started).unwrap();
        assert!(
            late_service
                .update_service_class(Presence::Value(ServiceClass::StandardCapacity))
                .is_err()
        );
        assert!(late_service.encode(&terminal()).is_err());
        assert!(late_service.finish().is_err());
        let mut ambiguous = response.clone();
        ambiguous.semantic = ambiguous
            .semantic
            .clone()
            .with_details(TerminalDetails {
                completion: Some(CompletionCause::NaturalOrStopSequence),
                ..Default::default()
            })
            .unwrap();
        assert!(encode_client_response(&ambiguous, "m").is_err());
        let mut ambiguous_stream = ClientEventEncoder::new("r", "m");
        ambiguous_stream.encode(&StreamEvent::Usage(usage)).unwrap();
        ambiguous_stream
            .encode(&StreamEvent::Progress(InteractionProgress::TurnFinished))
            .unwrap();
        assert!(
            ambiguous_stream
                .encode(&StreamEvent::Terminal {
                    terminal: StreamTerminal::Completed,
                    details: ambiguous.semantic.details().clone(),
                })
                .is_err()
        );
        let original = response.clone();
        let (retained, wire) = morphiecore::adapter::ClientAdapter::Messages
            .snapshot(response.clone())
            .unwrap();
        assert_eq!(wire["usage"]["input_tokens"], 8);
        assert_eq!(wire["usage"]["service_tier"], "standard");
        assert_eq!(
            retained.metadata.context.service_class,
            response.metadata.context.service_class
        );
        let retained_usage = retained.semantic.usage().unwrap();
        assert_eq!(retained_usage.input_tokens, Some(8));
        assert_eq!(
            retained_usage.input_relation,
            InputTokenRelation::ExcludesCacheReadAndWrite
        );
        assert_eq!(retained_usage.total_tokens, None);
        assert_eq!(response, original);
        let mut stream = ClientEventEncoder::new("r", "m");
        stream
            .update_service_class(response.metadata.context.service_class.clone())
            .unwrap();
        let mut frames = stream.encode(&StreamEvent::Usage(usage)).unwrap();
        frames.extend(
            stream
                .encode(&StreamEvent::Progress(InteractionProgress::TurnFinished))
                .unwrap(),
        );
        frames.extend(stream.encode(&terminal()).unwrap());
        let values: Vec<serde_json::Value> = frames
            .iter()
            .map(|frame| {
                let data = std::str::from_utf8(frame)
                    .unwrap()
                    .lines()
                    .find_map(|line| line.strip_prefix("data: "))
                    .unwrap();
                serde_json::from_str(data).unwrap()
            })
            .collect();
        assert_eq!(values[0]["message"]["usage"]["service_tier"], "standard");
        assert!(values[1]["usage"].get("service_tier").is_none());
        assert!(frames.iter().any(|frame| {
            std::str::from_utf8(frame)
                .unwrap()
                .contains("\"input_tokens\":8")
        }));
        stream.finish().unwrap();

        // Inclusive cache categories may overlap; their sum cannot be subtracted.
        response.metadata.context.service_class = Presence::Absent;
        usage.input_cache_write_tokens = Some(1);
        response.semantic = response.semantic.with_usage(Some(usage)).unwrap();
        assert!(encode_client_response(&response, "m").is_err());

        // OpenAI and Google report no write category; it stays absent, not zero.
        usage.input_cache_write_tokens = None;
        response.semantic = response.semantic.with_usage(Some(usage)).unwrap();
        assert_eq!(
            encode_client_response(&response, "m").unwrap()["usage"],
            json!({"input_tokens":8,"output_tokens":3,"cache_read_input_tokens":2})
        );
        usage.reasoning_tokens = Some(1);
        response.semantic = response.semantic.with_usage(Some(usage)).unwrap();
        assert_eq!(
            encode_client_response(&response, "m").unwrap()["usage"]["output_tokens_details"],
            json!({"thinking_tokens":1})
        );
        usage.reasoning_tokens = None;
        // Without the read subset, input cannot be split.
        usage.cached_input_tokens = None;
        response.semantic = response.semantic.with_usage(Some(usage)).unwrap();
        assert!(encode_client_response(&response, "m").is_err());
    }
    #[test]
    fn messages_rejects_audio_in_static_output_and_each_event_shape() {
        let mut response = response();
        let mut usage = Usage::operation(0, 3, 3);
        usage.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
        response.semantic = response.semantic.with_usage(Some(usage)).unwrap();
        let mut audio = response.clone();
        let mut items = audio.semantic.items().to_vec();
        let Item::Message(message) = &mut items[0].1 else {
            panic!("message")
        };
        let text = |s: &str| morphiecore::semantic::value::Text::new(s, "synthetic", 128).unwrap();
        message.parts[0].content = ContentPart::Audio(
            GeneratedAudio::new(
                AudioReference::new(text("a"), Some(2000)).unwrap(),
                text("AQID"),
                text("answer"),
            )
            .unwrap(),
        );
        audio.semantic = audio.semantic.with_items(items).unwrap();
        assert!(encode_client_response(&audio, "m").is_err());
        for event in [
            StreamEvent::PartStarted {
                item: ItemId::new(1),
                part: PartId::new(1),
                kind: PartKind::Audio,
            },
            StreamEvent::AudioDelta {
                item: ItemId::new(1),
                part: PartId::new(1),
                update: AudioUpdate {
                    data: Some(vec![1, 2, 3]),
                    ..Default::default()
                },
            },
        ] {
            let mut stream = ClientEventEncoder::new("r", "m");
            stream.encode(&StreamEvent::Started).unwrap();
            stream
                .encode(&crate::events_support::start(
                    1,
                    ItemKind::Message { phase: None },
                ))
                .unwrap();
            assert_eq!(
                stream.encode(&event).unwrap_err(),
                CodecError::Unsupported("Messages audio output".into()),
                "{event:?}"
            );
            assert_eq!(
                stream.encode(&StreamEvent::Started).unwrap_err(),
                CodecError::Invalid("Messages stream closed")
            );
            assert!(stream.finish().is_err());
        }
    }

    #[test]
    fn messages_preserves_parts_and_rejects_unrepresentable_calls() {
        let mut response = response();
        let mut usage = Usage::operation(0, 3, 3);
        usage.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
        response.semantic = response.semantic.with_usage(Some(usage)).unwrap();
        let text = |s: &str| morphiecore::semantic::value::Text::new(s, "synthetic", 128).unwrap();
        assert_eq!(
            encode_client_response(&response, "m").unwrap()["content"],
            json!([{"type":"text","text":"answer"}])
        );
        // An incomplete terminal closes the started text block by its wire index.
        let mut truncated = ClientEventEncoder::new("r", "m");
        let mut frames = vec![];
        for event in [
            StreamEvent::Started,
            StreamEvent::ItemStarted {
                item: ItemId::new(1),
                kind: ItemKind::Message { phase: None },
                replay: None,
            },
            StreamEvent::PartStarted {
                item: ItemId::new(1),
                part: PartId::new(1),
                kind: PartKind::Refusal,
            },
            StreamEvent::Usage(usage),
            StreamEvent::Terminal {
                terminal: StreamTerminal::Incomplete,
                details: TerminalDetails {
                    incomplete: Some(IncompleteReason::MaxOutputTokens),
                    ..Default::default()
                },
            },
        ] {
            frames.extend(truncated.encode(&event).unwrap());
        }
        truncated.finish().unwrap();
        let frames: String = frames
            .iter()
            .map(|frame| std::str::from_utf8(frame).unwrap())
            .collect();
        assert!(frames.contains(r#"{"type":"content_block_stop","index":0}"#));
        assert!(!frames.contains(r#""index":null"#));
        let mut split = response.clone();
        let mut items = split.semantic.items().to_vec();
        let Item::Message(message) = &mut items[0].1 else {
            panic!("message")
        };
        for (id, value) in [(2, ""), (3, "tail")] {
            message.parts.push(Part {
                replay: None,
                id: PartId::new(id),
                cache: None,
                content: ContentPart::Text(
                    morphiecore::semantic::value::Text::allowing_empty(value, "test", 128)
                        .unwrap()
                        .into(),
                ),
            });
        }
        split.semantic = split.semantic.with_items(items).unwrap();
        let (retained, wire) = morphiecore::adapter::ClientAdapter::Messages
            .snapshot(split.clone())
            .unwrap();
        assert_eq!(retained.semantic.items(), split.semantic.items());
        assert_eq!(
            wire["content"],
            json!([
            {"type":"text","text":"answer"},{"type":"text","text":""},{"type":"text","text":"tail"}])
        );
        let mut stream = ClientEventEncoder::new("r", "m");
        let mut frames = stream
            .encode(&crate::events_support::start(
                1,
                ItemKind::Message { phase: None },
            ))
            .unwrap();
        for (id, value) in [(1, "answer"), (2, ""), (3, "tail")] {
            for event in crate::events_support::part(1, id, PartKind::Text, value) {
                frames.extend(stream.encode(&event).unwrap());
            }
        }
        frames.extend(
            stream
                .encode(&crate::events_support::close(1, ItemLifecycle::Completed))
                .unwrap(),
        );
        let blocks: Vec<serde_json::Value> = frames
            .iter()
            .filter_map(|frame| {
                std::str::from_utf8(frame)
                    .unwrap()
                    .lines()
                    .find_map(|line| line.strip_prefix("data: "))
                    .map(|data| serde_json::from_str(data).unwrap())
            })
            .filter(|value: &serde_json::Value| value["type"] != "message_start")
            .collect();
        assert_eq!(
            blocks,
            vec![
                json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
                json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"answer"}}),
                json!({"type":"content_block_stop","index":0}),
                json!({"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}),
                json!({"type":"content_block_stop","index":1}),
                json!({"type":"content_block_start","index":2,"content_block":{"type":"text","text":""}}),
                json!({"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"tail"}}),
                json!({"type":"content_block_stop","index":2}),
            ]
        );
        let mut call = ToolCall {
            replay: None,
            call_id: text("c"),
            cache: None,
            name: text("lookup"),
            arguments: ToolArguments::Raw("{}".into()),
            status: ItemLifecycle::Completed,
            context: Default::default(),
        };
        let mut unknown_call = response.clone();
        unknown_call.semantic = GenerationResponse::new(
            vec![(ItemId::new(1), Item::ToolCall(call.clone()))],
            Outcome::Completed,
        )
        .unwrap()
        .with_usage(Some(usage))
        .unwrap();
        let (retained, wire) = morphiecore::adapter::ClientAdapter::Messages
            .snapshot(unknown_call)
            .unwrap();
        assert_eq!(wire["stop_reason"], "tool_use");
        assert_eq!(
            retained.semantic.progress(),
            InteractionProgress::Unreported
        );
        let mut unknown_stream = ClientEventEncoder::new("r", "m");
        unknown_stream
            .encode(&crate::events_support::start(
                1,
                ItemKind::ToolCall {
                    format: ArgumentFormat::Raw,
                    call_id: call.call_id.clone(),
                    name: call.name.clone(),
                    context: call.context.clone(),
                },
            ))
            .unwrap();
        unknown_stream.encode(&StreamEvent::Usage(usage)).unwrap();
        let terminal_frames = unknown_stream
            .encode(&StreamEvent::Terminal {
                terminal: StreamTerminal::Completed,
                details: TerminalDetails::default(),
            })
            .unwrap();
        assert!(terminal_frames.iter().any(|frame| {
            std::str::from_utf8(frame)
                .unwrap()
                .contains(r#""stop_reason":"tool_use""#)
        }));
        let tool_response = |call: ToolCall| {
            let mut value = response.clone();
            value.semantic = GenerationResponse::from_resources(
                vec![(ItemId::new(1), Item::ToolCall(call))],
                Outcome::Completed,
                ResourceTable::default(),
            )
            .unwrap()
            .with_progress(InteractionProgress::AwaitingToolResults)
            .unwrap()
            .with_usage(Some(usage))
            .unwrap();
            value
        };
        assert_eq!(
            encode_client_response(&tool_response(call.clone()), "m").unwrap()["content"],
            json!([{"type":"tool_use","id":"c","name":"lookup","input":{}}])
        );
        call.context.namespace = Some(text("private"));
        assert!(encode_client_response(&tool_response(call.clone()), "m").is_err());
        let mut stream = ClientEventEncoder::new("r", "m");
        assert!(
            stream
                .encode(&crate::events_support::start(
                    1,
                    ItemKind::ToolCall {
                        format: ArgumentFormat::Raw,
                        call_id: call.call_id.clone(),
                        name: call.name.clone(),
                        context: call.context.clone()
                    }
                ))
                .is_err()
        );
        call.context = Default::default();
        call.arguments = ToolArguments::Raw(r#"{"x":1,"x":2}"#.into());
        assert!(encode_client_response(&tool_response(call.clone()), "m").is_err());
        call.arguments = ToolArguments::Raw("1".into());
        assert!(encode_client_response(&tool_response(call), "m").is_err());
    }

    #[test]
    fn messages_rejects_citations_in_static_output_and_each_event_shape() {
        let mut response = response();
        let mut usage = Usage::operation(0, 3, 3);
        usage.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
        response.semantic = response.semantic.with_usage(Some(usage)).unwrap();
        let resource = ResourceId::scoped(LocalScope::ROOT, 1);
        let resources = ResourceTable::default()
            .insert(
                resource,
                ResourceLocation::Url(
                    morphiecore::semantic::value::Text::new(
                        "https://example.invalid/source",
                        "synthetic",
                        128,
                    )
                    .unwrap(),
                ),
            )
            .unwrap();
        let annotation = Annotation::new(
            Citation {
                kind: CitationKind::Citation,
                claim: ClaimAnchor::Whole,
                source: resource,
                coordinates: SourceCoordinates::Unreported,
                label: None,
                reported_ordinal: None,
            },
            &resources,
        )
        .unwrap();
        let mut items = response.semantic.items().to_vec();
        let Item::Message(message) = &mut items[0].1 else {
            panic!("message")
        };
        let ContentPart::Text(text) = &message.parts[0].content else {
            panic!("text")
        };
        message.parts[0].content = ContentPart::Text(
            TextContent::new(
                morphiecore::semantic::value::Text::new(text.as_str(), "synthetic", 128).unwrap(),
                vec![annotation.clone()],
                Presence::Absent,
            )
            .unwrap(),
        );
        response.semantic = response
            .semantic
            .with_resources(resources)
            .unwrap()
            .with_items(items)
            .unwrap();
        assert!(encode_client_response(&response, "m").is_err());
        for event in [
            StreamEvent::AnnotationAdded {
                item: ItemId::new(1),
                part: PartId::new(1),
                annotation: annotation.clone(),
            },
            StreamEvent::TextMetadata {
                item: ItemId::new(1),
                part: PartId::new(1),
                annotations: vec![annotation],
                logprobs: Presence::Absent,
            },
        ] {
            let mut stream = ClientEventEncoder::new("r", "m");
            stream.encode(&StreamEvent::Started).unwrap();
            stream
                .encode(&crate::events_support::start(
                    1,
                    ItemKind::Message { phase: None },
                ))
                .unwrap();
            stream
                .encode(&StreamEvent::PartStarted {
                    item: ItemId::new(1),
                    part: PartId::new(1),
                    kind: PartKind::Text,
                })
                .unwrap();
            assert_eq!(
                stream.encode(&event).unwrap_err(),
                CodecError::Unsupported("Messages citations".into()),
                "{event:?}"
            );
        }
    }
    #[test]
    fn client_intake_carries_caps_and_rejects_active_fields_it_cannot_carry() {
        let body = json!({"model":"m","max_tokens":8,"stop_sequences":[],
            "messages":[{"role":"user","content":"go"}]});
        assert!(decode_client_request(body.to_string().as_bytes()).is_ok());
        let mut strict = body.clone();
        strict["tools"] = json!([{"name":"lookup","input_schema":{"type":"object"},"strict":true}]);
        let decoded = decode_client_request(strict.to_string().as_bytes()).unwrap();
        assert!(matches!(&decoded.task.semantic.tools()[0],
            ToolDefinition::Function(t) if t.strict == FunctionStrictness::Explicit(true)));
        use morphiecore::protocol::{anthropic, native::ReplayTarget};
        let target = ReplayTarget::new("synthetic", "synthetic").unwrap();
        let mut caps = anthropic::TargetCaps::new(
            anthropic::Profile::AdaptiveTextTools,
            anthropic::Dialect::Standard,
        );
        use morphiecore::semantic::context::ServicePolicy;
        let mut selected = body.clone();
        selected["service_tier"] = json!("standard_only");
        let selected = decode_client_request(selected.to_string().as_bytes()).unwrap();
        assert_eq!(
            selected.service_policy,
            Presence::Value(ServicePolicy::StandardCapacityOnly)
        );
        assert!(selected.check_context(false).is_err());
        assert_eq!(
            caps.encode_request(&selected, &target).unwrap().value["service_tier"],
            "standard_only"
        );
        let mut manual = decode_client_request(body.to_string().as_bytes()).unwrap();
        manual.service_policy = Presence::Value(ServicePolicy::PreferCommittedCapacity);
        let wire = caps.encode_request(&manual, &target).unwrap().value;
        assert_eq!(wire["service_tier"], "auto");
        assert!(
            anthropic::Profile::AdaptiveTextTools
                .decode_generation_request(wire.to_string().as_bytes(), LocalScope::ROOT, &target)
                .is_err()
        );
        assert!(
            Adapter::new(Profile::Responses, Dialect::Standard, None)
                .encode_request(
                    &manual,
                    "synthetic",
                    &morphiecore::protocol::openai::TargetCaps::full()
                )
                .is_err()
        );
        manual.service_policy = Presence::Value(ServicePolicy::ProjectConfigured);
        assert!(caps.encode_request(&manual, &target).is_err());
        assert!(caps.encode_request(&decoded, &target).is_err());
        caps.semantics.strict_tools = true;
        assert!(caps.encode_request(&decoded, &target).is_err());
        strict["tools"][0]["input_schema"]["additionalProperties"] = json!(false);
        let eligible = decode_client_request(strict.to_string().as_bytes()).unwrap();
        let projected = caps.encode_request(&eligible, &target).unwrap();
        assert_eq!(
            projected.value["tools"],
            json!([
                {"name":"lookup","input_schema":{"type":"object","additionalProperties":false},"strict":true}
            ])
        );
        assert!(projected.rules.is_empty());
        for (key, value) in [
            ("defer_loading", json!(true)),
            ("allowed_callers", json!(["code_execution_20250825"])),
            ("input_examples", json!([{}])),
        ] {
            let mut active = strict.clone();
            active["tools"][0][key] = value;
            assert!(
                decode_client_request(active.to_string().as_bytes()).is_err(),
                "{key}"
            );
        }
        // The IR owns top_k; a Messages target rejects it at projection, not intake.
        let mut sampled = body.clone();
        sampled["top_k"] = json!(4);
        let sampled = decode_client_request(sampled.to_string().as_bytes()).unwrap();
        assert_eq!(sampled.task.semantic.controls().top_k, Some(4));
        // Mid-conversation system messages are ordered, text-only instructions.
        let mut system = body.clone();
        system["messages"] = json!([{"role":"user","content":"go"},
            {"role":"system","content":[{"type":"text","text":"Be brief."}]}]);
        let decoded = decode_client_request(system.to_string().as_bytes()).unwrap();
        assert!(
            matches!(&decoded.task.semantic.items()[1].1, Item::Instruction(i)
            if i.authority == InstructionAuthority::System
                && i.texts().map(|(_, t)| t).collect::<Vec<_>>() == ["Be brief."])
        );
        for messages in [
            json!([{"role":"system","content":"first"},{"role":"user","content":"go"}]),
            json!([{"role":"user","content":"go"},{"role":"system","content":"x"},
                {"role":"user","content":"again"}]),
            json!([{"role":"user","content":"go"},{"role":"system","content":[
                {"type":"image","source":{"type":"url","url":"https://example.invalid/a.png"}}]}]),
            json!([{"role":"user","content":"go"},{"role":"system","content":"x",
                "clear_at":"next_user_message"}]),
        ] {
            system["messages"] = messages;
            assert!(decode_client_request(system.to_string().as_bytes()).is_err());
        }
        let mut thinking = body.clone();
        thinking["messages"][0]["content"] =
            json!([{"type":"thinking","thinking":"plan","signature":"synthetic"}]);
        assert!(decode_client_request(thinking.to_string().as_bytes()).is_err());
        thinking["messages"][0]["role"] = json!("assistant");
        assert!(decode_client_request(thinking.to_string().as_bytes()).is_ok());
        let mut cached_content = body.clone();
        cached_content["messages"][0]["content"] = json!([
            {"type":"text","text":"go","cache_control":{"type":"ephemeral","ttl":"1h"}}
        ]);
        let cached = decode_client_request(cached_content.to_string().as_bytes()).unwrap();
        cached_content["messages"][0]["content"][0]["cache_control"]["future_cache_metadata"] =
            json!(true);
        let unknown = decode_client_request(cached_content.to_string().as_bytes()).unwrap();
        assert_eq!(unknown.task.semantic, cached.task.semantic);
        let mut calls = body.clone();
        calls["messages"] = json!([
            {"role":"assistant","content":[{"type":"tool_use","id":"c","name":"lookup","input":{},
                "caller":{"type":"direct"}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"c","content":"ok"}]}]);
        assert!(decode_client_request(calls.to_string().as_bytes()).is_ok());
        // `content` is optional on a tool result; absence is an empty result.
        let mut bare = calls.clone();
        bare["messages"][1]["content"] =
            json!([{"type":"tool_result","tool_use_id":"c","is_error":true}]);
        let bare = decode_client_request(bare.to_string().as_bytes()).unwrap();
        assert!(
            matches!(&bare.task.semantic.items()[1].1, Item::ToolResult(r)
            if matches!(&r.output, ToolOutput::Parts(parts) if parts.is_empty())
                && r.is_error == Some(true))
        );
        calls["messages"][0]["content"][0]["caller"] =
            json!({"type":"code_execution_20250825","tool_id":"program_1"});
        assert!(decode_client_request(calls.to_string().as_bytes()).is_err());
        for (key, value) in [
            ("stop_sequences", json!([7])),
            ("service_tier", json!("priority")),
            ("container", json!("container_1")),
            (
                "mcp_servers",
                json!([{"type":"url","url":"https://example.invalid","name":"x"}]),
            ),
            (
                "context_management",
                json!({"edits":[{"type":"clear_tool_uses_20250919"}]}),
            ),
            ("compaction", json!({})),
        ] {
            let mut active = body.clone();
            active[key] = value;
            assert!(
                decode_client_request(active.to_string().as_bytes()).is_err(),
                "{key}"
            );
        }

        // The client-facing Chat dialect accepts the deprecated cap spelling.
        let chat = Adapter::new(Profile::Chat, Dialect::MorphieCore, None);
        let decode =
            |body: &serde_json::Value| chat.decode_client_request(body.to_string().as_bytes());
        // Empty stop values request no stop control.
        let body = json!({"model":"m","max_tokens":7,"stop":"","functions":[],
            "messages":[{"role":"user","content":"go"}]});
        assert_eq!(
            decode(&body)
                .unwrap()
                .task
                .semantic
                .settings()
                .controls
                .max_output_tokens,
            Some(7)
        );
        let mut prediction = body.clone();
        prediction["prediction"] = json!({"type":"content","content":"synthetic predicted output"});
        let decoded = decode(&prediction).unwrap();
        assert_eq!(decoded.task.semantic, decode(&body).unwrap().task.semantic);
        assert!(
            decoded
                .task
                .fidelity
                .normalizations()
                .iter()
                .any(|normalization| matches!(
                    normalization,
                    morphiecore::protocol::fidelity::Normalization::Omitted { rule, field }
                        if *rule == "chat.omit-prediction.v1" && *field == "generation.prediction"
                ))
        );
        assert!(!format!("{:?}", decoded.task.fidelity).contains("synthetic predicted output"));
        let wire = chat
            .encode_request(
                &decoded,
                "m",
                &morphiecore::protocol::openai::TargetCaps::full(),
            )
            .unwrap();
        assert!(wire.get("prediction").is_none());
        prediction["prediction"]["content"] = json!(7);
        assert!(decode(&prediction).is_err());
        let mut controlled = body.clone();
        controlled["stop"] = json!(["END", "DONE"]);
        controlled["seed"] = json!(0);
        controlled["presence_penalty"] = json!(0.25);
        controlled["frequency_penalty"] = json!(-0.5);
        let controlled = decode(&controlled).unwrap();
        assert_eq!(controlled.task.semantic.controls().seed, Some(0));
        assert_eq!(
            controlled.task.semantic.controls().presence_penalty(),
            Some(0.25)
        );
        assert_eq!(
            controlled.task.semantic.controls().frequency_penalty(),
            Some(-0.5)
        );
        assert_eq!(
            controlled
                .task
                .semantic
                .controls()
                .stop_sequences
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
            ["END", "DONE"]
        );
        let wire = chat
            .encode_request(
                &controlled,
                "m",
                &morphiecore::protocol::openai::TargetCaps::full(),
            )
            .unwrap();
        assert_eq!(wire["stop"], json!(["END", "DONE"]));
        assert_eq!(wire["seed"], 0);
        assert_eq!(wire["presence_penalty"], 0.25);
        assert_eq!(wire["frequency_penalty"], -0.5);
        for (key, value) in [
            ("max_completion_tokens", json!(7)),
            ("stop", json!([7])),
            (
                "functions",
                json!([{"name":"lookup","parameters":{"type":"object"}}]),
            ),
            ("function_call", json!("auto")),
            ("web_search_options", json!({})),
            ("seed", json!("0")),
            ("frequency_penalty", json!(3)),
            ("presence_penalty", json!(3)),
            ("logit_bias", json!({"1": 1})),
            // Stored completions need storage the gateway does not provide.
            ("store", json!(true)),
        ] {
            let mut active = body.clone();
            active[key] = value;
            assert!(decode(&active).is_err(), "{key}");
        }
        let mut stored = body.clone();
        stored["store"] = json!(false);
        assert!(decode(&stored).is_ok());
        // A participant name is model-visible and has no carrier.
        let mut named = body.clone();
        named["messages"][0]["name"] = json!("alice");
        assert!(decode(&named).is_err());
        named["messages"][0]["name"] = serde_json::Value::Null;
        assert!(decode(&named).is_ok());
        // Assistant history may mix text and refusal parts; both keep their kinds.
        let mut refused = body.clone();
        refused["messages"] = json!([{"role":"user","content":"go"},
            {"role":"assistant","content":[{"type":"text","text":"partial"},
                {"type":"refusal","refusal":"no"}]},
            {"role":"user","content":"again"}]);
        let refused = decode(&refused).unwrap();
        assert!(
            matches!(&refused.task.semantic.items()[1].1, Item::Message(m)
            if matches!(m.parts.as_slice(), [
                Part { content: ContentPart::Text(_), .. },
                Part { content: ContentPart::Refusal(_), .. }
            ]))
        );
        let wire = chat
            .encode_request(
                &refused,
                "m",
                &morphiecore::protocol::openai::TargetCaps::full(),
            )
            .unwrap();
        assert_eq!(
            wire["messages"][1]["content"],
            json!([{"type":"text","text":"partial"},{"type":"refusal","refusal":"no"}])
        );
    }
}
