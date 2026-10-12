//! Completed or partial Generation output, independent of transport success.
use crate::semantic::task::generation::{
    GenerationError, Item, ItemId, ItemLifecycle, MAX_TEXT_BYTES, Usage,
};
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
    resources: crate::semantic::task::generation::ResourceTable,
    message_owners: crate::semantic::task::generation::history::envelope::MessageEnvelopes,
    outcome: Outcome,
    progress: crate::semantic::task::generation::InteractionProgress,
    usage: Vec<Usage>,
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
            && items
                .iter()
                .any(|(_, i)| i.lifecycle().is_some_and(|s| s != ItemLifecycle::Completed))
        {
            return Err(GenerationError::InvalidResponse);
        }
        Ok(Self {
            items,
            resources,
            message_owners: Default::default(),
            outcome,
            progress: crate::semantic::task::generation::InteractionProgress::Unreported,
            usage: vec![],
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
    pub fn with_usage(self, usage: Usage) -> Result<Self, GenerationError> {
        self.with_usage_reports(vec![usage])
    }
    pub fn with_usage_reports(mut self, reports: Vec<Usage>) -> Result<Self, GenerationError> {
        if reports.len() > crate::semantic::task::generation::MAX_ITEMS {
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
            crate::semantic::task::generation::validate::items(&self.items, true, &self.resources)?
        };
        if bytes
            .saturating_add(details.bytes())
            .saturating_add(self.resources.uncharged_bytes(&self.items)?)
            .saturating_add(self.usage.len() * std::mem::size_of::<Usage>())
            .saturating_add(self.message_owners.validate(&self.items)?)
            > crate::semantic::task::generation::MAX_TOTAL_BYTES
        {
            return Err(GenerationError::Limit);
        }
        self.details = details;
        Ok(self)
    }
    pub fn items(&self) -> &[(ItemId, Item)] {
        &self.items
    }
    pub fn message_owners(&self) -> &std::collections::BTreeMap<ItemId, ItemId> {
        self.message_owners.chat()
    }
    pub fn message_envelopes(&self) -> &[crate::semantic::task::generation::MessageEnvelope] {
        self.message_owners.values()
    }
    pub fn with_message_envelopes(
        mut self,
        groups: Vec<crate::semantic::task::generation::MessageEnvelope>,
    ) -> Result<Self, GenerationError> {
        self.message_owners =
            crate::semantic::task::generation::history::envelope::MessageEnvelopes::new(
                &self.items,
                groups,
            )?;
        let details = self.details.clone();
        self.with_details(details)
    }
    pub fn with_message_owners(
        mut self,
        owners: Vec<(ItemId, ItemId)>,
    ) -> Result<Self, GenerationError> {
        self.message_owners =
            crate::semantic::task::generation::history::envelope::MessageEnvelopes::from_chat(
                &self.items,
                owners,
            )?;
        let details = self.details.clone();
        self.with_details(details)
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
    pub fn resources(&self) -> &crate::semantic::task::generation::ResourceTable {
        &self.resources
    }
    pub fn with_resources(
        mut self,
        resources: crate::semantic::task::generation::ResourceTable,
    ) -> Result<Self, GenerationError> {
        self.resources = resources;
        let details = self.details.clone();
        self.with_details(details)
    }
    pub fn details(&self) -> &TerminalDetails {
        &self.details
    }
    /// Editing content never changes response outcome or discards terminal details.
    pub fn with_items(self, items: Vec<(ItemId, Item)>) -> Result<Self, GenerationError> {
        if items.len() > crate::semantic::task::generation::MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        let groups = self.message_owners.edited(&items)?.values().to_vec();
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
        Ok(response)
    }
}
