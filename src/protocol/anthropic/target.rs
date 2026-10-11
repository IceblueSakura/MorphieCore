//! Provider/model compatibility labels, never runtime addresses or credentials.
use crate::{
    protocol::CodecError,
    semantic::value::{ReplayOrigin, Text},
};
/// Trusted compatibility labels, never an upstream URL, credential locator or client hint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayTarget {
    provider: Text,
    model: Text,
}
impl ReplayTarget {
    pub fn new(provider: &str, model: &str) -> Result<Self, CodecError> {
        super::shape::label(provider)?;
        super::shape::label(model)?;
        Ok(Self {
            provider: Text::new(provider, "provider label", 256).map_err(|_| CodecError::Limit)?,
            model: Text::new(model, "model label", 256).map_err(|_| CodecError::Limit)?,
        })
    }
    pub fn provider(&self) -> &str {
        self.provider.as_str()
    }
    pub fn model(&self) -> &str {
        self.model.as_str()
    }
    pub(crate) fn origin(&self) -> ReplayOrigin {
        crate::protocol::replay_envelope::issuer(self.provider(), self.model())
    }
}
