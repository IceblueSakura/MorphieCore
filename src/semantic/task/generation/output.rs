//! Text output contract; schema syntax is bounded data, not a gateway execution program.
use crate::semantic::value::{Presence, Text};
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub enum OutputConstraint {
    #[default]
    Text,
    JsonObject,
    JsonSchema {
        /// OpenAI requires a name; Messages and Gemini formats have none.
        name: Option<Text>,
        description: Option<Text>,
        schema: crate::semantic::task::generation::SchemaDocument,
        strict: Option<bool>,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verbosity {
    Low,
    Medium,
    High,
}
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct TextOptions {
    /// Container state; null is a reported observation, not request intent.
    pub presence: Presence<()>,
    pub format: Presence<OutputConstraint>,
    pub verbosity: Option<Verbosity>,
}
