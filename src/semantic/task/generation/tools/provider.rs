//! Reported Provider operations, not client tool definitions or execution commands.
use crate::semantic::task::generation::*;
use crate::semantic::value::Text;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderRequester {
    Model,
    Client,
    Unreported,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderExecutionProgress {
    Queued,
    Running,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderResultFormat {
    Text,
    Json,
}
/// The sole writable reference. Resolution is a borrowed-history operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderOperationReference {
    Local(ItemId),
    Native(Text),
}
/// Reported action content, never an instruction to execute or fetch a URL.
/// Search fields are independent observations, not aliases to normalize.
/// https://developers.openai.com/api/docs/guides/tools-web-search
#[derive(Clone, Eq, PartialEq)]
pub enum ProviderAction {
    Search {
        query: Option<Text>,
        queries: Option<Vec<Text>>,
    },
    OpenPage {
        url: Option<Text>,
    },
    FindInPage {
        url: Text,
        pattern: Text,
    },
}
impl std::fmt::Debug for ProviderAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProviderAction([redacted])")
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderOperation {
    Reported {
        tool: Text,
        alias: Option<Text>,
        requester: ProviderRequester,
        /// None means no action content was reported, not empty parameters.
        action: Option<ProviderAction>,
    },
    Reference(ProviderOperationReference),
}
#[derive(Clone, Eq, PartialEq)]
pub struct ProviderToolObservation {
    /// Declared by trusted intake/caller; this label is not authentication.
    pub source: NativeAliasDomain,
    pub operation: ProviderOperation,
    pub progress: Option<ProviderExecutionProgress>,
    pub execution: Option<ToolExecution>,
    pub output: Option<ToolOutput>,
    pub artifact_status: Option<ItemLifecycle>,
}
impl std::fmt::Debug for ProviderToolObservation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProviderToolObservation([redacted])")
    }
}
