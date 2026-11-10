//! Listener-owned credential maintenance, independent of inference and routing.
#[cfg(test)]
mod tests;
use super::Gateway;
use super::startup::credentials::RenewalAccount;
use opentelemetry::logs::{AnyValue, Severity};
use tokio::time::{Instant, MissedTickBehavior};

impl Gateway {
    pub(super) async fn renew_credentials(&self) {
        let mut accounts: Vec<(RenewalAccount, Instant)> = self
            .state
            .oauth_accounts
            .iter()
            .cloned()
            .map(|binding| (binding, Instant::now()))
            .collect();
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
            for (account, next_attempt) in &mut accounts {
                if Instant::now() < *next_attempt {
                    continue;
                }
                let result = tokio::select! {
                    biased;
                    _ = self.state.shutdown.cancelled() => return,
                    result = tokio::time::timeout(
                        policy.oauth_renewal_timeout,
                        account.binding.refresh_if_due(policy.oauth_refresh_ahead_seconds),
                    ) => result,
                };
                match result {
                    Ok(Ok(true)) => {
                        // Short-lived replacement grants must not cause a refresh storm.
                        *next_attempt = Instant::now() + policy.oauth_minimum_renewal_interval;
                    }
                    Ok(Ok(false)) => {}
                    Ok(Err(crate::credential::CredentialError::Busy)) => {
                        *next_attempt = Instant::now() + policy.oauth_failed_renewal_delay;
                    }
                    failure @ (Ok(Err(_)) | Err(_)) => {
                        // A delay never restores uncertain material or resumes pending
                        // verification. Only the lifecycle owner authorizes reuse.
                        *next_attempt = Instant::now() + policy.oauth_failed_renewal_delay;
                        use crate::credential::CredentialError as Error;
                        let (severity, code) = match failure {
                            Err(_) | Ok(Err(Error::Timeout)) => {
                                (Severity::Warn, "oauth.renewal_timeout")
                            }
                            Ok(Err(Error::Network)) => (Severity::Warn, "oauth.renewal_network"),
                            Ok(Err(Error::IdentityMismatch)) => {
                                (Severity::Error, "oauth.identity_mismatch")
                            }
                            Ok(Err(Error::PersistenceUncertain | Error::Storage)) => {
                                (Severity::Error, "oauth.persistence")
                            }
                            Ok(Err(
                                Error::InvalidGrant
                                | Error::GrantExpired
                                | Error::GrantReused
                                | Error::GrantRevoked
                                | Error::LoginRequired,
                            )) => (Severity::Error, "oauth.login_required"),
                            Ok(Err(_)) => (Severity::Error, "oauth.renewal_rejected"),
                            Ok(Ok(_)) => unreachable!("successful renewal handled"),
                        };
                        if let Some(sink) = &self.state.diagnostics {
                            let mut attributes = vec![
                                ("error.type".into(), AnyValue::from(code)),
                                ("stage".into(), AnyValue::from("credential_renewal")),
                                (
                                    "profile".into(),
                                    AnyValue::from(account.binding.profile().to_owned()),
                                ),
                            ];
                            if let Some(provider) = &account.provider {
                                attributes
                                    .push(("provider".into(), AnyValue::from(provider.clone())));
                            }
                            if let Some(name) = &account.name {
                                attributes.push(("account".into(), AnyValue::from(name.clone())));
                            }
                            sink.emit(
                                "gateway.credential.failure",
                                severity,
                                attributes,
                                AnyValue::from(code),
                            );
                        } else {
                            eprintln!(
                                "OAuth automatic renewal failed; inspect morphiecore-auth status"
                            );
                        }
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
