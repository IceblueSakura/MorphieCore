//! Derived continuation facts for admitted calls and history, never execution policy.
use super::{GenerationRequest, GenerationResponse, Item, ItemId, ItemLifecycle, Outcome};
use std::collections::BTreeMap;

/// Explicit caller/profile-declared conditions on a Provider observation's successor.
/// This is not inferred from reported progress, an execution command or issuer proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderContinuationRequirement {
    owner: ItemId,
    configuration: super::ConfigurationId,
    proof: super::RequestDependencyProof,
}
impl ProviderContinuationRequirement {
    pub fn capture(
        request: &GenerationRequest,
        owner: ItemId,
        settings: super::SettingsDependency,
    ) -> Result<Self, super::GenerationError> {
        let configuration = request
            .configuration_revision()
            .ok_or(super::GenerationError::InvalidDependency)?;
        if !request
            .items()
            .iter()
            .any(|(id, item)| *id == owner && matches!(item, Item::ProviderTool(_)))
        {
            return Err(super::GenerationError::InvalidProviderObservation);
        }
        let proof = super::RequestDependencyProof::capture(
            request,
            super::HistoryDependency::Owners(vec![owner]),
            settings,
        )?;
        Ok(Self {
            owner,
            configuration,
            proof,
        })
    }
    pub const fn owner(&self) -> ItemId {
        self.owner
    }
    pub const fn original_configuration(&self) -> super::ConfigurationId {
        self.configuration
    }
    pub fn check(&self, request: &GenerationRequest) -> Result<(), super::GenerationError> {
        self.proof.check(request)
    }
}

/// Local owner and wire call identity are distinct; positions are not references.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CallReference<'a> {
    pub item: ItemId,
    pub call_id: &'a str,
}

/// Facts derivable from a completed response or fully reported call/result history.
/// This view neither validates tool arguments nor authorizes tool execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Continuation<'a> {
    /// No supported continuation fact is known. This never asserts end-of-turn,
    /// complete dependencies, or readiness to send another request.
    Unreported,
    /// Nonempty, ordered references to calls without a matching subsequent result.
    ToolResults(Vec<CallReference<'a>>),
}

pub(super) fn pending_results(items: &[(ItemId, Item)]) -> Continuation<'_> {
    // Scan backwards so only a later result can resolve a call. Response program
    // outputs may be self-describing; an earlier or wrong-kind output cannot do so.
    // Both collections are bounded by the validated item budget and borrow values.
    let mut results = BTreeMap::new();
    let mut pending = Vec::new();
    for (item, value) in items.iter().rev() {
        let (domain, call_id) = match value {
            Item::ToolResult(result) | Item::CustomResult(result) => {
                results.insert(
                    (
                        result.context.alias_domain.as_ref(),
                        result.call_id.as_str(),
                    ),
                    value,
                );
                continue;
            }
            Item::ProgramOutput(result) => {
                results.insert((None, result.call_id.as_str()), value);
                continue;
            }
            Item::ToolCall(call) => (call.context.alias_domain.as_ref(), call.call_id.as_str()),
            Item::CustomCall(call) => (call.context.alias_domain.as_ref(), call.call_id.as_str()),
            Item::Program(call) => (None, call.call_id.as_str()),
            _ => continue,
        };
        if !matches!(
            (value, results.get(&(domain, call_id)).copied()),
            (Item::ToolCall(_), Some(Item::ToolResult(_)))
                | (Item::CustomCall(_), Some(Item::CustomResult(_)))
                | (Item::Program(_), Some(Item::ProgramOutput(_)))
        ) {
            pending.push(CallReference {
                item: *item,
                call_id,
            });
        }
    }
    pending.reverse();
    if pending.is_empty() {
        Continuation::Unreported
    } else {
        Continuation::ToolResults(pending)
    }
}

impl GenerationResponse {
    /// Recompute after edits instead of retaining stale call IDs or a finish label.
    pub fn continuation(&self) -> Continuation<'_> {
        if self.outcome() != Outcome::Completed {
            return Continuation::Unreported;
        }
        pending_results(self.items())
    }
}

impl GenerationRequest {
    /// Derive missing results across final history, not an inferred turn boundary.
    /// Terminal result presence does not prove tool success or artifact completeness.
    /// Partial calls or ongoing result reports leave this complete-exchange view unknown.
    pub fn continuation(&self) -> Continuation<'_> {
        if self.items().iter().any(|(_, item)| match item {
            Item::ToolCall(call) => call.status != ItemLifecycle::Completed,
            Item::ToolResult(result) | Item::CustomResult(result) => {
                result.status == Some(ItemLifecycle::InProgress)
            }
            _ => false,
        }) {
            return Continuation::Unreported;
        }
        pending_results(self.items())
    }
}
