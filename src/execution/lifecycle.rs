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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_state_is_monotonic_and_gates_fallback() {
        let mut lifecycle = Lifecycle::new();
        assert_eq!(lifecycle.state(), DeliveryState::Uncommitted);
        assert!(lifecycle.may_retry_or_fallback());

        lifecycle.commit().unwrap();
        assert_eq!(lifecycle.state(), DeliveryState::Committed);
        assert!(
            !lifecycle.may_retry_or_fallback(),
            "post-commit replay is forbidden"
        );
        lifecycle.commit().unwrap();

        lifecycle.terminal().unwrap();
        assert_eq!(lifecycle.state(), DeliveryState::Terminal);
        assert_eq!(lifecycle.terminal(), Err(LifecycleError::AlreadyTerminal));
        assert_eq!(lifecycle.commit(), Err(LifecycleError::OutputAfterTerminal));
    }

    #[test]
    fn a_terminal_without_output_is_still_terminal() {
        let mut lifecycle = Lifecycle::new();
        lifecycle.terminal().unwrap();
        assert!(!lifecycle.may_retry_or_fallback());
    }
}
