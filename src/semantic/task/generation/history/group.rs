//! Message envelope identity.
use crate::semantic::task::generation::LocalScope;

/// Caller-allocated group identity; neither a message nor a reported native ID.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct GroupId {
    scope: LocalScope,
    value: u64,
}
impl GroupId {
    pub const fn new(scope: LocalScope, value: u64) -> Self {
        Self { scope, value }
    }
    pub const fn scope(self) -> LocalScope {
        self.scope
    }
    pub const fn get(self) -> u64 {
        self.value
    }
}
