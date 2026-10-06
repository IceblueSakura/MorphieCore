//! Independent, static ordered image task. No conversation, runtime target or file service.
use crate::semantic::value::{ImageFormat, Presence};
use base64::engine::general_purpose::STANDARD;
mod controls;
pub use controls::{
    ImageBackgroundRequest, ImageModeration, ImageQualityRequest, ImageSizeRequest,
};

pub const MAX_PROMPT_CHARS: usize = 32_000;
pub const MAX_PROMPT_BYTES: usize = MAX_PROMPT_CHARS * 4;
pub const MAX_IMAGE_BYTES: usize = 8 << 20;
pub const MAX_IMAGE_BASE64_BYTES: usize = MAX_IMAGE_BYTES.div_ceil(3) * 4;
pub const MAX_IMAGE_COUNT: u8 = 10;
/// Fixed aggregate decoded budget, never multiplied by the requested count.
pub const MAX_IMAGES_BYTES: usize = 8 << 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("invalid or over-budget image generation value")]
pub struct ImageError;

#[derive(Clone, Eq, PartialEq)]
pub struct ImageGenerationRequest {
    prompt: String,
    /// Exact delivery requirement; absent/null means one, not an upstream guarantee.
    pub count: Presence<u8>,
    pub format: Presence<ImageFormat>,
    pub size: Presence<ImageSizeRequest>,
    pub quality: Presence<ImageQualityRequest>,
    pub background: Presence<ImageBackgroundRequest>,
    pub compression: Presence<u8>,
    pub moderation: Presence<ImageModeration>,
}
impl std::fmt::Debug for ImageGenerationRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageGenerationRequest")
            .field("prompt", &"[redacted]")
            .field("count", &self.count)
            .field("format", &self.format)
            .field("size", &self.size)
            .field("quality", &self.quality)
            .field("background", &self.background)
            .field("compression", &self.compression)
            .field("moderation", &self.moderation)
            .finish()
    }
}
impl ImageGenerationRequest {
    pub fn new(prompt: impl Into<String>) -> Result<Self, ImageError> {
        let value = Self {
            prompt: prompt.into(),
            count: Presence::Absent,
            format: Presence::Absent,
            size: Presence::Absent,
            quality: Presence::Absent,
            background: Presence::Absent,
            compression: Presence::Absent,
            moderation: Presence::Absent,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn prompt(&self) -> &str {
        &self.prompt
    }
    pub fn requested_count(&self) -> u8 {
        self.count.value().copied().unwrap_or(1)
    }
    pub fn set_prompt(&mut self, prompt: impl Into<String>) -> Result<(), ImageError> {
        let next = Self::new(prompt)?;
        self.prompt = next.prompt;
        Ok(())
    }
    pub fn validate(&self) -> Result<(), ImageError> {
        if self.prompt.is_empty()
            || self.prompt.len() > MAX_PROMPT_BYTES
            || self.prompt.chars().count() > MAX_PROMPT_CHARS
            || !(1..=MAX_IMAGE_COUNT).contains(&self.requested_count())
            || self.format.value().is_some_and(|f| {
                !matches!(f, ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Webp)
            })
        {
            return Err(ImageError);
        }
        if let Some(ImageSizeRequest::Exact(size)) = self.size.value() {
            size.validate()?;
        }
        if self.compression.value().is_some_and(|v| *v > 100)
            || self.compression.value().is_some()
                && self.format == Presence::Value(ImageFormat::Png)
            || self.background
                == Presence::Value(ImageBackgroundRequest::Exact(ImageBackground::Transparent))
                && self.format == Presence::Value(ImageFormat::Jpeg)
        {
            return Err(ImageError);
        }
        Ok(())
    }
}

/// Encoded bytes are inert. This checks Base64 and budgets, not pixels or MIME truth.
#[derive(Clone, Eq, PartialEq)]
pub struct ImageData {
    encoded: String,
    decoded_bytes: usize,
}
impl std::fmt::Debug for ImageData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ImageData([redacted])")
    }
}
impl ImageData {
    pub fn new(data: &str) -> Result<Self, ImageError> {
        if data.is_empty() || data.len() > MAX_IMAGE_BASE64_BYTES {
            return Err(ImageError);
        }
        let mut reader = base64::read::DecoderReader::new(data.as_bytes(), &STANDARD);
        let bytes = std::io::copy(&mut reader, &mut std::io::sink()).map_err(|_| ImageError)?;
        if bytes == 0 || bytes > MAX_IMAGE_BYTES as u64 {
            return Err(ImageError);
        }
        Ok(Self {
            encoded: data.into(),
            decoded_bytes: usize::try_from(bytes).map_err(|_| ImageError)?,
        })
    }
    pub fn as_base64(&self) -> &str {
        &self.encoded
    }
    pub fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageSize {
    pub width: u32,
    pub height: u32,
}
impl ImageSize {
    pub fn validate(self) -> Result<(), ImageError> {
        if self.width == 0 || self.height == 0 || self.width > 65_536 || self.height > 65_536 {
            Err(ImageError)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum ImageBackground {
    Transparent,
    Opaque,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum ImageQuality {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedImage {
    pub data: ImageData,
    /// Reported facts only. A request default does not prove an artifact's format.
    pub format: Presence<ImageFormat>,
    pub size: Presence<ImageSize>,
    pub background: Presence<ImageBackground>,
    pub quality: Presence<ImageQuality>,
}
impl GeneratedImage {
    pub fn new(data: ImageData) -> Self {
        Self {
            data,
            format: Presence::Absent,
            size: Presence::Absent,
            background: Presence::Absent,
            quality: Presence::Absent,
        }
    }
    pub fn validate(&self) -> Result<(), ImageError> {
        if self
            .format
            .value()
            .is_some_and(|f| !matches!(f, ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Webp))
            || self.background == Presence::Value(ImageBackground::Transparent)
                && self.format == Presence::Value(ImageFormat::Jpeg)
        {
            return Err(ImageError);
        }
        if let Some(size) = self.size.value() {
            size.validate()?;
        }
        Ok(())
    }
    /// Standard top-level reports must describe every artifact, including presence.
    pub fn same_reports(&self, other: &Self) -> bool {
        self.format == other.format
            && self.size == other.size
            && self.background == other.background
            && self.quality == other.quality
    }
    fn validate_for(&self, request: &ImageGenerationRequest) -> Result<(), ImageError> {
        if let (Some(expected), Some(reported)) = (request.format.value(), self.format.value())
            && expected != reported
        {
            return Err(ImageError);
        }
        if let (Some(ImageSizeRequest::Exact(expected)), Some(reported)) =
            (request.size.value(), self.size.value())
            && expected != reported
        {
            return Err(ImageError);
        }
        if let (Some(ImageQualityRequest::Exact(expected)), Some(reported)) =
            (request.quality.value(), self.quality.value())
            && expected != reported
        {
            return Err(ImageError);
        }
        if let (Some(ImageBackgroundRequest::Exact(expected)), Some(reported)) =
            (request.background.value(), self.background.value())
            && expected != reported
        {
            return Err(ImageError);
        }
        if request.background
            == Presence::Value(ImageBackgroundRequest::Exact(ImageBackground::Transparent))
            && self.format == Presence::Value(ImageFormat::Jpeg)
        {
            return Err(ImageError);
        }
        Ok(())
    }
}
mod accounting;
pub use accounting::{
    ImageBilling, ImageCostBreakdown, ImageTokenBreakdown, ImageUsage, UsdAmount,
};
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageGenerationResponse {
    pub created: u64,
    pub images: Vec<GeneratedImage>,
    pub usage: Presence<ImageUsage>,
}
impl ImageGenerationResponse {
    /// Missing reports stay unknown; a reported contradiction cannot satisfy an explicit intent.
    pub fn validate_for(&self, request: &ImageGenerationRequest) -> Result<(), ImageError> {
        request.validate()?;
        self.validate()?;
        if self.images.len() != usize::from(request.requested_count()) {
            return Err(ImageError);
        }
        for image in &self.images {
            image.validate_for(request)?;
        }
        Ok(())
    }
    pub fn new(created: u64, images: Vec<GeneratedImage>) -> Result<Self, ImageError> {
        let response = Self {
            created,
            images,
            usage: Presence::Absent,
        };
        response.validate()?;
        Ok(response)
    }
    pub fn decoded_bytes(&self) -> Result<usize, ImageError> {
        self.images.iter().try_fold(0usize, |total, image| {
            total
                .checked_add(image.data.decoded_bytes())
                .ok_or(ImageError)
        })
    }
    pub fn validate(&self) -> Result<(), ImageError> {
        if !(1..=usize::from(MAX_IMAGE_COUNT)).contains(&self.images.len())
            || self.decoded_bytes()? > MAX_IMAGES_BYTES
        {
            return Err(ImageError);
        }
        for image in &self.images {
            image.validate()?;
        }
        if let Some(usage) = self.usage.value() {
            usage.validate()?;
        }
        Ok(())
    }
}
