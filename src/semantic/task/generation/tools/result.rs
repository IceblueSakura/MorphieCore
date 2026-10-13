//! Closed tool result values. Error reporting never changes a generation outcome.
use crate::semantic::task::generation::{CallContext, InputPart, ItemLifecycle, PartId};
use crate::semantic::value::Text;
/// Execution conclusion is independent of the result value and artifact lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolExecution {
    NotExecuted,
    Succeeded,
    Failed { code: Option<Text> },
    Cancelled,
    Unknown,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolOutput {
    Text(String),
    Parts(Vec<(PartId, InputPart)>),
    Structured(crate::semantic::task::generation::StructuredValue),
}
impl ToolOutput {
    pub fn is_text_only(&self) -> bool {
        match self {
            Self::Text(_) => true,
            Self::Parts(parts) => parts
                .iter()
                .all(|(_, part)| matches!(part, InputPart::Text(_))),
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
    /// Independent result report: missing is unknown, never inferred from execution or text.
    pub is_error: Option<bool>,
    /// Missing remains unreported, not successful execution.
    pub execution: Option<ToolExecution>,
    /// Artifact lifecycle, not tool success; absent remains unreported.
    pub status: Option<ItemLifecycle>,
    pub context: CallContext,
}
