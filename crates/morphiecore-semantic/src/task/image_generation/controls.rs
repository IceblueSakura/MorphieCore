//! Generation preferences are distinct from effective artifact reports.
use super::{ImageBackground, ImageQuality, ImageSize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageSizeRequest {
    Auto,
    Exact(ImageSize),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageQualityRequest {
    Auto,
    Exact(ImageQuality),
}
impl ImageQualityRequest {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Exact(value) => value.into(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageBackgroundRequest {
    Auto,
    Exact(ImageBackground),
}
impl ImageBackgroundRequest {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Exact(value) => value.into(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum ImageModeration {
    Auto,
    Low,
}
