//! Usage facts, views, scopes and named projections.
mod scoped_reports {
    use morphiecore::semantic::task::generation::*;
    use proptest::prelude::*;

    fn report(scope: UsageScope, stage: UsageStage, tokens: u64) -> UsageReport {
        let mut report = UsageReport::operation(Usage::operation(tokens, 0, tokens));
        report.scope = scope;
        report.stage = stage;
        report
    }

    fn split_modalities(input: u64, tool: u64, relation: ToolInputRelation) -> UsageReport {
        let mut counts = Usage::operation(0, 0, 0);
        counts.input_tokens = None;
        counts.output_tokens = None;
        counts.total_tokens = None;
        counts.output_relation = OutputTokenRelation::ExcludesReasoning;
        counts.total_relation = TotalTokenRelation::Unreported;
        let mut report = UsageReport::operation(counts);
        report.accounting = UsageAccounting::SplitGeneration {
            tool_input_tokens: None,
            tool_input_relation: relation,
            total_relation: SplitTotalRelation::Unreported,
        };
        report.details.modalities = vec![
            ModalityTokens {
                bucket: UsageBucket::Input,
                modality: Some(TokenModality::Text),
                tokens: Some(input),
                defaulted: false,
            },
            ModalityTokens {
                bucket: UsageBucket::ToolInput,
                modality: Some(TokenModality::Text),
                tokens: Some(tool),
                defaulted: false,
            },
        ];
        report
    }

    #[test]
    fn split_modality_relations_do_not_turn_known_overflow_into_unknown() {
        let valid = split_modalities(u64::MAX - 1, 1, ToolInputRelation::Additional);
        valid.validate().unwrap();
        assert_eq!(valid.summary().input_text_tokens, Some(u64::MAX));
        assert_eq!(valid.summary().input_tokens, None);
        assert_eq!(
            split_modalities(u64::MAX, 1, ToolInputRelation::Additional).validate(),
            Err(GenerationError::InvalidResponse)
        );
        assert_eq!(
            split_modalities(1, 2, ToolInputRelation::Included).validate(),
            Err(GenerationError::InvalidResponse)
        );
        let included = split_modalities(2, 1, ToolInputRelation::Included);
        included.validate().unwrap();
        assert_eq!(included.summary().input_text_tokens, Some(2));
        let unknown = split_modalities(u64::MAX, 1, ToolInputRelation::Unreported);
        unknown.validate().unwrap();
        assert_eq!(unknown.summary().input_text_tokens, None);
        let mut sparse = valid.clone();
        sparse.details.modalities[1].tokens = None;
        sparse.validate().unwrap();
        assert_eq!(sparse.summary().input_text_tokens, None);
        assert_eq!(valid.details.modalities[1].tokens, Some(1));
    }

    proptest! {
        #![proptest_config(crate::test_properties::config())]
        #[test]
        fn property_split_modality_relationships_use_checked_arithmetic(
            input in prop_oneof![Just(0u64), Just(u64::MAX), any::<u64>()],
            tool in prop_oneof![Just(0u64), Just(u64::MAX), any::<u64>()],
            relation in 0u8..3,
            modality in prop::sample::select(vec![
                TokenModality::Text, TokenModality::Image, TokenModality::Audio
            ])
        ) {
            let relation = match relation {
                0 => ToolInputRelation::Additional,
                1 => ToolInputRelation::Included,
                _ => ToolInputRelation::Unreported,
            };
            let mut report = split_modalities(input, tool, relation);
            for part in &mut report.details.modalities {
                part.modality = Some(modality);
            }
            let valid = match relation {
                ToolInputRelation::Additional => input.checked_add(tool).is_some(),
                ToolInputRelation::Included => tool <= input,
                ToolInputRelation::Unreported => true,
            };
            prop_assert_eq!(report.validate().is_ok(), valid);
            if valid {
                let summary = report.summary();
                let actual = match modality {
                    TokenModality::Text => summary.input_text_tokens,
                    TokenModality::Image => summary.input_image_tokens,
                    TokenModality::Audio => summary.input_audio_tokens,
                    _ => unreachable!(),
                };
                let expected = match relation {
                    ToolInputRelation::Additional => input.checked_add(tool),
                    ToolInputRelation::Included => Some(input),
                    ToolInputRelation::Unreported => None,
                };
                prop_assert_eq!(actual, expected);
                prop_assert_eq!(summary.input_tokens, None);
            }
        }
    }

    #[test]
    fn replacement_is_atomic_and_clears_old_sparse_facts_and_charges() {
        let mut first = report(UsageScope::Operation, UsageStage::Checkpoint, 10);
        first.details.modalities.push(ModalityTokens {
            bucket: UsageBucket::Input,
            modality: Some(TokenModality::Text),
            tokens: Some(4),
            defaulted: false,
        });
        let step = report(UsageScope::Step(0), UsageStage::Final, 9);
        let mut reports = UsageReports::default();
        reports.replace(first.clone()).unwrap();
        reports.replace(step.clone()).unwrap();
        let before = reports.clone();
        let mut invalid = first.clone();
        invalid.counts.total_tokens = Some(9);
        assert_eq!(
            reports.replace(invalid),
            Err(GenerationError::InvalidResponse)
        );
        assert_eq!(reports, before);
        assert_eq!(reports.bytes(), before.bytes());

        let mut replacement = report(UsageScope::Operation, UsageStage::Final, 0);
        replacement.counts.input_tokens = None;
        replacement.counts.total_tokens = None;
        reports.replace(replacement.clone()).unwrap();
        assert_eq!(reports.operation(), Some(&replacement));
        assert_eq!(
            reports.reports().find(|r| r.scope == step.scope),
            Some(&step)
        );
        assert_eq!(reports.summary().unwrap().output_tokens, Some(0));
        assert_eq!(reports.summary().unwrap().input_tokens, None);
        assert_eq!(reports.summary().unwrap().total_tokens, None);
        assert_eq!(
            reports.bytes(),
            before.bytes() - first.bytes() + replacement.bytes()
        );
    }

    #[test]
    fn scoped_snapshots_replace_without_summing_and_edits_preserve_them() {
        let mut reports = UsageReports::default();
        reports
            .replace(report(UsageScope::Operation, UsageStage::Checkpoint, 100))
            .unwrap();
        let bytes = reports.bytes();
        reports
            .replace(report(UsageScope::Step(0), UsageStage::Final, 100))
            .unwrap();
        reports
            .replace(report(UsageScope::Operation, UsageStage::Checkpoint, 220))
            .unwrap();
        reports
            .replace(report(UsageScope::Step(1), UsageStage::Final, 120))
            .unwrap();
        assert_eq!(reports.bytes(), bytes * 3);
        reports
            .replace(report(UsageScope::Operation, UsageStage::Final, 220))
            .unwrap();
        assert_eq!(reports.summary().unwrap().total_tokens, Some(220));
        assert_eq!(
            reports
                .reports()
                .map(|r| (r.scope, r.counts.total_tokens))
                .collect::<Vec<_>>(),
            [
                (UsageScope::Operation, Some(220)),
                (UsageScope::Step(0), Some(100)),
                (UsageScope::Step(1), Some(120))
            ]
        );
        let before = reports.clone();
        assert!(
            reports
                .replace(report(UsageScope::Step(0), UsageStage::Checkpoint, 999))
                .is_err()
        );
        assert_eq!(reports, before);
        let response = GenerationResponse::new(vec![], Outcome::Completed)
            .unwrap()
            .with_usage_reports(reports.clone())
            .unwrap()
            .with_items(vec![])
            .unwrap();
        assert_eq!(response.usage_reports(), &reports);
        let steps_only = {
            let mut reports = UsageReports::default();
            reports
                .replace(report(UsageScope::Step(0), UsageStage::Final, 100))
                .unwrap();
            reports
        };
        assert!(steps_only.summary().is_none());
    }

    #[test]
    fn details_have_bounded_unique_owners_and_sparse_breakdowns_are_not_totals() {
        let mut report = report(UsageScope::Operation, UsageStage::Final, 100);
        report.details.modalities = vec![ModalityTokens {
            bucket: UsageBucket::Input,
            modality: Some(TokenModality::Image),
            tokens: Some(60),
            defaulted: false,
        }];
        report.validate().unwrap();
        assert_eq!(report.summary().input_tokens, Some(100));
        let mut duplicate = report.clone();
        duplicate
            .details
            .modalities
            .push(duplicate.details.modalities[0]);
        assert!(duplicate.validate().is_err());
        report.details.modalities[0].tokens = Some(101);
        assert!(report.validate().is_err());
        report.details.modalities[0].tokens = Some(60);
        report.details.modalities[0].defaulted = true;
        assert!(report.validate().is_err());
        report.details.modalities.clear();
        report.counts.input_cache_write_tokens = Some(13);
        report.details.cache_writes = vec![
            CacheWriteTokens {
                ttl: PrefixCacheTtl::FiveMinutes,
                tokens: 8,
            },
            CacheWriteTokens {
                ttl: PrefixCacheTtl::OneHour,
                tokens: 5,
            },
        ];
        report.details.tool_requests = vec![ToolUseCount {
            kind: ToolUsageKind::WebSearch,
            count: Some(2),
        }];
        report.validate().unwrap();
        assert_eq!(report.summary().total_tokens, Some(100));
        report.details.cache_writes[1].tokens = 4;
        assert!(report.validate().is_err());
    }

    #[test]
    fn stream_reports_cannot_update_a_final_scope_or_follow_a_terminal() {
        let state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
        let state = reduce(
            state,
            StreamEvent::UsageReport(report(UsageScope::Operation, UsageStage::Checkpoint, 10)),
        )
        .unwrap();
        let state = reduce(
            state,
            StreamEvent::UsageReport(report(UsageScope::Operation, UsageStage::Final, 20)),
        )
        .unwrap();
        assert!(
            reduce(
                state.clone(),
                StreamEvent::UsageReport(report(UsageScope::Operation, UsageStage::Final, 30))
            )
            .is_err()
        );
        let state = reduce(
            state,
            StreamEvent::UsageReport(report(UsageScope::Step(0), UsageStage::Final, 20)),
        )
        .unwrap();
        let state = reduce(
            state,
            StreamEvent::Terminal {
                terminal: StreamTerminal::Completed,
                details: TerminalDetails::default(),
            },
        )
        .unwrap();
        assert_eq!(
            materialize(&state).unwrap().usage().unwrap().total_tokens,
            Some(20)
        );
        assert!(
            reduce(
                state,
                StreamEvent::UsageReport(report(UsageScope::Step(1), UsageStage::Final, 20))
            )
            .is_err()
        );
    }
}

mod usage_views {
    //! Named subtraction views borrow reported facts and never add overlapping details.
    use morphiecore::semantic::task::generation::*;
    #[test]
    fn exclusive_cache_counts_require_explicit_disjoint_premises_without_zero_fill() {
        let mut report = Usage::operation(10, 2, 4108);
        report.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
        report.total_relation = TotalTokenRelation::InputCacheAndOutput;
        report.cached_input_tokens = Some(4096);
        report.input_cache_write_tokens = Some(0);
        report.validate().unwrap();
        assert_eq!(
            report
                .derive(UsageFormula::InputPlusCacheReadAndWrite)
                .unwrap()
                .unwrap()
                .tokens(),
            4106
        );
        assert!(
            report
                .derive(UsageFormula::InputMinusCacheRead)
                .unwrap()
                .is_none()
        );
        let original = report;
        report.input_cache_write_tokens = None;
        assert!(
            report
                .derive(UsageFormula::InputPlusCacheReadAndWrite)
                .unwrap()
                .is_none()
        );
        report = original;
        report.input_relation = InputTokenRelation::IncludesCache;
        assert!(report.validate().is_err());
        report = original;
        report.total_tokens = None;
        report.total_relation = TotalTokenRelation::Unreported;
        report.input_relation = InputTokenRelation::Unreported;
        assert!(
            report
                .derive(UsageFormula::InputPlusCacheReadAndWrite)
                .unwrap()
                .is_none()
        );
        report.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
        report.input_tokens = Some(u64::MAX);
        assert!(
            report
                .derive(UsageFormula::InputPlusCacheReadAndWrite)
                .is_err()
        );
        assert_eq!(original.input_tokens, Some(10));
    }
    fn usage() -> Usage {
        Usage {
            input_relation:
                morphiecore::semantic::task::generation::InputTokenRelation::IncludesCache,
            output_relation: OutputTokenRelation::IncludesReasoning,
            total_relation: TotalTokenRelation::InputAndOutput,
            input_tokens: Some(12),
            output_tokens: Some(10),
            total_tokens: Some(22),
            cached_input_tokens: Some(7),
            input_cache_write_tokens: Some(9),
            reasoning_tokens: Some(6),
            input_text_tokens: Some(12),
            input_image_tokens: Some(8),
            input_audio_tokens: None,
            output_audio_tokens: None,
            output_text_tokens: Some(10),
            accepted_prediction_tokens: Some(2),
            rejected_prediction_tokens: Some(3),
        }
    }
    #[test]
    fn formulas_have_explicit_provenance_and_do_not_subtract_overlapping_details() {
        let usage = usage();
        let input = usage
            .derive(UsageFormula::InputMinusCacheRead)
            .unwrap()
            .unwrap();
        assert_eq!(input.tokens(), 5);
        assert_eq!(input.formula(), UsageFormula::InputMinusCacheRead);
        assert!(std::ptr::eq(input.source(), &usage));
        let output = usage
            .derive(UsageFormula::OutputMinusReasoning)
            .unwrap()
            .unwrap();
        assert_eq!(output.tokens(), 4);
        // This is not visible text, billing, or a sum of modality/prediction counts.
        assert_ne!(Some(output.tokens()), usage.output_text_tokens);
        assert_eq!(usage.total_tokens, Some(22));
    }
}
mod usage_relations {
    //! Counting relationships remain explicit reported facts.
    #[test]
    fn prediction_subsets_obey_the_joint_bound_not_only_individual_bounds() {
        use morphiecore::semantic::task::generation::*;
        for (output, accepted, rejected, valid) in [
            (10, 9, 1, true),
            (10, 6, 6, false),
            (u64::MAX, u64::MAX, 1, false),
        ] {
            let mut usage = Usage::operation(0, output, output);
            usage.accepted_prediction_tokens = Some(accepted);
            usage.rejected_prediction_tokens = Some(rejected);
            assert!(accepted <= output && rejected <= output);
            assert_eq!(
                usage.validate(),
                if valid {
                    Ok(())
                } else {
                    Err(GenerationError::InvalidResponse)
                }
            );
        }
    }

    use crate::events_support::{self as events, text};
    use morphiecore::{
        lowering::generation::RepresentationError,
        protocol::{
            fidelity::FidelityRecords,
            openai::{Profile, TargetCaps as Contract, lowering::lower_response},
        },
        semantic::task::generation::*,
    };
    fn message() -> (ItemId, Item) {
        (
            ItemId::new(1),
            Item::Message(Message {
                role: MessageRole::Assistant,
                parts: vec![Part {
                    id: PartId::new(2),
                    content: ContentPart::Text(text("a").into()),
                    cache: None,
                }],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        )
    }
    #[test]
    fn separate_reasoning_and_missing_counts_do_not_guess_totals_or_views() {
        let mut usage = Usage::operation(7, 20, 49);
        usage.reasoning_tokens = Some(22);
        usage.output_relation = OutputTokenRelation::ExcludesReasoning;
        usage.total_relation = TotalTokenRelation::InputOutputAndReasoning;
        usage.validate().unwrap();
        assert_eq!(
            usage.derive(UsageFormula::OutputMinusReasoning).unwrap(),
            None
        );
        usage.input_tokens = None;
        usage.total_relation = TotalTokenRelation::Unreported;
        usage.validate().unwrap();
        assert_eq!(usage.input_tokens, None);
        let response = GenerationResponse::new(vec![message()], Outcome::Completed)
            .unwrap()
            .with_usage(Some(usage))
            .unwrap();
        assert_eq!(response.usage().unwrap().input_tokens, None);
        for profile in [Profile::Chat, Profile::Responses] {
            assert_eq!(
                lower_response(
                    &response,
                    &FidelityRecords::default(),
                    &events::metadata(),
                    profile,
                    Contract::full()
                )
                .err(),
                Some(RepresentationError::UsageProjection)
            );
        }
    }
    #[test]
    fn contradictory_relations_and_a_second_final_report_are_rejected() {
        let mut partial = Usage::operation(2, 3, 5);
        partial.input_tokens = None;
        partial.total_tokens = Some(2);
        assert!(partial.validate().is_err());
        let state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
        let state = reduce(state, StreamEvent::Usage(Usage::operation(2, 3, 5))).unwrap();
        assert_eq!(
            reduce(state, StreamEvent::Usage(Usage::operation(2, 3, 5))).err(),
            Some(EventError::Lifecycle)
        );
    }
}
mod billing_modal_usage {
    //! Billing views must promote independent modality facts, never replay stale counters.
    use morphiecore::{
        protocol::openai::{Adapter, Dialect, Profile, TargetCaps as Contract},
        semantic::task::generation::Usage,
    };
    use serde_json::{Value, json};
    fn fixture() -> Value {
        json!({"object":"response","id":"r","model":"m","created_at":1,"status":"completed",
        "output":[{"type":"message","id":"msg","role":"assistant","status":"completed","content":[{"type":"output_text","text":"x","annotations":[]}]}],
        "usage":{"input_tokens":10,"output_tokens":2,"total_tokens":12,
            "input_tokens_details":{"cached_tokens":1},"output_tokens_details":{"reasoning_tokens":0},
            "x_details":[{"x_billing_type":"response_api","input_tokens":10,"output_tokens":2,"total_tokens":12,"image_tokens":7,
                "input_tokens_details":{"image_tokens":7,"text_tokens":3},"prompt_tokens_details":{"cached_tokens":1},
                "output_tokens_details":{"reasoning_tokens":0,"text_tokens":2}}]}})
    }
    fn expected_usage() -> Value {
        json!({"input_tokens":10,"output_tokens":2,"total_tokens":12,"input_tokens_details":{"cached_tokens":1,"image_tokens":7,"text_tokens":3},"output_tokens_details":{"reasoning_tokens":0,"text_tokens":2}})
    }
    #[test]
    fn modality_billing_view_promotes_typed_facts_and_edits_cannot_restore_the_view() {
        let provider = Adapter::new(Profile::Responses, Dialect::Bailian, None);
        let mut decoded = provider
            .decode_response(&serde_json::to_vec(&fixture()).unwrap())
            .unwrap();
        let usage = decoded.semantic.usage().unwrap();
        assert_eq!(
            (
                usage.input_image_tokens,
                usage.input_text_tokens,
                usage.output_text_tokens
            ),
            (Some(7), Some(3), Some(2))
        );
        let client = Adapter::new(Profile::Responses, Dialect::MorphieCore, None);
        assert_eq!(
            client.encode_response(&decoded, &Contract::full()).unwrap()["usage"],
            expected_usage()
        );
        let standard = Adapter::new(Profile::Responses, Dialect::Standard, None)
            .encode_response(&decoded, &Contract::full())
            .unwrap();
        assert_eq!(
            standard["usage"],
            json!({"input_tokens":10,"output_tokens":2,"total_tokens":12,
        "input_tokens_details":{"cached_tokens":1},"output_tokens_details":{"reasoning_tokens":0}})
        );
        let native = provider
            .encode_response(&decoded, &Contract::full())
            .unwrap();
        assert_eq!(
            native["usage"]["input_tokens_details"],
            json!({"cached_tokens":1})
        );
        assert_eq!(
            native["usage"]["x_details"][0]["input_tokens_details"],
            json!({"image_tokens":7,"text_tokens":3})
        );
        decoded.semantic = decoded
            .semantic
            .clone()
            .with_usage(Some(Usage {
                input_relation:
                    morphiecore::semantic::task::generation::InputTokenRelation::IncludesCache,
                output_relation:
                    morphiecore::semantic::task::generation::OutputTokenRelation::IncludesReasoning,
                total_relation:
                    morphiecore::semantic::task::generation::TotalTokenRelation::InputAndOutput,
                input_image_tokens: Some(5),
                input_text_tokens: Some(2),
                ..usage
            }))
            .unwrap();
        let edited = provider
            .encode_response(&decoded, &Contract::full())
            .unwrap();
        assert_eq!(edited["usage"]["x_details"][0]["image_tokens"], 5);
        assert_eq!(
            edited["usage"]["x_details"][0]["input_tokens_details"]["text_tokens"],
            2
        );
        decoded.semantic = decoded
            .semantic
            .clone()
            .with_usage(Some(Usage {
                input_relation:
                    morphiecore::semantic::task::generation::InputTokenRelation::IncludesCache,
                output_relation:
                    morphiecore::semantic::task::generation::OutputTokenRelation::IncludesReasoning,
                total_relation:
                    morphiecore::semantic::task::generation::TotalTokenRelation::InputAndOutput,
                input_image_tokens: None,
                input_text_tokens: None,
                output_text_tokens: None,
                ..usage
            }))
            .unwrap();
        let deleted = client.encode_response(&decoded, &Contract::full()).unwrap();
        assert_eq!(
            deleted["usage"]["input_tokens_details"],
            json!({"cached_tokens":1})
        );
        assert_eq!(
            deleted["usage"]["output_tokens_details"],
            json!({"reasoning_tokens":0})
        );
        let native = provider
            .encode_response(&decoded, &Contract::full())
            .unwrap();
        assert!(
            native["usage"]["x_details"][0]
                .get("image_tokens")
                .is_none()
        );
    }
}
mod unreported_event_probabilities {
    //! A named unreported-probability rule must not weaken complete SSE or fabricate facts.
    use morphiecore::protocol::openai::{
        Adapter, Dialect, Profile, responses::sse::ResponsesSseDecoder,
    };
    use serde_json::{Value, json};
    fn fixture() -> (Value, Vec<Value>) {
        let message = json!({"id":"msg","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"x","annotations":[]}]});
        let response = json!({"id":"r","object":"response","model":"m","created_at":1,"status":"completed","output":[message],"usage":null,"error":null,"incomplete_details":null});
        let mut initial = response.clone();
        initial["status"] = json!("in_progress");
        initial["output"] = json!([]);
        let mut added = message.clone();
        added["status"] = json!("in_progress");
        added["content"] = json!([]);
        let mut events = vec![
            json!({"type":"response.created","response":initial}),
            json!({"type":"response.output_item.added","output_index":0,"item":added}),
            json!({"type":"response.content_part.added","output_index":0,"item_id":"msg","content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
            json!({"type":"response.output_text.delta","output_index":0,"item_id":"msg","content_index":0,"delta":"x"}),
            json!({"type":"response.output_text.done","output_index":0,"item_id":"msg","content_index":0,"text":"x"}),
            json!({"type":"response.content_part.done","output_index":0,"item_id":"msg","content_index":0,"part":message["content"][0]}),
            json!({"type":"response.output_item.done","output_index":0,"item":message}),
            json!({"type":"response.completed","response":response}),
        ];
        for (n, event) in events.iter_mut().enumerate() {
            event["sequence_number"] = json!(n + 1);
        }
        (response, events)
    }
    fn sse(events: &[Value]) -> Vec<u8> {
        events
            .iter()
            .map(|event| {
                format!(
                    "event: {}\ndata: {event}\n\n",
                    event["type"].as_str().unwrap()
                )
            })
            .collect::<String>()
            .into_bytes()
    }
    fn decoder(dialect: Dialect) -> ResponsesSseDecoder {
        ResponsesSseDecoder::with_decoder(
            200,
            "text/event-stream",
            Default::default(),
            Adapter::new(Profile::Responses, dialect, None).event_decoder(),
        )
        .unwrap()
    }

    #[test]
    fn named_omission_does_not_hide_malformed_probabilities_or_missing_complete_fields() {
        let (_, source) = fixture();
        let mut cases = vec![];
        for index in [3, 4] {
            for value in [
                Value::Null,
                json!(false),
                json!([{"token":"x","logprob":1}]),
            ] {
                let mut events = source.clone();
                events[index]["logprobs"] = value;
                cases.push(events);
            }
        }
        for (index, key) in [(0, "sequence_number"), (2, "part"), (6, "item_id")] {
            let mut events = source.clone();
            if key == "item_id" {
                events[6]["item"]
                    .as_object_mut()
                    .unwrap()
                    .shift_remove("id");
            } else {
                events[index].as_object_mut().unwrap().shift_remove(key);
            }
            cases.push(events);
        }
        let mut events = source.clone();
        events[3]["logprobs"] =
            json!([{"token":"x","logprob":-0.5,"bytes":[120],"top_logprobs":[]}]);
        cases.push(events);
        for events in cases {
            let mut decoded = decoder(Dialect::Xiaomi);
            let bytes = sse(&events);
            let mut rest = bytes.as_slice();
            let mut rejected = false;
            while !rest.is_empty() {
                match decoded.consume(rest) {
                    Ok((used, _)) => rest = &rest[used..],
                    Err(_) => {
                        rejected = true;
                        break;
                    }
                }
            }
            assert!(rejected);
            assert!(decoded.consume(&sse(&source[source.len() - 1..])).is_err());
            assert!(decoded.finish().is_err());
        }
    }
}
mod subscription_accounting {
    //! Independent closed accounting and event-owned output counterexamples.
    use morphiecore::{
        protocol::openai::{Adapter, Dialect, Profile, TargetCaps as Contract},
        semantic::value::ReplayOrigin,
    };
    use serde_json::{Value, json};
    fn adapter(dialect: Dialect, scope: &str) -> Adapter {
        Adapter::new(
            Profile::Responses,
            dialect,
            Some(ReplayOrigin::new(scope).unwrap()),
        )
    }

    #[test]
    fn siwc_uses_only_closed_public_programs_in_static_and_event_snapshots() {
        let source = adapter(Dialect::Siwc, "one");
        let mut response = crate::wire::response(2);
        response["access_programs"] = json!({"cyber":"standard"});
        response["frequency_penalty"] = json!(0.0);
        response["presence_penalty"] = json!(0);
        response["moderation"] = Value::Null;
        let attribution = json!({"items":{"input.0":{"input_tokens":7,"output_tokens":0,
        "cached_tokens":0,"cache_write_tokens":0,"content":[]},
        "output.0":{"input_tokens":0,"output_tokens":2,"cached_tokens":0,"cache_write_tokens":0}},
        "request_fields":{"tools":{"input_tokens":3,"output_tokens":0,"cached_tokens":0,"cache_write_tokens":0}}});
        response["usage"]["attribution"] = attribution.clone();
        let zero_tools = json!({"image_gen":{"input_tokens":0,"input_tokens_details":{"image_tokens":0,"text_tokens":0},"output_tokens":0,"output_tokens_details":{"image_tokens":0,"text_tokens":0},"total_tokens":0},"web_search":{"num_requests":0}});
        response["tool_usage"] = zero_tools.clone();
        let decoded = source
            .decode_response(response.to_string().as_bytes())
            .unwrap();
        assert_eq!(
            source.encode_response(&decoded, &Contract::full()).unwrap()["access_programs"],
            json!({"cyber":"standard"})
        );
        assert_eq!(
            source.encode_response(&decoded, &Contract::full()).unwrap()["tool_usage"],
            zero_tools
        );
        assert_eq!(
            source.encode_response(&decoded, &Contract::full()).unwrap()["frequency_penalty"],
            json!(0.0)
        );
        assert_eq!(
            source.encode_response(&decoded, &Contract::full()).unwrap()["presence_penalty"],
            json!(0)
        );
        assert_eq!(
            source.encode_response(&decoded, &Contract::full()).unwrap()["moderation"],
            Value::Null
        );
        assert_eq!(
            source.encode_response(&decoded, &Contract::full()).unwrap()["usage"]["attribution"],
            attribution
        );
        assert_eq!(decoded.semantic.usage().unwrap().input_tokens, Some(3));
        assert!(
            adapter(Dialect::MorphieCore, "one")
                .encode_response(&decoded, &Contract::full())
                .unwrap()
                .get("access_programs")
                .is_none()
        );
        let mut decoder = source.event_decoder();
        for mut event in crate::wire::events(2) {
            if let Some(snapshot) = event.get_mut("response").and_then(Value::as_object_mut) {
                snapshot.insert("access_programs".into(), json!({"cyber":"standard"}));
                snapshot.insert("tool_usage".into(), zero_tools.clone());
                snapshot.insert("frequency_penalty".into(), json!(0));
                snapshot.insert("presence_penalty".into(), json!(0.0));
                snapshot.insert("moderation".into(), Value::Null);
                if let Some(usage) = snapshot.get_mut("usage").and_then(Value::as_object_mut) {
                    usage.insert("attribution".into(), attribution.clone());
                }
            }
            decoder.push(&event).unwrap();
        }
        decoder.finish().unwrap();
        let streamed = decoder.materialize().unwrap();
        assert_eq!(
            source
                .encode_response(&streamed, &Contract::full())
                .unwrap()["usage"]["attribution"],
            attribution
        );
        for value in [
            json!({"unknown":{}}),
            json!({"items":{"x":{"input_tokens":-1,"output_tokens":0,"cached_tokens":0,"cache_write_tokens":0,"content":[]}}}),
            json!({"items":{"x":{"input_tokens":0,"output_tokens":0,"cached_tokens":0,"cache_write_tokens":0,"content":null}}}),
        ] {
            let mut invalid = response.clone();
            invalid["usage"]["attribution"] = value.clone();
            assert!(
                source
                    .decode_response(invalid.to_string().as_bytes())
                    .is_err(),
                "{value}"
            );
        }
        for value in [json!({"cyber":17}), json!({"cyber":"contains spaces"})] {
            let mut invalid = response.clone();
            invalid["access_programs"] = value;
            assert!(
                source
                    .decode_response(invalid.to_string().as_bytes())
                    .is_err()
            );
        }
        for field in ["frequency_penalty", "presence_penalty"] {
            for value in [json!(1), json!("0"), Value::Null] {
                let mut invalid = response.clone();
                invalid[field] = value;
                assert!(
                    source
                        .decode_response(invalid.to_string().as_bytes())
                        .is_err()
                );
            }
        }
        response["moderation"] = json!({"blocked":true});
        assert!(
            source
                .decode_response(response.to_string().as_bytes())
                .is_err()
        );
        response.as_object_mut().unwrap().remove("moderation");
        response["tool_usage"]["web_search"]["num_requests"] = json!(1);
        assert!(
            source
                .decode_response(response.to_string().as_bytes())
                .is_err()
        );
        response["tool_usage"] = json!({"web_search":{"num_requests":0}});
        assert!(
            source
                .decode_response(response.to_string().as_bytes())
                .is_err()
        );
    }

    #[test]
    fn accounting_keeps_distinct_counters_scoped_and_invalidates_after_edits() {
        let source = adapter(Dialect::Grok, "one");
        let mut response = crate::wire::response(2);
        response["frequency_penalty"] = json!(0.0);
        response["usage"]["num_sources_used"] = json!(0);
        response["usage"]["num_server_side_tools_used"] = json!(0);
        response["usage"]["cost_in_usd_ticks"] = json!(9007199254740993u64);
        response["usage"]["context_details"] = json!({"input_tokens":71,"output_tokens":83});
        let mut decoded = source
            .decode_response(response.to_string().as_bytes())
            .unwrap();
        assert_eq!(decoded.semantic.usage().unwrap().output_tokens, Some(5));
        let wire = source.encode_response(&decoded, &Contract::full()).unwrap();
        assert_eq!(
            wire["usage"]["context_details"],
            json!({"input_tokens":71,"output_tokens":83})
        );
        assert_eq!(wire["usage"]["cost_in_usd_ticks"], 9007199254740993u64);
        for other in [
            adapter(Dialect::MorphieCore, "one"),
            adapter(Dialect::Grok, "two"),
        ] {
            let wire = other.encode_response(&decoded, &Contract::full()).unwrap();
            assert!(wire["usage"].get("context_details").is_none());
        }
        let mut usage = decoded.semantic.usage().unwrap();
        usage.output_tokens = Some(7);
        usage.total_tokens = Some(10);
        decoded.semantic = decoded.semantic.with_usage(Some(usage)).unwrap();
        assert!(
            source.encode_response(&decoded, &Contract::full()).unwrap()["usage"]
                .get("cost_in_usd_ticks")
                .is_none()
        );
        for (path, value) in [
            ("/usage/num_sources_used", json!(1)),
            ("/usage/cost_in_usd_ticks", json!(-1)),
            ("/usage/context_details", json!({"input_tokens":"71"})),
        ] {
            let mut invalid = response.clone();
            *invalid.pointer_mut(path).unwrap() = value;
            assert!(
                source
                    .decode_response(invalid.to_string().as_bytes())
                    .is_err(),
                "{path}"
            );
        }
    }
}
mod projection {
    //! Named Chat loss never changes the source or repairs invalid reports.
    use crate::events_support::{metadata, text};
    use morphiecore::{
        protocol::{
            fidelity::FidelityRecords,
            openai::{Profile, TargetCaps as Contract, chat, lowering::lower_response},
        },
        semantic::task::generation::*,
    };

    fn response() -> GenerationResponse {
        GenerationResponse::new(
            vec![(
                ItemId::new(1),
                Item::Message(Message {
                    role: MessageRole::Assistant,
                    parts: vec![Part {
                        id: PartId::new(2),
                        content: ContentPart::Text(text("answer").into()),
                        cache: None,
                    }],
                    status: ItemLifecycle::Completed,
                    phase: None,
                }),
            )],
            Outcome::Completed,
        )
        .unwrap()
        .with_usage(Some(Usage {
            input_image_tokens: Some(7),
            ..Usage::operation(10, 2, 12)
        }))
        .unwrap()
    }

    #[test]
    fn responses_fingerprint_projection_is_explicit_and_keeps_source_metadata() {
        use crate::events_support::terminal;
        use morphiecore::{
            protocol::openai::{events::EventEncoder, responses},
            semantic::value::Presence,
        };
        let source = response().with_usage(None).unwrap();
        let fidelity = FidelityRecords::default();
        for fingerprint in [
            Presence::Absent,
            Presence::Null,
            Presence::Value("synthetic-fp".into()),
        ] {
            let mut meta = metadata();
            meta.context.system_fingerprint = fingerprint.clone();
            let original = meta.clone();
            let projected = lower_response(
                &source,
                &fidelity,
                &meta,
                Profile::Responses,
                Contract::full(),
            )
            .unwrap();
            let wire = responses::encode_response(&projected).unwrap();
            assert!(wire.get("system_fingerprint").is_none());
            assert_eq!(wire["id"].as_str(), meta.id.as_deref());
            assert_eq!(meta, original);
            assert_eq!(
                projected
                    .projection()
                    .iter()
                    .filter(|s| s
                        .loss
                        .is_some_and(|rule| rule.name() == "responses.omit-system-fingerprint.v1"))
                    .count(),
                usize::from(!fingerprint.is_absent())
            );
            let back = projected
                .reproject(Profile::Chat, Contract::full())
                .unwrap();
            assert!(
                chat::encode_response(&back)
                    .unwrap()
                    .get("system_fingerprint")
                    .is_none()
            );
            let native =
                lower_response(&source, &fidelity, &meta, Profile::Chat, Contract::full()).unwrap();
            assert_eq!(
                chat::encode_response(&native)
                    .unwrap()
                    .get("system_fingerprint")
                    .is_some(),
                !fingerprint.is_absent()
            );
            let mut encoder = EventEncoder::new(Profile::Responses, meta.clone()).unwrap();
            for event in text_events() {
                encoder.encode(&event, &fidelity).unwrap();
            }
            assert_eq!(
                encoder
                    .projection()
                    .iter()
                    .filter(|stage| stage.loss.is_some())
                    .count(),
                usize::from(!fingerprint.is_absent())
            );
            let mut failed = EventEncoder::new(Profile::Responses, meta.clone()).unwrap();
            for event in text_events() {
                failed.encode(&event, &fidelity).unwrap();
            }
            assert!(failed.encode(&StreamEvent::Started, &fidelity).is_err());
            assert_eq!(
                failed
                    .projection()
                    .iter()
                    .filter(|stage| stage.loss.is_some())
                    .count(),
                usize::from(!fingerprint.is_absent())
            );
            assert!(failed.finish().is_err());
            let output = encoder
                .encode(&terminal(StreamTerminal::Completed), &fidelity)
                .unwrap();
            encoder.finish().unwrap();
            assert!(
                output.last().unwrap()["response"]
                    .get("system_fingerprint")
                    .is_none()
            );
            assert_eq!(
                encoder
                    .projection()
                    .iter()
                    .filter(|s| s.loss.is_some())
                    .count(),
                usize::from(!fingerprint.is_absent())
            );
        }
        for value in [String::new(), "x".repeat(257)] {
            let mut meta = metadata();
            meta.context.system_fingerprint = Presence::Value(value);
            assert!(
                lower_response(
                    &source,
                    &fidelity,
                    &meta,
                    Profile::Responses,
                    Contract::full()
                )
                .is_err()
            );
            assert!(EventEncoder::new(Profile::Responses, meta).is_err());
        }
    }

    #[test]
    fn unknown_rules_and_stage_overflow_are_rejected() {
        use morphiecore::{
            lowering::generation::RepresentationError,
            protocol::openai::{
                ResponseRepresentation,
                lowering::projection::{LossRule, MAX_PROJECTION_STAGES},
            },
        };
        assert_eq!(
            "responses.omit-input-image-tokens.v1"
                .parse::<LossRule>()
                .unwrap()
                .owner(),
            "generation.usage.input_image_tokens"
        );
        assert_eq!(
            "chat.merge-messages.v1".parse::<LossRule>(),
            Err(RepresentationError::UnknownProjectionRule)
        );
        fn descend(value: &ResponseRepresentation<'_>, remaining: usize) {
            if remaining == 0 {
                assert!(matches!(
                    value.reproject(Profile::Chat, Contract::full()),
                    Err(RepresentationError::ProjectionLimit)
                ));
            } else {
                let next = value.reproject(Profile::Chat, Contract::full()).unwrap();
                descend(&next, remaining - 1);
            }
        }
        let source = response();
        let fidelity = FidelityRecords::default();
        let metadata = metadata();
        let first = lower_response(
            &source,
            &fidelity,
            &metadata,
            Profile::Chat,
            Contract::full(),
        )
        .unwrap();
        assert_eq!(
            chat::encode_response(&first).unwrap()["usage"]["prompt_tokens_details"]["image_tokens"],
            7
        );
        descend(&first, MAX_PROJECTION_STAGES - 1);
    }

    fn text_events() -> Vec<StreamEvent> {
        use crate::events_support::{close, part, start};
        let mut events = vec![
            StreamEvent::Started,
            start(1, ItemKind::Message { phase: None }),
        ];
        events.extend(part(1, 2, PartKind::Text, "answer"));
        events.push(close(1, ItemLifecycle::Completed));
        events
    }

    #[test]
    fn a_stream_cannot_switch_target_policy_after_publication() {
        use morphiecore::protocol::openai::events::EventEncoder;
        let mut encoder = EventEncoder::new(Profile::Chat, metadata()).unwrap();
        let fidelity = FidelityRecords::default();
        encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
        let mut contract = Contract::full();
        contract.adaptation.rules.structured_chat_reasoning = true;
        encoder = encoder.with_contract(contract);
        assert!(encoder.encode(&text_events()[1], &fidelity).is_err());
    }

    #[test]
    fn accounting_loss_cannot_hide_group_phase_replay_or_resource_rejections() {
        let original = response();
        let fidelity = FidelityRecords::default();
        let metadata = metadata();
        let mut variants = vec![];
        let mut items = original.items().to_vec();
        let Item::Message(message) = &mut items[0].1 else {
            unreachable!()
        };
        message.phase = Some(Phase::FinalAnswer);
        variants.push(
            GenerationResponse::new(items, Outcome::Completed)
                .unwrap()
                .with_usage(original.usage())
                .unwrap(),
        );
        let mut items = original.items().to_vec();
        let Item::Message(message) = &mut items[0].1 else {
            unreachable!()
        };
        message.parts.push(Part {
            id: PartId::new(3),
            content: ContentPart::Text(text("second").into()),
            cache: None,
        });
        variants.push(original.clone().with_items(items).unwrap());
        let mut items = original.items().to_vec();
        let Item::Message(mut second) = items[0].1.clone() else {
            unreachable!()
        };
        second.parts[0].id = PartId::new(3);
        items.push((ItemId::new(3), Item::Message(second)));
        variants.push(original.clone().with_items(items).unwrap());
        let mut items = original.items().to_vec();
        items.insert(
            0,
            (
                ItemId::new(4),
                Item::Reasoning(ReasoningItem {
                    parts: vec![],
                    replay: Some(ReplayValue::final_value(
                        ReplayFormat::ResponsesEncrypted,
                        text("synthetic-opaque"),
                    )),
                    status: ItemLifecycle::Completed,
                }),
            ),
        );
        variants.push(original.clone().with_items(items).unwrap());
        for changed in variants {
            assert!(
                lower_response(
                    &changed,
                    &fidelity,
                    &metadata,
                    Profile::Chat,
                    Contract::full()
                )
                .is_err()
            );
            assert_eq!(changed.usage(), original.usage());
        }
        for profile in [Profile::Chat, Profile::Responses] {
            let request =
                GenerationRequest::new(original.items().to_vec(), GenerationControls::default())
                    .unwrap();
            let checked = morphiecore::protocol::openai::lowering::lower_request(
                &request,
                &fidelity,
                profile,
                Contract::full(),
            )
            .unwrap();
            assert_eq!(checked.semantic(), &request);
            assert_eq!(
                checked.requirements(),
                GenerationRequirements::derive(&request)
            );
            assert_eq!(checked.profile(), profile);
        }
    }
}
