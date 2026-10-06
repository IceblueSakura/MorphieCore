//! Scoped token reports. Missing counts and undeclared relationships stay unknown.
use super::{GenerationError, ItemId, MAX_ITEMS};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageScope {
    Operation,
    Item(ItemId),
    Session,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageBasis {
    Delta,
    Cumulative,
    Final,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputTokenRelation {
    IncludesReasoning,
    ExcludesReasoning,
    Unreported,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TotalTokenRelation {
    InputAndOutput,
    InputOutputAndReasoning,
    Unreported,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Usage {
    pub scope: UsageScope,
    pub basis: UsageBasis,
    pub output_relation: OutputTokenRelation,
    pub total_relation: TotalTokenRelation,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub input_cache_write_tokens: Option<u64>,
    pub input_text_tokens: Option<u64>,
    pub input_image_tokens: Option<u64>,
    pub input_audio_tokens: Option<u64>,
    pub output_audio_tokens: Option<u64>,
    pub output_text_tokens: Option<u64>,
    pub accepted_prediction_tokens: Option<u64>,
    pub rejected_prediction_tokens: Option<u64>,
}
impl Usage {
    /// Explicit inclusive final-operation accounting, not a default for other sources.
    pub fn operation(input: u64, output: u64, total: u64) -> Self {
        Self {
            scope: UsageScope::Operation,
            basis: UsageBasis::Final,
            output_relation: OutputTokenRelation::IncludesReasoning,
            total_relation: TotalTokenRelation::InputAndOutput,
            input_tokens: Some(input),
            output_tokens: Some(output),
            total_tokens: Some(total),
            reasoning_tokens: None,
            cached_input_tokens: None,
            input_cache_write_tokens: None,
            input_text_tokens: None,
            input_image_tokens: None,
            input_audio_tokens: None,
            output_audio_tokens: None,
            output_text_tokens: None,
            accepted_prediction_tokens: None,
            rejected_prediction_tokens: None,
        }
    }
    pub fn validate(self) -> Result<(), GenerationError> {
        let subset = |detail: Option<u64>, total: Option<u64>| !matches!((detail,total),(Some(a),Some(b)) if a>b);
        if self.counters().iter().all(Option::is_none)
            || self.output_relation == OutputTokenRelation::IncludesReasoning
                && !subset(self.reasoning_tokens, self.output_tokens)
            || !subset(self.cached_input_tokens, self.input_tokens)
            || !subset(self.input_cache_write_tokens, self.input_tokens)
            || !subset(self.input_text_tokens, self.input_tokens)
            || !subset(self.input_image_tokens, self.input_tokens)
            || !subset(self.input_audio_tokens, self.input_tokens)
            || !subset(self.output_audio_tokens, self.output_tokens)
            || !subset(self.output_text_tokens, self.output_tokens)
            || !subset(self.accepted_prediction_tokens, self.output_tokens)
            || !subset(self.rejected_prediction_tokens, self.output_tokens)
        {
            return Err(GenerationError::InvalidResponse);
        }
        if let (Some(a), Some(b)) = (
            self.accepted_prediction_tokens,
            self.rejected_prediction_tokens,
        ) {
            let sum = a.checked_add(b).ok_or(GenerationError::InvalidResponse)?;
            if self.output_tokens.is_some_and(|total| sum > total) {
                return Err(GenerationError::InvalidResponse);
            }
        }
        if self.total_relation == TotalTokenRelation::InputOutputAndReasoning
            && self.output_relation != OutputTokenRelation::ExcludesReasoning
        {
            return Err(GenerationError::InvalidResponse);
        }
        let operands = match self.total_relation {
            TotalTokenRelation::InputAndOutput => Some(vec![self.input_tokens, self.output_tokens]),
            TotalTokenRelation::InputOutputAndReasoning => Some(vec![
                self.input_tokens,
                self.output_tokens,
                self.reasoning_tokens,
            ]),
            TotalTokenRelation::Unreported => None,
        };
        if let (Some(operands), Some(total)) = (operands, self.total_tokens) {
            // Known operands give a lower bound, not a replacement for missing reports.
            let known = operands
                .iter()
                .flatten()
                .try_fold(0u64, |sum, value| sum.checked_add(*value))
                .ok_or(GenerationError::InvalidResponse)?;
            if known > total || operands.iter().all(Option::is_some) && known != total {
                return Err(GenerationError::InvalidResponse);
            }
        }
        Ok(())
    }
    pub(crate) fn counters(self) -> [Option<u64>; 13] {
        [
            self.input_tokens,
            self.output_tokens,
            self.total_tokens,
            self.reasoning_tokens,
            self.cached_input_tokens,
            self.input_cache_write_tokens,
            self.input_text_tokens,
            self.input_image_tokens,
            self.input_audio_tokens,
            self.output_audio_tokens,
            self.output_text_tokens,
            self.accepted_prediction_tokens,
            self.rejected_prediction_tokens,
        ]
    }
    pub(crate) fn update(reports: &mut Vec<Self>, next: Self) -> Result<(), GenerationError> {
        next.validate()?;
        if let Some(index) = reports.iter().rposition(|old| old.scope == next.scope) {
            let old = reports[index];
            if old.basis == UsageBasis::Final
                || old.output_relation != next.output_relation
                || old.total_relation != next.total_relation
            {
                return Err(GenerationError::InvalidResponse);
            }
            match (old.basis, next.basis) {
                (UsageBasis::Cumulative, UsageBasis::Cumulative | UsageBasis::Final) => {
                    if old.counters().into_iter().zip(next.counters()).any(|(a,b)|matches!((a,b),(Some(_),None)|(Some(_),Some(_)) if b.is_none() || a>b)) {
                        return Err(GenerationError::InvalidResponse);
                    }
                    reports[index] = next;
                    return Ok(());
                }
                (UsageBasis::Delta, UsageBasis::Delta | UsageBasis::Final) => {
                    let mut lower = [0u64; 13];
                    for report in reports.iter().filter(|report| {
                        report.scope == next.scope && report.basis == UsageBasis::Delta
                    }) {
                        for (bound, value) in lower.iter_mut().zip(report.counters()) {
                            if let Some(value) = value {
                                *bound = bound
                                    .checked_add(value)
                                    .ok_or(GenerationError::InvalidResponse)?;
                            }
                        }
                    }
                    if next.basis == UsageBasis::Final
                        && lower
                            .into_iter()
                            .zip(next.counters())
                            .any(|(bound, value)| value.is_some_and(|value| value < bound))
                    {
                        return Err(GenerationError::InvalidResponse);
                    }
                }
                _ => return Err(GenerationError::InvalidResponse),
            }
        }
        if reports.len() >= MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        reports.push(next);
        Ok(())
    }
    pub(crate) fn check_owners(
        reports: &[Self],
        items: &[(ItemId, super::Item)],
    ) -> Result<(), GenerationError> {
        if reports.iter().any(|report|matches!(report.scope,UsageScope::Item(id) if !items.iter().any(|(owner,_)|*owner==id))) {
            return Err(GenerationError::InvalidResponse);
        }
        Ok(())
    }
    pub(crate) fn final_operation(reports: &[Self]) -> Option<Self> {
        reports
            .iter()
            .find(|report| {
                report.scope == UsageScope::Operation && report.basis == UsageBasis::Final
            })
            .copied()
    }
}
