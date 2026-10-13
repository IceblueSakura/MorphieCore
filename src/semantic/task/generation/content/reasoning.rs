//! Reasoning controls and readable reasoning items from official Generation APIs.
//!
//! An absent object, an omitted child, and explicit `none` are distinct.
//! Opaque continuation values belong to their typed item, never to readable parts.
//! Source records bind provenance and dependencies without owning another payload.
use crate::semantic::task::generation::PartId;
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
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReasoningRequest {
    pub presence: ReasoningPresence,
    pub effort: crate::semantic::value::Presence<ReasoningEffort>,
    pub summary: crate::semantic::value::Presence<ReasoningSummary>,
    pub context: crate::semantic::value::Presence<ReasoningContext>,
    pub mode: crate::semantic::value::Presence<ReasoningMode>,
    /// Reasoning tokens, distinct from the response-token ceiling. Whether it is a
    /// target or a cap is the target's rule; Messages documents a target.
    pub budget_tokens: Option<u64>,
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
    pub fn validate(&self) -> Result<(), crate::semantic::task::generation::GenerationError> {
        if self.budget_tokens == Some(0) {
            return Err(crate::semantic::task::generation::GenerationError::InvalidControl);
        }
        if self.presence != ReasoningPresence::Present
            && (!self.effort.is_absent()
                || !self.summary.is_absent()
                || !self.context.is_absent()
                || !self.mode.is_absent()
                || self.budget_tokens.is_some()
                || !self.display.is_absent())
        {
            return Err(crate::semantic::task::generation::GenerationError::InvalidControl);
        }
        if self.effort() == Some(ReasoningEffort::None)
            && self
                .summary()
                .is_some_and(|s| s != ReasoningSummary::Disabled)
        {
            return Err(crate::semantic::task::generation::GenerationError::InvalidControl);
        }
        if self.mode.value() == Some(&ReasoningMode::Budgeted) && self.budget_tokens.is_none()
            || self.effort() == Some(ReasoningEffort::None)
                && (self.budget_tokens.is_some()
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
            return Err(crate::semantic::task::generation::GenerationError::InvalidControl);
        }
        Ok(())
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
    pub status: crate::semantic::task::generation::ItemLifecycle,
    pub replay: Option<crate::semantic::task::generation::ReplayValue>,
}
