//! Reasoning controls and readable reasoning items from official Generation APIs.
//!
//! An absent object, an omitted child and disabled activation are distinct.
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
pub enum EffortLevel {
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    Max,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffortScope {
    Reasoning,
    Response,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffortControl {
    pub scope: EffortScope,
    pub level: EffortLevel,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReasoningActivation {
    Disabled,
    Enabled,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReasoningSummary {
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
/// One presentation intent: visibility and summary granularity are independent axes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReasoningPresentation {
    pub visibility: crate::semantic::value::Presence<ReasoningDisplay>,
    pub summary: crate::semantic::value::Presence<ReasoningSummary>,
}
impl ReasoningPresentation {
    pub fn is_absent(&self) -> bool {
        self.visibility.is_absent() && self.summary.is_absent()
    }
    pub fn validate(&self) -> Result<(), crate::semantic::task::generation::GenerationError> {
        use crate::semantic::value::Presence;
        if self.summary.value().is_some()
            && self.visibility != Presence::Value(ReasoningDisplay::Summary)
            || self.summary == Presence::Null && !self.visibility.is_absent()
            || self.visibility == Presence::Null && !self.summary.is_absent()
        {
            return Err(crate::semantic::task::generation::GenerationError::InvalidControl);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReasoningRequest {
    pub presence: ReasoningPresence,
    pub activation: crate::semantic::value::Presence<ReasoningActivation>,
    pub presentation: ReasoningPresentation,
    pub context: crate::semantic::value::Presence<ReasoningContext>,
    /// Whether supplied historical reasoning is read. Absence uses the target's
    /// default; this is not a current-turn context window or replay permission.
    pub read_history: crate::semantic::value::Presence<bool>,
    pub mode: crate::semantic::value::Presence<ReasoningMode>,
    /// Reasoning tokens, distinct from the response-token ceiling. Whether it is a
    /// target or a cap is the target's rule; Messages documents a target.
    pub budget_tokens: Option<u64>,
    /// Handling of returned reasoning that fails the target's conversation check.
    pub replay_mismatch: Option<ReplayMismatch>,
    encrypted_output: bool,
}
/// Explicit policy for replayed reasoning that no longer matches its conversation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayMismatch {
    Reject,
    Drop,
}
impl ReasoningRequest {
    pub fn absent() -> Self {
        Self::default()
    }
    pub fn present(summary: Option<ReasoningSummary>) -> Self {
        use crate::semantic::value::Presence;
        Self {
            presence: ReasoningPresence::Present,
            presentation: ReasoningPresentation {
                visibility: summary.map_or(Presence::Absent, |_| {
                    Presence::Value(ReasoningDisplay::Summary)
                }),
                summary: summary.map_or(Presence::Absent, Presence::Value),
            },
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
    pub fn summary(&self) -> Option<ReasoningSummary> {
        self.presentation.summary.value().copied()
    }
    pub fn validate(&self) -> Result<(), crate::semantic::task::generation::GenerationError> {
        self.presentation.validate()?;
        if self.budget_tokens == Some(0) {
            return Err(crate::semantic::task::generation::GenerationError::InvalidControl);
        }
        if self.presence != ReasoningPresence::Present
            && (!self.activation.is_absent()
                || !self.presentation.is_absent()
                || !self.context.is_absent()
                || !self.read_history.is_absent()
                || !self.mode.is_absent()
                || self.budget_tokens.is_some())
        {
            return Err(crate::semantic::task::generation::GenerationError::InvalidControl);
        }
        if self.read_history == crate::semantic::value::Presence::Null {
            return Err(crate::semantic::task::generation::GenerationError::InvalidControl);
        }
        if self.mode.value() == Some(&ReasoningMode::Budgeted) && self.budget_tokens.is_none()
            || self.activation.value() == Some(&ReasoningActivation::Disabled)
                && (self.budget_tokens.is_some()
                    || matches!(
                        self.mode.value(),
                        Some(ReasoningMode::Adaptive | ReasoningMode::Budgeted)
                    )
                    || self.presentation.visibility.value() == Some(&ReasoningDisplay::Summary))
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
