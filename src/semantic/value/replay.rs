//! Trusted replay scope labels contain no endpoint or credential locator.
use super::{Text, ValueError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayOrigin(Text);
impl ReplayOrigin {
    pub fn new(label: &str) -> Result<Self, ValueError> {
        Text::new(label, "replay origin", 256).map(Self)
    }
}
