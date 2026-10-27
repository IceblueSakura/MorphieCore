//! Explicit reasoning adaptation selected by trusted bindings.
//!
//! Client interfaces express reasoning constraints either as qualitative effort
//! levels ([`EffortLevel`]) or discrete numeric token budgets (e.g. Anthropic
//! `thinking.budget_tokens`). Effort controls are scoped to either per-thought
//! reasoning ([`EffortScope::Reasoning`]) or turn-wide response generation
//! ([`EffortScope::Response`]).
//!
//! # Core Invariants
//!
//! - **Decoupled Activation**: Reasoning activation ([`ReasoningRequest`], enabled vs.
//!   disabled) is decoupled from effort scope and level. An explicit disabled state
//!   precludes response-wide effort settings.
//! - **Strict Directionality**: Conversions are directional policies declared per
//!   canonical model binding. No inverse mapping, default fallback, or nearest level
//!   is inferred unless explicitly configured in a policy.
//! - **Quantization & Upward Tie-Breaking**: Numeric budgets are quantized to discrete
//!   effort levels by minimal absolute distance (`abs_diff`). Ties resolve upward to
//!   the higher effort level (via `std::cmp::Reverse(rank(*level))`).
//! - **Ceiling Limits (P3b)**: A thinking budget cannot exceed the client-specified
//!   maximum output token ceiling.
use crate::{
    protocol::{CodecError, failure::CarrierReason},
    semantic::{task::generation::*, value::Presence},
};
use std::borrow::Cow;

/// Explicit numeric presets mapping qualitative [`EffortLevel`] variants to
/// representative token budgets.
///
/// These presets are not official token equivalents of qualitative effort; they
/// serve as discrete quantization thresholds when a target requires discrete levels
/// or when bridging between budget tokens and effort controls.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThinkingBudgets {
    pub entries: Cow<'static, [(EffortLevel, u64)]>,
}

/// The canonical 6-tier quantization table used across shared bindings.
///
/// # Preset Tiers and Boundaries
///
/// - [`EffortLevel::Minimal`]: 1,024 tokens (tie-break threshold: `< 1536`)
/// - [`EffortLevel::Low`]: 2,048 tokens (tie-break range: `1536 .. 3071`)
/// - [`EffortLevel::Medium`]: 4,096 tokens (tie-break range: `3072 .. 6143`)
/// - [`EffortLevel::High`]: 8,192 tokens (tie-break range: `6144 .. 12287`)
/// - [`EffortLevel::XHigh`]: 16,384 tokens (tie-break range: `12288 .. 20479`)
/// - [`EffortLevel::Max`]: 24,576 tokens (tie-break threshold: `>= 20480`)
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

/// Numeric ranking of effort levels for upward tie-breaking.
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
    /// Validates that entries are non-empty, contains at most 6 levels, all
    /// budgets are non-zero, and no duplicate effort levels exist.
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

    /// Returns the representative token budget for a given [`EffortLevel`].
    ///
    /// Returns `CodecError::Unsupported(CarrierReason::Reasoning)` if the level is
    /// missing from the preset table.
    pub fn budget(&self, level: EffortLevel) -> Result<u64, CodecError> {
        self.validate()?;
        self.entries
            .iter()
            .find(|(key, _)| *key == level)
            .map(|(_, budget)| *budget)
            .ok_or(CodecError::Unsupported(CarrierReason::Reasoning))
    }

    /// Quantizes an arbitrary positive token budget to the closest discrete
    /// [`EffortLevel`].
    ///
    /// Distance is measured by `budget.abs_diff(tokens)`. In case of an exact tie
    /// between two adjacent presets, the tie resolves upward to the higher effort
    /// level.
    ///
    /// Returns `Err(CodecError::invalid())` if budget is 0 or if validation fails.
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

/// Directional reasoning conversion strategy.
///
/// Each conversion operates strictly in one direction. No inverse conversion,
/// qualitative nearest level, or fallback default is inferred.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReasoningConversion {
    /// Translates effort levels between scopes across an explicit dictionary of
    /// `(from, to)` level pairs.
    Scope {
        from: EffortScope,
        to: EffortScope,
        levels: &'static [(EffortLevel, EffortLevel)],
    },
    /// Quantizes a numeric token budget into a qualitative effort level in the
    /// specified target scope using the provided [`ThinkingBudgets`] table.
    BudgetToEffort {
        scope: EffortScope,
        table: ThinkingBudgets,
    },
    /// Expands a qualitative effort level in the specified source scope into a
    /// numeric token budget using the provided [`ThinkingBudgets`] table.
    EffortToBudget {
        scope: EffortScope,
        table: ThinkingBudgets,
    },
    /// Suppresses response-scoped effort when reasoning is explicitly disabled,
    /// preventing downstream carriers from activating conflicting reasoning.
    PreserveDisabled,
}

/// A named reasoning policy applied to requests targeted at a specific canonical model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReasoningRequestPolicy {
    /// Canonical model identifier this policy applies to.
    pub canonical_model: Cow<'static, str>,
    /// Unique rule identifier for adaptation evidence attribution.
    pub rule: &'static str,
    /// The directional conversion to apply.
    pub conversion: ReasoningConversion,
}

impl ReasoningRequestPolicy {
    /// Validates rule identifier, canonical model, and inner conversion configuration.
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

    /// Checks whether this policy applies to the source request's effort scope.
    ///
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

    /// Emits a structured [`Effect::Reasoning`] evidence receipt.
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

    /// Projects a generation request according to the policy rules.
    ///
    /// Applies transformations to root generation settings and any item-level
    /// configuration updates. Rejects if item updates produce unsupported budget tokens.
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

    /// Projects generation settings in accordance with the conversion policy.
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

/// Hand-authored bidirectional mapping table for models with asymmetric or
/// constrained qualitative effort levels.
///
/// Both `encode` and `decode` pairs must map only to/from levels declared in
/// `native_levels`. Missing levels reject at validation rather than defaulting.
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
    /// Validates that rule and canonical model are non-empty, entries are unique,
    /// and all mappings reference only native levels.
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

    /// Encodes a qualitative effort level to the target model's native representation.
    pub fn encode(&self, level: EffortLevel) -> Result<EffortLevel, CodecError> {
        self.lookup(self.encode, level)
    }

    /// Decodes a qualitative effort level from the target model's native representation.
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
