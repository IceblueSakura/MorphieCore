use morphiecore::semantic::task::generation::*;
use proptest::prelude::*;

fn report(scope: UsageScope, stage: UsageStage, tokens: u64) -> UsageReport {
    let mut report = UsageReport::operation(Usage::operation(tokens, 0, tokens));
    report.scope = scope;
    report.stage = stage;
    report
}

#[test]
fn sparse_breakdowns_still_validate_aggregate_accounting_premises() {
    let mut usage = Usage::operation(0, 0, 0);
    usage.input_tokens = None;
    usage.output_tokens = None;
    usage.total_tokens = None;
    usage.input_text_tokens = Some(1);
    let valid = UsageReport::operation(usage);
    valid.validate().unwrap();
    assert_eq!(valid.summary().input_text_tokens, Some(1));
    assert_eq!(valid.summary().input_tokens, None);
    let mut reports = UsageReports::default();
    reports.replace(valid.clone()).unwrap();
    let before = reports.clone();
    for (total, input, output) in [
        (
            TotalTokenRelation::InputOutputAndReasoning,
            InputTokenRelation::IncludesCache,
            OutputTokenRelation::IncludesReasoning,
        ),
        (
            TotalTokenRelation::InputCacheAndOutput,
            InputTokenRelation::IncludesCache,
            OutputTokenRelation::IncludesReasoning,
        ),
        (
            TotalTokenRelation::InputCacheAndOutput,
            InputTokenRelation::ExcludesCacheReadAndWrite,
            OutputTokenRelation::ExcludesReasoning,
        ),
    ] {
        let invalid = Usage {
            total_relation: total,
            input_relation: input,
            output_relation: output,
            ..usage
        };
        assert_eq!(invalid.validate(), Err(GenerationError::InvalidResponse));
        assert_eq!(
            UsageReport::operation(invalid).validate(),
            Err(GenerationError::InvalidResponse)
        );
        assert_eq!(
            reports.replace(UsageReport::operation(invalid)),
            Err(GenerationError::InvalidResponse)
        );
        assert_eq!(reports, before);
        assert_eq!(reports.bytes(), before.bytes());
        assert_eq!(
            GenerationResponse::new(vec![], Outcome::Completed)
                .unwrap()
                .with_usage(Some(invalid)),
            Err(GenerationError::InvalidResponse)
        );
    }
    let mut sparse = valid;
    sparse.details.modalities[0].tokens = None;
    sparse.validate().unwrap();
    assert_eq!(sparse.summary().input_text_tokens, None);
}

#[test]
fn modality_observations_have_one_owner_and_flat_views_follow_edits() {
    let mut source = UsageReport::operation(Usage {
        input_text_tokens: Some(3),
        input_image_tokens: Some(4),
        input_audio_tokens: Some(0),
        ..Usage::operation(10, 2, 12)
    });
    source.validate().unwrap();
    assert_eq!(source.details.modalities.len(), 3);
    assert!(!source.has_unrepresented_details());
    assert_eq!(source.counts.view().input_text_tokens, None);
    assert_eq!(source.summary().input_text_tokens, Some(3));
    assert_eq!(source.summary().input_audio_tokens, Some(0));
    source.details.modalities[0].tokens = Some(6);
    source.validate().unwrap();
    assert_eq!(source.summary().input_text_tokens, Some(6));
    source.details.modalities[0].tokens = Some(7);
    assert_eq!(source.validate(), Err(GenerationError::InvalidResponse));
    source.details.modalities.remove(0);
    source.validate().unwrap();
    assert_eq!(source.summary().input_text_tokens, None);
    source.details.modalities[0].tokens = None;
    assert!(source.has_unrepresented_details());
    assert_eq!(source.summary().input_image_tokens, None);
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
