//! Request-local presentation fallbacks. Never mutate upstream observations.
use crate::{
    protocol::{ClientProtocol, CodecError, GenerationDelivery, failure::ShapeReason},
    semantic::task::generation::*,
};

pub(super) fn settings(request: &GenerationRequest) -> GenerationSettings {
    let source = request.settings();
    GenerationSettings {
        tools: Some(source.tools.clone().unwrap_or_default()),
        tool_choice: Some(source.tool_choice.clone().unwrap_or_default()),
        parallel_tool_calls: Some(source.parallel_tool_calls.unwrap_or(true)),
        ..Default::default()
    }
}

fn json_bytes(value: &serde_json::Value) -> usize {
    // Semantic values are already bounded; count the compact representation once.
    serde_json::to_vec(value).map_or(0, |v| v.len())
}
fn item_bytes(item: &Item) -> usize {
    match item {
        Item::Instruction(i) => i.texts().map(|(_, t)| t.len()).sum(),
        Item::Message(m) => m
            .parts
            .iter()
            .map(|p| match &p.content {
                ContentPart::Text(t) => t.as_str().len(),
                ContentPart::Refusal(t) => t.as_str().len(),
                _ => 0,
            })
            .sum(),
        Item::Reasoning(r) => r
            .parts
            .iter()
            .map(|(_, p)| match p {
                ReasoningContent::Text(t) | ReasoningContent::Summary(t) => t.as_str().len(),
            })
            .sum(),
        Item::ToolCall(c) => {
            c.name.as_str().len()
                + match &c.arguments {
                    ToolArguments::Raw(s) | ToolArguments::StructuredPartial(s) => s.len(),
                    ToolArguments::Structured(v) => json_bytes(v.value()),
                }
        }
        Item::CustomCall(c) => c.name.as_str().len() + c.input.len(),
        Item::ToolResult(r) | Item::CustomResult(r) => match &r.output {
            ToolOutput::Text(t) => t.len(),
            ToolOutput::Structured(v) => json_bytes(v.value()),
            ToolOutput::Parts(parts) => parts
                .iter()
                .filter_map(|p| p.content.as_text())
                .map(str::len)
                .sum(),
        },
        _ => 0,
    }
}
fn tokens(bytes: usize) -> u64 {
    bytes.div_ceil(4) as u64
}
fn infer_residual(total: Option<u64>, counts: &mut [Option<u64>]) -> Result<(), CodecError> {
    if let Some(total) = total
        && counts.iter().filter(|v| v.is_none()).count() == 1
    {
        let known = counts
            .iter()
            .flatten()
            .try_fold(0u64, |sum, n| sum.checked_add(*n))
            .ok_or(CodecError::Limit)?;
        *counts
            .iter_mut()
            .find(|v| v.is_none())
            .expect("one missing counter") =
            Some(total.checked_sub(known).ok_or(CodecError::invalid())?);
    }
    Ok(())
}
pub(super) fn input_estimate(request: &GenerationRequest) -> u64 {
    let s = request.settings();
    let mut bytes: usize = request.items().iter().map(|(_, i)| item_bytes(i)).sum();
    bytes += s.instructions.value().map_or(0, |root| root.text_bytes());
    for tool in s.tools.iter().flatten().flat_map(ToolDefinition::leaves) {
        bytes += tool.name().as_str().len();
        match tool {
            ToolDefinition::Function(f) => {
                bytes += f.description.as_ref().map_or(0, String::len);
                bytes += f.parameters.as_ref().map_or(0, |s| json_bytes(s.root()));
            }
            ToolDefinition::Custom(c) => {
                bytes += c.description.as_ref().map_or(0, String::len);
                if let Some(CustomFormat::Grammar { definition, .. }) = &c.format {
                    bytes += definition.as_str().len();
                }
            }
            ToolDefinition::Namespace(_) => {}
        }
    }
    tokens(bytes).max(1)
}

/// Complete target scalars using known accounting first, then bounded text estimates.
/// Unreported total relationships cannot assert an OpenAI sum; retain them only
/// in the source. Zero estimates for unknown cache categories are not reports.
#[derive(Clone, Copy)]
enum UsageMethod {
    Accounting,
    Residual,
    CacheInclusive,
    CacheExclusive,
    ReasoningInclusive,
    InputBytes,
    OutputBytes,
    ReasoningBytes,
    MissingCache,
    CacheOverlap,
    ToolOverlap,
    InputRelation,
    OutputRelation,
    SplitEstimate,
    ZeroCache,
    CompletedTotal,
    CompletedResidual,
    DetailFloor,
    DetailSum,
}
impl UsageMethod {
    fn estimated(self) -> bool {
        !matches!(
            self,
            Self::Accounting
                | Self::Residual
                | Self::CacheInclusive
                | Self::CacheExclusive
                | Self::ReasoningInclusive
                | Self::DetailSum
        )
    }
    fn rule(self) -> &'static str {
        match self {
            Self::Accounting => "generation.usage-known-accounting.v1",
            Self::Residual => "generation.usage-exact-residual.v1",
            Self::CacheInclusive => "generation.usage-cache-inclusive.v1",
            Self::CacheExclusive => "generation.usage-cache-exclusive.v1",
            Self::ReasoningInclusive => "generation.usage-reasoning-inclusive.v1",
            Self::InputBytes => "generation.usage-input-bytes.v1",
            Self::OutputBytes => "generation.usage-output-bytes.v1",
            Self::ReasoningBytes => "generation.usage-reasoning-bytes.v1",
            Self::MissingCache => "generation.usage-missing-cache-zero.v1",
            Self::CacheOverlap => "generation.usage-cache-overlap-max.v1",
            Self::ToolOverlap => "generation.usage-tool-overlap-max.v1",
            Self::InputRelation => "generation.usage-assumed-input-relation.v1",
            Self::OutputRelation => "generation.usage-assumed-output-relation.v1",
            Self::SplitEstimate => "generation.usage-split-total-bytes.v1",
            Self::ZeroCache => "generation.usage-unreported-cache-zero.v1",
            Self::CompletedTotal => "generation.usage-total-from-estimates.v1",
            Self::CompletedResidual => "generation.usage-residual-from-estimates.v1",
            Self::DetailFloor => "generation.usage-detail-floor.v1",
            Self::DetailSum => "generation.usage-detail-sum.v1",
        }
    }
}
pub(super) fn complete(
    delivery: &mut GenerationDelivery,
    response: &GenerationResponse,
    protocol: ClientProtocol,
    input_estimate: u64,
) -> Result<crate::protocol::conversion::generation::EvidenceDelta, CodecError> {
    let source = response.usage();
    let mut input_method = UsageMethod::Accounting;
    let mut output_method = UsageMethod::Accounting;
    let mut cache_read_method = UsageMethod::Accounting;
    let mut cache_write_method = UsageMethod::Accounting;
    let mut reasoning_method = UsageMethod::Accounting;
    let output_estimate = tokens(response.items().iter().map(|(_, i)| item_bytes(i)).sum());
    let reasoning_estimate = tokens(
        response
            .items()
            .iter()
            .filter(|(_, i)| matches!(i, Item::Reasoning(_)))
            .map(|(_, i)| item_bytes(i))
            .sum(),
    );
    let mut u = source.unwrap_or_else(|| {
        let mut u = Usage::operation(0, 0, 0);
        u.input_tokens = None;
        u.output_tokens = None;
        u.total_tokens = None;
        u
    });
    // Both TTL categories exhaust cache writes. A single category is only a
    // lower bound; retain the source's unknown parent and estimate on delivery.
    let cache_writes = response
        .usage_reports()
        .operation()
        .map(|r| r.details.cache_writes.as_slice())
        .unwrap_or_default();
    let write_floor = cache_writes.iter().try_fold(0u64, |sum, part| {
        sum.checked_add(part.tokens).ok_or(CodecError::Limit)
    })?;
    if u.input_cache_write_tokens.is_none() && cache_writes.len() == 2 {
        u.input_cache_write_tokens = Some(write_floor);
        cache_write_method = UsageMethod::DetailSum;
    }
    match u.total_relation {
        TotalTokenRelation::InputAndOutput => {
            let mut counts = [u.input_tokens, u.output_tokens];
            infer_residual(u.total_tokens, &mut counts)?;
            [u.input_tokens, u.output_tokens] = counts;
        }
        TotalTokenRelation::InputOutputAndReasoning => {
            let mut counts = [u.input_tokens, u.output_tokens, u.reasoning_tokens];
            infer_residual(u.total_tokens, &mut counts)?;
            [u.input_tokens, u.output_tokens, u.reasoning_tokens] = counts;
        }
        TotalTokenRelation::InputCacheAndOutput => {
            let mut counts = [
                u.input_tokens,
                u.cached_input_tokens,
                u.input_cache_write_tokens,
                u.output_tokens,
            ];
            infer_residual(u.total_tokens, &mut counts)?;
            [
                u.input_tokens,
                u.cached_input_tokens,
                u.input_cache_write_tokens,
                u.output_tokens,
            ] = counts;
        }
        _ => {}
    }
    if source.is_some_and(|s| s.input_tokens.is_none()) && u.input_tokens.is_some() {
        input_method = UsageMethod::Residual;
    }
    if source.is_some_and(|s| s.output_tokens.is_none()) && u.output_tokens.is_some() {
        output_method = UsageMethod::Residual;
    }
    if source.is_some_and(|s| s.cached_input_tokens.is_none()) && u.cached_input_tokens.is_some() {
        cache_read_method = UsageMethod::Residual;
    }
    if source.is_some_and(|s| s.input_cache_write_tokens.is_none())
        && u.input_cache_write_tokens.is_some()
        && matches!(cache_write_method, UsageMethod::Accounting)
    {
        cache_write_method = UsageMethod::Residual;
    }
    if source.is_some_and(|s| s.reasoning_tokens.is_none()) && u.reasoning_tokens.is_some() {
        reasoning_method = UsageMethod::Residual;
    }
    let inclusive_input_from_total = if u.total_relation == TotalTokenRelation::InputCacheAndOutput
    {
        u.total_tokens
            .zip(u.output_tokens)
            .map(|(t, o)| t.checked_sub(o).ok_or(CodecError::invalid()))
            .transpose()?
    } else {
        None
    };
    // Modalities within a reported bucket are disjoint; different buckets need
    // their own inclusion premise. Missing categories still do not become facts.
    let modality_floor = |bucket| {
        response
            .usage_reports()
            .operation()
            .into_iter()
            .flat_map(|report| &report.details.modalities)
            .filter(|detail| detail.bucket == bucket)
            .filter_map(|detail| detail.tokens)
            .try_fold(0u64, |sum, count| {
                sum.checked_add(count).ok_or(CodecError::Limit)
            })
    };
    let mut input_floor = modality_floor(UsageBucket::Input)?;
    let mut output_floor = modality_floor(UsageBucket::Output)?;
    let cache_floor = modality_floor(UsageBucket::CacheRead)?;
    if u.cached_input_tokens.is_none() && cache_floor > 0 {
        u.cached_input_tokens = Some(cache_floor);
        cache_read_method = UsageMethod::DetailFloor;
    }
    if u.input_cache_write_tokens.is_none() && write_floor > 0 {
        u.input_cache_write_tokens = Some(write_floor);
        cache_write_method = UsageMethod::DetailFloor;
    }
    if let Some(report) = response.usage_reports().operation()
        && report.counts.output_relation == OutputTokenRelation::ExcludesReasoning
    {
        output_floor = output_floor
            .checked_add(report.counts.reasoning_tokens.unwrap_or(0))
            .ok_or(CodecError::Limit)?;
    }
    if let Some(report) = response.usage_reports().operation()
        && let UsageAccounting::SplitGeneration {
            tool_input_tokens,
            tool_input_relation,
            ..
        } = report.accounting
    {
        let tool_floor = tool_input_tokens
            .unwrap_or(0)
            .max(modality_floor(UsageBucket::ToolInput)?);
        input_floor = input_floor.max(report.counts.input_tokens.unwrap_or(0));
        output_floor = output_floor.max(
            report
                .counts
                .output_tokens
                .unwrap_or(0)
                .checked_add(report.counts.reasoning_tokens.unwrap_or(0))
                .ok_or(CodecError::Limit)?,
        );
        input_floor = if tool_input_relation == ToolInputRelation::Additional {
            input_floor
                .checked_add(tool_floor)
                .ok_or(CodecError::Limit)?
        } else {
            input_floor.max(tool_floor)
        };
    }
    // Complete the source's attested inclusive total before changing target
    // conventions. Disjoint details bound the split, never change that total.
    if u.total_relation == TotalTokenRelation::InputAndOutput
        && u.input_relation == InputTokenRelation::IncludesCache
        && u.output_relation == OutputTokenRelation::IncludesReasoning
        && let (None, None, Some(total)) = (u.input_tokens, u.output_tokens, u.total_tokens)
    {
        let minimum_input = input_floor
            .max(u.cached_input_tokens.unwrap_or(0))
            .max(u.input_cache_write_tokens.unwrap_or(0));
        let minimum_output = output_floor.max(u.reasoning_tokens.unwrap_or(0)).max(
            u.accepted_prediction_tokens
                .unwrap_or(0)
                .checked_add(u.rejected_prediction_tokens.unwrap_or(0))
                .ok_or(CodecError::Limit)?,
        );
        let maximum_input = total
            .checked_sub(minimum_output)
            .filter(|maximum| *maximum >= minimum_input)
            .ok_or(CodecError::Invalid(ShapeReason::Usage))?;
        let input = input_estimate.clamp(minimum_input, maximum_input);
        u.input_tokens = Some(input);
        u.output_tokens = Some(total - input);
        input_method = if total == 0 {
            UsageMethod::Residual
        } else {
            UsageMethod::SplitEstimate
        };
        output_method = input_method;
    }
    let known_output = u
        .output_tokens
        .map(|n| {
            if u.output_relation == OutputTokenRelation::ExcludesReasoning {
                n.checked_add(u.reasoning_tokens.unwrap_or(0))
                    .ok_or(CodecError::Limit)
            } else {
                Ok(n)
            }
        })
        .transpose()?;
    if u.input_tokens.is_some_and(|n| n < input_floor)
        || known_output.is_some_and(|n| n < output_floor)
        || u.cached_input_tokens.is_some_and(|n| n < cache_floor)
        || u.input_cache_write_tokens.is_some_and(|n| n < write_floor)
    {
        return Err(CodecError::Invalid(ShapeReason::Usage));
    }
    if let Some(report) = response.usage_reports().operation()
        && matches!(
            report.accounting,
            UsageAccounting::SplitGeneration {
                total_relation: SplitTotalRelation::InputToolOutputAndReasoning,
                ..
            }
        )
        && u.reasoning_tokens.is_none()
        && let (Some(output), Some(visible)) = (u.output_tokens, report.counts.output_tokens)
    {
        u.reasoning_tokens = Some(
            output
                .checked_sub(visible)
                .ok_or(CodecError::Invalid(ShapeReason::Usage))?,
        );
        reasoning_method = if output_method.estimated() {
            UsageMethod::CompletedResidual
        } else {
            UsageMethod::Residual
        };
    }
    // A conservative scalar summary can hide known source buckets when an
    // inclusion relation or a sibling is missing. Estimate the missing relation,
    // not the already reported bucket. Unknown tool-input overlap uses max.
    if let Some(report) = response.usage_reports().operation()
        && let UsageAccounting::SplitGeneration {
            tool_input_tokens,
            tool_input_relation,
            ..
        } = report.accounting
    {
        if u.input_tokens.is_none()
            && let Some(input) = report.counts.input_tokens
        {
            input_method = if tool_input_relation == ToolInputRelation::Additional
                && tool_input_tokens.is_some()
            {
                UsageMethod::Accounting
            } else {
                UsageMethod::ToolOverlap
            };
            let tool = tool_input_tokens.unwrap_or(modality_floor(UsageBucket::ToolInput)?);
            u.input_tokens = Some(if tool_input_relation == ToolInputRelation::Additional {
                input.checked_add(tool).ok_or(CodecError::Limit)?
            } else {
                input.max(tool)
            });
        }
        if u.output_tokens.is_none() && report.counts.output_tokens.is_some() {
            if report.counts.reasoning_tokens.is_none() {
                output_method = UsageMethod::ReasoningBytes;
            }
            u.output_tokens = report.counts.output_tokens;
            u.output_relation = OutputTokenRelation::ExcludesReasoning;
            u.total_relation = TotalTokenRelation::Unreported;
        }
    }
    u.validate()
        .or_else(|e| if source.is_none() { Ok(()) } else { Err(e) })?;
    let messages = protocol == ClientProtocol::Messages;
    if u.input_relation == InputTokenRelation::Unreported {
        input_method = UsageMethod::InputRelation;
    }
    if u.output_relation == OutputTokenRelation::Unreported {
        output_method = UsageMethod::OutputRelation;
    }
    let add = |a: u64, b: u64| a.checked_add(b).ok_or(CodecError::Limit);
    let mut input = u.input_tokens;
    if let Some(n) = input {
        input = Some(match (messages, u.input_relation) {
            (false, InputTokenRelation::ExcludesCacheReadAndWrite) => {
                let method = if inclusive_input_from_total.is_some() {
                    UsageMethod::Residual
                } else if u.cached_input_tokens.is_none()
                    || u.input_cache_write_tokens.is_none()
                    || cache_read_method.estimated()
                    || cache_write_method.estimated()
                {
                    UsageMethod::MissingCache
                } else {
                    UsageMethod::CacheInclusive
                };
                if method.estimated() || !input_method.estimated() {
                    input_method = method;
                }
                match inclusive_input_from_total {
                    Some(input) => input,
                    None => add(
                        add(n, u.cached_input_tokens.unwrap_or(0))?,
                        u.input_cache_write_tokens.unwrap_or(0),
                    )?,
                }
            }
            (true, InputTokenRelation::IncludesCache) => {
                let method = if u.cached_input_tokens.is_none()
                    || u.input_cache_write_tokens.is_none()
                    || cache_read_method.estimated()
                    || cache_write_method.estimated()
                    || u.cached_input_tokens.is_some_and(|n| n > 0)
                        && u.input_cache_write_tokens.is_some_and(|n| n > 0)
                {
                    UsageMethod::CacheOverlap
                } else {
                    UsageMethod::CacheExclusive
                };
                if method.estimated() || !input_method.estimated() {
                    input_method = method;
                }
                // Overlap is unknown: estimate the union as its larger known subset.
                n.checked_sub(
                    u.cached_input_tokens
                        .unwrap_or(0)
                        .max(u.input_cache_write_tokens.unwrap_or(0)),
                )
                .ok_or(CodecError::Invalid(ShapeReason::Usage))?
            }
            _ => n,
        });
    }
    if !messages && input.is_none() {
        if inclusive_input_from_total.is_some() {
            input_method = UsageMethod::Residual;
        }
        input = inclusive_input_from_total;
    }
    let output = u
        .output_tokens
        .map(|n| {
            if u.output_relation == OutputTokenRelation::ExcludesReasoning {
                let method = if u.reasoning_tokens.is_some() {
                    UsageMethod::ReasoningInclusive
                } else {
                    UsageMethod::ReasoningBytes
                };
                if method.estimated() || !output_method.estimated() {
                    output_method = method;
                }
                add(n, u.reasoning_tokens.unwrap_or(reasoning_estimate))
            } else {
                Ok(n)
            }
        })
        .transpose()?;
    if input.is_none() {
        input_method = UsageMethod::InputBytes;
    }
    if output.is_none() {
        output_method = UsageMethod::OutputBytes;
    }
    let input = input.unwrap_or_else(|| {
        let floor = if messages && u.input_relation == InputTokenRelation::IncludesCache {
            input_floor.saturating_sub(
                u.cached_input_tokens
                    .unwrap_or(0)
                    .max(u.input_cache_write_tokens.unwrap_or(0)),
            )
        } else if !messages {
            input_floor
                .max(u.cached_input_tokens.unwrap_or(0))
                .max(u.input_cache_write_tokens.unwrap_or(0))
        } else {
            input_floor
        };
        input_estimate.max(floor)
    });
    let output = match output {
        Some(output) => output,
        None => output_estimate
            .max(output_floor)
            .max(u.reasoning_tokens.unwrap_or(0))
            .max(add(
                u.accepted_prediction_tokens.unwrap_or(0),
                u.rejected_prediction_tokens.unwrap_or(0),
            )?),
    };
    u.input_tokens = Some(input);
    u.output_tokens = Some(output);
    u.input_relation = if messages {
        InputTokenRelation::ExcludesCacheReadAndWrite
    } else {
        InputTokenRelation::IncludesCache
    };
    u.output_relation = OutputTokenRelation::IncludesReasoning;
    u.total_tokens = if messages {
        None
    } else {
        Some(add(input, output)?)
    };
    u.total_relation = if messages {
        TotalTokenRelation::Unreported
    } else {
        TotalTokenRelation::InputAndOutput
    };
    if protocol == ClientProtocol::Responses {
        if u.cached_input_tokens.is_none() {
            cache_read_method = UsageMethod::ZeroCache;
        }
        if u.input_cache_write_tokens.is_none() {
            cache_write_method = UsageMethod::ZeroCache;
        }
        if u.reasoning_tokens.is_none() {
            reasoning_method = UsageMethod::ReasoningBytes;
        }
        u.cached_input_tokens.get_or_insert(0);
        u.input_cache_write_tokens.get_or_insert(0);
        u.reasoning_tokens
            .get_or_insert(reasoning_estimate.min(output));
    }
    // Optional source-specific modality details are not scalar fallback authority.
    // Keep them only when their parent count has the same interpretation.
    if messages || source.is_some_and(|s| s.input_relation != InputTokenRelation::IncludesCache) {
        u.input_text_tokens = None;
        u.input_image_tokens = None;
        u.input_audio_tokens = None;
    }
    u.validate()?;
    delivery.usage = Some(u);
    use crate::protocol::conversion::generation::{
        Change, ConversionBoundary as B, Effect, EvidenceDelta, Field,
    };
    let mut evidence = EvidenceDelta::default();
    let total_method = if input_method.estimated() || output_method.estimated() {
        UsageMethod::CompletedTotal
    } else {
        UsageMethod::Accounting
    };
    for (field, before, after, method, relation_changed) in [
        (
            Field::InputTokens,
            source.and_then(|s| s.input_tokens),
            u.input_tokens,
            input_method,
            source.is_some_and(|s| s.input_relation != u.input_relation),
        ),
        (
            Field::OutputTokens,
            source.and_then(|s| s.output_tokens),
            u.output_tokens,
            output_method,
            source.is_some_and(|s| s.output_relation != u.output_relation),
        ),
        (
            Field::TotalTokens,
            source.and_then(|s| s.total_tokens),
            u.total_tokens,
            total_method,
            source.is_some_and(|s| s.total_relation != u.total_relation),
        ),
        (
            Field::CachedTokens,
            source.and_then(|s| s.cached_input_tokens),
            u.cached_input_tokens,
            cache_read_method,
            false,
        ),
        (
            Field::CacheWriteTokens,
            source.and_then(|s| s.input_cache_write_tokens),
            u.input_cache_write_tokens,
            cache_write_method,
            false,
        ),
        (
            Field::ReasoningTokens,
            source.and_then(|s| s.reasoning_tokens),
            u.reasoning_tokens,
            reasoning_method,
            false,
        ),
    ] {
        if before != after || relation_changed {
            let change = if after.is_none() {
                Change::Omit
            } else if method.estimated() {
                Change::Estimate
            } else {
                Change::Derive
            };
            evidence.push(Effect::new(
                method.rule(),
                field,
                B::ResponseProject,
                change,
            ));
        }
    }
    for (field, before, after) in [
        (
            Field::InputTextTokens,
            source.and_then(|s| s.input_text_tokens),
            u.input_text_tokens,
        ),
        (
            Field::InputImageTokens,
            source.and_then(|s| s.input_image_tokens),
            u.input_image_tokens,
        ),
        (
            Field::InputAudioTokens,
            source.and_then(|s| s.input_audio_tokens),
            u.input_audio_tokens,
        ),
    ] {
        if before.is_some() && after.is_none() {
            evidence.push(Effect::new(
                "generation.omit-incompatible-input-breakdown.v1",
                field,
                B::ResponseProject,
                Change::Omit,
            ));
        }
    }
    Ok(evidence)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_preserves_reports_derives_residuals_and_estimates_only_missing_fields() {
        let empty = GenerationResponse::new(vec![], Outcome::Completed).unwrap();
        for missing_input in [true, false] {
            let mut sparse = Usage::operation(7, 3, 10);
            if missing_input {
                sparse.input_tokens = None;
            } else {
                sparse.output_tokens = None;
            }
            sparse.cached_input_tokens = Some(0);
            sparse.input_cache_write_tokens = Some(0);
            sparse.reasoning_tokens = Some(0);
            let source = empty.clone().with_usage(Some(sparse)).unwrap();
            let mut delivery = GenerationDelivery::default();
            let evidence = complete(&mut delivery, &source, ClientProtocol::Messages, 100).unwrap();
            let usage = delivery.usage.unwrap();
            assert_eq!(
                (usage.input_tokens, usage.output_tokens),
                (Some(7), Some(3))
            );
            assert_eq!(source.usage(), Some(sparse));
            assert!(
                !evidence.iter().any(
                    |e| e.change() == crate::protocol::conversion::generation::Change::Estimate
                )
            );
        }
        let mut delivery = GenerationDelivery::default();
        complete(&mut delivery, &empty, ClientProtocol::Responses, 7).unwrap();
        let estimated = delivery.usage.unwrap();
        assert_eq!(
            (
                estimated.input_tokens,
                estimated.output_tokens,
                estimated.total_tokens
            ),
            (Some(7), Some(0), Some(7))
        );
        assert_eq!(
            (
                estimated.cached_input_tokens,
                estimated.input_cache_write_tokens,
                estimated.reasoning_tokens
            ),
            (Some(0), Some(0), Some(0))
        );
        assert!(empty.usage().is_none());

        let mut sparse = Usage::operation(0, 3, 10);
        sparse.input_tokens = None;
        sparse.cached_input_tokens = Some(2);
        let source = empty.clone().with_usage(Some(sparse)).unwrap();
        complete(&mut delivery, &source, ClientProtocol::Responses, 100).unwrap();
        assert_eq!(delivery.usage.unwrap().input_tokens, Some(7));
        assert_eq!(delivery.usage.unwrap().cached_input_tokens, Some(2));
        assert_eq!(source.usage(), Some(sparse));

        let zero = Usage::operation(0, 0, 0);
        let source = empty.clone().with_usage(Some(zero)).unwrap();
        complete(&mut delivery, &source, ClientProtocol::Chat, 100).unwrap();
        assert_eq!(delivery.usage, Some(zero));

        let mut disjoint = Usage::operation(11, 7, 0);
        disjoint.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
        disjoint.total_relation = TotalTokenRelation::Unreported;
        disjoint.total_tokens = None;
        disjoint.cached_input_tokens = Some(13);
        disjoint.input_cache_write_tokens = Some(17);
        let source = empty.with_usage(Some(disjoint)).unwrap();
        complete(&mut delivery, &source, ClientProtocol::Responses, 100).unwrap();
        assert_eq!(delivery.usage.unwrap().input_tokens, Some(41));
        assert_eq!(delivery.usage.unwrap().total_tokens, Some(48));
        complete(&mut delivery, &source, ClientProtocol::Messages, 100).unwrap();
        assert_eq!(delivery.usage.unwrap().input_tokens, Some(11));
        assert_eq!(source.usage(), Some(disjoint));
    }

    #[test]
    fn missing_parents_are_bounded_by_disjoint_modalities_without_rewriting_reports() {
        for complete_ttls in [false, true] {
            let mut report = UsageReport::operation(Usage::operation(100, 3, 103));
            report.details.cache_writes = vec![CacheWriteTokens {
                ttl: PrefixCacheTtl::FiveMinutes,
                tokens: 40,
            }];
            if complete_ttls {
                report.details.cache_writes.push(CacheWriteTokens {
                    ttl: PrefixCacheTtl::OneHour,
                    tokens: 30,
                });
            }
            let mut reports = UsageReports::default();
            reports.replace(report).unwrap();
            let source = GenerationResponse::new(vec![], Outcome::Completed)
                .unwrap()
                .with_usage_reports(reports)
                .unwrap();
            let mut delivery = GenerationDelivery::default();
            let evidence = complete(&mut delivery, &source, ClientProtocol::Responses, 1).unwrap();
            assert_eq!(
                delivery.usage.unwrap().input_cache_write_tokens,
                Some(if complete_ttls { 70 } else { 40 })
            );
            assert_eq!(source.usage().unwrap().input_cache_write_tokens, None);
            assert_eq!(
                evidence.iter().any(|e| e.semantic_field()
                    == crate::protocol::conversion::generation::Field::CacheWriteTokens
                    && e.change() == crate::protocol::conversion::generation::Change::Estimate),
                !complete_ttls
            );
        }
        let mut counts = Usage::operation(0, 0, 100);
        counts.input_tokens = None;
        counts.output_tokens = None;
        let mut report = UsageReport::operation(counts);
        report.details.modalities.push(ModalityTokens {
            bucket: UsageBucket::CacheRead,
            modality: Some(TokenModality::Text),
            tokens: Some(90),
            defaulted: false,
        });
        let mut reports = UsageReports::default();
        reports.replace(report).unwrap();
        let source = GenerationResponse::new(vec![], Outcome::Completed)
            .unwrap()
            .with_usage_reports(reports)
            .unwrap();
        let mut delivery = GenerationDelivery::default();
        complete(&mut delivery, &source, ClientProtocol::Responses, 1).unwrap();
        let usage = delivery.usage.unwrap();
        assert_eq!(
            (
                usage.input_tokens,
                usage.output_tokens,
                usage.cached_input_tokens
            ),
            (Some(90), Some(10), Some(90))
        );
        assert_eq!(source.usage().unwrap().cached_input_tokens, None);
        // A known total cannot override disjoint details, including details
        // which have no flat Usage field.
        for modalities in [
            vec![(TokenModality::Text, 5), (TokenModality::Image, 5)],
            vec![(TokenModality::Video, 10)],
        ] {
            let mut counts = Usage::operation(0, 3, 10);
            counts.input_tokens = None;
            let mut report = UsageReport::operation(counts);
            report.details.modalities = modalities
                .into_iter()
                .map(|(modality, count)| ModalityTokens {
                    bucket: UsageBucket::Input,
                    modality: Some(modality),
                    tokens: Some(count),
                    defaulted: false,
                })
                .collect();
            let mut reports = UsageReports::default();
            reports.replace(report).unwrap();
            let source = GenerationResponse::new(vec![], Outcome::Completed)
                .unwrap()
                .with_usage_reports(reports)
                .unwrap();
            for protocol in [
                ClientProtocol::Chat,
                ClientProtocol::Responses,
                ClientProtocol::Messages,
            ] {
                assert!(
                    complete(&mut GenerationDelivery::default(), &source, protocol, 1).is_err()
                );
            }
        }
        for (cache, reasoning, estimate, expected) in [
            (90, 0, 1, (90, 10)),
            (0, 90, 99, (10, 90)),
            (40, 60, 1, (40, 60)),
        ] {
            let mut counts = Usage::operation(0, 0, 100);
            counts.input_tokens = None;
            counts.output_tokens = None;
            counts.cached_input_tokens = Some(cache);
            counts.reasoning_tokens = Some(reasoning);
            let source = GenerationResponse::new(vec![], Outcome::Completed)
                .unwrap()
                .with_usage(Some(counts))
                .unwrap();
            for protocol in [ClientProtocol::Chat, ClientProtocol::Responses] {
                let mut delivery = GenerationDelivery::default();
                complete(&mut delivery, &source, protocol, estimate).unwrap();
                let usage = delivery.usage.unwrap();
                assert_eq!(
                    (usage.input_tokens, usage.output_tokens),
                    (Some(expected.0), Some(expected.1))
                );
                assert_eq!(usage.total_tokens, Some(100));
                assert_eq!(source.usage(), Some(counts));
            }
        }
        let mut counts = Usage::operation(0, 0, 0);
        counts.input_tokens = None;
        counts.output_tokens = None;
        counts.total_tokens = None;
        counts.total_relation = TotalTokenRelation::Unreported;
        counts.input_text_tokens = Some(8);
        counts.input_image_tokens = Some(7);
        counts.output_text_tokens = Some(6);
        counts.output_audio_tokens = Some(5);
        let source = GenerationResponse::new(vec![], Outcome::Completed)
            .unwrap()
            .with_usage(Some(counts))
            .unwrap();
        let original = source.clone();
        let mut delivery = GenerationDelivery::default();
        for protocol in [
            ClientProtocol::Chat,
            ClientProtocol::Responses,
            ClientProtocol::Messages,
        ] {
            complete(&mut delivery, &source, protocol, 1).unwrap();
            assert_eq!(delivery.usage.unwrap().input_tokens, Some(15));
            assert_eq!(delivery.usage.unwrap().output_tokens, Some(11));
            assert_eq!(source, original);
        }
    }

    #[test]
    fn split_accounting_uses_actual_buckets_and_attested_totals_before_estimates() {
        let response = |counts, accounting| {
            let mut reports = UsageReports::default();
            let mut report = UsageReport::operation(counts);
            report.accounting = accounting;
            reports.replace(report).unwrap();
            GenerationResponse::new(vec![], Outcome::Completed)
                .unwrap()
                .with_usage_reports(reports)
                .unwrap()
        };
        let mut delivery = GenerationDelivery::default();
        let mut counts = Usage::operation(10, 20, 100);
        counts.output_relation = OutputTokenRelation::ExcludesReasoning;
        counts.total_relation = TotalTokenRelation::Unreported;
        let exact = response(
            counts,
            UsageAccounting::SplitGeneration {
                tool_input_tokens: Some(0),
                tool_input_relation: ToolInputRelation::Additional,
                total_relation: SplitTotalRelation::InputToolOutputAndReasoning,
            },
        );
        let original = exact.clone();
        complete(&mut delivery, &exact, ClientProtocol::Responses, 7).unwrap();
        let usage = delivery.usage.unwrap();
        assert_eq!(
            (
                usage.input_tokens,
                usage.output_tokens,
                usage.total_tokens,
                usage.reasoning_tokens
            ),
            (Some(10), Some(90), Some(100), Some(70))
        );
        assert_eq!(exact, original);

        counts.input_tokens = None;
        counts.output_tokens = Some(0);
        counts.reasoning_tokens = Some(0);
        counts.total_tokens = None;
        let tool_only = response(
            counts,
            UsageAccounting::SplitGeneration {
                tool_input_tokens: Some(100),
                tool_input_relation: ToolInputRelation::Unreported,
                total_relation: SplitTotalRelation::Unreported,
            },
        );
        for protocol in [
            ClientProtocol::Chat,
            ClientProtocol::Responses,
            ClientProtocol::Messages,
        ] {
            complete(&mut delivery, &tool_only, protocol, 7).unwrap();
            assert_eq!(delivery.usage.unwrap().input_tokens, Some(100));
        }

        let mut counts = Usage::operation(10, 20, 100);
        counts.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
        counts.total_relation = TotalTokenRelation::InputCacheAndOutput;
        counts.cached_input_tokens = Some(30);
        let exact = response(counts, UsageAccounting::Direct);
        complete(&mut delivery, &exact, ClientProtocol::Chat, 7).unwrap();
        assert_eq!(delivery.usage.unwrap().input_tokens, Some(80));
        assert_eq!(delivery.usage.unwrap().input_cache_write_tokens, Some(40));
        assert_eq!(delivery.usage.unwrap().total_tokens, Some(100));
        let mut counts = Usage::operation(10, 20, 100);
        counts.output_relation = OutputTokenRelation::ExcludesReasoning;
        counts.total_relation = TotalTokenRelation::Unreported;
        let sparse = response(
            counts,
            UsageAccounting::SplitGeneration {
                tool_input_tokens: None,
                tool_input_relation: ToolInputRelation::Additional,
                total_relation: SplitTotalRelation::InputToolOutputAndReasoning,
            },
        );
        let original = sparse.clone();
        for protocol in [ClientProtocol::Responses, ClientProtocol::Chat] {
            let evidence = complete(&mut delivery, &sparse, protocol, 1).unwrap();
            let usage = delivery.usage.unwrap();
            assert_eq!(
                (
                    usage.input_tokens,
                    usage.output_tokens,
                    usage.total_tokens,
                    usage.reasoning_tokens
                ),
                (Some(10), Some(90), Some(100), Some(70))
            );
            assert!(evidence.iter().any(|e| e.semantic_field()
                == crate::protocol::conversion::generation::Field::ReasoningTokens
                && e.change() == crate::protocol::conversion::generation::Change::Estimate));
            assert_eq!(sparse, original);
        }
    }
}

#[cfg(test)]
mod provenance_tests {
    use super::*;
    use crate::protocol::conversion::generation::{Change, Field};
    #[test]
    fn unknown_optional_buckets_do_not_turn_reported_parent_counters_into_estimates() {
        let response = GenerationResponse::new(vec![], Outcome::Completed)
            .unwrap()
            .with_usage(Some(Usage::operation(10, 3, 13)))
            .unwrap();
        let mut delivery = GenerationDelivery::default();
        let effects = complete(&mut delivery, &response, ClientProtocol::Responses, 99).unwrap();
        for field in [Field::InputTokens, Field::OutputTokens, Field::TotalTokens] {
            assert!(
                !effects
                    .iter()
                    .any(|e| e.semantic_field() == field && e.change() == Change::Estimate)
            );
        }
        for field in [
            Field::CachedTokens,
            Field::CacheWriteTokens,
            Field::ReasoningTokens,
        ] {
            assert!(
                effects
                    .iter()
                    .any(|e| e.semantic_field() == field && e.change() == Change::Estimate)
            );
        }
        assert_eq!(response.usage().unwrap().cached_input_tokens, None);
        assert_eq!(delivery.usage.unwrap().total_tokens, Some(13));
    }
    #[test]
    fn known_total_residual_and_unknown_zero_have_different_provenance() {
        let mut usage = Usage::operation(10, 3, 13);
        usage.output_tokens = None;
        usage.cached_input_tokens = Some(0);
        usage.input_cache_write_tokens = Some(0);
        usage.reasoning_tokens = Some(0);
        let response = GenerationResponse::new(vec![], Outcome::Completed)
            .unwrap()
            .with_usage(Some(usage))
            .unwrap();
        let mut delivery = GenerationDelivery::default();
        let effects = complete(&mut delivery, &response, ClientProtocol::Responses, 999).unwrap();
        assert!(
            !effects
                .iter()
                .any(|e| e.semantic_field() == Field::OutputTokens && e.change() == Change::Derive)
        );
        assert!(!effects.iter().any(|e| e.change() == Change::Estimate));
        assert_eq!(delivery.usage.unwrap().output_tokens, Some(3));
        let empty = GenerationResponse::new(vec![], Outcome::Completed).unwrap();
        let effects = complete(
            &mut GenerationDelivery::default(),
            &empty,
            ClientProtocol::Responses,
            0,
        )
        .unwrap();
        assert!(
            effects.iter().any(
                |e| e.semantic_field() == Field::OutputTokens && e.change() == Change::Estimate
            )
        );
    }
}
