//! Native `image` and `document` blocks and their mapping to IR resources.
//! Messages has no audio or video carrier. Provider `file` sources and `content`
//! documents name credential-owned state or nested blocks, so they are rejected.
//! Source: docs/refs/multimodal.md.
use super::shape::*;
use crate::{
    protocol::CodecError,
    semantic::{task::generation::*, value::Text},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

const IMAGE_TYPES: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];
const PDF: &str = "application/pdf";
const PLAIN_TEXT: &str = "text/plain";

#[derive(Clone, Eq, PartialEq)]
pub enum Source {
    Base64 {
        media_type: String,
        data: String,
    },
    Url(String),
    /// A `text/plain` document body, sent as text rather than base64.
    Text(String),
}
#[derive(Clone, Eq, PartialEq)]
pub enum Media {
    Image(Source),
    Document(Source),
}
impl Media {
    pub(super) fn read(kind: &str, v: &Value) -> Result<Self, CodecError> {
        let o = object(v)?;
        // These change what the model sees or returns and have no IR slot.
        for key in ["title", "context", "citations"] {
            if o.get(key).is_some_and(|v| !v.is_null()) {
                return Err(CodecError::Unsupported(format!("Anthropic document {key}")));
            }
        }
        let source = object(required(o, "source")?)?;
        let field = |key| required(source, key).and_then(string);
        let source = match required(source, "type")?.as_str() {
            Some("base64") => Source::Base64 {
                media_type: field("media_type")?,
                data: field("data")?,
            },
            Some("url") => Source::Url(field("url")?),
            Some("text") if kind == "document" => {
                if field("media_type")? != PLAIN_TEXT {
                    return Err(CodecError::Invalid("Anthropic text document type"));
                }
                Source::Text(field("data")?)
            }
            _ => return Err(CodecError::Unsupported("Anthropic media source".into())),
        };
        let media = match kind {
            "image" => Self::Image(source),
            _ => Self::Document(source),
        };
        media.validate()?;
        Ok(media)
    }
    fn validate(&self) -> Result<(), CodecError> {
        match self {
            Self::Image(Source::Base64 { media_type, .. })
                if !IMAGE_TYPES.contains(&&**media_type) =>
            {
                Err(CodecError::Unsupported("Anthropic image type".into()))
            }
            Self::Document(Source::Base64 { media_type, .. }) if media_type != PDF => {
                Err(CodecError::Unsupported("Anthropic document type".into()))
            }
            Self::Image(Source::Text(_)) => Err(CodecError::Invalid("Anthropic image source")),
            _ => Ok(()),
        }
    }
    pub(super) fn bytes(&self) -> Result<usize, CodecError> {
        let (Self::Image(source) | Self::Document(source)) = self;
        self.validate()?;
        Ok(match source {
            Source::Base64 { media_type, data } => text(media_type)? + text(data)?,
            Source::Url(url) | Source::Text(url) => text(url)?,
        })
    }
    pub(super) fn wire(&self) -> Value {
        let (kind, source) = match self {
            Self::Image(source) => ("image", source),
            Self::Document(source) => ("document", source),
        };
        let source = match source {
            Source::Base64 { media_type, data } => {
                json!({"type":"base64","media_type":media_type,"data":data})
            }
            Source::Url(url) => json!({"type":"url","url":url}),
            Source::Text(data) => json!({"type":"text","media_type":PLAIN_TEXT,"data":data}),
        };
        json!({"type":kind,"source":source})
    }
    /// The IR view: a text document becomes an inline `text/plain` file.
    pub(super) fn resource(&self) -> Result<Resource, CodecError> {
        let text =
            |s: &str, kind| Text::new(s, kind, MAX_TEXT_BYTES).map_err(|_| CodecError::Limit);
        let (source, description) = match self {
            Self::Image(source) => (source, ResourceDescription::Image { detail: None }),
            Self::Document(source) => (
                source,
                ResourceDescription::File(FileDescription {
                    filename: None,
                    detail: None,
                }),
            ),
        };
        let location = match source {
            Source::Base64 { media_type, data } => ResourceLocation::Inline {
                media_type: text(media_type, "media type")?,
                data_base64: text(data, "media data")?,
            },
            Source::Url(url) => ResourceLocation::Url(text(url, "media URL")?),
            Source::Text(body) => ResourceLocation::Inline {
                media_type: text(PLAIN_TEXT, "media type")?,
                data_base64: text(&STANDARD.encode(body), "media data")?,
            },
        };
        let resource = Resource {
            location,
            description,
        };
        resource.validate()?;
        Ok(resource)
    }
    /// The native carrier for one IR resource. `omitted` reports a dropped per-use option.
    pub(super) fn from_resource(
        resource: ResourceView<'_>,
        omitted: &mut bool,
    ) -> Result<Self, CodecError> {
        let unsupported = || CodecError::Unsupported("Anthropic media".into());
        *omitted |= match resource.description {
            ResourceDescription::Image { detail } => detail.is_some(),
            ResourceDescription::File(file) => file.filename.is_some() || file.detail.is_some(),
            _ => false,
        };
        let source = match resource.location {
            // Anthropic fetches URL sources and validates their type itself.
            ResourceLocation::Url(url) => Source::Url(url.as_str().into()),
            ResourceLocation::Inline {
                media_type,
                data_base64,
            } => match (resource.kind(), media_type.as_str()) {
                (ResourceKind::Image, t) if IMAGE_TYPES.contains(&t) => Source::Base64 {
                    media_type: t.into(),
                    data: data_base64.as_str().into(),
                },
                (ResourceKind::File, PDF) => Source::Base64 {
                    media_type: PDF.into(),
                    data: data_base64.as_str().into(),
                },
                (ResourceKind::File, PLAIN_TEXT) => Source::Text(
                    String::from_utf8(
                        STANDARD
                            .decode(data_base64.as_str())
                            .map_err(|_| CodecError::Invalid("text document data"))?,
                    )
                    .map_err(|_| unsupported())?,
                ),
                _ => return Err(unsupported()),
            },
            _ => return Err(unsupported()),
        };
        match resource.kind() {
            ResourceKind::Image => Ok(Self::Image(source)),
            ResourceKind::File => Ok(Self::Document(source)),
            ResourceKind::Audio | ResourceKind::Video => Err(unsupported()),
        }
    }
}
/// `tool_result.content`: a string or text, image and document blocks.
#[derive(Clone, Eq, PartialEq)]
pub enum ResultContent {
    Text(String),
    Blocks(Vec<ResultBlock>),
}
#[derive(Clone, Eq, PartialEq)]
pub enum ResultBlock {
    Text(String),
    Media(Media),
}
impl ResultContent {
    pub(super) fn read(v: &Value) -> Result<Self, CodecError> {
        if v.is_string() {
            return Ok(Self::Text(string(v)?));
        }
        Ok(Self::Blocks(
            list(v)?
                .iter()
                .map(|v| match required(object(v)?, "type")?.as_str() {
                    Some("text") => Ok(ResultBlock::Text(string(required(
                        closed(v, &["type", "text"])?,
                        "text",
                    )?)?)),
                    Some(kind @ ("image" | "document")) => {
                        Ok(ResultBlock::Media(Media::read(kind, v)?))
                    }
                    _ => Err(CodecError::Unsupported(
                        "Anthropic tool result content".into(),
                    )),
                })
                .collect::<Result<_, _>>()?,
        ))
    }
    pub(super) fn bytes(&self) -> Result<usize, CodecError> {
        let mut n = 0;
        match self {
            Self::Text(s) => charge(&mut n, text(s)?)?,
            Self::Blocks(blocks) => {
                count(blocks.len())?;
                for block in blocks {
                    charge(
                        &mut n,
                        match block {
                            ResultBlock::Text(s) => text(s)?,
                            ResultBlock::Media(m) => m.bytes()?,
                        },
                    )?;
                }
            }
        }
        Ok(n)
    }
    pub(super) fn nodes(&self) -> usize {
        match self {
            Self::Text(_) => 1,
            Self::Blocks(blocks) => 1 + blocks.len() * 8,
        }
    }
    pub(super) fn wire(&self) -> Value {
        match self {
            Self::Text(s) => json!(s),
            Self::Blocks(blocks) => json!(
                blocks
                    .iter()
                    .map(|b| match b {
                        ResultBlock::Text(s) => json!({"type":"text","text":s}),
                        ResultBlock::Media(m) => m.wire(),
                    })
                    .collect::<Vec<_>>()
            ),
        }
    }
}
