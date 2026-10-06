//! Media values are inert: validation never fetches, decodes pixels or uploads.
use super::{GenerationError, MAX_TEXT_BYTES};
use crate::semantic::value::Text;
use base64::engine::general_purpose::STANDARD;

pub const MAX_RESOURCE_URL_BYTES: usize = 8192;
pub const MAX_RESOURCE_MEDIA_TYPE_BYTES: usize = 256;
pub const MAX_IMAGE_DECODED_BYTES: usize = 768 * 1024;
pub const MAX_FILE_DECODED_BYTES: usize = 768 * 1024;
pub const MAX_TOTAL_FILE_DECODED_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_FILE_NAME_BYTES: usize = 1024;
pub use crate::semantic::value::ImageFormat;
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum ImageDetail {
    Auto,
    Low,
    High,
    Original,
}
impl ImageDetail {
    pub fn label(self) -> &'static str {
        self.into()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceKind {
    Image,
    Audio,
    File,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum FileDetail {
    Auto,
    Low,
    High,
}
impl FileDetail {
    pub fn label(self) -> &'static str {
        self.into()
    }
}
#[derive(Clone, Eq, PartialEq)]
pub struct FileDescription {
    /// A descriptive name, never a local path to open. Empty differs from omitted.
    pub filename: Option<Text>,
    /// Omitted differs from explicit auto; not an image option or lifecycle fact.
    pub detail: Option<FileDetail>,
}
impl std::fmt::Debug for FileDescription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileDescription")
            .field("filename", &self.filename.as_ref().map(|_| "[redacted]"))
            .field("detail", &self.detail)
            .finish()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResourceDescription {
    Image { detail: Option<ImageDetail> },
    Audio,
    File(FileDescription),
}
#[derive(Clone, Eq, PartialEq)]
pub enum ResourceLocation {
    Url(Text),
    Inline { media_type: Text, data_base64: Text },
    OpaqueReference(Text),
}
// URLs, query strings and inline bodies can contain private data.
impl std::fmt::Debug for ResourceLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Url(_) => "Url([redacted])",
            Self::Inline { .. } => "Inline([redacted])",
            Self::OpaqueReference(_) => "OpaqueReference([redacted])",
        })
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Resource {
    pub location: ResourceLocation,
    pub description: ResourceDescription,
}
impl Resource {
    pub fn kind(&self) -> ResourceKind {
        match self.description {
            ResourceDescription::Image { .. } => ResourceKind::Image,
            ResourceDescription::Audio => ResourceKind::Audio,
            ResourceDescription::File(_) => ResourceKind::File,
        }
    }
    pub fn image_detail(&self) -> Option<ImageDetail> {
        match self.description {
            ResourceDescription::Image { detail } => detail,
            _ => None,
        }
    }
    pub fn inline_decoded_bytes(&self) -> Result<Option<usize>, GenerationError> {
        let ResourceLocation::Inline { data_base64, .. } = &self.location else {
            return Ok(None);
        };
        if data_base64.as_str().len() > MAX_TEXT_BYTES {
            return Err(GenerationError::Limit);
        }
        let mut reader =
            base64::read::DecoderReader::new(data_base64.as_str().as_bytes(), &STANDARD);
        let decoded = std::io::copy(&mut reader, &mut std::io::sink())
            .map_err(|_| GenerationError::InvalidResource)?;
        usize::try_from(decoded)
            .map(Some)
            .map_err(|_| GenerationError::Limit)
    }
    /// Account encoded and decoded sizes separately, without allocating decoded bytes.
    /// This proves representation, not format validity, dimensions or model acceptance.
    pub fn validate(&self) -> Result<usize, GenerationError> {
        let description_bytes = match &self.description {
            ResourceDescription::Image { detail } => detail.map_or(0, |d| d.label().len()),
            ResourceDescription::Audio => 0,
            ResourceDescription::File(file) => {
                let filename_bytes = file.filename.as_ref().map_or(0, |n| n.as_str().len());
                if filename_bytes > MAX_FILE_NAME_BYTES {
                    return Err(GenerationError::Limit);
                }
                filename_bytes + file.detail.map_or(0, |d| d.label().len())
            }
        };
        let bytes = match &self.location {
            ResourceLocation::Url(text) => {
                let raw = text.as_str();
                if raw.len() > MAX_RESOURCE_URL_BYTES {
                    return Err(GenerationError::Limit);
                }
                let url = url::Url::parse(raw).map_err(|_| GenerationError::InvalidResource)?;
                if !raw.starts_with("https://") && !raw.starts_with("http://")
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || raw.contains('\\')
                    || raw.split_once("://").is_some_and(|(_, rest)| {
                        rest.split(['/', '?', '#'])
                            .next()
                            .is_some_and(|authority| authority.contains('@'))
                    })
                    || raw
                        .bytes()
                        .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
                {
                    return Err(GenerationError::InvalidResource);
                }
                raw.len()
            }
            ResourceLocation::Inline {
                media_type,
                data_base64,
            } => {
                let data = data_base64.as_str();
                if data.len() > MAX_TEXT_BYTES {
                    return Err(GenerationError::Limit);
                }
                if media_type.as_str().len() > MAX_RESOURCE_MEDIA_TYPE_BYTES {
                    return Err(GenerationError::Limit);
                }
                if self.kind() == ResourceKind::Image
                    && media_type.as_str().parse::<ImageFormat>().is_err()
                {
                    return Err(GenerationError::InvalidResource);
                }
                if self.kind() == ResourceKind::File {
                    let mime = media_type
                        .as_str()
                        .parse::<mime::Mime>()
                        .map_err(|_| GenerationError::InvalidResource)?;
                    if mime.type_() == mime::STAR || mime.subtype() == mime::STAR {
                        return Err(GenerationError::InvalidResource);
                    }
                }
                if data.is_empty() {
                    return Err(GenerationError::InvalidResource);
                }
                // Write decoded bytes to a counter, not a pixel buffer or a second payload.
                let decoded = self
                    .inline_decoded_bytes()?
                    .ok_or(GenerationError::InvalidResource)?;
                let limit = match self.kind() {
                    ResourceKind::File => MAX_FILE_DECODED_BYTES,
                    ResourceKind::Image | ResourceKind::Audio => MAX_IMAGE_DECODED_BYTES,
                };
                if decoded > limit {
                    return Err(GenerationError::Limit);
                }
                // Include the data URL header in the aggregate semantic budget.
                media_type.as_str().len() + data.len() + "data:;base64,".len()
            }
            ResourceLocation::OpaqueReference(id) => {
                if id.as_str().is_empty() || id.as_str().len() > 256 {
                    return Err(GenerationError::InvalidResource);
                }
                id.as_str().len()
            }
        };
        Ok(bytes + description_bytes)
    }
}
