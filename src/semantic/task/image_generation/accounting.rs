//! Final-operation image accounting: missing modality reports remain unknown.
use super::ImageError;
use crate::semantic::value::Presence;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ImageTokenBreakdown {
    pub text: Presence<u64>,
    pub image: Presence<u64>,
    pub audio: Presence<u64>,
    pub file: Presence<u64>,
    pub video: Presence<u64>,
    /// Cache and reasoning are subsets, not additional modalities to sum.
    pub cached: Presence<u64>,
    pub cache_write: Presence<u64>,
    pub reasoning: Presence<u64>,
}
impl ImageTokenBreakdown {
    pub fn text_image(text: u64, image: u64) -> Self {
        Self {
            text: Presence::Value(text),
            image: Presence::Value(image),
            ..Self::default()
        }
    }
    pub fn validate(&self, total: u64) -> Result<(), ImageError> {
        if [
            &self.text,
            &self.image,
            &self.audio,
            &self.file,
            &self.video,
            &self.cached,
            &self.cache_write,
            &self.reasoning,
        ]
        .iter()
        .any(|n| n.value().is_some_and(|n| *n > total))
        {
            return Err(ImageError);
        }
        // Text and image partitions cannot exceed their owning count; other subsets may overlap.
        let sum = self
            .text
            .value()
            .copied()
            .unwrap_or(0)
            .checked_add(self.image.value().copied().unwrap_or(0))
            .ok_or(ImageError)?;
        if sum > total {
            return Err(ImageError);
        }
        Ok(())
    }
}
/// Exact reported USD amount. Floating-point conversion is only a bounded validation view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsdAmount(serde_json::Number);
impl UsdAmount {
    pub fn new(value: serde_json::Number) -> Result<Self, ImageError> {
        let text = value.to_string();
        if text.len() > 128
            || text.starts_with('-')
            || value.as_f64().is_none_or(|v| !v.is_finite())
        {
            return Err(ImageError);
        }
        Ok(Self(value))
    }
    pub fn amount(&self) -> &serde_json::Number {
        &self.0
    }
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ImageCostBreakdown {
    pub upstream_total: Presence<UsdAmount>,
    pub upstream_input: Presence<UsdAmount>,
    pub upstream_output: Presence<UsdAmount>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ImageBilling {
    pub cost: Presence<UsdAmount>,
    pub upstream: Presence<ImageCostBreakdown>,
    pub byok: Presence<bool>,
}
impl ImageBilling {
    pub fn is_absent(&self) -> bool {
        self.cost.is_absent() && self.upstream.is_absent() && self.byok.is_absent()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageUsage {
    pub input: u64,
    pub input_details: Presence<ImageTokenBreakdown>,
    pub output: u64,
    pub output_details: Presence<ImageTokenBreakdown>,
    pub total: u64,
    pub billing: ImageBilling,
}
impl ImageUsage {
    pub fn validate(&self) -> Result<(), ImageError> {
        if self.input.checked_add(self.output) != Some(self.total) {
            return Err(ImageError);
        }
        if let Some(details) = self.input_details.value() {
            details.validate(self.input)?;
        }
        if let Some(details) = self.output_details.value() {
            details.validate(self.output)?;
        }
        Ok(())
    }
}
