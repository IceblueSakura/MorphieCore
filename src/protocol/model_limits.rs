//! Trusted model-effective limits, checked after target-local control projection.
use crate::{
    protocol::CodecError,
    semantic::task::generation::{EffortLevel, GenerationRequest, Item},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ModelLimits {
    /// Absence declares no additional level restriction; it grants no carrier.
    pub allowed_effort: Option<Box<[EffortLevel]>>,
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
        }) || self.maximum_output_tokens == Some(0)
        {
            return Err(CodecError::Invalid("model value limits"));
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
        }) || configured.maximum_output_tokens.is_some_and(|maximum| {
            self.maximum_output_tokens
                .is_some_and(|base| maximum > base)
        }) {
            return Err(CodecError::Invalid("widened model value limits"));
        }
        Ok(Self {
            allowed_effort: configured
                .allowed_effort
                .or_else(|| self.allowed_effort.clone()),
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
            return Err(CodecError::Unsupported(
                "target maximum output tokens".into(),
            ));
        }
        if let Some(allowed) = &self.allowed_effort
            && efforts.into_iter().any(|effort| !allowed.contains(&effort))
        {
            return Err(CodecError::Unsupported("target effort".into()));
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
        };
        assert_eq!(unbounded.narrowed(limits.clone()).unwrap(), limits);
        assert_eq!(limits.narrowed(ModelLimits::default()).unwrap(), limits);
    }
}
