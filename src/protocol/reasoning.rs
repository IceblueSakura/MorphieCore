//! Explicit reasoning adaptation selected by trusted bindings.
use crate::{
    protocol::{CodecError, failure::CarrierReason},
    semantic::{task::generation::*, value::Presence},
};
use std::borrow::Cow;

/// Explicit numeric presets, not official token equivalents of qualitative effort.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThinkingBudgets {
    pub entries: Cow<'static, [(EffortLevel, u64)]>,
}
pub const SHARED_BUDGETS: ThinkingBudgets = ThinkingBudgets {
    entries: Cow::Borrowed(&[
        (EffortLevel::Minimal, 1024),
        (EffortLevel::Low, 2048),
        (EffortLevel::Medium, 4096),
        (EffortLevel::High, 8192),
        (EffortLevel::XHigh, 16384),
        (EffortLevel::Max, 24576),
    ]),
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
            return Err(CodecError::invalid());
        }
        for (index, (level, budget)) in self.entries.iter().enumerate() {
            if *budget == 0 || self.entries[..index].iter().any(|(old, _)| old == level) {
                return Err(CodecError::invalid());
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
            .ok_or(CodecError::Unsupported(CarrierReason::Reasoning))
    }
    pub fn nearest(&self, budget: u64) -> Result<(EffortLevel, u64), CodecError> {
        self.validate()?;
        if budget == 0 {
            return Err(CodecError::invalid());
        }
        self.entries
            .iter()
            .copied()
            .min_by_key(|(level, tokens)| {
                (budget.abs_diff(*tokens), std::cmp::Reverse(rank(*level)))
            })
            .ok_or(CodecError::invalid())
    }
}

/// One selected direction. No inverse, qualitative nearest level or fallback is inferred.
#[derive(Clone, Debug, Eq, PartialEq)]
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReasoningRequestPolicy {
    pub canonical_model: Cow<'static, str>,
    pub rule: &'static str,
    pub conversion: ReasoningConversion,
}
impl ReasoningRequestPolicy {
    pub fn validate(&self) -> Result<(), CodecError> {
        if self.rule.is_empty() || self.canonical_model.is_empty() {
            return Err(CodecError::invalid());
        }
        match &self.conversion {
            ReasoningConversion::Scope { from, to, levels } => {
                if from == to
                    || levels.is_empty()
                    || levels.len() > 6
                    || levels
                        .iter()
                        .enumerate()
                        .any(|(i, (key, _))| levels[..i].iter().any(|(k, _)| k == key))
                {
                    return Err(CodecError::invalid());
                }
            }
            ReasoningConversion::BudgetToEffort { table, .. }
            | ReasoningConversion::EffortToBudget { table, .. } => table.validate()?,
            ReasoningConversion::PreserveDisabled => {}
        }
        Ok(())
    }

    /// Scope policies apply only to their declared input scope. Other controls
    /// still reach ordinary target validation; this is not a fallback mapping.
    pub(crate) fn applies_to(&self, source: &GenerationRequest) -> bool {
        let ReasoningConversion::Scope { from, .. } = self.conversion else {
            return true;
        };
        source
            .settings()
            .effort
            .value()
            .is_some_and(|c| c.scope == from)
            || source.items().iter().any(|(_, item)| {
                matches!(item, Item::ConfigurationUpdate(update)
                    if update.effort.is_some_and(|c| c.scope == from))
            })
    }

    pub(crate) fn receipt(
        &self,
        source: &GenerationRequest,
        target: &GenerationRequest,
    ) -> Result<crate::protocol::conversion::generation::Effect, CodecError> {
        let preset_tokens = match &self.conversion {
            ReasoningConversion::BudgetToEffort { table, .. } => target
                .settings()
                .effort
                .value()
                .map(|effort| table.budget(effort.level))
                .transpose()?,
            ReasoningConversion::EffortToBudget { .. } => target.reasoning().budget_tokens,
            _ => None,
        };
        Ok(crate::protocol::conversion::generation::Effect::Reasoning {
            rule: crate::protocol::conversion::generation::RuleId::new(self.rule),
            input: crate::protocol::conversion::generation::ReasoningControls::request(source),
            output: crate::protocol::conversion::generation::ReasoningControls::request(target),
            preset_tokens,
            owner: crate::protocol::conversion::generation::Owner::Root,
        })
    }
    pub fn project_request(
        &self,
        source: &GenerationRequest,
        exact: impl Fn(&GenerationSettings) -> bool,
    ) -> Result<Option<GenerationRequest>, CodecError> {
        source.validate()?;
        self.validate()?;
        let settings = if exact(source.settings()) {
            source.settings().clone()
        } else {
            self.project(source.settings())?
        };
        let mut items = Cow::Borrowed(source.items());
        for (index, (_, item)) in source.items().iter().enumerate() {
            if let Item::ConfigurationUpdate(update) = item {
                let mut controls = GenerationSettings {
                    effort: update.effort.map_or(Presence::Absent, Presence::Value),
                    ..Default::default()
                };
                if let Some(activation) = update.activation {
                    controls.reasoning.presence = ReasoningPresence::Present;
                    controls.reasoning.activation = Presence::Value(activation);
                }
                let projected = if exact(&controls) {
                    controls.clone()
                } else {
                    self.project(&controls)?
                };
                if projected.reasoning.budget_tokens.is_some() {
                    return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                }
                let effort = projected.effort.value().copied();
                if update.effort != effort {
                    items.to_mut()[index].1 =
                        Item::ConfigurationUpdate(ConfigurationUpdate { effort, ..*update });
                }
            }
        }
        if &settings == source.settings() && matches!(items, Cow::Borrowed(_)) {
            return Ok(None);
        }
        let mut target = source.clone().with_settings(settings)?;
        if let Cow::Owned(items) = items {
            target = target.with_items(items)?;
        }
        Ok(Some(target))
    }
    pub fn project(&self, source: &GenerationSettings) -> Result<GenerationSettings, CodecError> {
        source.validate()?;
        self.validate()?;
        let mut target = source.clone();
        match &self.conversion {
            ReasoningConversion::Scope { from, to, levels } => {
                if let Some(control) = source.effort.value()
                    && control.scope == *from
                {
                    let level = levels
                        .iter()
                        .find(|(key, _)| *key == control.level)
                        .map(|(_, to)| *to)
                        .ok_or(CodecError::Unsupported(CarrierReason::Reasoning))?;
                    target.effort = Presence::Value(EffortControl { scope: *to, level });
                }
            }
            ReasoningConversion::BudgetToEffort { scope, table } => {
                if let Some(budget) = source.reasoning.budget_tokens {
                    if !source.effort.is_absent() {
                        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                    }
                    let (level, _) = table.nearest(budget)?;
                    target.reasoning.budget_tokens = None;
                    if target.reasoning.mode == Presence::Value(ReasoningMode::Budgeted) {
                        target.reasoning.mode = Presence::Absent;
                    }
                    target.effort = Presence::Value(EffortControl {
                        scope: *scope,
                        level,
                    });
                }
            }
            ReasoningConversion::EffortToBudget { scope, table } => {
                if let Some(control) = source.effort.value() {
                    if control.scope != *scope || source.reasoning.budget_tokens.is_some() {
                        return Err(CodecError::Unsupported(CarrierReason::Reasoning));
                    }
                    if !source.reasoning.mode.is_absent()
                        && source.reasoning.mode != Presence::Value(ReasoningMode::Budgeted)
                    {
                        return Err(CodecError::Unsupported(CarrierReason::Reasoning));
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
            return Err(CodecError::invalid());
        }
        for entries in [self.encode, self.decode] {
            for (index, (from, _)) in entries.iter().enumerate() {
                if entries[..index].iter().any(|(key, _)| key == from) {
                    return Err(CodecError::invalid());
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
            return Err(CodecError::invalid());
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
            .ok_or(CodecError::Unsupported(CarrierReason::Reasoning))
    }
}
