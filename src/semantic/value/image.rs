//! Image media type shared by perception inputs and generated artifacts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
pub enum ImageFormat {
    #[strum(serialize = "image/png")]
    Png,
    #[strum(serialize = "image/jpeg")]
    Jpeg,
    #[strum(serialize = "image/gif")]
    Gif,
    #[strum(serialize = "image/webp")]
    Webp,
    #[strum(serialize = "image/bmp")]
    Bmp,
}
