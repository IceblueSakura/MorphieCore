//! Lossy reasoning projection shared by targets without a numeric budget carrier.
use crate::semantic::task::generation::{ReasoningEffort, ReasoningRequest};

/// Gateway policy, not an equivalence defined by any official API.
/// Callers record their named budget-to-effort rule when selecting this mapping.
pub(crate) fn budget_as_effort(reasoning: &ReasoningRequest) -> Option<ReasoningEffort> {
    Some(match reasoning.budget_tokens? {
        0..8192 => ReasoningEffort::Low,
        8192..24576 => ReasoningEffort::Medium,
        _ => ReasoningEffort::High,
    })
}
