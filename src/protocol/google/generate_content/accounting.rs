//! `usageMetadata` → IR accounting. Zero-valued proto3 integers are absent, never invented.
use crate::protocol::failure::ShapeReason;
use crate::{
    protocol::{CodecError, stream::SseError},
    semantic::task::generation::*,
};
use serde_json::{Map, Value};

fn invalid(_what: &'static str) -> SseError {
    CodecError::Invalid(ShapeReason::Usage).into()
}
fn object<'a>(value: &'a Value, what: &'static str) -> Result<&'a Map<String, Value>, SseError> {
    value.as_object().ok_or_else(|| invalid(what))
}

pub(super) fn counter(o: &Map<String, Value>, key: &'static str) -> Result<u64, SseError> {
    match o.get(key) {
        None => Ok(0),
        Some(Value::Number(n)) => n.as_u64().ok_or_else(|| invalid(key)),
        Some(Value::String(s)) => s.parse().map_err(|_| invalid(key)),
        Some(_) => Err(invalid(key)),
    }
}
pub(super) fn usage(v: &Value) -> Result<UsageReport, SseError> {
    let o = object(v, "usageMetadata")?;
    let prompt = counter(o, "promptTokenCount")?;
    let tool_prompt = counter(o, "toolUsePromptTokenCount")?;
    let candidates = counter(o, "candidatesTokenCount")?;
    let thoughts = counter(o, "thoughtsTokenCount")?;
    let mut usage = Usage::operation(prompt, candidates, counter(o, "totalTokenCount")?);
    usage.output_relation = OutputTokenRelation::ExcludesReasoning;
    usage.total_relation = TotalTokenRelation::Unreported;
    usage.reasoning_tokens = Some(thoughts);
    usage.cached_input_tokens = Some(counter(o, "cachedContentTokenCount")?);
    let mut report = UsageReport::operation(usage);
    report.accounting = UsageAccounting::SplitGeneration {
        tool_input_tokens: Some(tool_prompt),
        tool_input_relation: ToolInputRelation::Additional,
        total_relation: SplitTotalRelation::InputToolOutputAndReasoning,
    };
    for (field, counter) in [
        ("promptTokenCount", UsageCounter::Input),
        ("toolUsePromptTokenCount", UsageCounter::ToolInput),
        ("candidatesTokenCount", UsageCounter::Output),
        ("thoughtsTokenCount", UsageCounter::Reasoning),
        ("cachedContentTokenCount", UsageCounter::CacheRead),
        ("totalTokenCount", UsageCounter::Total),
    ] {
        if !o.contains_key(field) {
            report.details.defaulted.push(counter);
        }
    }
    for (field, bucket) in [
        ("promptTokensDetails", UsageBucket::Input),
        ("toolUsePromptTokensDetails", UsageBucket::ToolInput),
        ("candidatesTokensDetails", UsageBucket::Output),
        ("cacheTokensDetails", UsageBucket::CacheRead),
    ] {
        if let Some(value) = o.get(field) {
            report
                .details
                .modalities
                .extend(crate::protocol::usage::modality_tokens(
                    value, bucket, true,
                )?);
        }
    }
    report.validate().map_err(CodecError::from)?;
    Ok(report)
}
