//! Caller-owned local scopes and native reference domains, never authentication.
use super::{Item, ItemId, MAX_ITEMS};
use crate::semantic::value::Text;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct LocalScope(u64);
impl LocalScope {
    /// Deterministic scope for a single standalone intake, not a global allocator.
    pub const ROOT: Self = Self(0);
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// The protocol's reference domain may outlive the response carrying a result.
/// `source` identifies a declared source/profile, not a trusted principal.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NativeAliasDomain {
    pub source: Text,
    pub scope: LocalScope,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NativeIdKind {
    FunctionCall,
    CustomCall,
    ProgramCall,
    ProviderOperation,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeCallAlias {
    pub domain: Option<NativeAliasDomain>,
    pub kind: NativeIdKind,
    pub value: Text,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AliasResolutionError {
    #[error("native call reference is missing")]
    Missing,
    #[error("native call reference is ambiguous")]
    Ambiguous,
    #[error("native reference names a different call kind")]
    WrongKind,
    #[error("native reference lookup exceeds limits")]
    Limit,
}

/// Resolve only reported aliases in explicitly selected items. No nearest-match
/// heuristic, source body, session lookup or ID synthesis participates.
pub fn resolve_call_alias(
    items: &[(ItemId, Item)],
    alias: &NativeCallAlias,
) -> Result<ItemId, AliasResolutionError> {
    if items.len() > MAX_ITEMS
        || alias.value.as_str().is_empty()
        || alias.value.as_str().len() > 256
        || alias.domain.as_ref().is_some_and(|domain| {
            domain.source.as_str().is_empty() || domain.source.as_str().len() > 256
        })
    {
        return Err(AliasResolutionError::Limit);
    }
    let mut found = None;
    let mut wrong_kind = false;
    for (owner, item) in items {
        let Some((domain, kind, value)) = call_alias(item) else {
            continue;
        };
        if domain != alias.domain.as_ref() || value != alias.value.as_str() {
            continue;
        }
        if kind != alias.kind {
            wrong_kind = true;
            continue;
        }
        if found.replace(*owner).is_some() {
            return Err(AliasResolutionError::Ambiguous);
        }
    }
    found.ok_or(if wrong_kind {
        AliasResolutionError::WrongKind
    } else {
        AliasResolutionError::Missing
    })
}

pub(super) fn call_alias(item: &Item) -> Option<(Option<&NativeAliasDomain>, NativeIdKind, &str)> {
    match item {
        Item::ToolCall(call) => Some((
            call.context.alias_domain.as_ref(),
            NativeIdKind::FunctionCall,
            call.call_id.as_str(),
        )),
        Item::CustomCall(call) => Some((
            call.context.alias_domain.as_ref(),
            NativeIdKind::CustomCall,
            call.call_id.as_str(),
        )),
        Item::Program(call) => Some((None, NativeIdKind::ProgramCall, call.call_id.as_str())),
        Item::ProviderTool(observed) => match &observed.operation {
            super::ProviderOperation::Reported {
                alias: Some(alias), ..
            } => Some((
                Some(&observed.source),
                NativeIdKind::ProviderOperation,
                alias.as_str(),
            )),
            _ => None,
        },
        _ => None,
    }
}

fn same_call_context(a: &super::CallContext, b: &super::CallContext) -> bool {
    a.namespace == b.namespace
        && a.async_call == b.async_call
        && a.caller == b.caller
        && a.alias_domain == b.alias_domain
        && a.definition == b.definition
}
fn same_call_meaning(left: &Item, right: &Item) -> bool {
    match (left, right) {
        (Item::ToolCall(a), Item::ToolCall(b)) => {
            a.call_id == b.call_id
                && a.name == b.name
                && a.arguments.same_authority(&b.arguments)
                && same_call_context(&a.context, &b.context)
        }
        (Item::CustomCall(a), Item::CustomCall(b)) => {
            a.call_id == b.call_id
                && a.name == b.name
                && a.input == b.input
                && a.context == b.context
        }
        (Item::Program(a), Item::Program(b)) => a == b,
        (Item::ProviderTool(a), Item::ProviderTool(b)) => {
            a.source == b.source && a.operation == b.operation
        }
        _ => false,
    }
}
/// In-place call edits and native-alias reuse cannot reinterpret prior results.
/// A new invocation needs both an explicitly allocated owner and a new alias.
pub(super) fn check_call_edits(
    source: &[(ItemId, Item)],
    edited: &[(ItemId, Item)],
) -> Result<(), super::GenerationError> {
    for (owner, old) in source.iter().filter(|(_, item)| item.is_operation()) {
        for (next, value) in edited {
            if (*owner == *next
                || call_alias(old).is_some() && call_alias(old) == call_alias(value))
                && !same_call_meaning(old, value)
            {
                return Err(super::GenerationError::InvalidDependency);
            }
        }
    }
    Ok(())
}

/// Revising an observed Provider operation creates a proposal, not new reported facts.
pub(super) fn check_operation_revision(
    source: &Item,
    replacement: &Item,
) -> Result<(), super::GenerationError> {
    match (source, replacement) {
        (Item::ProviderTool(_), Item::ProviderTool(next))
            if (call_alias(source).is_none() || call_alias(source) != call_alias(replacement))
                && next.progress.is_none()
                && next.execution.is_none()
                && next.output.is_none()
                && next.replay.is_none()
                && next.artifact_status.is_none() =>
        {
            Ok(())
        }
        (Item::ProviderTool(_), _) | (_, Item::ProviderTool(_)) => {
            Err(super::GenerationError::InvalidDependency)
        }
        (_, Item::ToolCall(call)) if call.context.replay.is_some() => {
            Err(super::GenerationError::InvalidDependency)
        }
        _ => Ok(()),
    }
}
