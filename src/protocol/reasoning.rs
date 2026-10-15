//! Explicit reasoning adaptation selected by trusted bindings.
use crate::protocol::CodecError;
use crate::semantic::{task::generation::*, value::Presence};

/// Explicit numeric presets, not official token equivalents of qualitative effort.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThinkingBudgets {
    pub entries: &'static [(EffortLevel, u64)],
}
pub const SHARED_BUDGETS: ThinkingBudgets = ThinkingBudgets {
    entries: &[
        (EffortLevel::Minimal, 1024),
        (EffortLevel::Low, 2048),
        (EffortLevel::Medium, 4096),
        (EffortLevel::High, 8192),
        (EffortLevel::XHigh, 16384),
        (EffortLevel::Max, 24576),
    ],
};
fn rank(level: EffortLevel) -> u8 {
    match level {
        EffortLevel::Minimal => 0,
        EffortLevel::Low => 1,
        EffortLevel::Medium => 2,
        EffortLevel::High => 3,
        EffortLevel::XHigh => 4,
        EffortLevel::Max => 5,
    }
}
impl ThinkingBudgets {
    pub fn validate(&self) -> Result<(), CodecError> {
        if self.entries.is_empty() || self.entries.len() > 6 {
            return Err(CodecError::Invalid("thinking preset"));
        }
        for (index, (level, budget)) in self.entries.iter().enumerate() {
            if *budget == 0 || self.entries[..index].iter().any(|(old, _)| old == level) {
                return Err(CodecError::Invalid("thinking preset"));
            }
        }
        Ok(())
    }
    pub fn budget(&self, level: EffortLevel) -> Result<u64, CodecError> {
        self.validate()?;
        self.entries
            .iter()
            .find(|(key, _)| *key == level)
            .map(|(_, budget)| *budget)
            .ok_or_else(|| CodecError::Unsupported("unmapped thinking budget".into()))
    }
    pub fn nearest(&self, budget: u64) -> Result<(EffortLevel, u64), CodecError> {
        self.validate()?;
        if budget == 0 {
            return Err(CodecError::Invalid("positive thinking budget"));
        }
        self.entries
            .iter()
            .copied()
            .min_by_key(|(level, tokens)| {
                (budget.abs_diff(*tokens), std::cmp::Reverse(rank(*level)))
            })
            .ok_or(CodecError::Invalid("thinking preset"))
    }
}

/// One selected direction. No inverse, qualitative nearest level or fallback is inferred.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReasoningConversion {
    Scope {
        from: EffortScope,
        to: EffortScope,
        levels: &'static [(EffortLevel, EffortLevel)],
    },
    BudgetToEffort {
        scope: EffortScope,
        table: ThinkingBudgets,
    },
    EffortToBudget {
        scope: EffortScope,
        table: ThinkingBudgets,
    },
    PreserveDisabled,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReasoningRequestPolicy {
    pub canonical_model: &'static str,
    pub rule: &'static str,
    pub conversion: ReasoningConversion,
}
impl ReasoningRequestPolicy {
    pub fn project_request(
        &self,
        source: &GenerationRequest,
    ) -> Result<Option<GenerationRequest>, CodecError> {
        let settings = self.project(source.settings())?;
        if &settings == source.settings() {
            return Ok(None);
        }
        // Configuration history has its own ordered source contract.
        if source
            .items()
            .iter()
            .any(|(_, item)| matches!(item, Item::ConfigurationUpdate(_)))
        {
            return Err(CodecError::Unsupported(
                "reasoning policy configuration history".into(),
            ));
        }
        Ok(Some(source.clone().with_settings(settings)?))
    }
    pub fn project(&self, source: &GenerationSettings) -> Result<GenerationSettings, CodecError> {
        source.validate()?;
        if self.rule.is_empty() || self.canonical_model.is_empty() {
            return Err(CodecError::Invalid("reasoning policy identity"));
        }
        let mut target = source.clone();
        match self.conversion {
            ReasoningConversion::Scope { from, to, levels } => {
                if from == to
                    || levels.is_empty()
                    || levels.len() > 6
                    || levels
                        .iter()
                        .enumerate()
                        .any(|(i, (key, _))| levels[..i].iter().any(|(k, _)| k == key))
                {
                    return Err(CodecError::Invalid("reasoning scope policy"));
                }
                if let Some(control) = source.effort.value() {
                    if control.scope != from {
                        return Err(CodecError::Unsupported("reasoning scope".into()));
                    }
                    let level = levels
                        .iter()
                        .find(|(key, _)| *key == control.level)
                        .map(|(_, to)| *to)
                        .ok_or_else(|| {
                            CodecError::Unsupported("unmapped reasoning scope".into())
                        })?;
                    target.effort = Presence::Value(EffortControl { scope: to, level });
                }
            }
            ReasoningConversion::BudgetToEffort { scope, table } => {
                table.validate()?;
                if let Some(budget) = source.reasoning.budget_tokens {
                    if !source.effort.is_absent() {
                        return Err(CodecError::Unsupported(
                            "combined thinking allocation".into(),
                        ));
                    }
                    let (level, _) = table.nearest(budget)?;
                    target.reasoning.budget_tokens = None;
                    if target.reasoning.mode == Presence::Value(ReasoningMode::Budgeted) {
                        target.reasoning.mode = Presence::Absent;
                    }
                    target.effort = Presence::Value(EffortControl { scope, level });
                }
            }
            ReasoningConversion::EffortToBudget { scope, table } => {
                table.validate()?;
                if let Some(control) = source.effort.value() {
                    if control.scope != scope || source.reasoning.budget_tokens.is_some() {
                        return Err(CodecError::Unsupported("thinking allocation scope".into()));
                    }
                    target.reasoning.presence = ReasoningPresence::Present;
                    target.reasoning.mode = Presence::Value(ReasoningMode::Budgeted);
                    target.reasoning.budget_tokens = Some(table.budget(control.level)?);
                    target.effort = Presence::Absent;
                }
            }
            ReasoningConversion::PreserveDisabled => {
                if source.reasoning.activation == Presence::Value(ReasoningActivation::Disabled)
                    && source
                        .effort
                        .value()
                        .is_some_and(|c| c.scope == EffortScope::Response)
                {
                    target.effort = Presence::Absent;
                }
            }
        }
        target.validate()?;
        Ok(target)
    }
}

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
