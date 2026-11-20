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
