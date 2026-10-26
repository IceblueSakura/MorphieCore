//! Listener-owned credential maintenance, independent of inference and routing.
#[cfg(test)]
mod tests;
use super::{Gateway, startup::credentials::Source};
use crate::credential::AccessBinding;
use std::time::Duration;
use tokio::time::{Instant, MissedTickBehavior};

const SCAN_INTERVAL: Duration = Duration::from_secs(15);
const REFRESH_AHEAD: u64 = 120;
const MIN_RENEWAL_INTERVAL: Duration = Duration::from_secs(60);
const RENEWAL_TIMEOUT: Duration = Duration::from_secs(90);

impl Gateway {
    pub(super) async fn renew_credentials(&self) {
        let mut accounts: Vec<(AccessBinding, Instant)> = Vec::new();
        for candidate in self
            .state
            .entries
            .values()
            .flat_map(|entry| &entry.candidates)
        {
            if let Source::Account(binding) = &candidate.secret
                && !accounts
                    .iter()
                    .any(|(other, _)| binding.same_account(other))
            {
                accounts.push((binding.clone(), Instant::now()));
            }
        }
        if accounts.is_empty() {
            return;
        }
        let mut ticks = tokio::time::interval(SCAN_INTERVAL);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = self.state.shutdown.cancelled() => return,
                _ = ticks.tick() => {},
            }
            for (binding, next_attempt) in &mut accounts {
                if Instant::now() < *next_attempt {
                    continue;
                }
                let result = tokio::select! {
                    biased;
                    _ = self.state.shutdown.cancelled() => return,
                    result = tokio::time::timeout(
                        RENEWAL_TIMEOUT, binding.refresh_if_due(REFRESH_AHEAD),
                    ) => result,
                };
                // Short-lived replacement grants must not cause a refresh storm.
                // Failed rotation already quarantines consumed material; never
                // resume pending verification or restore old tokens automatically.
                if !matches!(result, Ok(Ok(false))) {
                    *next_attempt = Instant::now() + MIN_RENEWAL_INTERVAL;
                }
                match result {
                    Ok(Ok(true)) => tracing::info!("OAuth credential automatically renewed"),
                    Ok(Ok(false)) | Ok(Err(crate::credential::CredentialError::Busy)) => {}
                    Ok(Err(_)) | Err(_) => tracing::warn!(
                        "OAuth automatic renewal failed; inspect morphiecore-auth status"
                    ),
                }
            }
        }
    }
}
