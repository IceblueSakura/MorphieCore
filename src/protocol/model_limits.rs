//! Trusted model-effective limits, checked after target-local control projection.
use crate::protocol::failure::CarrierReason;
use crate::{
    protocol::CodecError,
    semantic::task::generation::{EffortLevel, GenerationRequest, Item},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ModelLimits {
    /// Absence declares no additional level restriction; it grants no carrier.
    pub allowed_effort: Option<Box<[EffortLevel]>>,
    /// Trusted planning metadata, not a tokenizer or a request-admission rule.
    pub context_window_tokens: Option<u64>,
    /// Independent input capacity; never derived from context minus output.
    pub maximum_input_tokens: Option<u64>,
    /// A maximum is not a default and never fills an absent request ceiling.
    pub maximum_output_tokens: Option<u64>,
}
impl ModelLimits {
    pub fn validate(&self) -> Result<(), CodecError> {
        if self.allowed_effort.as_ref().is_some_and(|levels| {
            levels.is_empty()
                || levels.len() > 6
                || levels
                    .iter()
                    .enumerate()
                    .any(|(i, level)| levels[..i].contains(level))
        }) || [
            self.context_window_tokens,
            self.maximum_input_tokens,
            self.maximum_output_tokens,
        ]
        .contains(&Some(0))
        {
            return Err(CodecError::invalid());
        }
        Ok(())
    }

    /// Configuration may narrow a catalog declaration, never widen it.
    pub(crate) fn narrowed(&self, configured: Self) -> Result<Self, CodecError> {
        self.validate()?;
        configured.validate()?;
        if configured.allowed_effort.as_ref().is_some_and(|levels| {
            self.allowed_effort
                .as_ref()
                .is_some_and(|base| levels.iter().any(|level| !base.contains(level)))
        }) || [
            (self.context_window_tokens, configured.context_window_tokens),
            (self.maximum_input_tokens, configured.maximum_input_tokens),
            (self.maximum_output_tokens, configured.maximum_output_tokens),
        ]
        .iter()
        .any(|(base, configured)| base.zip(*configured).is_some_and(|(a, b)| b > a))
        {
            return Err(CodecError::invalid());
        }
        Ok(Self {
            allowed_effort: configured
                .allowed_effort
                .or_else(|| self.allowed_effort.clone()),
            context_window_tokens: configured
                .context_window_tokens
                .or(self.context_window_tokens),
            maximum_input_tokens: configured
                .maximum_input_tokens
                .or(self.maximum_input_tokens),
            maximum_output_tokens: configured
                .maximum_output_tokens
                .or(self.maximum_output_tokens),
        })
    }

    pub(crate) fn check_request(&self, request: &GenerationRequest) -> Result<(), CodecError> {
        self.check_values(
            request.controls().max_output_tokens,
            request
                .settings()
                .effort
                .value()
                .map(|effort| effort.level)
                .into_iter()
                .chain(request.items().iter().filter_map(|(_, item)| match item {
                    Item::ConfigurationUpdate(update) => update.effort.map(|effort| effort.level),
                    _ => None,
                })),
        )
    }

    pub(crate) fn check_values(
        &self,
        output: Option<u64>,
        efforts: impl IntoIterator<Item = EffortLevel>,
    ) -> Result<(), CodecError> {
        self.validate()?;
        if output
            .zip(self.maximum_output_tokens)
            .is_some_and(|(value, maximum)| value > maximum)
        {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        if let Some(allowed) = &self.allowed_effort
            && efforts.into_iter().any(|effort| !allowed.contains(&effort))
        {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declarations_are_validated_when_compiling_embedded_endpoints() {
        for limits in [
            ModelLimits {
                allowed_effort: Some(vec![].into()),
                ..Default::default()
            },
            ModelLimits {
                allowed_effort: Some(vec![EffortLevel::High; 2].into()),
                ..Default::default()
            },
            ModelLimits {
                maximum_output_tokens: Some(0),
                ..Default::default()
            },
        ] {
            let mut endpoint = crate::topology::catalog::GOOGLE_BINDINGS[0].endpoint();
            *endpoint.representation.model_limits_mut() = limits;
            assert_eq!(
                endpoint.validate(),
                Err(crate::topology::TopologyError::ContractUnsatisfiable)
            );
        }
        let unbounded = ModelLimits::default();
        let limits = ModelLimits {
            allowed_effort: Some(vec![EffortLevel::Max].into()),
            maximum_output_tokens: Some(u64::MAX),
            ..Default::default()
        };
        assert_eq!(unbounded.narrowed(limits.clone()).unwrap(), limits);
        assert_eq!(limits.narrowed(ModelLimits::default()).unwrap(), limits);
    }

    #[test]
    fn planning_capacities_inherit_and_narrow_independently_without_enforcement() {
        let base = ModelLimits {
            context_window_tokens: Some(200_000),
            maximum_input_tokens: Some(180_000),
            ..Default::default()
        };
        assert_eq!(base.narrowed(ModelLimits::default()).unwrap(), base);
        let narrower = ModelLimits {
            context_window_tokens: Some(100_000),
            maximum_input_tokens: Some(90_000),
            ..Default::default()
        };
        assert_eq!(base.narrowed(narrower.clone()).unwrap(), narrower);
        assert!(narrower.narrowed(base).is_err());
        for invalid in [
            ModelLimits {
                context_window_tokens: Some(0),
                ..Default::default()
            },
            ModelLimits {
                maximum_input_tokens: Some(0),
                ..Default::default()
            },
        ] {
            assert!(invalid.validate().is_err());
        }
        // Planning fields do not turn output requests into input-token estimates.
        narrower
            .check_values(Some(300_000), std::iter::empty())
            .unwrap();
        let input_only = ModelLimits {
            maximum_input_tokens: Some(1_048_576),
            ..Default::default()
        };
        assert_eq!(input_only.context_window_tokens, None);
        assert_eq!(input_only.maximum_output_tokens, None);
    }
}
