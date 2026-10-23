//! Source accounting and the exact Messages subset of shared usage reports.
use super::{CacheCreation, ServerToolUsage};
use crate::{
    protocol::CodecError,
    semantic::{context::ServiceClass, task::generation::*, value::Presence},
};

pub(super) fn counts(wire: &super::Usage) -> Usage {
    let mut counts = Usage::operation(0, wire.output_tokens, 0);
    counts.input_tokens = wire.input_tokens.value().copied();
    counts.total_tokens = None;
    counts.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
    counts.total_relation = TotalTokenRelation::Unreported;
    counts.cached_input_tokens = wire.cache_read_input_tokens.value().copied();
    counts.input_cache_write_tokens = wire.cache_creation_input_tokens.value().copied();
    counts.reasoning_tokens = wire.thinking_tokens.value().copied();
    counts
}

pub(super) fn report(wire: &super::Usage, stage: UsageStage) -> UsageReport {
    let mut report = UsageReport::operation(counts(wire));
    report.stage = stage;
    if let Some(cache) = wire.cache_creation.value() {
        report.details.cache_writes = vec![
            CacheWriteTokens {
                ttl: PrefixCacheTtl::FiveMinutes,
                tokens: cache.ephemeral_5m_input_tokens,
            },
            CacheWriteTokens {
                ttl: PrefixCacheTtl::OneHour,
                tokens: cache.ephemeral_1h_input_tokens,
            },
        ];
    }
    if let Some(tools) = wire.server_tool_use.value() {
        report.details.tool_requests = vec![
            ToolUseCount {
                kind: ToolUsageKind::WebSearch,
                count: Some(tools.web_search_requests),
            },
            ToolUseCount {
                kind: ToolUsageKind::WebFetch,
                count: Some(tools.web_fetch_requests),
            },
        ];
    }
    report
}

fn cache(report: &UsageReport) -> Option<CacheCreation> {
    let count = |ttl| {
        report
            .details
            .cache_writes
            .iter()
            .find(|c| c.ttl == ttl)
            .map(|c| c.tokens)
    };
    Some(CacheCreation {
        ephemeral_5m_input_tokens: count(PrefixCacheTtl::FiveMinutes)?,
        ephemeral_1h_input_tokens: count(PrefixCacheTtl::OneHour)?,
    })
}
fn tools(report: &UsageReport) -> Option<ServerToolUsage> {
    let count = |kind| {
        report
            .details
            .tool_requests
            .iter()
            .find(|c| c.kind == kind)
            .and_then(|c| c.count)
    };
    Some(ServerToolUsage {
        web_search_requests: count(ToolUsageKind::WebSearch)?,
        web_fetch_requests: count(ToolUsageKind::WebFetch)?,
    })
}

/// Messages usage: input excludes cache reads and reported cache writes.
/// An unreported write category stays absent; the remaining input keeps any such writes.
fn project_usage(u: Usage) -> Result<Usage, CodecError> {
    let unsupported = || CodecError::Unsupported("Messages usage".into());
    u.validate()?;
    let input = u.input_tokens.ok_or_else(unsupported)?;
    let input = match u.input_relation {
        InputTokenRelation::ExcludesCacheReadAndWrite => input,
        InputTokenRelation::IncludesCache => {
            let read = u.cached_input_tokens.ok_or_else(unsupported)?;
            let excluded = match u.input_cache_write_tokens {
                None => read,
                // IncludesCache does not promise disjoint cache-read and cache-write subsets.
                Some(write) if read != 0 && write != 0 => return Err(unsupported()),
                Some(write) => read.max(write),
            };
            input.checked_sub(excluded).ok_or_else(unsupported)?
        }
        InputTokenRelation::Unreported => return Err(unsupported()),
    };
    let output = u.output_tokens.ok_or_else(unsupported)?;
    let output = match u.output_relation {
        OutputTokenRelation::IncludesReasoning => output,
        OutputTokenRelation::ExcludesReasoning => output
            .checked_add(u.reasoning_tokens.ok_or_else(unsupported)?)
            .ok_or_else(unsupported)?,
        OutputTokenRelation::Unreported => return Err(unsupported()),
    };
    let mut selected = Usage::operation(input, output, 0);
    selected.total_tokens = None;
    selected.total_relation = TotalTokenRelation::Unreported;
    selected.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
    selected.cached_input_tokens = u.cached_input_tokens;
    selected.input_cache_write_tokens = u.input_cache_write_tokens;
    selected.reasoning_tokens = u.reasoning_tokens;
    Ok(selected)
}

/// Named delivery projection; never fills a missing native child with zero.
pub(super) fn project(report: &UsageReport) -> Result<Option<UsageReport>, CodecError> {
    report.validate()?;
    if report.scope != UsageScope::Operation {
        return Ok(None);
    }
    let Some(summary) = report.scalar_summary() else {
        return Ok(None);
    };
    let mut selected = report.clone();
    selected.counts = project_usage(summary)?.aggregate_counts();
    selected.accounting = UsageAccounting::Direct;
    selected.details.modalities.clear();
    selected.details.defaulted.clear();
    if cache(report).is_none() {
        selected.details.cache_writes.clear();
    }
    if tools(report).is_none() {
        selected.details.tool_requests.clear();
    } else {
        selected
            .details
            .tool_requests
            .retain(|c| matches!(c.kind, ToolUsageKind::WebSearch | ToolUsageKind::WebFetch));
    }
    selected.validate()?;
    Ok(Some(selected))
}

pub(super) fn project_reports(reports: &UsageReports) -> Result<UsageReports, CodecError> {
    let mut selected = UsageReports::default();
    if let Some(mut report) = reports.operation().map(project).transpose()?.flatten() {
        // Complete JSON has a final observation carrier, not a checkpoint carrier.
        report.stage = UsageStage::Final;
        selected.replace(report)?;
    }
    Ok(selected)
}

/// Pure native encoding accepts only an exact report, unlike the named client projection.
pub(super) fn wire(
    report: &UsageReport,
    service_class: Presence<ServiceClass>,
) -> Result<super::Usage, CodecError> {
    if project(report)?.as_ref() != Some(report) {
        return Err(CodecError::Unsupported("Messages usage report".into()));
    }
    let counts = report.counts;
    Ok(super::Usage {
        service_class,
        input_tokens: counts
            .input_tokens
            .map_or(Presence::Absent, Presence::Value),
        output_tokens: counts
            .output_tokens
            .ok_or(CodecError::Invalid("Messages output usage"))?,
        cache_creation_input_tokens: counts
            .input_cache_write_tokens
            .map_or(Presence::Absent, Presence::Value),
        cache_read_input_tokens: counts
            .cached_input_tokens
            .map_or(Presence::Absent, Presence::Value),
        thinking_tokens: counts
            .reasoning_tokens
            .map_or(Presence::Absent, Presence::Value),
        cache_creation: cache(report).map_or(Presence::Absent, Presence::Value),
        server_tool_use: tools(report).map_or(Presence::Absent, Presence::Value),
    })
}

/// A downstream-only completed view; never used by native decoders or encoders.
pub(super) fn delivery_wire(counts: Usage) -> Result<super::Usage, CodecError> {
    let report = UsageReport::operation(project_usage(counts)?);
    wire(&report, Presence::Absent)
}

pub(super) fn json_with_fallback(
    reports: &UsageReports,
    fallback: Option<Usage>,
) -> Result<serde_json::Value, CodecError> {
    if reports.operation().is_none()
        && let Some(counts) = fallback
    {
        return Ok(delivery_wire(counts)?.wire());
    }
    if reports.reports().len() != 1 {
        return Err(CodecError::Unsupported("Messages usage scopes".into()));
    }
    Ok(wire(
        reports
            .operation()
            .ok_or(CodecError::Invalid("Messages usage"))?,
        Presence::Absent,
    )?
    .wire())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scalar_projection_preserves_missing_counts_and_checks_accounting_premises() {
        let mut source = Usage::operation(20, 5, 25);
        source.cached_input_tokens = Some(3);
        let selected = project_usage(source).unwrap();
        assert_eq!(selected.input_tokens, Some(17));
        assert_eq!(selected.output_tokens, Some(5));
        assert_eq!(selected.input_cache_write_tokens, None);
        assert_eq!(selected.total_tokens, None);
        assert_eq!(
            selected.input_relation,
            InputTokenRelation::ExcludesCacheReadAndWrite
        );
        source.input_cache_write_tokens = Some(4);
        source.validate().unwrap();
        assert!(matches!(
            project_usage(source),
            Err(CodecError::Unsupported(_))
        ));
        source.cached_input_tokens = Some(0);
        assert_eq!(project_usage(source).unwrap().input_tokens, Some(16));
        source.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
        source.output_relation = OutputTokenRelation::ExcludesReasoning;
        source.total_relation = TotalTokenRelation::Unreported;
        source.total_tokens = None;
        source.reasoning_tokens = Some(2);
        assert_eq!(project_usage(source).unwrap().output_tokens, Some(7));
        for output in [None, Some(u64::MAX)] {
            source.output_tokens = output;
            source.validate().unwrap();
            assert!(matches!(
                project_usage(source),
                Err(CodecError::Unsupported(_))
            ));
        }
    }

    fn expected() -> UsageReport {
        let mut counts = Usage::operation(11, 7, 0);
        counts.total_tokens = None;
        counts.total_relation = TotalTokenRelation::Unreported;
        counts.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
        counts.cached_input_tokens = Some(3);
        counts.input_cache_write_tokens = Some(4);
        counts.reasoning_tokens = Some(2);
        let mut report = UsageReport::operation(counts);
        report.details.cache_writes = vec![
            CacheWriteTokens {
                ttl: PrefixCacheTtl::FiveMinutes,
                tokens: 1,
            },
            CacheWriteTokens {
                ttl: PrefixCacheTtl::OneHour,
                tokens: 3,
            },
        ];
        report.details.tool_requests = vec![
            ToolUseCount {
                kind: ToolUsageKind::WebSearch,
                count: Some(0),
            },
            ToolUseCount {
                kind: ToolUsageKind::WebFetch,
                count: Some(2),
            },
        ];
        report
    }
    fn usage_json() -> serde_json::Value {
        json!({"input_tokens":11,"output_tokens":7,"cache_read_input_tokens":3,
            "cache_creation_input_tokens":4,
            "cache_creation":{"ephemeral_5m_input_tokens":1,"ephemeral_1h_input_tokens":3},
            "server_tool_use":{"web_search_requests":0,"web_fetch_requests":2},
            "output_tokens_details":{"thinking_tokens":2}})
    }
    fn message() -> serde_json::Value {
        json!({"id":"msg","type":"message","role":"assistant","model":"model",
            "content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn",
            "stop_sequence":null,"usage":usage_json()})
    }
    #[test]
    fn source_breakdowns_and_native_writer_have_independent_expectations() {
        let p = super::super::Profile::AdaptiveTextTools;
        let decoded = p
            .decode_generation_response(
                message().to_string().as_bytes(),
                LocalScope::new(7),
                &crate::protocol::native::ReplayTarget::new("provider", "model").unwrap(),
            )
            .unwrap();
        assert_eq!(
            decoded.semantic.usage_reports().operation(),
            Some(&expected())
        );
        assert_eq!(decoded.semantic.usage().unwrap().output_text_tokens, None);
        let before = decoded.clone();
        let (retained, body) = crate::adapter::ClientAdapter::Messages
            .snapshot(decoded.clone())
            .unwrap();
        assert_eq!(body["usage"], usage_json());
        assert_eq!(
            retained.semantic.usage_reports(),
            decoded.semantic.usage_reports()
        );
        assert_eq!(decoded, before);
        assert_eq!(
            wire(&expected(), Presence::Absent).unwrap().wire(),
            usage_json()
        );

        let mut checkpoint = expected();
        checkpoint.stage = UsageStage::Checkpoint;
        let mut reports = UsageReports::default();
        reports.replace(checkpoint).unwrap();
        let mut source = decoded.clone();
        source.semantic = source.semantic.with_usage_reports(reports).unwrap();
        assert!(
            super::super::client::content_losses(&source.semantic)
                .iter()
                .any(|loss| loss.rule == "generation.usage-report-view.v1")
        );
        let (retained, body) = crate::adapter::ClientAdapter::Messages
            .snapshot(source.clone())
            .unwrap();
        let reparsed = p
            .decode_generation_response(
                body.to_string().as_bytes(),
                LocalScope::new(8),
                &crate::protocol::native::ReplayTarget::new("provider", "model").unwrap(),
            )
            .unwrap();
        assert_eq!(
            retained.semantic.usage_reports(),
            reparsed.semantic.usage_reports()
        );
        assert_eq!(
            source.semantic.usage_reports().operation().unwrap().stage,
            UsageStage::Checkpoint
        );

        let mut sparse = expected();
        sparse.details.tool_requests.pop();
        assert!(wire(&sparse, Presence::Absent).is_err());
        let selected = project(&sparse).unwrap().unwrap();
        assert!(selected.details.tool_requests.is_empty());
        assert_eq!(
            selected.details.cache_writes,
            expected().details.cache_writes
        );
    }
    #[test]
    fn strict_known_children_parent_consistency_and_late_opening_only_details() {
        let p = super::super::Profile::AdaptiveTextTools;
        for (field, value) in [
            ("cache_creation", json!({})),
            (
                "cache_creation",
                json!({"ephemeral_5m_input_tokens":null,"ephemeral_1h_input_tokens":3}),
            ),
            (
                "cache_creation",
                json!({"ephemeral_5m_input_tokens":2,"ephemeral_1h_input_tokens":3}),
            ),
            ("server_tool_use", json!({"web_search_requests":0})),
            (
                "server_tool_use",
                json!({"web_search_requests":-1,"web_fetch_requests":2}),
            ),
            ("output_tokens_details", json!({"thinking_tokens":8})),
        ] {
            let mut body = message();
            body["usage"][field] = value;
            assert!(
                p.decode_response(body.to_string().as_bytes()).is_err(),
                "{field}"
            );
        }
        let mut body = message();
        body["usage"]["cache_creation"]["future"] = json!(true);
        body["usage"]["server_tool_use"]["future"] = json!(null);
        assert!(p.decode_response(body.to_string().as_bytes()).is_ok());
        let delta = json!({"type":"message_delta","delta":{},
            "usage":{"output_tokens":7,"cache_creation":usage_json()["cache_creation"]}});
        assert!(
            matches!(p.decode_event("message_delta",delta.to_string().as_bytes()),
            Err(CodecError::Unsupported(field)) if field == "Messages delta cache breakdown")
        );

        let mut encoder = super::super::client::sse::ClientEventEncoder::new("id", "model");
        assert!(encoder.encode(&StreamEvent::Started).unwrap().is_empty());
        let frames = encoder
            .encode(&StreamEvent::ItemStarted {
                item: ItemId::new(1),
                kind: ItemKind::Message { phase: None },
                replay: None,
            })
            .unwrap();
        assert!(!frames.is_empty());
        assert!(
            matches!(encoder.encode(&StreamEvent::UsageReport(expected())),
            Err(CodecError::Unsupported(field)) if field == "Messages late cache breakdown")
        );
        assert!(encoder.finish().is_err());
    }
    #[test]
    fn final_reports_cannot_resurrect_or_silently_retain_opening_observations() {
        let mut opening = expected();
        opening.details.cache_writes.clear();
        opening.stage = UsageStage::Checkpoint;
        let terminal = StreamEvent::Terminal {
            terminal: StreamTerminal::Completed,
            details: TerminalDetails {
                completion: Some(CompletionCause::Natural),
                ..Default::default()
            },
        };
        for field in 0..5 {
            let mut encoder = super::super::client::sse::ClientEventEncoder::new("id", "model");
            encoder.encode(&StreamEvent::Started).unwrap();
            assert!(
                !encoder
                    .encode(&StreamEvent::UsageReport(opening.clone()))
                    .unwrap()
                    .is_empty()
            );
            encoder
                .encode(&StreamEvent::Progress(InteractionProgress::TurnFinished))
                .unwrap();
            let mut last = opening.clone();
            last.stage = UsageStage::Final;
            match field {
                0 => last.details.tool_requests.clear(),
                1 => last.counts.reasoning_tokens = None,
                2 => last.counts.cached_input_tokens = None,
                3 => last.counts.input_cache_write_tokens = None,
                4 => {
                    let mut counts = Usage::operation(0, 0, 0);
                    counts.input_tokens = None;
                    counts.output_tokens = None;
                    counts.total_tokens = None;
                    counts.total_relation = TotalTokenRelation::Unreported;
                    last.counts = counts.aggregate_counts();
                }
                _ => unreachable!(),
            }
            last.validate().unwrap();
            let expected = if field == 4 {
                "Messages usage"
            } else {
                "Messages erased usage observation"
            };
            assert!(matches!(encoder.encode(&StreamEvent::UsageReport(last)),
                Err(CodecError::Unsupported(reason)) if reason == expected));
            assert!(encoder.encode(&terminal).is_err());
            assert!(encoder.finish().is_err());
        }
    }
}
