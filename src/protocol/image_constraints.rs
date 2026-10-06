//! Value-sensitive target restrictions, independent of the model's vision semantics.
use crate::semantic::task::generation::{
    ContentPart, GenerationRequest, ImageDetail, ImageFormat, Item, MAX_IMAGE_DECODED_BYTES,
    Resource, ResourceKind, ResourceLocation, ToolOutput, ToolResultPart,
};
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageConstraints {
    pub urls: bool,
    pub inline_formats: Vec<ImageFormat>,
    pub details: Vec<ImageDetail>,
    pub max_images: usize,
    pub max_inline_bytes: usize,
}
impl Default for ImageConstraints {
    fn default() -> Self {
        Self::common()
    }
}
impl ImageConstraints {
    pub(crate) fn intersect(&mut self, profile: &Self) {
        self.urls &= profile.urls;
        self.inline_formats
            .retain(|f| profile.inline_formats.contains(f));
        self.details.retain(|d| profile.details.contains(d));
        self.max_images = self.max_images.min(profile.max_images);
        self.max_inline_bytes = self.max_inline_bytes.min(profile.max_inline_bytes);
    }
    pub fn all() -> Self {
        Self {
            urls: true,
            inline_formats: vec![
                ImageFormat::Png,
                ImageFormat::Jpeg,
                ImageFormat::Gif,
                ImageFormat::Webp,
                ImageFormat::Bmp,
            ],
            details: vec![
                ImageDetail::Auto,
                ImageDetail::Low,
                ImageDetail::High,
                ImageDetail::Original,
            ],
            max_images: 1024,
            max_inline_bytes: MAX_IMAGE_DECODED_BYTES,
        }
    }
    pub fn common() -> Self {
        let mut value = Self::all();
        value.inline_formats.retain(|f| *f != ImageFormat::Bmp);
        value
    }
    pub(crate) fn check(
        &self,
        request: &GenerationRequest,
    ) -> Result<(), crate::semantic::task::generation::GenerationError> {
        let mut count = 0;
        let mut check = |resource: &Resource| {
            if resource.kind() != ResourceKind::Image {
                return Ok(());
            }
            count += 1;
            let source_ok = match &resource.location {
                ResourceLocation::Url(_) => self.urls,
                ResourceLocation::Inline { media_type, .. } => {
                    media_type
                        .as_str()
                        .parse::<ImageFormat>()
                        .is_ok_and(|format| self.inline_formats.contains(&format))
                        && resource
                            .inline_decoded_bytes()?
                            .is_some_and(|n| n <= self.max_inline_bytes)
                }
                ResourceLocation::OpaqueReference(_) => false,
            };
            if count > self.max_images
                || !source_ok
                || resource
                    .image_detail()
                    .is_some_and(|d| !self.details.contains(&d))
            {
                return Err(crate::semantic::task::generation::GenerationError::InvalidResource);
            }
            Ok(())
        };
        for (_, item) in request.items() {
            match item {
                Item::Message(message) => {
                    for part in &message.parts {
                        if let ContentPart::Resource(resource) = &part.content {
                            check(resource)?;
                        }
                    }
                }
                Item::ToolResult(result) | Item::CustomResult(result) => {
                    if let ToolOutput::Parts(parts) = &result.output {
                        for (_, part) in parts {
                            if let ToolResultPart::Resource(resource) = part {
                                check(resource)?;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}
