//! Protocol-neutral constraints declared by trusted Generation targets.

use crate::disposition::Dispositions;
use morphiecore_semantic::task::generation::{Feature, GenerationRequest, ReasoningActivation};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SamplingConstraints {
    /// Sampling and probability controls require explicitly disabled reasoning.
    pub requires_reasoning_off: bool,
    pub values: SamplingValues,
}

impl SamplingConstraints {
    pub fn apply(self, source: &GenerationRequest, table: &mut Dispositions) {
        if self.requires_reasoning_off
            && source.resolved_configuration().active.activation
                != Some(ReasoningActivation::Disabled)
        {
            for feature in [Feature::Temperature, Feature::TopP, Feature::Logprobs] {
                table.set_native(feature, false);
            }
        }
        for feature in self.values.incompatible(source) {
            table.set_native(feature, false);
        }
    }
}

/// Model-effective value restrictions, separate from wire ranges and field support.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SamplingValues {
    temperature: Temperature,
    top_p: Option<u64>,
    presence_penalty: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Temperature {
    #[default]
    Unrestricted,
    Fixed(u64),
    ThinkingMinimum(u64),
}

impl SamplingValues {
    pub const fn fixed(temperature: f64, top_p: f64, presence_penalty: f64) -> Option<Self> {
        if !temperature.is_finite()
            || !(top_p >= 0.0 && top_p <= 1.0)
            || !presence_penalty.is_finite()
        {
            return None;
        }
        Some(Self {
            temperature: Temperature::Fixed(temperature.to_bits()),
            top_p: Some(top_p.to_bits()),
            presence_penalty: Some(presence_penalty.to_bits()),
        })
    }

    pub const fn minimum_temperature_while_thinking(minimum: f64) -> Option<Self> {
        if !minimum.is_finite() {
            return None;
        }
        Some(Self {
            temperature: Temperature::ThinkingMinimum(minimum.to_bits()),
            top_p: None,
            presence_penalty: None,
        })
    }

    /// Shared comparison; native input rejects these values, target projection
    /// uses the selected field dispositions. Absent controls acquire no defaults.
    pub fn incompatible(self, source: &GenerationRequest) -> impl Iterator<Item = Feature> {
        let controls = source.controls();
        let differs = |expected: Option<u64>, actual: Option<f64>| {
            expected
                .zip(actual)
                .is_some_and(|(expected, actual)| f64::from_bits(expected) != actual)
        };
        [
            (
                Feature::Temperature,
                controls
                    .temperature()
                    .is_some_and(|value| match self.temperature {
                        Temperature::Unrestricted => false,
                        Temperature::Fixed(expected) => value != f64::from_bits(expected),
                        // This restriction applies to the settings being validated,
                        // not the effective history gate used by requires_reasoning_off.
                        Temperature::ThinkingMinimum(minimum) => {
                            source.reasoning().activation != Some(ReasoningActivation::Disabled)
                                && value < f64::from_bits(minimum)
                        }
                    }),
            ),
            (Feature::TopP, differs(self.top_p, controls.top_p())),
            (
                Feature::PresencePenalty,
                differs(self.presence_penalty, controls.presence_penalty()),
            ),
        ]
        .into_iter()
        .filter_map(|(feature, incompatible)| incompatible.then_some(feature))
    }
}

use crate::{
    CodecError,
    disposition::{Disposition, Rule},
    model_limits::ModelLimits,
    reasoning::{ReasoningEffortMapping, ReasoningRequestPolicy},
    tool_set::OrderedCarrier,
};
use morphiecore_semantic::task::generation::{
    EffortControl, EffortLevel, EffortScope, GenerationSettings, ImageDetail, PrefixCacheTtl,
    ReasoningDisplay, ReasoningMode, ReasoningRequest, ReasoningSummary,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Presentation {
    Native,
    Absent,
    Summarize {
        rule: Rule,
        only_when_allocated: bool,
    },
    ExplicitAuto,
    Unavailable,
}

/// Model-qualified request limits independent of carrier grammar.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ModelRequestPolicy {
    pub reject_reasoning_off: bool,
    pub text_output_only: bool,
    pub reject_required_tools: bool,
    pub reject_forced_tools_while_thinking: bool,
    pub reply_only_ceiling_while_thinking: bool,
    pub function_strictness: Option<Disposition>,
}

impl ModelRequestPolicy {
    pub fn forced_tools_unavailable(self, reasoning: &ReasoningRequest) -> bool {
        self.reject_forced_tools_while_thinking
            && reasoning.activation != Some(ReasoningActivation::Disabled)
    }

    pub fn apply(self, source: &GenerationRequest, table: &mut Dispositions) {
        let disabled = source.reasoning().activation == Some(ReasoningActivation::Disabled);
        if let Some(strictness) = self.function_strictness {
            table.set(Feature::StrictTools, strictness);
        }
        if self.reject_reasoning_off && disabled {
            table.set(
                Feature::ReasoningActivation,
                Disposition::Reject("this model has no thinking-off carrier"),
            );
        }
        if self.reply_only_ceiling_while_thinking && !disabled {
            table.set(
                Feature::MaxOutputTokens,
                Disposition::Approximate(crate::disposition::REPLY_ONLY_OUTPUT_CEILING),
            );
        }
    }
}

/// Semantic behavior selected by a trusted carrier adapter. Wire grammar and
/// envelope policies are resolved by the caller; no codec or model IDs enter
/// request projection.
#[derive(Clone, Copy)]
pub struct TargetCapabilities<'a> {
    pub limits: &'a ModelLimits,
    pub effort_scope: EffortScope,
    pub effort_levels: &'static [EffortLevel],
    pub effort_mapping: Option<&'static ReasoningEffortMapping>,
    pub reasoning_policy: Option<&'a ReasoningRequestPolicy>,
    pub effort: bool,
    pub budget: bool,
    pub adaptive_budget: bool,
    pub presentation: Presentation,
    pub reject_hidden_reasoning: bool,
    pub omit_unavailable_allocation_mode: bool,
    pub require_output_ceiling: bool,
    pub default_output_tokens: Option<u64>,
    pub automatic_cache_default: Option<PrefixCacheTtl>,
    pub named_tool_subsets: bool,
    pub whole_inventory_choice: bool,
    pub omit_explicit_text_format: bool,
    pub require_tool_description: bool,
    pub ordered_tools: OrderedCarrier,
    pub filter_allowed_inventory: bool,
    pub filter_singleton_inventory: bool,
    pub image_details: &'a [ImageDetail],
    pub original_image_detail: bool,
}

impl<'a> TargetCapabilities<'a> {
    pub fn limits(self) -> &'a ModelLimits {
        self.limits
    }
    pub fn has_effort(self) -> bool {
        self.effort
    }
    pub fn has_budget(self) -> bool {
        self.budget
    }
    pub fn effort_scope(self) -> EffortScope {
        self.effort_scope
    }
    pub fn policy(self) -> Option<&'a ReasoningRequestPolicy> {
        self.reasoning_policy
    }
    pub(crate) fn budget_fits(self, settings: &GenerationSettings) -> bool {
        if !self.has_budget() {
            return false;
        }
        let Some(budget) = settings.reasoning.budget_tokens else {
            return true;
        };
        let output = settings
            .controls
            .max_output_tokens
            .or(self.default_output_tokens);
        let ceiling = match (output, self.limits().maximum_output_tokens) {
            (Some(output), Some(maximum)) => Some(output.min(maximum)),
            (output, maximum) => output.or(maximum),
        };
        ceiling.is_none_or(|ceiling| budget < ceiling || !self.adaptive_budget && budget == ceiling)
    }
    pub(crate) fn levels(self) -> Vec<EffortLevel> {
        let native = self
            .effort_mapping
            .map_or(self.effort_levels, |m| m.native_levels);
        native
            .iter()
            .copied()
            .filter(|level| {
                self.limits()
                    .allowed_effort
                    .as_ref()
                    .is_none_or(|allowed| allowed.contains(level))
            })
            .collect()
    }
    pub(crate) fn map_effort(
        self,
        mut effort: EffortControl,
        levels: &[EffortLevel],
    ) -> Result<(EffortControl, &'static str), CodecError> {
        effort.scope = self.effort_scope();
        let mapping = self.effort_mapping;
        if let Some(mapping) = mapping {
            effort.level = mapping.encode(effort.level)?;
        }
        effort.level = crate::reasoning::nearest_effort(effort.level, levels)?;
        Ok((
            effort,
            mapping.map_or(super::disposition::EFFORT.id, |mapping| mapping.rule),
        ))
    }
    pub fn presentation_rows(self, settings: &GenerationSettings, table: &mut Dispositions) {
        if self.explicit_presentation_rows(&settings.reasoning, table) {
            return;
        }
        let presentation = &settings.reasoning.presentation;
        let auto = presentation.summary.as_ref() == Some(&ReasoningSummary::Auto);
        let summary_carrier = self.presentation == Presentation::Native;
        let native_summary = match self.presentation {
            Presentation::Summarize { rule, .. } => Some(rule),
            _ => None,
        };
        let omission = Disposition::Omit(super::disposition::OMIT_REASONING_SUMMARY);
        let display = if self.reject_hidden_reasoning
            && presentation.visibility.as_ref() == Some(&ReasoningDisplay::Omitted)
        {
            Disposition::Reject("the selected carrier cannot guarantee hidden reasoning display")
        } else if summary_carrier || native_summary.is_some() {
            Disposition::Native
        } else if auto {
            Disposition::Equivalent
        } else {
            omission
        };
        table.set(Feature::ReasoningDisplay, display);
        let summary = if let Some(rule) = native_summary {
            match &presentation.summary {
                Some(ReasoningSummary::Auto) => {
                    if matches!(
                        self.presentation,
                        Presentation::Summarize {
                            only_when_allocated: true,
                            ..
                        }
                    ) && presentation.visibility.as_ref() == Some(&ReasoningDisplay::Summary)
                    {
                        table.set(Feature::ReasoningDisplay, Disposition::Equivalent);
                    }
                    Disposition::Equivalent
                }
                Some(ReasoningSummary::Concise | ReasoningSummary::Detailed) => {
                    let no_thinking = matches!(
                        self.presentation,
                        Presentation::Summarize {
                            only_when_allocated: true,
                            ..
                        }
                    ) && settings.reasoning.activation.as_ref()
                        != Some(&ReasoningActivation::Enabled)
                        && settings.reasoning.mode.value() != Some(&ReasoningMode::Adaptive)
                        && settings.reasoning.budget_tokens.is_none();
                    if no_thinking {
                        table.set(Feature::ReasoningDisplay, omission);
                        omission
                    } else {
                        Disposition::Approximate(rule)
                    }
                }
                None => Disposition::Native,
            }
        } else if summary_carrier {
            Disposition::Native
        } else if auto {
            Disposition::Equivalent
        } else {
            omission
        };
        table.set(Feature::ReasoningSummary, summary);
        table.set(
            Feature::EncryptedReasoning,
            if summary_carrier {
                Disposition::Native
            } else if settings.reasoning.activation.as_ref() == Some(&ReasoningActivation::Disabled)
            {
                Disposition::Equivalent
            } else {
                Disposition::Omit(super::disposition::OMIT_REASONING_INCLUDE)
            },
        );
    }
    fn explicit_presentation_rows(
        self,
        reasoning: &ReasoningRequest,
        table: &mut Dispositions,
    ) -> bool {
        use crate::disposition::{
            Disposition as D, OMIT_REASONING_INCLUDE, OMIT_REASONING_SUMMARY,
        };
        use morphiecore_semantic::task::generation::Feature as F;
        let disabled = reasoning.activation == Some(ReasoningActivation::Disabled);
        let unsupported = D::Omit(OMIT_REASONING_SUMMARY);
        let (mut display, summary, encrypted) = if self.presentation == Presentation::ExplicitAuto {
            if reasoning.presentation.summary == Some(ReasoningSummary::Auto)
                && matches!(
                    reasoning.presentation.visibility,
                    None | Some(ReasoningDisplay::Summary)
                )
            {
                // Explicit auto summaries request ciphertext on this wire without `include`.
                (D::Native, D::Native, D::Equivalent)
            } else {
                let presentation = if reasoning.presentation.is_absent() {
                    D::Native
                } else {
                    D::Reject(
                        "Qwen4 Responses only supports explicit automatic reasoning summaries",
                    )
                };
                (presentation, presentation, D::Omit(OMIT_REASONING_INCLUDE))
            }
        } else if self.presentation == Presentation::Unavailable {
            (unsupported, unsupported, D::Omit(OMIT_REASONING_INCLUDE))
        } else {
            return false;
        };
        if reasoning.presentation.visibility.as_ref() == Some(&ReasoningDisplay::Omitted) {
            display = D::Reject("the selected carrier cannot guarantee hidden reasoning display");
        }
        for (feature, row) in [
            (F::ReasoningDisplay, display),
            (F::ReasoningSummary, summary),
            (
                F::EncryptedReasoning,
                if disabled { D::Equivalent } else { encrypted },
            ),
        ] {
            table.set(feature, row);
        }
        true
    }
}
