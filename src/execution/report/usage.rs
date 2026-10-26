//! Bounded source-accounting observations, never delivery estimates or billing totals.
use crate::{protocol::conversion::generation::EvidenceDelta, semantic::task::generation::*};
use serde::{Serialize, Serializer};
use serde_json::{Value, json};

const MAX_SCOPES: usize = 32;
const MAX_BYTES: usize = 16 << 10;

#[derive(Clone, Debug)]
struct Observation {
    report: UsageReport,
    latest: bool,
    cache_write_basis: CacheWriteBasis,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum CacheWriteBasis {
    Source,
    SelectedPolicy,
    Unassessed,
}
#[derive(Clone, Debug, Default)]
pub struct SourceUsage {
    reports: Vec<Observation>,
    omitted_observations: u64,
    bytes: usize,
}
impl SourceUsage {
    /// Call only after successful source validation. Repeated snapshots replace
    /// their scope; observing the same final report again is idempotent.
    pub(super) fn observe(&mut self, report: &UsageReport, evidence: &EvidenceDelta) {
        let basis = if evidence.contains_rule("openai.provider-cache-write-default.v1") {
            CacheWriteBasis::SelectedPolicy
        } else if evidence.omitted_entries != 0 {
            CacheWriteBasis::Unassessed
        } else {
            CacheWriteBasis::Source
        };
        let previous = self
            .reports
            .iter()
            .position(|r| r.report.scope == report.scope);
        let old_bytes = previous.map_or(0, |i| retained_bytes(&self.reports[i].report));
        let bytes = self.bytes - old_bytes + retained_bytes(report);
        if bytes > MAX_BYTES || previous.is_none() && self.reports.len() >= MAX_SCOPES {
            if let Some(i) = previous {
                // Keep the prior source fact, but never present it as the latest.
                self.reports[i].latest = false;
            }
            self.omitted_observations = self.omitted_observations.saturating_add(1);
            return;
        }
        match previous {
            Some(i) => {
                if self.reports[i].report != *report {
                    self.reports[i].report = report.clone();
                }
                self.reports[i].latest = true;
                self.reports[i].cache_write_basis = basis;
            }
            None => self.reports.push(Observation {
                report: report.clone(),
                latest: true,
                cache_write_basis: basis,
            }),
        }
        self.bytes = bytes;
    }
}
fn retained_bytes(report: &UsageReport) -> usize {
    report.bytes() + std::mem::size_of::<Observation>() - std::mem::size_of::<UsageReport>()
}
impl Serialize for SourceUsage {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("SourceUsage", 2)?;
        state.serialize_field("reports", &self.reports)?;
        state.serialize_field("omitted_observations", &self.omitted_observations)?;
        state.end()
    }
}
// Closed semantic enums and numeric counters only. Never serialize arbitrary IR,
// Provider payloads, labels or debug strings into operational telemetry.
macro_rules! label {
    ($value:expr, $kind:ident, $($variant:ident => $label:literal),+ $(,)?) => {
        match $value { $($kind::$variant => $label),+ }
    };
}
fn bucket(value: UsageBucket) -> &'static str {
    label!(value, UsageBucket, Input => "input", ToolInput => "tool_input", Output => "output", CacheRead => "cache_read")
}
fn counter(value: UsageCounter) -> &'static str {
    label!(value, UsageCounter, Input => "input", ToolInput => "tool_input", Output => "output", Reasoning => "reasoning", CacheRead => "cache_read", Total => "total")
}
impl Serialize for Observation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let r = &self.report;
        let c = r.counts;
        let accounting = match r.accounting {
            UsageAccounting::Direct => json!({"kind":"direct"}),
            UsageAccounting::SplitGeneration {
                tool_input_tokens,
                tool_input_relation,
                total_relation,
            } => json!({
                "kind":"split_generation", "tool_input_tokens":tool_input_tokens,
                "tool_input_relation":label!(tool_input_relation, ToolInputRelation, Included => "included", Additional => "additional", Unreported => "unreported"),
                "total_relation":label!(total_relation, SplitTotalRelation, InputToolOutputAndReasoning => "input_tool_output_and_reasoning", Unreported => "unreported")
            }),
        };
        let modalities: Vec<Value> = r.details.modalities.iter().map(|m| json!({
            "bucket":bucket(m.bucket), "tokens":m.tokens, "defaulted":m.defaulted,
            "modality":m.modality.map(|v| label!(v, TokenModality, Unspecified => "unspecified", Text => "text", Image => "image", Audio => "audio", Video => "video", Document => "document"))
        })).collect();
        let cache_writes: Vec<Value> = r.details.cache_writes.iter().map(|c| json!({
            "ttl":label!(c.ttl, PrefixCacheTtl, FiveMinutes => "5m", OneHour => "1h"), "tokens":c.tokens
        })).collect();
        let tool_requests: Vec<Value> = r.details.tool_requests.iter().map(|t| json!({
            "kind":label!(t.kind, ToolUsageKind, Unreported => "unreported", WebSearch => "web_search", WebFetch => "web_fetch", MapSearch => "map_search", Retrieval => "retrieval"), "count":t.count
        })).collect();
        json!({
            "latest":self.latest,
            "cache_write_basis":self.cache_write_basis,
            "scope":match r.scope { UsageScope::Operation => json!({"kind":"operation"}), UsageScope::Step(id) => json!({"kind":"step","id":id}) },
            "stage":label!(r.stage, UsageStage, Checkpoint => "checkpoint", Final => "final"),
            "counts":{
                "input_tokens":c.input_tokens, "output_tokens":c.output_tokens, "total_tokens":c.total_tokens,
                "reasoning_tokens":c.reasoning_tokens, "cached_input_tokens":c.cached_input_tokens,
                "input_cache_write_tokens":match self.cache_write_basis {
                    CacheWriteBasis::Source => c.input_cache_write_tokens,
                    CacheWriteBasis::SelectedPolicy | CacheWriteBasis::Unassessed => None,
                },
                "accepted_prediction_tokens":c.accepted_prediction_tokens, "rejected_prediction_tokens":c.rejected_prediction_tokens,
                "input_relation":label!(c.input_relation, InputTokenRelation, IncludesCache => "includes_cache", ExcludesCacheReadAndWrite => "excludes_cache_read_and_write", Unreported => "unreported"),
                "output_relation":label!(c.output_relation, OutputTokenRelation, IncludesReasoning => "includes_reasoning", ExcludesReasoning => "excludes_reasoning", Unreported => "unreported"),
                "total_relation":label!(c.total_relation, TotalTokenRelation, InputAndOutput => "input_and_output", InputOutputAndReasoning => "input_output_and_reasoning", InputCacheAndOutput => "input_cache_and_output", Unreported => "unreported")
            },
            "accounting":accounting,
            "details":{ "modalities":modalities, "cache_writes":cache_writes, "tool_requests":tool_requests,
                "defaulted":r.details.defaulted.iter().copied().map(counter).collect::<Vec<_>>() }
        }).serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepseek_policy_zero_is_not_source_zero_in_static_or_streamed_accounting() {
        use crate::{
            execution::{Attempt, report::Report},
            protocol::{
                openai::{Dialect, GenerationApi, GenerationCodec},
                stream::SseLimits,
            },
        };
        for profile in [GenerationApi::Chat, GenerationApi::Responses] {
            for streaming in [false, true] {
                for explicit_zero in [false, true] {
                    let chat = profile == GenerationApi::Chat;
                    let mut usage = if chat {
                        json!({"prompt_tokens":3,"completion_tokens":2,"total_tokens":5})
                    } else {
                        json!({"input_tokens":3,"output_tokens":2,"total_tokens":5})
                    };
                    if explicit_zero {
                        usage[if chat {
                            "prompt_tokens_details"
                        } else {
                            "input_tokens_details"
                        }] = json!({"cache_write_tokens":0});
                    }
                    let snapshot = if chat {
                        json!({"id":"native","object":"chat.completion","created":1,"model":"deepseek",
                            "choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],
                            "usage":usage})
                    } else {
                        json!({"id":"native","object":"response","created_at":1,"status":"completed",
                            "model":"deepseek","output":[],"usage":usage})
                    };
                    let packets = if !streaming {
                        vec![snapshot.to_string()]
                    } else if chat {
                        vec![
                            format!(
                                "data: {}\n\n",
                                json!({"id":"native","object":"chat.completion.chunk",
                                "created":1,"model":"deepseek","choices":[{"index":0,
                                "delta":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],
                                "usage":snapshot["usage"]})
                            ),
                            "data: [DONE]\n\n".into(),
                        ]
                    } else {
                        let mut opening = snapshot.clone();
                        opening["status"] = json!("in_progress");
                        opening["usage"] = Value::Null;
                        vec![
                            format!(
                                "data: {}\n\n",
                                json!({"type":"response.created","sequence_number":0,"response":opening})
                            ),
                            format!(
                                "data: {}\n\n",
                                json!({"type":"response.completed","sequence_number":1,"response":snapshot})
                            ),
                        ]
                    };
                    let mut attempt = Attempt::new(
                        GenerationCodec::new(profile, Dialect::DeepSeek, None),
                        4096,
                        SseLimits::default(),
                    );
                    let report = Report::default();
                    report.begin(1);
                    attempt
                        .begin(
                            200,
                            if streaming {
                                "text/event-stream"
                            } else {
                                "application/json"
                            },
                        )
                        .unwrap();
                    for wire in packets {
                        let (_, events) = attempt.push(wire.as_bytes()).unwrap();
                        let evidence = attempt.take_evidence();
                        for event in events {
                            if let StreamEvent::Usage(usage) = event {
                                report.source_usage(&UsageReport::operation(usage), &evidence);
                            }
                        }
                    }
                    attempt.finish().unwrap();
                    let evidence = attempt.take_evidence();
                    let normalized = attempt
                        .response()
                        .unwrap()
                        .semantic
                        .usage_reports()
                        .operation()
                        .unwrap();
                    assert_eq!(normalized.counts.input_cache_write_tokens, Some(0));
                    if !streaming {
                        report.source_usage(normalized, &evidence);
                    }
                    let value = serde_json::to_value(report.snapshot()).unwrap();
                    let source = &value["attempts"][0]["source_usage"]["reports"][0];
                    assert_eq!(
                        source["cache_write_basis"],
                        if explicit_zero {
                            "source"
                        } else {
                            "selected_policy"
                        },
                        "{profile:?}, streaming={streaming}, explicit_zero={explicit_zero}"
                    );
                    assert_eq!(
                        source["counts"]["input_cache_write_tokens"],
                        if explicit_zero { json!(0) } else { Value::Null }
                    );
                }
            }
        }
    }

    #[test]
    fn current_unit_provenance_replaces_policy_and_missing_evidence_stays_unassessed() {
        let mut usage = SourceUsage::default();
        let mut source = UsageReport::operation(Usage::operation(1, 0, 1));
        source.counts.input_cache_write_tokens = Some(0);
        let mut policy = EvidenceDelta::default();
        policy.record_cache_write_default(
            "synthetic",
            crate::protocol::conversion::generation::ConversionBoundary::EventDecode,
        );
        usage.observe(&source, &policy);
        assert_eq!(
            serde_json::to_value(&usage).unwrap()["reports"][0]["cache_write_basis"],
            "selected_policy"
        );
        usage.observe(&source, &Default::default());
        let value = serde_json::to_value(&usage).unwrap();
        assert_eq!(value["reports"][0]["cache_write_basis"], "source");
        assert_eq!(value["reports"][0]["counts"]["input_cache_write_tokens"], 0);
        let mut partial = EvidenceDelta::default();
        partial.omitted_entries = 1;
        usage.observe(&source, &partial);
        let value = serde_json::to_value(&usage).unwrap();
        assert_eq!(value["reports"][0]["cache_write_basis"], "unassessed");
        assert!(value["reports"][0]["counts"]["input_cache_write_tokens"].is_null());
    }

    #[test]
    fn scopes_replace_without_summing_or_filling_unknowns_and_remain_attempt_local() {
        let report = super::super::Report::default();
        report.begin(1);
        let mut source = UsageReport::operation(Usage::operation(10, 2, 12));
        source.stage = UsageStage::Checkpoint;
        source.validate().unwrap();
        report.source_usage(&source, &Default::default());
        source.counts = Usage::operation(20, 3, 23).aggregate_counts();
        source.counts.total_tokens = None;
        source.counts.cached_input_tokens = Some(0);
        source.details.defaulted = vec![UsageCounter::CacheRead];
        source.validate().unwrap();
        report.source_usage(&source, &Default::default());
        source.scope = UsageScope::Step(9);
        source.stage = UsageStage::Final;
        report.source_usage(&source, &Default::default());
        report.end(false);
        report.begin(2);
        let final_report = UsageReport::operation(Usage::operation(4, 0, 4));
        report.source_usage(&final_report, &Default::default());
        report.source_usage(&final_report, &Default::default());
        report.end(true);
        let value = serde_json::to_value(report.snapshot()).unwrap();
        let first = &value["attempts"][0]["source_usage"]["reports"];
        assert_eq!(first.as_array().unwrap().len(), 2);
        assert_eq!(first[0]["counts"]["input_tokens"], 20);
        assert!(first[0]["counts"]["total_tokens"].is_null());
        assert!(first[0]["counts"]["reasoning_tokens"].is_null());
        assert_eq!(first[0]["details"]["defaulted"], json!(["cache_read"]));
        assert_eq!(first[0]["counts"]["cached_input_tokens"], 0);
        assert_eq!(first[1]["scope"], json!({"kind":"step","id":9}));
        assert_eq!(first[1]["stage"], "final");
        let second = &value["attempts"][1]["source_usage"]["reports"];
        assert_eq!(second.as_array().unwrap().len(), 1);
        assert_eq!(second[0]["counts"]["output_tokens"], 0);
    }

    #[test]
    fn retention_loss_is_bounded_and_old_observations_are_not_claimed_current() {
        let mut usage = SourceUsage::default();
        let mut source = UsageReport::operation(Usage::operation(1, 0, 1));
        for id in 0..MAX_SCOPES as u64 {
            source.scope = UsageScope::Step(id);
            usage.observe(&source, &Default::default());
        }
        source.details.modalities = [
            UsageBucket::Input,
            UsageBucket::ToolInput,
            UsageBucket::Output,
            UsageBucket::CacheRead,
        ]
        .into_iter()
        .flat_map(|bucket| {
            [
                None,
                Some(TokenModality::Unspecified),
                Some(TokenModality::Text),
                Some(TokenModality::Image),
                Some(TokenModality::Audio),
                Some(TokenModality::Video),
                Some(TokenModality::Document),
            ]
            .into_iter()
            .map(move |modality| ModalityTokens {
                bucket,
                modality,
                tokens: Some(0),
                defaulted: false,
            })
        })
        .collect();
        source.validate().unwrap();
        let mut rejected = None;
        for i in 0..MAX_SCOPES {
            source.scope = UsageScope::Step(i as u64);
            usage.observe(&source, &Default::default());
            if !usage.reports[i].latest {
                rejected = Some(i);
                break;
            }
        }
        let rejected = rejected.expect("valid breakdowns reach the byte bound");
        assert_eq!(usage.reports[rejected].report.counts.input_tokens, Some(1));
        assert_eq!(usage.omitted_observations, 1);
        source.details.modalities.clear();
        source.validate().unwrap();
        usage.observe(&source, &Default::default());
        assert!(usage.reports[rejected].latest);
        source.scope = UsageScope::Step(MAX_SCOPES as u64);
        usage.observe(&source, &Default::default());
        assert!(usage.reports.len() <= MAX_SCOPES);
        assert!(usage.bytes <= MAX_BYTES);
        assert!(usage.omitted_observations >= 2);
    }
}
