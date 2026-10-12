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
    fn message() -> Value {
        json!({"id":"msg_synthetic","type":"message","role":"assistant","model":"synthetic-model",
        "content":[{"type":"thinking","thinking":"","signature":"synthetic-signature"},
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
        let decoded = p
            .decode_response(&message().to_string().into_bytes())
            .unwrap();
        assert_eq!(p.encode_response(&decoded).unwrap(), message());
        assert!(!format!("{decoded:?}").contains("synthetic-signature"));
        // Unknown upstream fields are ignored; known ones are re-encoded unchanged.
        let mut unknown = message();
        unknown["created_at"] = json!(0);
        unknown["usage"]["service_tier"] = json!("standard");
        unknown["usage"]["cache_creation"] = json!({"ephemeral_5m_input_tokens":0});
        unknown["content"][1]["extra"] = json!(null);
        let decoded = p.decode_response(unknown.to_string().as_bytes()).unwrap();
        assert_eq!(p.encode_response(&decoded).unwrap(), message());
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
        for strict in [Value::Null, json!(true)] {
            let mut body = request();
            body["tools"][0]["strict"] = strict;
            assert!(p.decode_request(body.to_string().as_bytes()).is_err());
        }
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
        semantic::{task::generation::*, value::Text},
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
                    call_id: Text::new("c", "call", 64).unwrap(),
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
                Some(IncompleteReason::MaxOutputTokens),
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
        }
        assert!(decode("end_turn", json!("END")).is_err());
        assert!(decode("stop_sequence", Value::Null).is_err());
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
    fn ordered_system_blocks_and_adaptive_controls_have_independent_semantic_owners() {
        let p = Profile::AdaptiveTextTools;
        let raw = br#"{"model":"synthetic-model","max_tokens":32,"system":[{"type":"text","text":"one"},{"type":"text","text":"two"}],"thinking":{"type":"adaptive","display":"omitted"},"output_config":{"effort":"low"},"cache_control":{"type":"ephemeral"},"messages":[{"role":"user","content":"go"}]}"#;
        let decoded = p
            .decode_generation_request(raw, LocalScope::new(1), &target())
            .unwrap();
        assert!(decoded.semantic.instructions().is_absent());
        let Item::Instruction(i) = &decoded.semantic.items()[0].1 else {
            panic!()
        };
        assert_eq!(i.authority, InstructionAuthority::System);
        assert_eq!(
            i.parts.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(),
            ["one", "two"]
        );
        let encoded = p
            .encode_generation_request(&decoded.semantic, &decoded.fidelity, &target())
            .unwrap();
        assert_eq!(
            encoded.value["system"],
            json!([{"type":"text","text":"one"},{"type":"text","text":"two"}])
        );
        assert_eq!(
            encoded.value["thinking"],
            json!({"type":"adaptive","display":"omitted"})
        );
        assert_eq!(encoded.value["output_config"], json!({"effort":"low"}));
        assert_eq!(encoded.value["cache_control"], json!({"type":"ephemeral"}));
        assert_eq!(encoded.value["max_tokens"], 32);
        // Messages requires max_tokens; a source without a cap gets the fixed default.
        let mut settings = decoded.semantic.settings().clone();
        settings.controls.max_output_tokens = None;
        let uncapped = decoded.semantic.clone().with_settings(settings).unwrap();
        let encoded = p
            .encode_generation_request(&uncapped, &decoded.fidelity, &target())
            .unwrap();
        assert_eq!(encoded.value["max_tokens"], 32_000);
        let ttl = std::str::from_utf8(raw).unwrap().replace(
            r#"{"type":"ephemeral"}"#,
            r#"{"type":"ephemeral","ttl":"1h"}"#,
        );
        assert!(
            p.decode_generation_request(ttl.as_bytes(), LocalScope::new(1), &target())
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
        blocks[0] = Block::Media(Media::Image(Source::Base64 {
            media_type: "image/png".into(),
            data: "AAAA".repeat(10_000_000 / 4),
        }));
        assert!(profile.encode_request(&request).is_ok());
        let MessageContent::Blocks(blocks) = &mut request.messages[0].content else {
            unreachable!()
        };
        let Block::Media(Media::Image(Source::Base64 { data, .. })) = &mut blocks[0] else {
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
                {"type":"image","source":{"type":"base64","media_type":"image/png","data":"AQID"}},
                {"type":"image","source":{"type":"url","url":"https://example.invalid/a.jpg"}},
                {"type":"document","source":{"type":"base64","media_type":"application/pdf","data":"BAUG"}},
                {"type":"document","source":{"type":"text","media_type":"text/plain","data":"notes"}},
                {"type":"text","text":"Compare."}]},
            {"role":"assistant","content":[
                {"type":"tool_use","id":"toolu_1","name":"shot","input":{}}]},
            {"role":"user","content":[
                {"type":"tool_result","tool_use_id":"toolu_1","content":[
                    {"type":"text","text":"screen"},
                    {"type":"image","source":{"type":"base64","media_type":"image/webp","data":"BwgJ"}}]}]}
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
        assert!(inline("application/pdf", "BAUG"));
        // A text document is an inline text/plain file: base64("notes").
        assert!(inline("text/plain", "bm90ZXM="));
        let Item::ToolResult(result) = &semantic.items()[6].1 else {
            panic!("tool result")
        };
        let ToolOutput::Parts(parts) = &result.output else {
            panic!("parts")
        };
        let ToolResultPart::Resource(shot) = &parts[1].1 else {
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
    fn unrepresentable_sources_fail_and_foreign_options_are_named_losses() {
        for block in [
            json!({"type":"image","source":{"type":"file","file_id":"file_synthetic"}}),
            json!({"type":"document","source":{"type":"content","content":"x"}}),
            json!({"type":"document","context":"private","source":{"type":"url","url":"https://example.invalid/a.pdf"}}),
            json!({"type":"image","source":{"type":"base64","media_type":"image/bmp","data":"AQID"}}),
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
        let (wire, rules) = endpoint.encode_request_with_rules(&request).unwrap();
        assert_eq!(
            wire["messages"][0]["content"],
            json!([{"type":"image","source":{"type":"url","url":"https://example.invalid/a.png"}}])
        );
        assert_eq!(rules, ["messages.omit-media-options.v1"]);
        let audio =
            decode(json!([{"type":"input_audio","input_audio":{"data":"AQID","format":"wav"}}]));
        assert!(endpoint.encode_request_with_rules(&audio).is_err());
    }
}
