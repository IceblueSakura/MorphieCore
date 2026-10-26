//! Trusted model/API sampling policy; never inferred from request model labels.
use crate::protocol::failure::CarrierReason;
use crate::{
    lowering::generation::Effect, protocol::CodecError,
    semantic::task::generation::GenerationRequest,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SamplingPolicy {
    /// Exact native controls, only where the selected API/model supports them.
    Native,
    Reject,
    /// The selected model ignores these controls; omit them with actual loss evidence.
    Ignored,
}

pub(super) fn project(
    request: &GenerationRequest,
    policy: SamplingPolicy,
    rule: &'static str,
) -> Result<Option<(GenerationRequest, Effect)>, CodecError> {
    request.validate()?;
    let controls = request.controls();
    let (temperature, top_p, top_k) = (
        controls.temperature().is_some(),
        controls.top_p().is_some(),
        controls.top_k.is_some(),
    );
    if !temperature && !top_p && !top_k || policy == SamplingPolicy::Native {
        return Ok(None);
    }
    if policy == SamplingPolicy::Reject {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    let mut settings = request.settings().clone();
    settings.controls = settings.controls.without_sampling();
    settings.controls.top_k = None;
    Ok(Some((
        request.clone().with_settings(settings)?,
        Effect::Sampling {
            rule: crate::lowering::generation::RuleId::new(rule),
            temperature,
            top_p,
            top_k,
            owner: crate::lowering::generation::Owner::Root,
        },
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::{
        task::generation::*,
        value::{Presence, Text},
    };

    #[test]
    fn ignored_sampling_receipt_identifies_only_changed_controls_and_keeps_other_settings() {
        let mut settings = GenerationSettings {
            instructions: Presence::Value(Text::new("go", "fixture", 32).unwrap().into()),
            ..Default::default()
        };
        settings.controls.top_k = Some(7);
        settings.controls.seed = Some(0);
        let source = GenerationRequest::from_settings(vec![], settings).unwrap();
        let original = source.clone();
        let (projected, loss) = project(&source, SamplingPolicy::Ignored, "fixture.omit-sampling")
            .unwrap()
            .unwrap();
        assert_eq!(
            loss,
            Effect::Sampling {
                rule: crate::lowering::generation::RuleId::new("fixture.omit-sampling"),
                temperature: false,
                top_p: false,
                top_k: true,
                owner: crate::lowering::generation::Owner::Root,
            }
        );
        assert_eq!(projected.controls().top_k, None);
        assert_eq!(projected.controls().seed, Some(0));
        assert_eq!(source, original);
        assert!(project(&source, SamplingPolicy::Reject, "fixture.omit-sampling").is_err());
        assert!(
            project(&source, SamplingPolicy::Native, "fixture.omit-sampling")
                .unwrap()
                .is_none()
        );
        assert!(
            project(&projected, SamplingPolicy::Ignored, "fixture.omit-sampling")
                .unwrap()
                .is_none()
        );
    }
}
