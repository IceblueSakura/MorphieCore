//! Reported Provider operations, not client tool definitions or execution commands.
use super::*;
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
impl ProviderAction {
    // Only the bounded dependency hasher may consume the unredacted view.
    pub(super) fn write_dependency(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
        match self {
            Self::Search { query, queries } => write!(writer, "Search:{query:?}:{queries:?}"),
            Self::OpenPage { url } => write!(writer, "OpenPage:{url:?}"),
            Self::FindInPage { url, pattern } => write!(writer, "FindInPage:{url:?}:{pattern:?}"),
        }
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
    pub replay: Option<ReplayValue>,
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
impl ProviderToolObservation {
    pub fn resolve(&self, items: &[(ItemId, Item)]) -> Result<ItemId, AliasResolutionError> {
        if items.len() > MAX_ITEMS {
            return Err(AliasResolutionError::Limit);
        }
        match &self.operation {
            ProviderOperation::Reported { .. } => Err(AliasResolutionError::WrongKind),
            ProviderOperation::Reference(ProviderOperationReference::Native(value)) => {
                resolve_call_alias(
                    items,
                    &NativeCallAlias {
                        domain: Some(self.source.clone()),
                        kind: NativeIdKind::ProviderOperation,
                        value: value.clone(),
                    },
                )
            }
            ProviderOperation::Reference(ProviderOperationReference::Local(owner)) => {
                let mut found = None;
                for (id, item) in items.iter().filter(|(id, _)| id == owner) {
                    match item {
                        Item::ProviderTool(observed)
                            if observed.source == self.source
                                && matches!(
                                    observed.operation,
                                    ProviderOperation::Reported { .. }
                                ) =>
                        {
                            if found.replace(*id).is_some() {
                                return Err(AliasResolutionError::Ambiguous);
                            }
                        }
                        _ => return Err(AliasResolutionError::WrongKind),
                    }
                }
                found.ok_or(AliasResolutionError::Missing)
            }
        }
    }
}
