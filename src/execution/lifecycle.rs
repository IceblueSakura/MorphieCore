//! Monotonic delivery lifecycle for one attempt chain.
//!
//! `Uncommitted -> Committed -> Terminal` is one-way. Retry or fallback is only
//! legal while uncommitted; once any semantic output is visible, replaying or
//! advancing to another candidate is forbidden.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryState {
    Uncommitted,
    Committed,
    Terminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LifecycleError {
    #[error("output after terminal is not deliverable")]
    OutputAfterTerminal,
    #[error("attempt is already terminal")]
    AlreadyTerminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Lifecycle {
    state: DeliveryState,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl Lifecycle {
    pub fn new() -> Self {
        Self {
            state: DeliveryState::Uncommitted,
        }
    }

    pub fn state(&self) -> DeliveryState {
        self.state
    }

    /// First visible semantic output commits the attempt.
    pub fn commit(&mut self) -> Result<(), LifecycleError> {
        match self.state {
            DeliveryState::Uncommitted => {
                self.state = DeliveryState::Committed;
                Ok(())
            }
            DeliveryState::Committed => Ok(()),
            DeliveryState::Terminal => Err(LifecycleError::OutputAfterTerminal),
        }
    }

    /// A validated protocol terminal closes the attempt.
    pub fn terminal(&mut self) -> Result<(), LifecycleError> {
        match self.state {
            DeliveryState::Terminal => Err(LifecycleError::AlreadyTerminal),
            DeliveryState::Uncommitted | DeliveryState::Committed => {
                self.state = DeliveryState::Terminal;
                Ok(())
            }
        }
    }

    pub fn may_retry_or_fallback(&self) -> bool {
        self.state == DeliveryState::Uncommitted
    }
}
