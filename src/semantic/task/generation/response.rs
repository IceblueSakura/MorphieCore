//! Completed or partial Generation output, independent of transport success.
use super::{GenerationError, Item, ItemId, ItemLifecycle, MAX_TEXT_BYTES, Usage};
use crate::semantic::value::Text;

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
    ContentFilter,
    Other(Text),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseError {
    pub code: Option<Text>,
    pub message: Text,
    pub param: Option<Text>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TerminalDetails {
    pub error: Option<ResponseError>,
    pub incomplete: Option<IncompleteReason>,
}
impl TerminalDetails {
    pub fn bytes(&self) -> usize {
        let error = self.error.as_ref().map_or(0, |e| {
            e.code.as_ref().map_or(0, |c| c.as_str().len())
                + e.message.as_str().len()
                + e.param.as_ref().map_or(0, |p| p.as_str().len())
        });
        error
            + match &self.incomplete {
                Some(IncompleteReason::Other(t)) => t.as_str().len(),
                Some(_) => 32,
                None => 0,
            }
    }
    pub fn validate(&self, outcome: Outcome) -> Result<(), GenerationError> {
        if self.error.is_some() && outcome != Outcome::Failed
            || self.incomplete.is_some() && outcome != Outcome::Incomplete
        {
            return Err(GenerationError::InvalidResponse);
        }
        if let Some(e) = &self.error
            && (e
                .code
                .as_ref()
                .is_some_and(|c| c.as_str().is_empty() || c.as_str().len() > 128)
                || e.message.as_str().len() > MAX_TEXT_BYTES
                || e.param.as_ref().is_some_and(|p| p.as_str().len() > 256))
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
    outcome: Outcome,
    progress: super::InteractionProgress,
    usage: Vec<Usage>,
    details: TerminalDetails,
}
impl GenerationResponse {
    pub fn new(items: Vec<(ItemId, Item)>, outcome: Outcome) -> Result<Self, GenerationError> {
        if !items.is_empty() {
            super::validate::items(&items, true)?;
        }
        if outcome == Outcome::Completed
            && items
                .iter()
                .any(|(_, i)| i.lifecycle().is_some_and(|s| s != ItemLifecycle::Completed))
        {
            return Err(GenerationError::InvalidResponse);
        }
        Ok(Self {
            items,
            outcome,
            progress: super::InteractionProgress::Unreported,
            usage: vec![],
            details: TerminalDetails::default(),
        })
    }
    pub fn with_progress(
        mut self,
        progress: super::InteractionProgress,
    ) -> Result<Self, GenerationError> {
        progress.validate(self.outcome, &self.items)?;
        self.progress = progress;
        Ok(self)
    }
    pub const fn progress(&self) -> super::InteractionProgress {
        self.progress
    }
    pub fn with_usage(self, usage: Usage) -> Result<Self, GenerationError> {
        self.with_usage_reports(vec![usage])
    }
    pub fn with_usage_reports(mut self, reports: Vec<Usage>) -> Result<Self, GenerationError> {
        if reports.len() > super::MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        let mut values = Vec::new();
        for report in reports {
            Usage::update(&mut values, report)?;
        }
        Usage::check_owners(&values, &self.items)?;
        self.usage = values;
        let details = self.details.clone();
        self.with_details(details)
    }
    pub fn with_details(mut self, details: TerminalDetails) -> Result<Self, GenerationError> {
        details.validate(self.outcome)?;
        if details.error.as_ref().is_some_and(|e| e.code.is_none()) {
            return Err(GenerationError::InvalidResponse);
        }
        let bytes = if self.items.is_empty() {
            0
        } else {
            super::validate::items(&self.items, true)?
        };
        if bytes
            .saturating_add(details.bytes())
            .saturating_add(self.usage.len() * std::mem::size_of::<Usage>())
            > super::MAX_TOTAL_BYTES
        {
            return Err(GenerationError::Limit);
        }
        self.details = details;
        Ok(self)
    }
    pub fn items(&self) -> &[(ItemId, Item)] {
        &self.items
    }
    pub const fn outcome(&self) -> Outcome {
        self.outcome
    }
    pub fn usage(&self) -> Option<Usage> {
        Usage::final_operation(&self.usage)
    }
    pub fn usage_reports(&self) -> &[Usage] {
        &self.usage
    }
    pub fn details(&self) -> &TerminalDetails {
        &self.details
    }
    /// Editing content never changes response outcome or discards terminal details.
    pub fn with_items(self, items: Vec<(ItemId, Item)>) -> Result<Self, GenerationError> {
        let mut response = Self::new(items, self.outcome)?
            .with_progress(self.progress)?
            .with_details(self.details)?;
        response = response.with_usage_reports(self.usage)?;
        Ok(response)
    }
}
