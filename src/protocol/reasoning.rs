//! Explicit reasoning adaptation selected by trusted bindings.
use crate::protocol::CodecError;
use crate::semantic::task::generation::{EffortLevel, EffortScope};

/// Independent, manually declared directions. Missing entries are rejected,
/// including identities; neither a reverse map nor a nearest level is inferred.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReasoningEffortMapping {
    pub canonical_model: &'static str,
    pub rule: &'static str,
    pub scope: EffortScope,
    pub native_levels: &'static [EffortLevel],
    pub encode: &'static [(EffortLevel, EffortLevel)],
    pub decode: &'static [(EffortLevel, EffortLevel)],
}
impl ReasoningEffortMapping {
    pub fn validate(&self) -> Result<(), CodecError> {
        if self.canonical_model.is_empty() || self.rule.is_empty() {
            return Err(CodecError::Invalid("reasoning mapping identity"));
        }
        for entries in [self.encode, self.decode] {
            for (index, (from, _)) in entries.iter().enumerate() {
                if entries[..index].iter().any(|(key, _)| key == from) {
                    return Err(CodecError::Invalid("duplicate reasoning mapping"));
                }
            }
        }
        if self
            .encode
            .iter()
            .any(|(_, to)| !self.native_levels.contains(to))
            || self
                .decode
                .iter()
                .any(|(from, _)| !self.native_levels.contains(from))
        {
            return Err(CodecError::Invalid("unsupported native reasoning level"));
        }
        Ok(())
    }
    pub fn encode(&self, level: EffortLevel) -> Result<EffortLevel, CodecError> {
        self.lookup(self.encode, level)
    }
    pub fn decode(&self, level: EffortLevel) -> Result<EffortLevel, CodecError> {
        self.lookup(self.decode, level)
    }
    fn lookup(
        &self,
        entries: &[(EffortLevel, EffortLevel)],
        level: EffortLevel,
    ) -> Result<EffortLevel, CodecError> {
        self.validate()?;
        entries
            .iter()
            .find(|(from, _)| *from == level)
            .map(|(_, to)| *to)
            .ok_or_else(|| CodecError::Unsupported("unmapped reasoning effort".into()))
    }
}
