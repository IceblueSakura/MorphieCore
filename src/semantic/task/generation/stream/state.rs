//! Bounded stream state; mutation is private to the stream module.
use crate::semantic::task::generation::*;
use std::collections::BTreeSet;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamPart {
    pub id: PartId,
    pub kind: PartKind,
    pub(super) data: StreamPartValue,
    pub value_finished: bool,
    pub finished: bool,
    pub annotations: Vec<Annotation>,
    pub logprobs: crate::semantic::value::Presence<Vec<Logprob>>,
    pub(super) metadata_bytes: usize,
    pub(super) metadata_finished: bool,
}
impl StreamPart {
    pub fn value(&self) -> &StreamPartValue {
        &self.data
    }
    pub fn text(&self) -> Option<&str> {
        self.data.as_text()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamItem {
    pub id: ItemId,
    pub kind: ItemKind,
    pub parts: Vec<StreamPart>,
    pub status: Option<ItemLifecycle>,
    pub replay: Option<ReasoningReplay>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StreamState {
    pub(super) started: bool,
    pub(super) resources: ResourceTable,
    pub(super) used_resources: BTreeSet<ResourceId>,
    pub(super) file_decoded_bytes: usize,
    pub(super) queued: bool,
    pub(super) items: Vec<StreamItem>,
    pub(super) replay_groups: Vec<ReplayGroup>,
    pub(super) message_envelopes: Vec<MessageEnvelope>,
    pub(super) part_ids: BTreeSet<PartId>,
    pub(super) terminal: Option<StreamTerminal>,
    pub(super) usage: Vec<Usage>,
    pub(super) progress: InteractionProgress,
    pub(super) details: TerminalDetails,
    pub(super) bytes: usize,
}
impl StreamState {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn items(&self) -> &[StreamItem] {
        &self.items
    }
    pub fn resources(&self) -> &ResourceTable {
        &self.resources
    }
    pub fn replay_groups(&self) -> &[ReplayGroup] {
        &self.replay_groups
    }
    pub fn message_envelopes(&self) -> &[MessageEnvelope] {
        &self.message_envelopes
    }
    pub fn started(&self) -> bool {
        self.started
    }
    pub fn item(&self, id: ItemId) -> Result<&StreamItem, EventError> {
        self.items
            .iter()
            .find(|i| i.id == id)
            .ok_or(EventError::Identity)
    }
    pub fn part(&self, item: ItemId, part: PartId) -> Result<&StreamPart, EventError> {
        self.item(item)?
            .parts
            .iter()
            .find(|p| p.id == part)
            .ok_or(EventError::Identity)
    }
    pub fn terminal(&self) -> Option<StreamTerminal> {
        self.terminal
    }
    pub fn usage(&self) -> Option<Usage> {
        Usage::final_operation(&self.usage)
    }
    pub fn usage_reports(&self) -> &[Usage] {
        &self.usage
    }
    pub const fn progress(&self) -> InteractionProgress {
        self.progress
    }
    pub fn details(&self) -> &TerminalDetails {
        &self.details
    }
    pub(super) fn open_item(&mut self, id: ItemId) -> Result<&mut StreamItem, EventError> {
        self.items
            .iter_mut()
            .find(|i| i.id == id && i.status.is_none())
            .ok_or(EventError::Lifecycle)
    }
    pub(super) fn open_part(
        &mut self,
        item: ItemId,
        part: PartId,
    ) -> Result<&mut StreamPart, EventError> {
        self.open_item(item)?
            .parts
            .iter_mut()
            .find(|p| p.id == part && !p.finished)
            .ok_or(EventError::Lifecycle)
    }
    pub(super) fn charge(&mut self, n: usize) -> Result<(), EventError> {
        self.bytes = self.bytes.checked_add(n).ok_or(EventError::Limit)?;
        if self.bytes > MAX_TOTAL_BYTES {
            return Err(EventError::Limit);
        }
        Ok(())
    }
}
impl StreamItem {
    pub(super) fn attachment_slot(
        &mut self,
        owner: ReplayOwner,
        format: ReplayFormat,
    ) -> Result<&mut Option<ReplayValue>, EventError> {
        if self.status.is_some() {
            return Err(EventError::Lifecycle);
        }
        match (owner, &mut self.kind) {
            (ReplayOwner::Item(_), ItemKind::ProviderTool(observed)) => {
                if format != ReplayFormat::GoogleInteractionsV1Step {
                    return Err(EventError::Identity);
                }
                Ok(&mut observed.replay)
            }
            (ReplayOwner::Item(_), ItemKind::ProviderResult { observation, .. }) => {
                if format != ReplayFormat::GoogleInteractionsV1Step
                    || self.parts.len() != 1
                    || self.parts.iter().any(|p| !p.finished)
                {
                    return Err(EventError::Lifecycle);
                }
                Ok(&mut observation.replay)
            }
            _ => Err(EventError::Identity),
        }
    }
}
