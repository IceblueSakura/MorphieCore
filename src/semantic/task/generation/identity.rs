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
        _ => None,
    }
}
