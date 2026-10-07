//! Named, premise-checked views over one reported usage snapshot, never billing.
use super::{GenerationError, InputTokenRelation, OutputTokenRelation, Usage};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageFormula {
    /// Input total less the reported cache-read subset; cache-write may overlap.
    InputMinusCacheRead,
    /// Sum only explicitly disjoint reported input/cache categories.
    InputPlusCacheReadAndWrite,
    /// Output total less reported reasoning; the remainder is not visible text.
    OutputMinusReasoning,
}
/// No independently mutable count is stored. This view does not assert finality
/// of a snapshot and must never be summed across cumulative stream reports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DerivedTokenCount<'a> {
    source: &'a Usage,
    formula: UsageFormula,
}
impl<'a> DerivedTokenCount<'a> {
    pub const fn source(&self) -> &'a Usage {
        self.source
    }
    pub const fn formula(&self) -> UsageFormula {
        self.formula
    }
    pub fn tokens(&self) -> u64 {
        match self.formula {
            UsageFormula::InputPlusCacheReadAndWrite => {
                self.source.input_tokens.expect("validated premise")
                    + self.source.cached_input_tokens.expect("validated premise")
                    + self
                        .source
                        .input_cache_write_tokens
                        .expect("validated premise")
            }
            UsageFormula::InputMinusCacheRead => {
                self.source
                    .input_tokens
                    .expect("validated reported premise")
                    - self
                        .source
                        .cached_input_tokens
                        .expect("validated reported premise")
            }
            UsageFormula::OutputMinusReasoning => {
                self.source
                    .output_tokens
                    .expect("validated reported premise")
                    - self
                        .source
                        .reasoning_tokens
                        .expect("validated reported premise")
            }
        }
    }
}
impl Usage {
    /// Missing detail is unknown, including when other overlapping details are
    /// present. Zero is reported; malformed/overflowing totals remain errors.
    pub fn derive(
        &self,
        formula: UsageFormula,
    ) -> Result<Option<DerivedTokenCount<'_>>, GenerationError> {
        self.validate()?;
        let premise = match formula {
            UsageFormula::InputMinusCacheRead
                if self.input_relation == InputTokenRelation::IncludesCache =>
            {
                self.input_tokens.zip(self.cached_input_tokens)
            }
            UsageFormula::InputPlusCacheReadAndWrite
                if self.input_relation == InputTokenRelation::ExcludesCacheReadAndWrite =>
            {
                match (
                    self.input_tokens,
                    self.cached_input_tokens,
                    self.input_cache_write_tokens,
                ) {
                    (Some(input), Some(read), Some(write)) => {
                        input
                            .checked_add(read)
                            .and_then(|sum| sum.checked_add(write))
                            .ok_or(GenerationError::InvalidResponse)?;
                        Some((input, read))
                    }
                    _ => None,
                }
            }
            UsageFormula::InputMinusCacheRead | UsageFormula::InputPlusCacheReadAndWrite => None,
            UsageFormula::OutputMinusReasoning
                if self.output_relation == OutputTokenRelation::IncludesReasoning =>
            {
                self.output_tokens.zip(self.reasoning_tokens)
            }
            UsageFormula::OutputMinusReasoning => None,
        };
        Ok(premise.map(|_| DerivedTokenCount {
            source: self,
            formula,
        }))
    }
}
