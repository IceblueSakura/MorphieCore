//! File-source target limits, independent of image budgets and model admission.
use crate::semantic::task::generation::{
    ContentPart, FileDetail, GenerationError, GenerationRequest, Item, MAX_FILE_DECODED_BYTES,
    MAX_ITEMS, MAX_TOTAL_FILE_DECODED_BYTES, ResourceDescription, ResourceLocation,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileConstraints {
    /// URL acceptance is independent of inline MIME limits; remote bytes are unknown.
    pub urls: bool,
    /// None admits any concrete MIME label; Some is an explicit target allowlist.
    pub inline_media_types: Option<Vec<String>>,
    pub details: Vec<FileDetail>,
    pub require_filename: bool,
    pub max_files: usize,
    pub max_inline_bytes: usize,
    pub max_total_inline_bytes: usize,
}
impl Default for FileConstraints {
    fn default() -> Self {
        Self::all()
    }
}
impl FileConstraints {
    pub fn all() -> Self {
        Self {
            urls: true,
            inline_media_types: None,
            details: vec![FileDetail::Auto, FileDetail::Low, FileDetail::High],
            require_filename: false,
            max_files: MAX_ITEMS,
            max_inline_bytes: MAX_FILE_DECODED_BYTES,
            max_total_inline_bytes: MAX_TOTAL_FILE_DECODED_BYTES,
        }
    }
    /// Conservative PDF slice within the endpoint's existing 256 KiB body budget.
    /// These are local admission limits, not claims about an upstream maximum.
    pub fn pdf() -> Self {
        Self {
            urls: false,
            inline_media_types: Some(vec!["application/pdf".into()]),
            max_files: 4,
            max_inline_bytes: 128 * 1024,
            max_total_inline_bytes: 128 * 1024,
            ..Self::all()
        }
    }
    /// A dialect without declared file admission cannot gain it from a generic contract.
    pub fn none() -> Self {
        Self {
            urls: false,
            max_files: 0,
            ..Self::all()
        }
    }
    pub(crate) fn intersect(&mut self, profile: &Self) {
        self.urls &= profile.urls;
        if let Some(allowed) = &profile.inline_media_types {
            match &mut self.inline_media_types {
                Some(current) => current.retain(|m| allowed.contains(m)),
                None => self.inline_media_types = Some(allowed.clone()),
            }
        }
        self.details.retain(|d| profile.details.contains(d));
        self.require_filename |= profile.require_filename;
        self.max_files = self.max_files.min(profile.max_files);
        self.max_inline_bytes = self.max_inline_bytes.min(profile.max_inline_bytes);
        self.max_total_inline_bytes = self
            .max_total_inline_bytes
            .min(profile.max_total_inline_bytes);
    }
    pub(crate) fn check(&self, request: &GenerationRequest) -> Result<(), GenerationError> {
        let mut count = 0;
        let mut total = 0usize;
        for (_, item) in request.items() {
            let Item::Message(message) = item else {
                continue;
            };
            for part in &message.parts {
                let ContentPart::Resource(resource) = &part.content else {
                    continue;
                };
                let ResourceDescription::File(file) = &resource.description else {
                    continue;
                };
                count += 1;
                let source_ok = match &resource.location {
                    ResourceLocation::Url(_) => self.urls,
                    ResourceLocation::Inline { media_type, .. } => {
                        let bytes = resource
                            .inline_decoded_bytes()?
                            .ok_or(GenerationError::InvalidResource)?;
                        total = total.checked_add(bytes).ok_or(GenerationError::Limit)?;
                        bytes <= self.max_inline_bytes
                            && total <= self.max_total_inline_bytes
                            && self
                                .inline_media_types
                                .as_ref()
                                .is_none_or(|types| types.iter().any(|t| t == media_type.as_str()))
                    }
                    ResourceLocation::OpaqueReference(_) => false,
                };
                if count > self.max_files
                    || !source_ok
                    || file.detail.is_some_and(|d| !self.details.contains(&d))
                    || self.require_filename
                        && file.filename.as_ref().is_none_or(|n| n.as_str().is_empty())
                {
                    return Err(GenerationError::InvalidResource);
                }
            }
        }
        Ok(())
    }
}
