//! Value-sensitive target restrictions, independent of the model's vision semantics.
use crate::semantic::task::generation::{
    GenerationRequest, ImageDetail, ImageFormat, ImageSizePolicy, InputPart, Item, ItemId,
    MAX_IMAGE_DECODED_BYTES, ResourceDescription, ResourceKind, ResourceLocation, ToolOutput,
};
/// Targets without an exact size-policy carrier must reject this guarantee.
pub(crate) fn rejects_downsize(items: &[(ItemId, Item)]) -> bool {
    let strict = |description: &ResourceDescription| {
        matches!(
            description,
            ResourceDescription::Image {
                size_policy: ImageSizePolicy::RejectOversized,
                ..
            }
        )
    };
    items.iter().any(|(_, item)| {
        item.resource_uses().any(|r| strict(&r.description))
            || matches!(item, Item::ProviderTool(observed)
                if matches!(&observed.output, Some(ToolOutput::Parts(parts))
                    if parts.iter().any(|p|
                        matches!(&p.content, InputPart::Resource(r) if strict(&r.description)))))
    })
}
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
        let mut check = |resource: crate::semantic::task::generation::ResourceView<'_>| {
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
                ResourceLocation::OpaqueReference(_)
                | ResourceLocation::NamespacedReference { .. } => false,
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
        for resource in request.items().iter().flat_map(|(_, i)| i.resource_uses()) {
            check(resource.media(request.resources())?)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::{task::generation::*, value::Text};

    #[test]
    fn size_policy_checks_provider_output_uses_without_part_id_sidecars() {
        let text = |s: &str| Text::new(s, "fixture", 256).unwrap();
        let observation = |size_policy| {
            Item::ProviderTool(ProviderToolObservation {
                source: NativeAliasDomain {
                    source: text("synthetic"),
                    scope: LocalScope::ROOT,
                },
                operation: ProviderOperation::Reported {
                    tool: text("search"),
                    alias: None,
                    requester: ProviderRequester::Unreported,
                    action: None,
                },
                progress: None,
                execution: None,
                artifact_status: Some(ItemLifecycle::Completed),
                output: Some(ToolOutput::Parts(vec![Part::new(
                    PartId::new(1),
                    InputPart::Resource(ResourceUse {
                        id: ResourceId::new(1),
                        description: ResourceDescription::Image {
                            detail: None,
                            size_policy,
                        },
                    }),
                )])),
            })
        };
        assert!(!rejects_downsize(&[(
            ItemId::new(1),
            observation(ImageSizePolicy::AllowDownsize)
        )]));
        assert!(rejects_downsize(&[(
            ItemId::new(1),
            observation(ImageSizePolicy::RejectOversized)
        )]));
    }
}
