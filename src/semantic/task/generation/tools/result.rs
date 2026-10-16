//! Closed tool result values. Error reporting never changes a generation outcome.
use crate::semantic::task::generation::{
    CallContext, GenerationError, InputPart, ItemLifecycle, Part, PrefixCacheTtl,
};
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
    Parts(Vec<Part<InputPart>>),
    Structured(crate::semantic::task::generation::StructuredValue),
}
impl ToolOutput {
    pub fn is_text_only(&self) -> bool {
        match self {
            Self::Text(_) => true,
            Self::Parts(parts) => parts
                .iter()
                .all(|part| matches!(part.content, InputPart::Text(_))),
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
    /// A reported result may omit the association; request history must supply it.
    pub call_id: Option<Text>,
    /// Reported name, never inferred from a missing call reference.
    pub name: Option<Text>,
    pub output: ToolOutput,
    /// Independent result report: missing is unknown, never inferred from execution or text.
    pub is_error: Option<bool>,
    /// Missing remains unreported, not successful execution.
    pub execution: Option<ToolExecution>,
    /// Artifact lifecycle, not tool success; absent remains unreported.
    pub status: Option<ItemLifecycle>,
    pub context: CallContext,
    pub cache: Option<PrefixCacheTtl>,
}
impl ToolResult {
    pub fn required_call_id(&self) -> Result<&str, GenerationError> {
        self.call_id
            .as_ref()
            .map(Text::as_str)
            .ok_or(GenerationError::InvalidToolResult)
    }
}
