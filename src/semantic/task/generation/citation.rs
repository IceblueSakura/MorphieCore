//! Owner-local claims and source dependencies; no native annotation DTOs.
use super::{
    GenerationError, MAX_TEXT_BYTES, ResourceBody, ResourceDependency, ResourceId,
    ResourceLocation, ResourceTable,
};
use crate::semantic::value::Text;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextUnit {
    Utf8Bytes,
    UnicodeScalars,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextRange {
    pub unit: TextUnit,
    pub start: usize,
    pub end: usize,
}
impl TextRange {
    pub fn validate(self, text: &str) -> Result<(), GenerationError> {
        let valid = self.start <= self.end
            && match self.unit {
                TextUnit::Utf8Bytes => {
                    self.end <= text.len()
                        && text.is_char_boundary(self.start)
                        && text.is_char_boundary(self.end)
                }
                TextUnit::UnicodeScalars => self.end <= text.chars().count(),
            };
        if valid {
            Ok(())
        } else {
            Err(GenerationError::InvalidCitation)
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimAnchor {
    Unreported,
    Whole,
    Range(TextRange),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceCoordinates {
    Unreported,
    Text(TextRange),
    /// One-based, exclusive end.
    Pages {
        start: usize,
        end: usize,
    },
    /// Zero-based, exclusive end; never inferred from newlines.
    Blocks {
        start: usize,
        end: usize,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceBounds {
    Verified,
    Unreported,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CitationKind {
    Citation,
    ArtifactReference,
}
#[derive(Clone, Eq, PartialEq)]
pub struct Citation {
    pub kind: CitationKind,
    pub claim: ClaimAnchor,
    pub source: ResourceId,
    pub coordinates: SourceCoordinates,
    pub label: Option<Text>,
    /// Reported list position, not an output offset or a resource identity.
    pub reported_ordinal: Option<usize>,
}
impl std::fmt::Debug for Citation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Citation")
            .field("kind", &self.kind)
            .field("claim", &self.claim)
            .field("source", &self.source)
            .field("coordinates", &self.coordinates)
            .field("ordinal", &self.reported_ordinal)
            .field("label", &self.label.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Annotation {
    value: Citation,
    source: ResourceDependency,
    // Accounting only, not another source payload; each wire use can repeat a locator.
    source_bytes: usize,
}
impl Annotation {
    pub fn new(value: Citation, resources: &ResourceTable) -> Result<Self, GenerationError> {
        let source = ResourceDependency::capture(resources, value.source)?;
        let source_bytes = match &resources
            .get(value.source)
            .ok_or(GenerationError::InvalidCitation)?
            .body
        {
            ResourceBody::Media(
                location @ (ResourceLocation::Url(_)
                | ResourceLocation::OpaqueReference(_)
                | ResourceLocation::NamespacedReference { .. }),
            ) => location.source_view().validate()?,
            _ => 0,
        };
        let result = Self {
            value,
            source,
            source_bytes,
        };
        result.check_source(resources)?;
        Ok(result)
    }
    pub fn value(&self) -> &Citation {
        &self.value
    }
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            .saturating_add(self.source_bytes)
            .saturating_add(self.value.label.as_ref().map_or(0, |v| v.as_str().len()))
    }
    pub fn validate_claim(&self, text: &str) -> Result<(), GenerationError> {
        if self.bytes() > MAX_TEXT_BYTES {
            return Err(GenerationError::Limit);
        }
        if let ClaimAnchor::Range(range) = self.value.claim {
            range.validate(text)?;
        }
        Ok(())
    }
    pub fn check_source(&self, resources: &ResourceTable) -> Result<(), GenerationError> {
        self.source.check(resources)?;
        if self.bytes() > MAX_TEXT_BYTES {
            return Err(GenerationError::Limit);
        }
        if matches!(self.value.claim, ClaimAnchor::Range(r) if r.start > r.end) {
            return Err(GenerationError::InvalidCitation);
        }
        match self.value.coordinates {
            SourceCoordinates::Text(range) if range.start > range.end => {
                return Err(GenerationError::InvalidCitation);
            }
            SourceCoordinates::Pages { start, end } if start == 0 || start >= end => {
                return Err(GenerationError::InvalidCitation);
            }
            SourceCoordinates::Blocks { start, end } if start >= end => {
                return Err(GenerationError::InvalidCitation);
            }
            _ => {}
        }
        if let SourceCoordinates::Text(range) = self.value.coordinates
            && let Some(source) = resources.get(self.value.source)
            && let ResourceBody::Text(text) = &source.body
        {
            range.validate(text.as_str())?;
        }
        Ok(())
    }
    pub fn source_bounds(
        &self,
        resources: &ResourceTable,
    ) -> Result<SourceBounds, GenerationError> {
        self.check_source(resources)?;
        Ok(
            if matches!(self.value.coordinates, SourceCoordinates::Text(_))
                && matches!(
                    resources.get(self.value.source).map(|s| &s.body),
                    Some(ResourceBody::Text(_))
                )
            {
                SourceBounds::Verified
            } else {
                SourceBounds::Unreported
            },
        )
    }
    pub(crate) fn fingerprint(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        hash.update(format!("{self:?}").as_bytes());
        if let Some(label) = &self.value.label {
            hash.update(label.as_str().as_bytes());
        }
        hash.finalize().into()
    }
}
