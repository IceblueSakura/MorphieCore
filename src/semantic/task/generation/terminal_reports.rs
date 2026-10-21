//! Bounded terminal observations, separate from content, errors and stop causes.
use super::{GenerationError, MAX_TERMINAL_REPORT_BYTES};
use crate::semantic::value::{Presence, Text};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefusalCategory {
    Cyber,
    Biological,
    FrontierModelDevelopment,
    ReasoningExtraction,
    GeneralHarms,
}

#[derive(Clone, Eq, PartialEq)]
pub struct RefusalDetails {
    pub category: Presence<RefusalCategory>,
    pub explanation: Presence<Text>,
}
impl RefusalDetails {
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            .saturating_add(self.explanation.value().map_or(0, |v| v.as_str().len()))
    }
    pub fn validate(&self) -> Result<(), GenerationError> {
        if self.bytes() > MAX_TERMINAL_REPORT_BYTES {
            return Err(GenerationError::Limit);
        }
        Ok(())
    }
    pub(crate) fn fingerprint(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        hash.update(format!("{self:?}").as_bytes());
        if let Some(text) = self.explanation.value() {
            hash.update(text.as_str().as_bytes());
        }
        hash.finalize().into()
    }
}
impl std::fmt::Debug for RefusalDetails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RefusalDetails")
            .field("category", &self.category)
            .field(
                "explanation",
                &match self.explanation {
                    Presence::Absent => "absent",
                    Presence::Null => "null",
                    Presence::Value(_) => "[redacted]",
                },
            )
            .finish()
    }
}
