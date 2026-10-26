//! Native `image` and `document` blocks and their mapping to IR resources.
//! Messages has no audio or video carrier. Provider `file` sources and `content`
//! documents name credential-owned state or nested blocks, so they are rejected.
//! Source: docs/refs/multimodal.md.
use super::shape::*;
use crate::protocol::failure::CarrierReason;
use crate::{
    protocol::CodecError,
    semantic::{
        task::generation::*,
        value::{Presence, Text},
    },
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

const IMAGE_TYPES: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];
// Direct Messages limits count encoded data, including tool-result images.
// https://platform.claude.com/docs/en/build-with-claude/vision
const MAX_IMAGE_BASE64_BYTES: usize = 10_000_000;
pub(super) const MAX_IMAGES: usize = 600;
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
    /// `reject_oversized` is `transformations.oversized_image: "error"`; the
    /// default `downsize` permits resizing without requesting it.
    Image {
        source: Source,
        reject_oversized: bool,
    },
    /// `title` is the model-visible document name, the IR file name.
    Document {
        source: Source,
        title: Option<String>,
        context: Presence<String>,
    },
}
impl Media {
    pub(super) fn read(kind: &str, v: &Value) -> Result<Self, CodecError> {
        let o = object(v)?;
        // Citations still need their own source/quote owners.
        if o.get("citations").is_some_and(|v| !v.is_null()) {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
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
                    return Err(CodecError::invalid());
                }
                Source::Text(field("data")?)
            }
            _ => {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
        };
        let media = match kind {
            "image" => Self::Image {
                source,
                reject_oversized: match o.get("transformations") {
                    None | Some(Value::Null) => false,
                    Some(t) => match object(t)?.get("oversized_image") {
                        None => false,
                        Some(v) => match v.as_str() {
                            Some("downsize") => false,
                            Some("error") => true,
                            _ => {
                                return Err(CodecError::invalid());
                            }
                        },
                    },
                },
            },
            _ => Self::Document {
                source,
                context: presence(o, "context", string)?,
                title: match o.get("title") {
                    None | Some(Value::Null) => None,
                    Some(v) => Some(string(v)?),
                },
            },
        };
        media.validate()?;
        Ok(media)
    }
    fn validate(&self) -> Result<(), CodecError> {
        match self {
            Self::Image {
                source: Source::Base64 { data, .. },
                ..
            } if data.len() > MAX_IMAGE_BASE64_BYTES => Err(CodecError::Limit),
            Self::Image {
                source: Source::Base64 { media_type, .. },
                ..
            } if !IMAGE_TYPES.contains(&&**media_type) => {
                Err(CodecError::Unsupported(CarrierReason::Unavailable))
            }
            Self::Document {
                source: Source::Base64 { media_type, .. },
                ..
            } if media_type != PDF => Err(CodecError::Unsupported(CarrierReason::Unavailable)),
            Self::Image {
                source: Source::Text(_),
                ..
            } => Err(CodecError::invalid()),
            _ => Ok(()),
        }
    }
    pub(super) fn bytes(&self) -> Result<usize, CodecError> {
        let (Self::Image { source, .. } | Self::Document { source, .. }) = self;
        self.validate()?;
        let title = match self {
            Self::Document { title: Some(t), .. } => text(t)?,
            _ => 0,
        };
        let context = match self {
            Self::Document { context, .. } => context.value().map_or(Ok(0), |c| text(c))?,
            _ => 0,
        };
        Ok(title
            + context
            + match source {
                Source::Base64 { media_type, data } => text(media_type)? + text(data)?,
                Source::Url(url) | Source::Text(url) => text(url)?,
            })
    }
    pub(super) fn wire(&self) -> Value {
        let (kind, source) = match self {
            Self::Image { source, .. } => ("image", source),
            Self::Document { source, .. } => ("document", source),
        };
        let source = match source {
            Source::Base64 { media_type, data } => {
                json!({"type":"base64","media_type":media_type,"data":data})
            }
            Source::Url(url) => json!({"type":"url","url":url}),
            Source::Text(data) => json!({"type":"text","media_type":PLAIN_TEXT,"data":data}),
        };
        let mut block = json!({"type":kind,"source":source});
        match self {
            Self::Image {
                reject_oversized: true,
                ..
            } => block["transformations"] = json!({"oversized_image":"error"}),
            Self::Document { title, context, .. } => {
                if let Some(title) = title {
                    block["title"] = json!(title);
                }
                put(
                    block.as_object_mut().expect("document"),
                    "context",
                    context,
                    |c| json!(c),
                );
            }
            _ => {}
        }
        block
    }
    /// The IR view: a text document becomes an inline `text/plain` file.
    pub(super) fn resource(&self) -> Result<Resource, CodecError> {
        let text =
            |s: &str, kind| Text::new(s, kind, MAX_TEXT_BYTES).map_err(|_| CodecError::Limit);
        let (source, description) = match self {
            Self::Image {
                source,
                reject_oversized,
            } => (
                source,
                ResourceDescription::Image {
                    detail: None,
                    size_policy: if *reject_oversized {
                        ImageSizePolicy::RejectOversized
                    } else {
                        ImageSizePolicy::AllowDownsize
                    },
                },
            ),
            Self::Document {
                source,
                title,
                context,
            } => (
                source,
                ResourceDescription::File(FileDescription {
                    filename: title
                        .as_deref()
                        .map(|t| {
                            Text::allowing_empty(t, "document title", MAX_FILE_NAME_BYTES)
                                .map_err(|_| CodecError::Limit)
                        })
                        .transpose()?,
                    detail: None,
                    context: match context {
                        Presence::Absent => Presence::Absent,
                        Presence::Null => Presence::Null,
                        Presence::Value(c) => Presence::Value(
                            Text::allowing_empty(c, "document context", MAX_TEXT_BYTES)
                                .map_err(|_| CodecError::Limit)?,
                        ),
                    },
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
    /// The native carrier for one IR resource; uncalibrated detail cannot disappear.
    pub(super) fn from_resource(resource: ResourceView<'_>) -> Result<Self, CodecError> {
        let unsupported = || CodecError::Unsupported(CarrierReason::Unavailable);
        if match resource.description {
            ResourceDescription::Image { detail, .. } => detail.is_some(),
            ResourceDescription::File(file) => file.detail.is_some(),
            _ => false,
        } {
            return Err(unsupported());
        }
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
                            .map_err(|_| CodecError::invalid())?,
                    )
                    .map_err(|_| unsupported())?,
                ),
                _ => return Err(unsupported()),
            },
            _ => return Err(unsupported()),
        };
        let media = match resource.kind() {
            ResourceKind::Image => Self::Image {
                source,
                reject_oversized: matches!(
                    resource.description,
                    ResourceDescription::Image {
                        size_policy: ImageSizePolicy::RejectOversized,
                        ..
                    }
                ),
            },
            ResourceKind::File => Self::Document {
                source,
                context: match resource.description {
                    ResourceDescription::File(file) => match &file.context {
                        Presence::Absent => Presence::Absent,
                        Presence::Null => Presence::Null,
                        Presence::Value(c) => Presence::Value(c.as_str().into()),
                    },
                    _ => Presence::Absent,
                },
                title: match resource.description {
                    ResourceDescription::File(file) => {
                        file.filename.as_ref().map(|n| n.as_str().into())
                    }
                    _ => None,
                },
            },
            ResourceKind::Audio | ResourceKind::Video => return Err(unsupported()),
        };
        media.validate()?;
        Ok(media)
    }
}
/// `tool_result.content`: a string or text, image and document blocks.
#[derive(Clone, Eq, PartialEq)]
pub enum ResultContent {
    Text(String),
    Blocks(Vec<super::CacheMarked<ResultBlock>>),
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
                .map(|v| {
                    super::CacheMarked::read(v, |v| match required(object(v)?, "type")?.as_str() {
                        Some("text") => {
                            let o = closed(v, &["type", "text", "citations", "cache_control"])?;
                            uncited(o)?;
                            Ok(ResultBlock::Text(string(required(o, "text")?)?))
                        }
                        Some(kind @ ("image" | "document")) => {
                            Ok(ResultBlock::Media(Media::read(kind, v)?))
                        }
                        _ => Err(CodecError::Unsupported(CarrierReason::Unavailable)),
                    })
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
                        match &block.content {
                            ResultBlock::Text(s) => text(s)?,
                            ResultBlock::Media(m) => m.bytes()?,
                        },
                    )?;
                    charge(&mut n, usize::from(block.cache.is_some()) * 32)?;
                }
            }
        }
        Ok(n)
    }
    pub(super) fn nodes(&self) -> usize {
        match self {
            Self::Text(_) => 1,
            Self::Blocks(blocks) => 1 + blocks.len() * 16,
        }
    }
    pub(super) fn wire(&self) -> Value {
        match self {
            Self::Text(s) => json!(s),
            Self::Blocks(blocks) => json!(
                blocks
                    .iter()
                    .map(|b| b.wire(|b| match b {
                        ResultBlock::Text(s) => json!({"type":"text","text":s}),
                        ResultBlock::Media(m) => m.wire(),
                    }))
                    .collect::<Vec<_>>()
            ),
        }
    }
}
