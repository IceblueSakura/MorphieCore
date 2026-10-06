//! Text output contract; schema syntax is bounded data, not a gateway execution program.
use crate::semantic::value::{Presence, Text};
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub enum OutputConstraint {
    #[default]
    Text,
    JsonObject,
    JsonSchema {
        name: Text,
        description: Option<Text>,
        schema: serde_json::Value,
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
    pub presence: bool,
    pub format: Presence<OutputConstraint>,
    pub verbosity: Presence<Verbosity>,
}
