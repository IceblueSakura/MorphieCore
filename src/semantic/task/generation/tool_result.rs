//! Closed tool result values. Error reporting never changes a generation outcome.
use super::{CallContext, ItemLifecycle, PartId, Resource};
use crate::semantic::value::Text;
/// Execution conclusion is independent of the result value and artifact lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolExecution {
    Succeeded,
    Failed { code: Option<Text> },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolResultPart {
    Text(Text),
    Resource(Resource),
}
impl ToolResultPart {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text.as_str()),
            Self::Resource(_) => None,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolOutput {
    Text(String),
    Parts(Vec<(PartId, ToolResultPart)>),
    Structured(super::StructuredValue),
}
impl ToolOutput {
    pub fn is_text_only(&self) -> bool {
        match self {
            Self::Text(_) => true,
            Self::Parts(parts) => parts
                .iter()
                .all(|(_, part)| matches!(part, ToolResultPart::Text(_))),
            Self::Structured(_) => false,
        }
    }
}
impl From<String> for ToolOutput {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl From<&str> for ToolOutput {
    fn from(value: &str) -> Self {
        Self::Text(value.into())
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolResult {
    pub call_id: Text,
    pub output: ToolOutput,
    /// Missing remains unreported, not successful execution.
    pub execution: Option<ToolExecution>,
    /// Artifact lifecycle, not tool success; absent remains unreported.
    pub status: Option<ItemLifecycle>,
    pub context: CallContext,
}
