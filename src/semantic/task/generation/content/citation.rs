//! Owner-local claims and source dependencies; no native annotation DTOs.
use crate::semantic::task::generation::{
    GenerationError, MAX_TEXT_BYTES, ResourceDependency, ResourceId, ResourceLocation,
    ResourceTable,
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
/// An insertion boundary in one text part, not a span or annotation-list position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextPoint {
    pub unit: TextUnit,
    pub offset: usize,
}
impl TextPoint {
    pub fn validate(self, text: &str) -> Result<(), GenerationError> {
        TextRange {
            unit: self.unit,
            start: self.offset,
            end: self.offset,
        }
        .validate(text)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimAnchor {
    Unreported,
    Whole,
    Point(TextPoint),
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
        let source_bytes = match resources
            .get(value.source)
            .ok_or(GenerationError::InvalidCitation)?
        {
            ResourceLocation::Inline { .. } => 0,
            location => location.source_view().validate()?,
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
        match self.value.claim {
            ClaimAnchor::Point(point) => point.validate(text)?,
            ClaimAnchor::Range(range) => range.validate(text)?,
            ClaimAnchor::Unreported | ClaimAnchor::Whole => {}
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
        if matches!(self.value.claim, ClaimAnchor::Point(p) if p.offset > MAX_TEXT_BYTES) {
            return Err(GenerationError::Limit);
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
        Ok(())
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
