//! Source-owned accounting snapshots. Scopes never sum into one another.
use super::*;
use crate::semantic::task::generation::{MAX_ITEMS, MAX_TOTAL_BYTES, PrefixCacheTtl};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum UsageScope {
    Operation,
    /// Response-local execution step identity, not an item or stream part index.
    Step(u64),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageStage {
    Checkpoint,
    /// Last observed snapshot for this scope; not a claim of exhaustive billing.
    Final,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolInputRelation {
    Included,
    Additional,
    Unreported,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SplitTotalRelation {
    InputToolOutputAndReasoning,
    Unreported,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageAccounting {
    Direct,
    /// Counters retain the base input and non-reasoning output as reported.
    SplitGeneration {
        tool_input_tokens: Option<u64>,
        tool_input_relation: ToolInputRelation,
        total_relation: SplitTotalRelation,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum UsageBucket {
    Input,
    ToolInput,
    Output,
    CacheRead,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum TokenModality {
    Unspecified,
    Text,
    Image,
    Audio,
    Video,
    Document,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModalityTokens {
    pub bucket: UsageBucket,
    pub modality: Option<TokenModality>,
    pub tokens: Option<u64>,
    pub defaulted: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheWriteTokens {
    pub ttl: PrefixCacheTtl,
    pub tokens: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ToolUsageKind {
    Unreported,
    WebSearch,
    WebFetch,
    MapSearch,
    Retrieval,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolUseCount {
    pub kind: ToolUsageKind,
    pub count: Option<u64>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum UsageCounter {
    Input,
    ToolInput,
    Output,
    Reasoning,
    CacheRead,
    Total,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UsageDetails {
    /// Sparse breakdowns; absent categories are not reported zeros.
    pub modalities: Vec<ModalityTokens>,
    pub cache_writes: Vec<CacheWriteTokens>,
    pub tool_requests: Vec<ToolUseCount>,
    /// Zero from a source-defined scalar default, not an explicit wire report.
    pub defaulted: Vec<UsageCounter>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageReport {
    pub scope: UsageScope,
    pub stage: UsageStage,
    pub counts: Usage,
    pub accounting: UsageAccounting,
    pub details: UsageDetails,
}
impl UsageReport {
    pub fn operation(counts: Usage) -> Self {
        Self {
            scope: UsageScope::Operation,
            stage: UsageStage::Final,
            counts,
            accounting: UsageAccounting::Direct,
            details: UsageDetails::default(),
        }
    }
    pub fn tool_input_tokens(&self) -> Option<u64> {
        match self.accounting {
            UsageAccounting::Direct => None,
            UsageAccounting::SplitGeneration {
                tool_input_tokens, ..
            } => tool_input_tokens,
        }
    }
    fn parent(&self, bucket: UsageBucket) -> Option<u64> {
        match bucket {
            UsageBucket::Input => self.counts.input_tokens,
            UsageBucket::ToolInput => self.tool_input_tokens(),
            UsageBucket::Output => self.counts.output_tokens,
            UsageBucket::CacheRead => self.counts.cached_input_tokens,
        }
    }
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + std::mem::size_of_val(self.details.modalities.as_slice())
            + std::mem::size_of_val(self.details.cache_writes.as_slice())
            + std::mem::size_of_val(self.details.tool_requests.as_slice())
            + std::mem::size_of_val(self.details.defaulted.as_slice())
    }
    pub fn validate(&self) -> Result<(), GenerationError> {
        if self.counts.counters().iter().any(Option::is_some) {
            self.counts.validate()?;
        } else if self.tool_input_tokens().is_none() && self.details == UsageDetails::default() {
            return Err(GenerationError::InvalidResponse);
        }
        let invalid = || GenerationError::InvalidResponse;
        if self.details.modalities.len() > 28
            || self.details.cache_writes.len() > 2
            || self.details.tool_requests.len() > 5
            || self.details.defaulted.len() > 6
        {
            return Err(GenerationError::Limit);
        }
        if let UsageAccounting::SplitGeneration {
            tool_input_tokens,
            tool_input_relation,
            total_relation,
        } = self.accounting
        {
            if self.counts.output_relation != OutputTokenRelation::ExcludesReasoning
                || self.counts.input_relation != InputTokenRelation::IncludesCache
                || self.counts.total_relation != TotalTokenRelation::Unreported
                || tool_input_relation == ToolInputRelation::Included
                    && matches!((tool_input_tokens, self.counts.input_tokens), (Some(a), Some(b)) if a > b)
            {
                return Err(invalid());
            }
            for (a, b) in [
                self.counts.output_tokens.zip(self.counts.reasoning_tokens),
                (tool_input_relation == ToolInputRelation::Additional)
                    .then_some(())
                    .and(self.counts.input_tokens.zip(tool_input_tokens)),
            ]
            .into_iter()
            .flatten()
            {
                a.checked_add(b).ok_or_else(invalid)?;
            }
            if total_relation == SplitTotalRelation::InputToolOutputAndReasoning {
                if tool_input_relation != ToolInputRelation::Additional {
                    return Err(invalid());
                }
                let operands = [
                    self.counts.input_tokens,
                    tool_input_tokens,
                    self.counts.output_tokens,
                    self.counts.reasoning_tokens,
                ];
                let known = operands
                    .iter()
                    .flatten()
                    .try_fold(0u64, |n, v| n.checked_add(*v))
                    .ok_or_else(invalid)?;
                if self.counts.total_tokens.is_some_and(|t| {
                    known > t || operands.iter().all(Option::is_some) && known != t
                }) {
                    return Err(invalid());
                }
            }
        }
        let mut seen = BTreeSet::new();
        for part in &self.details.modalities {
            if !seen.insert((part.bucket, part.modality))
                || part.defaulted && part.tokens != Some(0)
            {
                return Err(invalid());
            }
        }
        for bucket in [
            UsageBucket::Input,
            UsageBucket::ToolInput,
            UsageBucket::Output,
            UsageBucket::CacheRead,
        ] {
            let sum = self
                .details
                .modalities
                .iter()
                .filter(|p| p.bucket == bucket)
                .filter_map(|p| p.tokens)
                .try_fold(0u64, |n, tokens| n.checked_add(tokens))
                .ok_or_else(invalid)?;
            if self.parent(bucket).is_some_and(|parent| sum > parent) {
                return Err(invalid());
            }
        }
        if let UsageAccounting::SplitGeneration {
            tool_input_relation,
            ..
        } = self.accounting
        {
            // Missing parent totals do not erase relationships between two known
            // counts of the same reported modality. Unknown modality is not an identity.
            for input in self
                .details
                .modalities
                .iter()
                .filter(|p| p.bucket == UsageBucket::Input && p.modality.is_some())
            {
                let tool =
                    self.details.modalities.iter().find(|p| {
                        p.bucket == UsageBucket::ToolInput && p.modality == input.modality
                    });
                if let Some((input, tool)) = input.tokens.zip(tool.and_then(|p| p.tokens)) {
                    match tool_input_relation {
                        ToolInputRelation::Additional => {
                            input.checked_add(tool).ok_or_else(invalid)?;
                        }
                        ToolInputRelation::Included if tool > input => return Err(invalid()),
                        _ => {}
                    }
                }
            }
        }
        if self.details.cache_writes.len() == 2
            && self.details.cache_writes[0].ttl == self.details.cache_writes[1].ttl
        {
            return Err(invalid());
        }
        let writes = self
            .details
            .cache_writes
            .iter()
            .try_fold(0u64, |n, p| n.checked_add(p.tokens))
            .ok_or_else(invalid)?;
        if self.counts.input_cache_write_tokens.is_some_and(|total| {
            writes > total || self.details.cache_writes.len() == 2 && writes != total
        }) {
            return Err(invalid());
        }
        let mut tools = BTreeSet::new();
        if self
            .details
            .tool_requests
            .iter()
            .any(|p| !tools.insert(p.kind))
        {
            return Err(invalid());
        }
        let mut defaults = BTreeSet::new();
        for counter in &self.details.defaulted {
            let value = match counter {
                UsageCounter::Input => self.counts.input_tokens,
                UsageCounter::ToolInput => self.tool_input_tokens(),
                UsageCounter::Output => self.counts.output_tokens,
                UsageCounter::Reasoning => self.counts.reasoning_tokens,
                UsageCounter::CacheRead => self.counts.cached_input_tokens,
                UsageCounter::Total => self.counts.total_tokens,
            };
            if !defaults.insert(counter) || value != Some(0) {
                return Err(invalid());
            }
        }
        Ok(())
    }
    /// A derived operation view; original counters and their inclusion evidence stay here.
    pub fn summary(&self) -> Usage {
        let UsageAccounting::SplitGeneration {
            tool_input_tokens,
            tool_input_relation,
            total_relation,
        } = self.accounting
        else {
            return self.counts;
        };
        let mut summary = self.counts;
        summary.input_tokens = match tool_input_relation {
            ToolInputRelation::Included => self.counts.input_tokens,
            ToolInputRelation::Additional => self
                .counts
                .input_tokens
                .zip(tool_input_tokens)
                .and_then(|(a, b)| a.checked_add(b)),
            ToolInputRelation::Unreported if tool_input_tokens == Some(0) => {
                self.counts.input_tokens
            }
            ToolInputRelation::Unreported => None,
        };
        summary.output_tokens = self
            .counts
            .output_tokens
            .zip(self.counts.reasoning_tokens)
            .and_then(|(a, b)| a.checked_add(b));
        summary.output_relation = OutputTokenRelation::IncludesReasoning;
        summary.total_relation = match total_relation {
            SplitTotalRelation::InputToolOutputAndReasoning => TotalTokenRelation::InputAndOutput,
            SplitTotalRelation::Unreported => TotalTokenRelation::Unreported,
        };
        let modality = |bucket, kind| {
            self.details
                .modalities
                .iter()
                .find(|part| part.bucket == bucket && part.modality == Some(kind))
                .and_then(|part| part.tokens)
        };
        let input = |kind| match tool_input_relation {
            ToolInputRelation::Included => modality(UsageBucket::Input, kind),
            _ if tool_input_tokens == Some(0) => modality(UsageBucket::Input, kind),
            ToolInputRelation::Additional => modality(UsageBucket::Input, kind)
                .zip(modality(UsageBucket::ToolInput, kind))
                .and_then(|(a, b)| a.checked_add(b)),
            ToolInputRelation::Unreported => None,
        };
        summary.input_text_tokens = input(TokenModality::Text);
        summary.input_image_tokens = input(TokenModality::Image);
        summary.input_audio_tokens = input(TokenModality::Audio);
        summary.output_text_tokens = modality(UsageBucket::Output, TokenModality::Text);
        summary.output_audio_tokens = modality(UsageBucket::Output, TokenModality::Audio);
        summary
    }
    pub fn scalar_summary(&self) -> Option<Usage> {
        let summary = self.summary();
        summary
            .counters()
            .iter()
            .any(Option::is_some)
            .then_some(summary)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UsageReports {
    reports: BTreeMap<UsageScope, UsageReport>,
    bytes: usize,
}
impl UsageReports {
    pub fn reports(&self) -> impl ExactSizeIterator<Item = &UsageReport> {
        self.reports.values()
    }
    pub fn operation(&self) -> Option<&UsageReport> {
        self.reports.get(&UsageScope::Operation)
    }
    pub fn summary(&self) -> Option<Usage> {
        self.operation()?.scalar_summary()
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    pub fn has_details(&self) -> bool {
        self.reports.values().any(|r| {
            r.scope != UsageScope::Operation
                || r.accounting != UsageAccounting::Direct
                || r.details != UsageDetails::default()
        })
    }
    pub fn replace(&mut self, report: UsageReport) -> Result<(), GenerationError> {
        report.validate()?;
        let old_bytes = if let Some(previous) = self.reports.get(&report.scope) {
            if previous.stage == UsageStage::Final {
                return Err(GenerationError::InvalidResponse);
            }
            previous.bytes() + 64
        } else {
            if self.reports.len() >= MAX_ITEMS {
                return Err(GenerationError::Limit);
            }
            0
        };
        let bytes = self.bytes - old_bytes + report.bytes() + 64;
        if bytes > MAX_TOTAL_BYTES {
            return Err(GenerationError::Limit);
        }
        self.reports.insert(report.scope, report);
        self.bytes = bytes;
        Ok(())
    }
    pub fn from_usage(usage: Option<Usage>) -> Result<Self, GenerationError> {
        let mut reports = Self::default();
        if let Some(usage) = usage {
            reports.replace(UsageReport::operation(usage))?;
        }
        Ok(reports)
    }
}
