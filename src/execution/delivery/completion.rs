//! Request-local presentation fallbacks. Never mutate upstream observations.
use crate::{
    adapter::ClientProtocol,
    protocol::{CodecError, GenerationDelivery},
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
            .expect("one missing counter") = Some(
            total
                .checked_sub(known)
                .ok_or(CodecError::Invalid("delivery total"))?,
        );
    }
    Ok(())
}
pub(super) fn input_estimate(request: &GenerationRequest) -> u64 {
    let s = request.settings();
    let mut bytes: usize = request.items().iter().map(|(_, i)| item_bytes(i)).sum();
    bytes += s.instructions.value().map_or(0, |t| t.as_str().len());
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
pub(super) fn complete(
    delivery: &mut GenerationDelivery,
    response: &GenerationResponse,
    protocol: ClientProtocol,
    input_estimate: u64,
) -> Result<Option<&'static str>, CodecError> {
    let source = response.usage();
    let mut estimated = source.is_none();
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
    match u.total_relation {
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
    let inclusive_input_from_total = if u.total_relation == TotalTokenRelation::InputCacheAndOutput
    {
        u.total_tokens
            .zip(u.output_tokens)
            .map(|(t, o)| {
                t.checked_sub(o)
                    .ok_or(CodecError::Invalid("delivery total"))
            })
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
            total_relation,
        } = report.accounting
    {
        let tool_floor = tool_input_tokens
            .unwrap_or(0)
            .max(modality_floor(UsageBucket::ToolInput)?);
        input_floor = if tool_input_relation == ToolInputRelation::Additional {
            input_floor
                .checked_add(tool_floor)
                .ok_or(CodecError::Limit)?
        } else {
            input_floor.max(tool_floor)
        };
        // Residual inference precedes recovery/estimation of exclusive buckets.
        // The attested total already includes tool input and reasoning.
        if total_relation == SplitTotalRelation::InputToolOutputAndReasoning
            && let Some(total) = report.counts.total_tokens
        {
            if let (Some(input), None) = (u.input_tokens, u.output_tokens) {
                let output = total
                    .checked_sub(input)
                    .ok_or(CodecError::Invalid("delivery total"))?;
                u.output_tokens = Some(output);
                if u.reasoning_tokens.is_none()
                    && let Some(visible) = report.counts.output_tokens
                {
                    u.reasoning_tokens = Some(
                        output
                            .checked_sub(visible)
                            .ok_or(CodecError::Invalid("delivery reasoning"))?,
                    );
                }
            } else if let (None, Some(output)) = (u.input_tokens, u.output_tokens) {
                u.input_tokens = Some(
                    total
                        .checked_sub(output)
                        .ok_or(CodecError::Invalid("delivery total"))?,
                );
            }
        }
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
            estimated = true;
            let tool = tool_input_tokens.unwrap_or(modality_floor(UsageBucket::ToolInput)?);
            u.input_tokens = Some(if tool_input_relation == ToolInputRelation::Additional {
                input.checked_add(tool).ok_or(CodecError::Limit)?
            } else {
                input.max(tool)
            });
        }
        if u.output_tokens.is_none() && report.counts.output_tokens.is_some() {
            estimated |= report.counts.reasoning_tokens.is_none();
            u.output_tokens = report.counts.output_tokens;
            u.output_relation = OutputTokenRelation::ExcludesReasoning;
            u.total_relation = TotalTokenRelation::Unreported;
        }
    }
    u.validate()
        .or_else(|e| if source.is_none() { Ok(()) } else { Err(e) })?;
    let messages = protocol == ClientProtocol::Messages;
    estimated |= u.input_relation == InputTokenRelation::Unreported
        || u.output_relation == OutputTokenRelation::Unreported
        || u.output_relation == OutputTokenRelation::ExcludesReasoning
            && u.reasoning_tokens.is_none()
        || !messages
            && u.input_relation == InputTokenRelation::ExcludesCacheReadAndWrite
            && (u.cached_input_tokens.is_none() || u.input_cache_write_tokens.is_none())
        || messages
            && u.input_relation == InputTokenRelation::IncludesCache
            && (u.cached_input_tokens.is_none()
                || u.cached_input_tokens.is_some_and(|n| n > 0)
                    && u.input_cache_write_tokens.is_some_and(|n| n > 0));
    let add = |a: u64, b: u64| a.checked_add(b).ok_or(CodecError::Limit);
    let mut input = u.input_tokens;
    if let Some(n) = input {
        input = Some(match (messages, u.input_relation) {
            (false, InputTokenRelation::ExcludesCacheReadAndWrite) => {
                match inclusive_input_from_total {
                    Some(input) => input,
                    None => add(
                        add(n, u.cached_input_tokens.unwrap_or(0))?,
                        u.input_cache_write_tokens.unwrap_or(0),
                    )?,
                }
            }
            (true, InputTokenRelation::IncludesCache) => {
                // Overlap is unknown: estimate the union as its larger known subset.
                n.checked_sub(
                    u.cached_input_tokens
                        .unwrap_or(0)
                        .max(u.input_cache_write_tokens.unwrap_or(0)),
                )
                .ok_or(CodecError::Invalid("delivery cache usage"))?
            }
            _ => n,
        });
    }
    if !messages && input.is_none() {
        input = inclusive_input_from_total;
    }
    let output = u
        .output_tokens
        .map(|n| {
            if u.output_relation == OutputTokenRelation::ExcludesReasoning {
                add(n, u.reasoning_tokens.unwrap_or(reasoning_estimate))
            } else {
                Ok(n)
            }
        })
        .transpose()?;
    let total = (!messages
        && u.input_relation == InputTokenRelation::IncludesCache
        && u.output_relation == OutputTokenRelation::IncludesReasoning
        && u.total_relation == TotalTokenRelation::InputAndOutput)
        .then_some(u.total_tokens)
        .flatten();
    estimated |= total.is_none() && (input.is_none() || output.is_none())
        || input.is_none() && output.is_none() && total.is_some_and(|n| n > 0);
    let (input, output) = match (input, output, total) {
        (Some(i), Some(o), _) => (i, o),
        (Some(i), None, Some(t)) => (
            i,
            t.checked_sub(i)
                .ok_or(CodecError::Invalid("delivery total"))?,
        ),
        (None, Some(o), Some(t)) => (
            t.checked_sub(o)
                .ok_or(CodecError::Invalid("delivery total"))?,
            o,
        ),
        (None, None, Some(t)) => (input_estimate.min(t), t - input_estimate.min(t)),
        (i, o, None) => (
            i.unwrap_or(input_estimate.max(input_floor)),
            o.unwrap_or(output_estimate),
        ),
    };
    let input = if u.input_tokens.is_none() && total.is_none() && !messages {
        input
            .max(u.cached_input_tokens.unwrap_or(0))
            .max(u.input_cache_write_tokens.unwrap_or(0))
            .max(input_floor)
    } else {
        input
    };
    let output = if u.output_tokens.is_none() && total.is_none() {
        output
            .max(u.reasoning_tokens.unwrap_or(0))
            .max(output_floor)
            .max(add(
                u.accepted_prediction_tokens.unwrap_or(0),
                u.rejected_prediction_tokens.unwrap_or(0),
            )?)
    } else {
        output
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
        estimated |= u.cached_input_tokens.is_none()
            || u.input_cache_write_tokens.is_none()
            || u.reasoning_tokens.is_none();
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
    Ok((source != Some(u)).then_some(if estimated { "estimate" } else { "derive" }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_preserves_reports_derives_residuals_and_estimates_only_missing_fields() {
        let empty = GenerationResponse::new(vec![], Outcome::Completed).unwrap();
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
    }
}
