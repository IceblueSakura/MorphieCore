//! Responses reasoning controls and readable reasoning items.
//!
//! An absent object, an omitted child, and explicit `none` are distinct.
//! Opaque Responses values belong to their typed item, never to readable parts.
//! Source records bind provenance and dependencies without owning another payload.
use super::PartId;
use crate::semantic::value::Text;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ReasoningPresence {
    #[default]
    Absent,
    Null,
    Present,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
pub enum ReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    Max,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReasoningSummary {
    Disabled,
    Auto,
    Concise,
    Detailed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum ReasoningContext {
    Auto,
    CurrentTurn,
    AllTurns,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum ReasoningMode {
    Standard,
    Pro,
    /// Model-selected allocation, compatible with explicit effort guidance.
    Adaptive,
    /// Explicit numeric allocation intent; never a bare thinking-enabled switch.
    Budgeted,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReasoningDisplay {
    Summary,
    Omitted,
}
/// Reasoning-token constraints, distinct from a total response-token ceiling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReasoningBudget {
    pub hard_limit: Option<u64>,
    pub soft_target: Option<u64>,
}
impl ReasoningBudget {
    pub fn validate(self) -> Result<(), super::GenerationError> {
        if self.hard_limit.is_none() && self.soft_target.is_none()
            || self.hard_limit == Some(0)
            || self.soft_target == Some(0)
            || matches!((self.soft_target, self.hard_limit), (Some(target), Some(limit)) if target > limit)
        {
            return Err(super::GenerationError::InvalidControl);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReasoningRequest {
    pub presence: ReasoningPresence,
    pub effort: crate::semantic::value::Presence<ReasoningEffort>,
    pub summary: crate::semantic::value::Presence<ReasoningSummary>,
    pub context: crate::semantic::value::Presence<ReasoningContext>,
    pub mode: crate::semantic::value::Presence<ReasoningMode>,
    pub budget: Option<ReasoningBudget>,
    pub display: crate::semantic::value::Presence<ReasoningDisplay>,
    encrypted_output: bool,
}
impl ReasoningRequest {
    pub fn absent() -> Self {
        Self::default()
    }
    pub fn present(effort: Option<ReasoningEffort>, summary: Option<ReasoningSummary>) -> Self {
        use crate::semantic::value::Presence;
        Self {
            presence: ReasoningPresence::Present,
            effort: effort.map_or(Presence::Absent, Presence::Value),
            summary: summary.map_or(Presence::Absent, Presence::Value),
            ..Default::default()
        }
    }
    pub fn with_encrypted_output(mut self, enabled: bool) -> Self {
        self.encrypted_output = enabled;
        self
    }
    pub fn encrypted_output(&self) -> bool {
        self.encrypted_output
    }
    pub fn presence(&self) -> ReasoningPresence {
        self.presence
    }
    pub fn effort(&self) -> Option<ReasoningEffort> {
        self.effort.value().copied()
    }
    pub fn summary(&self) -> Option<ReasoningSummary> {
        self.summary.value().copied()
    }
    pub fn validate(&self) -> Result<(), super::GenerationError> {
        if let Some(budget) = self.budget {
            budget.validate()?;
        }
        if self.presence != ReasoningPresence::Present
            && (!self.effort.is_absent()
                || !self.summary.is_absent()
                || !self.context.is_absent()
                || !self.mode.is_absent()
                || self.budget.is_some()
                || !self.display.is_absent())
        {
            return Err(super::GenerationError::InvalidControl);
        }
        if self.effort() == Some(ReasoningEffort::None)
            && self
                .summary()
                .is_some_and(|s| s != ReasoningSummary::Disabled)
        {
            return Err(super::GenerationError::InvalidControl);
        }
        if self.mode.value() == Some(&ReasoningMode::Budgeted) && self.budget.is_none()
            || self.effort() == Some(ReasoningEffort::None)
                && (self.budget.is_some()
                    || matches!(
                        self.mode.value(),
                        Some(ReasoningMode::Adaptive | ReasoningMode::Budgeted)
                    )
                    || self.display.value() == Some(&ReasoningDisplay::Summary))
            || self.display.value() == Some(&ReasoningDisplay::Omitted)
                && self
                    .summary()
                    .is_some_and(|s| s != ReasoningSummary::Disabled)
            || self.display.value() == Some(&ReasoningDisplay::Summary)
                && self.summary() == Some(ReasoningSummary::Disabled)
        {
            return Err(super::GenerationError::InvalidControl);
        }
        Ok(())
    }
    /// No current Responses/Chat carrier can preserve these independent intents.
    pub fn requires_extended_controls(&self) -> bool {
        self.budget.is_some()
            || !self.display.is_absent()
            || matches!(
                self.mode.value(),
                Some(ReasoningMode::Adaptive | ReasoningMode::Budgeted)
            )
    }
}
/// Event-owned value and trusted intake scope. Materialization moves the value
/// into its reasoning item; fidelity retains only a source/dependency binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReasoningReplay {
    pub value: super::ReplayValue,
    pub origin: Option<crate::semantic::value::ReplayOrigin>,
}
impl ReasoningReplay {
    pub fn validate(&self) -> Result<(), super::GenerationError> {
        self.value.validate_reasoning()
    }
    pub fn permits(&self, target: Option<&crate::semantic::value::ReplayOrigin>) -> bool {
        self.origin.is_some() && self.origin.as_ref() == target
    }
}
/// Readable reasoning content. This is not assistant message text or encrypted replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReasoningContent {
    Summary(Text),
    Text(Text),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReasoningItem {
    pub parts: Vec<(PartId, ReasoningContent)>,
    pub status: super::ItemLifecycle,
    pub replay: Option<super::ReplayValue>,
}
