//! Caller-owned local scopes and native reference domains, never authentication.
use crate::semantic::task::generation::Item;
use crate::semantic::value::Text;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct LocalScope(u64);
impl LocalScope {
    /// Deterministic scope for a single standalone intake, not a global allocator.
    pub const ROOT: Self = Self(0);
    /// Items a candidate copy derives under a new kind; no intake allocates here.
    pub const DERIVED: Self = Self(u64::MAX);
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
pub(in crate::semantic::task::generation) fn call_alias(
    item: &Item,
) -> Option<(Option<&NativeAliasDomain>, NativeIdKind, &str)> {
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
            crate::semantic::task::generation::ProviderOperation::Reported {
                alias: Some(alias),
                ..
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

fn same_call_context(
    a: &crate::semantic::task::generation::CallContext,
    b: &crate::semantic::task::generation::CallContext,
) -> bool {
    a.namespace == b.namespace
        && a.async_call == b.async_call
        && a.caller == b.caller
        && a.alias_domain == b.alias_domain
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
) -> Result<(), crate::semantic::task::generation::GenerationError> {
    for (owner, old) in source.iter().filter(|(_, item)| item.is_operation()) {
        for (next, value) in edited {
            if (*owner == *next
                || call_alias(old).is_some() && call_alias(old) == call_alias(value))
                && !same_call_meaning(old, value)
            {
                return Err(crate::semantic::task::generation::GenerationError::InvalidDependency);
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ItemId {
    scope: LocalScope,
    value: u64,
}
impl ItemId {
    pub const fn new(v: u64) -> Self {
        Self::scoped(LocalScope::ROOT, v)
    }
    pub const fn scoped(scope: LocalScope, value: u64) -> Self {
        Self { scope, value }
    }
    pub const fn scope(self) -> LocalScope {
        self.scope
    }
    pub const fn get(self) -> u64 {
        self.value
    }
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PartId {
    scope: LocalScope,
    value: u64,
}
impl PartId {
    pub const fn new(v: u64) -> Self {
        Self::scoped(LocalScope::ROOT, v)
    }
    pub const fn scoped(scope: LocalScope, value: u64) -> Self {
        Self { scope, value }
    }
    pub const fn scope(self) -> LocalScope {
        self.scope
    }
    pub const fn get(self) -> u64 {
        self.value
    }
}
