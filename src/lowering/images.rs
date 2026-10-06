//! Explicit standard-Images accounting projection, never a mutation of source reports.
use crate::semantic::{
    task::image_generation::{
        ImageBilling, ImageError, ImageGenerationResponse, ImageTokenBreakdown, ImageUsage,
    },
    value::Presence,
};
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AccountingPolicy {
    #[default]
    Strict,
    /// Only usage that lacks a standard carrier and billing may be omitted.
    OmitUnrepresentableAccounting,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AccountingLoss {
    pub usage_omitted: bool,
    pub billing_omitted: bool,
}
pub struct ImageProjection {
    pub response: ImageGenerationResponse,
    pub loss: AccountingLoss,
}
fn standard_details(details: &ImageTokenBreakdown, total: u64) -> bool {
    matches!((details.text.value(),details.image.value()),(Some(a),Some(b)) if a.checked_add(*b)==Some(total))
        && [
            &details.audio,
            &details.file,
            &details.video,
            &details.cached,
            &details.cache_write,
            &details.reasoning,
        ]
        .iter()
        .all(|v| v.is_absent())
}
pub(crate) fn standard_tokens_representable(usage: &ImageUsage) -> bool {
    usage
        .input_details
        .value()
        .is_some_and(|d| standard_details(d, usage.input))
        && usage
            .output_details
            .value()
            .is_none_or(|d| standard_details(d, usage.output))
}
pub fn project_response(
    response: &ImageGenerationResponse,
    policy: AccountingPolicy,
) -> Result<ImageProjection, ImageError> {
    response.validate()?;
    let loss = response
        .usage
        .value()
        .map_or(AccountingLoss::default(), |u| AccountingLoss {
            usage_omitted: !standard_tokens_representable(u),
            billing_omitted: !u.billing.is_absent(),
        });
    if policy == AccountingPolicy::Strict && loss != AccountingLoss::default() {
        return Err(ImageError);
    }
    let mut projected = response.clone();
    if loss.usage_omitted {
        projected.usage = Presence::Absent;
    } else if let Presence::Value(usage) = &mut projected.usage {
        usage.billing = ImageBilling::default();
    }
    projected.validate()?;
    Ok(ImageProjection {
        response: projected,
        loss,
    })
}
