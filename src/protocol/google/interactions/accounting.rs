//! Interactions usage counters → IR accounting.
use super::sse::{invalid, object};
use crate::{
    protocol::{CodecError, stream::SseError},
    semantic::task::generation::*,
};
use serde_json::{Map, Value};

pub(super) fn counter(o: &Map<String, Value>, key: &'static str) -> Result<Option<u64>, SseError> {
    match o.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_u64().map(Some).ok_or_else(|| invalid(key)),
    }
}
pub(super) fn usage(
    v: &Value,
    scope: UsageScope,
    stage: UsageStage,
) -> Result<UsageReport, SseError> {
    let o = object(v, "Interactions usage")?;
    let output = counter(o, "total_output_tokens")?;
    let thoughts = counter(o, "total_thought_tokens")?;
    let mut u = Usage::operation(0, 0, 0);
    u.input_tokens = counter(o, "total_input_tokens")?;
    u.cached_input_tokens = counter(o, "total_cached_tokens")?;
    u.reasoning_tokens = thoughts;
    u.output_tokens = output;
    u.output_relation = OutputTokenRelation::ExcludesReasoning;
    u.total_relation = TotalTokenRelation::Unreported;
    u.total_tokens = counter(o, "total_tokens")?;
    let mut report = UsageReport::operation(u);
    report.scope = scope;
    report.stage = stage;
    report.accounting = UsageAccounting::SplitGeneration {
        tool_input_tokens: counter(o, "total_tool_use_tokens")?,
        tool_input_relation: ToolInputRelation::Unreported,
        total_relation: SplitTotalRelation::Unreported,
    };
    for (field, bucket) in [
        ("input_tokens_by_modality", UsageBucket::Input),
        ("tool_use_tokens_by_modality", UsageBucket::ToolInput),
        ("output_tokens_by_modality", UsageBucket::Output),
        ("cached_tokens_by_modality", UsageBucket::CacheRead),
    ] {
        if let Some(value) = o.get(field).filter(|v| !v.is_null()) {
            report
                .details
                .modalities
                .extend(crate::protocol::usage::modality_tokens(
                    value, bucket, false,
                )?);
        }
    }
    if let Some(value) = o.get("grounding_tool_count").filter(|v| !v.is_null()) {
        report.details.tool_requests = crate::protocol::usage::grounding_counts(value)?;
    }
    report.validate().map_err(CodecError::from)?;
    Ok(report)
}
