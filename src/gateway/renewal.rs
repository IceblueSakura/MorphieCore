//! Listener-owned credential maintenance, independent of inference and routing.
#[cfg(test)]
mod tests;
use super::Gateway;
use crate::credential::AccessBinding;
use tokio::time::{Instant, MissedTickBehavior};

impl Gateway {
    pub(super) async fn renew_credentials(&self) {
        let mut accounts: Vec<(AccessBinding, Instant)> = Vec::new();
        for binding in &self.state.oauth_accounts {
            if !accounts
                .iter()
                .any(|(other, _)| binding.same_account(other))
            {
                accounts.push((binding.clone(), Instant::now()));
            }
        }
        if accounts.is_empty() {
            return;
        }
        let policy = &self.state.limits;
        if policy.oauth_renewal_timeout.is_zero() {
            self.state.shutdown.cancelled().await;
            return;
        }
        // Zero disables periodic scans, not the startup check or shutdown.
        let mut ticks = (!policy.oauth_scan_interval.is_zero()).then(|| {
            let mut ticks = tokio::time::interval_at(
                Instant::now() + policy.oauth_scan_interval,
                policy.oauth_scan_interval,
            );
            ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
            ticks
        });
        loop {
            if self.state.shutdown.is_cancelled() {
                return;
            }
            for (binding, next_attempt) in &mut accounts {
                if Instant::now() < *next_attempt {
                    continue;
                }
                let result = tokio::select! {
                    biased;
                    _ = self.state.shutdown.cancelled() => return,
                    result = tokio::time::timeout(
                        policy.oauth_renewal_timeout,
                        binding.refresh_if_due(policy.oauth_refresh_ahead_seconds),
                    ) => result,
                };
                match result {
                    Ok(Ok(true)) => {
                        // Short-lived replacement grants must not cause a refresh storm.
                        *next_attempt = Instant::now() + policy.oauth_minimum_renewal_interval;
                        tracing::info!("OAuth credential automatically renewed");
                    }
                    Ok(Ok(false)) => {}
                    Ok(Err(crate::credential::CredentialError::Busy)) => {
                        *next_attempt = Instant::now() + policy.oauth_failed_renewal_delay;
                    }
                    Ok(Err(_)) | Err(_) => {
                        // A delay never restores uncertain material or resumes pending
                        // verification. Only the lifecycle owner authorizes reuse.
                        *next_attempt = Instant::now() + policy.oauth_failed_renewal_delay;
                        tracing::warn!(
                            "OAuth automatic renewal failed; inspect morphiecore-auth status"
                        );
                    }
                }
            }
            tokio::select! {
                biased;
                _ = self.state.shutdown.cancelled() => return,
                _ = async {
                    match &mut ticks {
                        Some(ticks) => { ticks.tick().await; }
                        None => std::future::pending::<()>().await,
                    }
                } => {},
            }
        }
    }
}
