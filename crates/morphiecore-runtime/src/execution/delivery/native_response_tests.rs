use crate::{
    adapter::{ClientAdapter, ClientProjection, UpstreamAdapter, emit_response_events},
    execution::{Attempt, ResponseDelivery, replay},
    protocol::{
        CodecError, DecodedResponse, anthropic,
        conversion::generation::Effect,
        openai::{
            Dialect, GenerationApi, GenerationCodec, TargetContract,
            lowering::native_response::EventProjection, obfuscation::Obfuscation,
        },
        replay_envelope,
        stream::SseLimits,
    },
    semantic::task::generation::*,
};

const BODY: &[u8] = br#"{"id":"native","type":"message","role":"assistant","model":"model","content":[{"type":"thinking","thinking":"plan","signature":"synthetic-signature"},{"type":"tool_use","id":"c","name":"lookup","input":{"n":9007199254740993.125}}],"stop_reason":"tool_use","usage":{"input_tokens":11,"cache_read_input_tokens":3,"cache_creation_input_tokens":2,"output_tokens":7}}"#;

fn fixture() -> DecodedResponse {
    let profile = anthropic::messages::Profile::AdaptiveTextTools;
    let target = crate::protocol::native::ReplayTarget::new("provider", "model").unwrap();
    profile
        .decode_generation_response(BODY, LocalScope::new(3), &target)
        .unwrap()
}

fn project(
    decoded: &mut DecodedResponse,
    profile: GenerationApi,
) -> Result<Vec<Effect>, CodecError> {
    let client = ClientAdapter::openai(
        GenerationCodec::new(profile, Dialect::MorphieCore, None),
        TargetContract::full(),
    );
    let mut projection = ClientProjection::new(client, true, false);
    emit_response_events(decoded, |event| projection.project(&[event]).map(|_| ()))?;
    decoded.semantic = projection.snapshot()?;
    decoded.fidelity.retain_owners(decoded.semantic.items());
    Ok(projection.take_evidence().iter().cloned().collect())
}

#[test]
fn projection_keeps_native_observation_and_derives_only_the_client_view() {
    let source = fixture();
    let before = source.clone();
    for profile in [GenerationApi::Responses, GenerationApi::Chat] {
        let mut client = source.clone();
        replay::seal_for_client(&mut client, &replay_envelope::CLIENT_ORIGIN).unwrap();
        project(&mut client, profile).unwrap();
        client.metadata.created = Some(1.into());
        let adapter = GenerationCodec::new(profile, Dialect::MorphieCore, None);
        let contract = TargetContract {
            replay_origin: Some(replay_envelope::CLIENT_ORIGIN.clone()),
            ..TargetContract::full()
        };
        let wire = adapter.encode_response(&client, &contract).unwrap();
        let args = if profile == GenerationApi::Responses {
            &wire["output"][1]["arguments"]
        } else {
            &wire["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"]
        };
        assert_eq!(args, "{\"n\":9007199254740993.125}");
        assert_eq!(wire["usage"]["total_tokens"], 23);
        assert_eq!(client.semantic.progress(), InteractionProgress::Unreported);
        assert_eq!(source, before);
        assert_eq!(source.metadata.created, None);
        assert_eq!(source.semantic.usage().unwrap().total_tokens, None);
        let Item::Reasoning(r) = &client.semantic.items()[0].1 else {
            panic!()
        };
        assert_eq!(
            replay_envelope::open(r.replay.as_ref().unwrap().as_str())
                .unwrap()
                .2
                .as_str(),
            "synthetic-signature"
        );
    }
}

#[test]
fn native_stream_preparation_records_losses_and_rejects_unrepresentable_metadata() {
    let source = fixture();
    let safety_settings = GenerationSettings {
        safety: vec![SafetySetting {
            category: SafetyCategory::Harassment,
            threshold: SafetyThreshold::MediumAndAbove,
        }],
        ..Default::default()
    };
    let terminal = StreamEvent::Terminal {
        terminal: StreamTerminal::Incomplete,
        details: TerminalDetails {
            incomplete: Some(IncompleteReason::MaxOutputTokens),
            ..Default::default()
        },
    };
    for profile in [GenerationApi::Responses, GenerationApi::Chat] {
        let adapter = GenerationCodec::new(profile, Dialect::MorphieCore, None);
        let contract = TargetContract {
            replay_origin: Some(replay_envelope::CLIENT_ORIGIN.clone()),
            ..TargetContract::full()
        };
        let client_adapter = ClientAdapter::openai(adapter.clone(), contract.clone());
        let mut attempt = Attempt::new(
            UpstreamAdapter::AnthropicMessages {
                profile: anthropic::messages::Profile::AdaptiveTextTools,
                dialect: anthropic::messages::Dialect::Standard,
                target: crate::protocol::native::ReplayTarget::new("provider", "model").unwrap(),
            },
            4096,
            SseLimits::default(),
        );
        attempt.begin(200, "text/event-stream").unwrap();
        for frame in [
            serde_json::json!({"type":"message_start","message":{"id":"native","type":"message","role":"assistant","model":"model","content":[],"stop_reason":null,"usage":{"input_tokens":11,"cache_read_input_tokens":3,"cache_creation_input_tokens":2,"output_tokens":0}}}),
            serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
            serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"plan"}}),
            serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"synthetic-signature"}}),
            serde_json::json!({"type":"content_block_stop","index":0}),
            serde_json::json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"c","name":"lookup","input":{}}}),
            serde_json::json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":r#"{"n":9007199254740993.125}"#}}),
            serde_json::json!({"type":"content_block_stop","index":1}),
            serde_json::json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":7}}),
            serde_json::json!({"type":"message_stop"}),
        ] {
            attempt
                .push(
                    format!(
                        "event: {}\ndata: {frame}\n\n",
                        frame["type"].as_str().unwrap()
                    )
                    .as_bytes(),
                )
                .unwrap();
        }
        attempt.finish().unwrap();
        let mut delivery = ResponseDelivery::new(
            client_adapter.clone(),
            "public",
            Default::default(),
            Default::default(),
            Obfuscation::Disabled,
        );
        delivery.encode_json(&attempt).unwrap();
        assert!(
            delivery
                .effects()
                .iter()
                .any(|loss| loss.field() == "generation.message_envelopes")
        );
        assert!(
            !delivery
                .effects()
                .iter()
                .any(|effect| effect.semantic_field()
                    == crate::protocol::conversion::generation::Field::InputTokens
                    && effect.change() == crate::protocol::conversion::generation::Change::Derive)
        );
        let mut pending = ResponseDelivery::new(
            client_adapter.clone(),
            "public",
            Default::default(),
            crate::semantic::context::StreamOptions {
                include_obfuscation: Some(false),
                ..Default::default()
            },
            Obfuscation::Disabled,
        );
        let (owner, call) = source
            .semantic
            .items()
            .iter()
            .find_map(|(id, item)| match item {
                Item::ToolCall(call) => Some((*id, call)),
                _ => None,
            })
            .unwrap();
        let opened = pending
            .prepare_events(
                &attempt,
                &[
                    StreamEvent::Started,
                    StreamEvent::ItemStarted {
                        item: owner,
                        kind: ItemKind::ToolCall {
                            format: ArgumentFormat::Json,
                            call_id: call.call_id.clone(),
                            name: call.name.clone(),
                            context: call.context.clone(),
                        },
                        replay: None,
                    },
                ],
            )
            .unwrap()
            .unwrap();
        let arguments = pending
            .prepare_events(
                &attempt,
                &[
                    StreamEvent::PartStarted {
                        item: owner,
                        part: PartId::new(20),
                        kind: PartKind::StructuredArguments,
                    },
                    StreamEvent::Delta {
                        item: owner,
                        part: PartId::new(20),
                        fragment: "{\"n\":9007199254740993.125}".into(),
                        logprobs: vec![],
                    },
                ],
            )
            .unwrap()
            .unwrap();
        assert!(arguments.events.iter().any(|event| matches!(
            event,
            StreamEvent::PartStarted {
                kind: PartKind::Arguments,
                ..
            }
        )));
        assert!(pending.lifecycle().may_retry_or_fallback());
        assert!(pending.commit().is_err(), "preparation is not rendering");
        let mut frames = pending
            .encode_prepared(opened)
            .map(|batch| batch.into_bytes())
            .unwrap();
        frames.extend(
            pending
                .encode_prepared(arguments)
                .map(|batch| batch.into_bytes())
                .unwrap(),
        );
        let text = String::from_utf8(frames.into_iter().flatten().collect()).unwrap();
        assert!(text.contains(r#"{\"n\":9007199254740993.125}"#));
        pending.cancel();
        let mut metadata = source.metadata.clone();
        metadata.created = Some(1.into());
        let mut encoder = client_adapter
            .stream_encoder(
                metadata.clone(),
                Default::default(),
                crate::semantic::context::StreamOptions {
                    include_obfuscation: Some(false),
                    ..Default::default()
                },
                Obfuscation::Disabled,
            )
            .unwrap();
        let mut projected = source.clone();
        let effects = project(&mut projected, profile).unwrap();
        assert!(
            effects
                .iter()
                .any(|loss| loss.field() == "generation.message_envelopes"
                    && loss.boundary().as_str() == "event.project")
        );
        let usage = projected.semantic.usage().unwrap();
        assert_eq!(usage.input_tokens, Some(16));
        assert_eq!(usage.output_tokens, Some(7));
        assert_eq!(usage.total_tokens, Some(23));
        encoder
            .encode(&StreamEvent::Started, &source.fidelity)
            .unwrap();
        let mut reported = metadata.clone();
        reported
            .context
            .reports
            .latest_mut()
            .expect("report")
            .settings = Some(safety_settings.clone());
        assert!(encoder.update_metadata(reported).is_err());
        assert!(encoder.encode(&terminal, &source.fidelity).is_err());
        assert!(encoder.finish().is_err());
        if profile == GenerationApi::Chat {
            let mut failed = EventProjection::new(profile);
            failed
                .project(&[StreamEvent::Usage(source.semantic.usage().unwrap())])
                .unwrap();
            let before = failed.effects().to_vec();
            assert!(
                failed
                    .project(&[
                        StreamEvent::ItemStarted {
                            item: ItemId::new(1),
                            replay: None,
                            kind: ItemKind::ToolCall {
                                format: ArgumentFormat::Json,
                                call_id: crate::semantic::value::Text::new("c", "test", 128)
                                    .unwrap(),
                                name: crate::semantic::value::Text::new("lookup", "test", 128)
                                    .unwrap(),
                                context: Default::default()
                            }
                        },
                        StreamEvent::ItemStarted {
                            item: ItemId::new(2),
                            replay: None,
                            kind: ItemKind::Message { phase: None }
                        }
                    ])
                    .is_err()
            );
            assert_eq!(failed.effects(), before);
        }
    }
}
