//! Completed or partial Generation output, independent of transport success.
use crate::semantic::task::generation::{
    GenerationError, Item, ItemId, ItemLifecycle, MAX_TEXT_BYTES, RefusalDetails, Usage,
    UsageReports,
};
use crate::semantic::value::{Presence, Text};

/// Result of one response, not completion of the logical turn or tool execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Completed,
    Incomplete,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IncompleteReason {
    Unspecified,
    MaxOutputTokens,
    /// The model context window, not the requested output ceiling, was exhausted.
    ContextWindowExceeded,
    ContentFilter,
    /// Output stopped by a recitation check, distinct from safety filtering.
    Recitation,
    Other(Text),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseError {
    pub code: Option<Text>,
    pub message: Text,
    pub param: Option<Text>,
    /// Original upstream error classification, not an event discriminator.
    pub original_type: Option<Text>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TerminalDetails {
    pub error: Option<ResponseError>,
    pub incomplete: Option<IncompleteReason>,
    pub completion: Option<CompletionCause>,
    pub refusal: Presence<RefusalDetails>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompletionCause {
    Natural,
    StopSequence(Text),
    /// The source reports normal termination without distinguishing a matched sequence.
    NaturalOrStopSequence,
}
impl TerminalDetails {
    pub fn bytes(&self) -> usize {
        let error = self.error.as_ref().map_or(0, |e| {
            e.code.as_ref().map_or(0, |c| c.as_str().len())
                + e.message.as_str().len()
                + e.param.as_ref().map_or(0, |p| p.as_str().len())
                + e.original_type.as_ref().map_or(0, |p| p.as_str().len())
        });
        self.refusal.value().map_or(0, RefusalDetails::bytes)
            + error
            + match &self.completion {
                Some(CompletionCause::StopSequence(text)) => text.as_str().len(),
                Some(_) => 1,
                None => 0,
            }
            + match &self.incomplete {
                Some(IncompleteReason::Other(t)) => t.as_str().len(),
                Some(_) => 32,
                None => 0,
            }
    }
    pub fn validate(&self, outcome: Outcome) -> Result<(), GenerationError> {
        if let Some(report) = self.refusal.value() {
            report.validate()?;
            if outcome != Outcome::Incomplete
                || self.incomplete != Some(IncompleteReason::ContentFilter)
            {
                return Err(GenerationError::InvalidResponse);
            }
        }
        if self.error.is_some() && outcome != Outcome::Failed
            || self.incomplete.is_some() && outcome != Outcome::Incomplete
            || self.completion.is_some() && outcome != Outcome::Completed
        {
            return Err(GenerationError::InvalidResponse);
        }
        if let Some(CompletionCause::StopSequence(text)) = &self.completion
            && (text.as_str().is_empty() || text.as_str().len() > MAX_TEXT_BYTES)
        {
            return Err(GenerationError::InvalidResponse);
        }
        if let Some(e) = &self.error
            && (e
                .code
                .as_ref()
                .is_some_and(|c| c.as_str().is_empty() || c.as_str().len() > 128)
                || e.message.as_str().len() > MAX_TEXT_BYTES
                || e.param.as_ref().is_some_and(|p| p.as_str().len() > 256)
                || e.original_type
                    .as_ref()
                    .is_some_and(|p| p.as_str().len() > 256))
        {
            return Err(GenerationError::Limit);
        }
        if let Some(IncompleteReason::Other(t)) = &self.incomplete
            && (t.as_str().is_empty() || t.as_str().len() > 128)
        {
            return Err(GenerationError::Limit);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationResponse {
    items: Vec<(ItemId, Item)>,
    resources: crate::semantic::task::generation::ResourceTable,
    envelopes: crate::semantic::task::generation::history::envelope::MessageEnvelopes,
    outcome: Outcome,
    progress: crate::semantic::task::generation::InteractionProgress,
    usage: UsageReports,
    safety_reports: Vec<crate::semantic::task::generation::SafetyReport>,
    details: TerminalDetails,
}
impl GenerationResponse {
    pub fn new(items: Vec<(ItemId, Item)>, outcome: Outcome) -> Result<Self, GenerationError> {
        Self::from_resources(
            items,
            outcome,
            crate::semantic::task::generation::ResourceTable::default(),
        )
    }
    pub fn from_resources(
        items: Vec<(ItemId, Item)>,
        outcome: Outcome,
        resources: crate::semantic::task::generation::ResourceTable,
    ) -> Result<Self, GenerationError> {
        let mut item_bytes = 0;
        if !items.is_empty() {
            item_bytes =
                crate::semantic::task::generation::validate::items(&items, true, &resources)?;
        }
        let resource_bytes = resources.uncharged_bytes(&items)?;
        if item_bytes.saturating_add(resource_bytes)
            > crate::semantic::task::generation::MAX_TOTAL_BYTES
        {
            return Err(GenerationError::Limit);
        }
        if outcome == Outcome::Completed
            && items.iter().any(|(_, i)| {
                i.lifecycle().is_some_and(|s| s != ItemLifecycle::Completed)
                    || matches!(i, Item::Compaction(value) if value.replay_token().is_none())
            })
        {
            return Err(GenerationError::InvalidResponse);
        }
        Ok(Self {
            items,
            resources,
            envelopes: Default::default(),
            outcome,
            progress: crate::semantic::task::generation::InteractionProgress::Unreported,
            usage: UsageReports::default(),
            safety_reports: vec![],
            details: TerminalDetails::default(),
        })
    }
    pub fn with_progress(
        mut self,
        progress: crate::semantic::task::generation::InteractionProgress,
    ) -> Result<Self, GenerationError> {
        progress.validate(self.outcome, &self.items)?;
        self.progress = progress;
        Ok(self)
    }
    pub const fn progress(&self) -> crate::semantic::task::generation::InteractionProgress {
        self.progress
    }
    pub fn with_usage(self, usage: Option<Usage>) -> Result<Self, GenerationError> {
        self.with_usage_reports(UsageReports::from_usage(usage)?)
    }
    pub fn with_usage_reports(mut self, usage: UsageReports) -> Result<Self, GenerationError> {
        self.usage = usage;
        self.validate_details_and_bounds()?;
        Ok(self)
    }
    pub fn with_details(mut self, details: TerminalDetails) -> Result<Self, GenerationError> {
        self.details = details;
        self.validate_details_and_bounds()?;
        Ok(self)
    }
    fn validate_details_and_bounds(&self) -> Result<(), GenerationError> {
        self.details.validate(self.outcome)?;
        if self
            .details
            .error
            .as_ref()
            .is_some_and(|e| e.code.is_none())
        {
            return Err(GenerationError::InvalidResponse);
        }
        let bytes = if self.items.is_empty() {
            0
        } else {
            crate::semantic::task::generation::validate::items(&self.items, true, &self.resources)?
        };
        if bytes
            .saturating_add(self.details.bytes())
            .saturating_add(self.resources.uncharged_bytes(&self.items)?)
            .saturating_add(self.usage.bytes())
            .saturating_add(self.envelopes.validate(&self.items)?)
            .saturating_add(crate::semantic::task::generation::safety::reports_bytes(
                &self.safety_reports,
            )?)
            > crate::semantic::task::generation::MAX_TOTAL_BYTES
        {
            return Err(GenerationError::Limit);
        }
        Ok(())
    }
    pub fn items(&self) -> &[(ItemId, Item)] {
        &self.items
    }
    pub fn safety_reports(&self) -> &[crate::semantic::task::generation::SafetyReport] {
        &self.safety_reports
    }
    pub fn with_safety_reports(
        mut self,
        reports: Vec<crate::semantic::task::generation::SafetyReport>,
    ) -> Result<Self, GenerationError> {
        self.safety_reports = reports;
        self.validate_details_and_bounds()?;
        Ok(self)
    }
    pub fn message_envelopes(&self) -> &[crate::semantic::task::generation::MessageEnvelope] {
        self.envelopes.values()
    }
    pub fn with_message_envelopes(
        mut self,
        groups: Vec<crate::semantic::task::generation::MessageEnvelope>,
    ) -> Result<Self, GenerationError> {
        self.envelopes =
            crate::semantic::task::generation::history::envelope::MessageEnvelopes::new(
                &self.items,
                groups,
            )?;
        self.validate_details_and_bounds()?;
        Ok(self)
    }
    pub const fn outcome(&self) -> Outcome {
        self.outcome
    }
    pub fn usage(&self) -> Option<Usage> {
        self.usage.summary()
    }
    pub fn usage_reports(&self) -> &UsageReports {
        &self.usage
    }
    pub fn resources(&self) -> &crate::semantic::task::generation::ResourceTable {
        &self.resources
    }
    pub fn with_resources(
        mut self,
        resources: crate::semantic::task::generation::ResourceTable,
    ) -> Result<Self, GenerationError> {
        self.resources = resources;
        self.validate_details_and_bounds()?;
        Ok(self)
    }
    pub fn details(&self) -> &TerminalDetails {
        &self.details
    }
    /// Editing content never changes response outcome or discards terminal details.
    pub fn with_items(self, items: Vec<(ItemId, Item)>) -> Result<Self, GenerationError> {
        if items.len() > crate::semantic::task::generation::MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        let groups = self.envelopes.edited(&items)?.values().to_vec();
        let mut response = Self::from_resources(items, self.outcome, self.resources)?
            .with_message_envelopes(groups)?
            .with_progress(self.progress)?
            .with_details(self.details)?;
        crate::semantic::task::generation::identity::check_call_edits(
            &self.items,
            response.items(),
        )?;
        crate::semantic::task::generation::request::context_transform::check_protected_edits(
            &self.items,
            response.items(),
        )?;
        response = response.with_usage_reports(self.usage)?;
        response = response.with_safety_reports(self.safety_reports)?;
        Ok(response)
    }
}
