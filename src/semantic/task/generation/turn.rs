//! Caller-declared local response relations; no wire inference or Agent scheduler.
use super::{
    CallReference, GenerationRequest, GenerationResponse, Item, ItemLifecycle, MAX_ITEMS, Outcome,
    RequestDependencyProof,
};

/// Local identity, not an upstream response ID or stream lane.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ResponseId(u64);
impl ResponseId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}
/// Caller-owned continuity label, never inferred from role, phase or adjacency.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TurnId(u64);
impl TurnId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResponseRelation {
    turn: TurnId,
    response: ResponseId,
    previous: Option<ResponseId>,
}
impl ResponseRelation {
    pub const fn new(turn: TurnId, response: ResponseId) -> Self {
        Self {
            turn,
            response,
            previous: None,
        }
    }
    pub const fn turn(self) -> TurnId {
        self.turn
    }
    pub const fn response(self) -> ResponseId {
        self.response
    }
    pub const fn previous(self) -> Option<ResponseId> {
        self.previous
    }
}
/// Association facts only. Even `ResultsComplete` is not execution readiness,
/// tool success, complete artifacts, replay compatibility or end-of-turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResultReadiness<'a> {
    Unreported,
    Awaiting {
        missing: Vec<CallReference<'a>>,
        in_progress: Vec<CallReference<'a>>,
    },
    ResultsComplete,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ContinuationError {
    #[error("final history is invalid")]
    InvalidHistory,
    #[error("response owners, values, order or grouping changed in final history")]
    ChangedResponse,
    #[error("this response's tool results are not completely reported")]
    ResultsNotComplete,
    #[error("declared continuation dependencies changed or exceed the item budget")]
    ChangedDependency,
    #[error("successor response identity conflicts with the current relation")]
    ConflictingResponse,
}
/// Borrows the only response authority. Global identity allocation and ancestor
/// uniqueness remain the caller's responsibility; this is not a session store.
#[derive(Clone, Copy, Debug)]
pub struct ResponseContinuation<'a> {
    relation: ResponseRelation,
    response: &'a GenerationResponse,
    dependencies: &'a [RequestDependencyProof],
}
impl<'a> ResponseContinuation<'a> {
    pub const fn new(relation: ResponseRelation, response: &'a GenerationResponse) -> Self {
        Self {
            relation,
            response,
            dependencies: &[],
        }
    }
    /// Trusted caller-declared requirements, not inferred wire dependencies.
    /// A successor needs its own declarations; these apply only to this response.
    pub const fn with_dependencies(mut self, dependencies: &'a [RequestDependencyProof]) -> Self {
        self.dependencies = dependencies;
        self
    }
    pub const fn relation(&self) -> ResponseRelation {
        self.relation
    }
    pub fn inspect(
        &self,
        history: &GenerationRequest,
    ) -> Result<ResultReadiness<'a>, ContinuationError> {
        if self.response.outcome() != Outcome::Completed {
            return Ok(ResultReadiness::Unreported);
        }
        history
            .validate()
            .map_err(|_| ContinuationError::InvalidHistory)?;
        for group in self.response.message_groups() {
            if !history
                .message_groups()
                .any(|candidate| candidate.items() == group.items())
            {
                return Err(ContinuationError::ChangedResponse);
            }
        }
        if self.dependencies.len() > MAX_ITEMS
            || self
                .dependencies
                .iter()
                .any(|proof| proof.check(history).is_err())
        {
            return Err(ContinuationError::ChangedDependency);
        }
        let mut pending = Vec::new();
        let mut in_progress = Vec::new();
        let mut calls = 0;
        let mut previous_position = None;
        for (owner, item) in self.response.items() {
            let position = history
                .items()
                .iter()
                .position(|(id, value)| id == owner && value == item)
                .ok_or(ContinuationError::ChangedResponse)?;
            if previous_position.is_some_and(|old| old >= position) {
                return Err(ContinuationError::ChangedResponse);
            }
            previous_position = Some(position);
            let call_id = match item {
                Item::ToolCall(call) => call.call_id.as_str(),
                Item::CustomCall(call) => call.call_id.as_str(),
                Item::Program(call) => call.call_id.as_str(),
                _ => continue,
            };
            calls += 1;
            let result = history.items()[position + 1..]
                .iter()
                .find_map(|(_, value)| match (item, value) {
                    (Item::ToolCall(_), Item::ToolResult(result))
                    | (Item::CustomCall(_), Item::CustomResult(result))
                        if result.call_id.as_str() == call_id =>
                    {
                        Some(result.status)
                    }
                    (Item::Program(_), Item::ProgramOutput(result))
                        if result.call_id.as_str() == call_id =>
                    {
                        Some(Some(result.status))
                    }
                    _ => None,
                });
            match result {
                None => pending.push(CallReference {
                    item: *owner,
                    call_id,
                }),
                Some(Some(ItemLifecycle::InProgress)) => in_progress.push(CallReference {
                    item: *owner,
                    call_id,
                }),
                Some(_) => {}
            }
        }
        Ok(if calls == 0 {
            ResultReadiness::Unreported
        } else if pending.is_empty() && in_progress.is_empty() {
            ResultReadiness::ResultsComplete
        } else {
            ResultReadiness::Awaiting {
                missing: pending,
                in_progress,
            }
        })
    }
    /// Explicit successor association, not a request dispatch. The caller must
    /// separately validate permissions, budgets, target and replay dependencies.
    pub fn advance<'b>(
        &self,
        history: &GenerationRequest,
        id: ResponseId,
        response: &'b GenerationResponse,
    ) -> Result<ResponseContinuation<'b>, ContinuationError> {
        if id == self.relation.response || Some(id) == self.relation.previous {
            return Err(ContinuationError::ConflictingResponse);
        }
        if self.inspect(history)? != ResultReadiness::ResultsComplete {
            return Err(ContinuationError::ResultsNotComplete);
        }
        Ok(ResponseContinuation::new(
            ResponseRelation {
                turn: self.relation.turn,
                response: id,
                previous: Some(self.relation.response),
            },
            response,
        ))
    }
}
