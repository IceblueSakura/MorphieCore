//! Explicit reasoning adaptation selected by trusted bindings.
use crate::protocol::CodecError;
use crate::semantic::task::generation::{ReasoningEffort, ReasoningRequest};

/// Independent, manually declared directions. Missing entries are rejected,
/// including identities; neither a reverse map nor a nearest level is inferred.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReasoningEffortMapping {
    pub canonical_model: &'static str,
    pub rule: &'static str,
    pub encode: &'static [(ReasoningEffort, ReasoningEffort)],
    pub decode: &'static [(ReasoningEffort, ReasoningEffort)],
}
impl ReasoningEffortMapping {
    pub fn validate(&self) -> Result<(), CodecError> {
        if self.canonical_model.is_empty() || self.rule.is_empty() {
            return Err(CodecError::Invalid("reasoning mapping identity"));
        }
        for entries in [self.encode, self.decode] {
            for (index, (from, to)) in entries.iter().enumerate() {
                if entries[..index].iter().any(|(key, _)| key == from) {
                    return Err(CodecError::Invalid("duplicate reasoning mapping"));
                }
                if (*from == ReasoningEffort::None) != (*to == ReasoningEffort::None) {
                    return Err(CodecError::Invalid("reasoning activation mapping"));
                }
            }
        }
        Ok(())
    }
    pub fn encode(&self, level: ReasoningEffort) -> Result<ReasoningEffort, CodecError> {
        self.lookup(self.encode, level)
    }
    pub fn decode(&self, level: ReasoningEffort) -> Result<ReasoningEffort, CodecError> {
        self.lookup(self.decode, level)
    }
    fn lookup(
        &self,
        entries: &[(ReasoningEffort, ReasoningEffort)],
        level: ReasoningEffort,
    ) -> Result<ReasoningEffort, CodecError> {
        self.validate()?;
        entries
            .iter()
            .find(|(from, _)| *from == level)
            .map(|(_, to)| *to)
            .ok_or_else(|| CodecError::Unsupported("unmapped reasoning effort".into()))
    }
}

/// Gateway policy, not an equivalence defined by any official API.
/// Callers record their named budget-to-effort rule when selecting this mapping.
pub(crate) fn budget_as_effort(reasoning: &ReasoningRequest) -> Option<ReasoningEffort> {
    Some(match reasoning.budget_tokens? {
        0..8192 => ReasoningEffort::Low,
        8192..24576 => ReasoningEffort::Medium,
        _ => ReasoningEffort::High,
    })
}
