//! Deployment policy groups. Defaults have one owner in Limits.
use super::limits::Limits;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf, time::Duration};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Configuration {
    pub version: u32,
    #[serde(default = "auth_file")]
    pub auth_file: PathBuf,
    #[serde(default = "listen")]
    pub bind: SocketAddr,
    pub credentials_dir: Option<PathBuf>,
    #[serde(default = "models_file")]
    pub models_file: PathBuf,
    #[serde(default = "accounts_file")]
    pub accounts_file: PathBuf,
    pub proxy: Option<String>,
    pub diagnostics: Option<PathBuf>,
    pub capture: Option<crate::gateway::CaptureConfig>,
    #[serde(default)]
    pub routing: Routing,
    #[serde(default)]
    pub oauth: OAuth,
    #[serde(default)]
    pub delivery: Delivery,
    #[serde(default)]
    pub resources: Resources,
}
fn listen() -> std::net::SocketAddr {
    ([127, 0, 0, 1], 8080).into()
}
fn models_file() -> std::path::PathBuf {
    "models.json".into()
}
fn accounts_file() -> std::path::PathBuf {
    "accounts.json".into()
}
fn auth_file() -> std::path::PathBuf {
    "auth.json".into()
}

#[derive(Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Routing {
    pub implicit_cache_affinity: bool,
    pub affinity_ttl_ms: u64,
    pub affinity_capacity: usize,
    pub max_wait_ms: u64,
    pub retry_hint_horizon_ms: u64,
}
impl Default for Routing {
    fn default() -> Self {
        let d = Limits::default();
        Self {
            implicit_cache_affinity: d.implicit_cache_affinity,
            affinity_ttl_ms: d.affinity_ttl.as_millis() as u64,
            affinity_capacity: d.affinity_capacity,
            max_wait_ms: d.max_wait.as_millis() as u64,
            retry_hint_horizon_ms: d.retry_hint_horizon.as_millis() as u64,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct OAuth {
    pub scan_interval_ms: u64,
    pub refresh_ahead_seconds: u64,
    pub renewal_timeout_ms: u64,
    pub failed_renewal_delay_ms: u64,
    pub minimum_renewal_interval_ms: u64,
}
impl Default for OAuth {
    fn default() -> Self {
        let d = Limits::default();
        Self {
            scan_interval_ms: d.oauth_scan_interval.as_millis() as u64,
            refresh_ahead_seconds: d.oauth_refresh_ahead_seconds,
            renewal_timeout_ms: d.oauth_renewal_timeout.as_millis() as u64,
            failed_renewal_delay_ms: d.oauth_failed_renewal_delay.as_millis() as u64,
            minimum_renewal_interval_ms: d.oauth_minimum_renewal_interval.as_millis() as u64,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Delivery {
    pub exchange_timeout_ms: u64,
    pub delta_coalescing: Option<crate::execution::delivery::DeltaCoalescing>,
}
impl Default for Delivery {
    fn default() -> Self {
        let d = Limits::default();
        Self {
            exchange_timeout_ms: d.exchange_timeout.as_millis() as u64,
            delta_coalescing: d.delta_coalescing,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Resources {
    pub concurrency: usize,
    pub request_bytes: usize,
    pub response_bytes: usize,
    pub body_timeout_ms: u64,
    pub history_entries: usize,
    pub history_bytes: usize,
    pub history_ttl_ms: u64,
}
impl Default for Resources {
    fn default() -> Self {
        let d = Limits::default();
        Self {
            concurrency: d.concurrency,
            request_bytes: d.request_bytes,
            response_bytes: d.response_bytes,
            body_timeout_ms: d.body_timeout.as_millis() as u64,
            history_entries: d.history_entries,
            history_bytes: d.history_bytes,
            history_ttl_ms: d.history_ttl.as_millis() as u64,
        }
    }
}

impl Configuration {
    pub fn limits(&self) -> Limits {
        Limits {
            implicit_cache_affinity: self.routing.implicit_cache_affinity,
            affinity_ttl: Duration::from_millis(self.routing.affinity_ttl_ms),
            affinity_capacity: self.routing.affinity_capacity,
            max_wait: Duration::from_millis(self.routing.max_wait_ms),
            retry_hint_horizon: Duration::from_millis(self.routing.retry_hint_horizon_ms),
            oauth_scan_interval: Duration::from_millis(self.oauth.scan_interval_ms),
            oauth_refresh_ahead_seconds: self.oauth.refresh_ahead_seconds,
            oauth_renewal_timeout: Duration::from_millis(self.oauth.renewal_timeout_ms),
            oauth_failed_renewal_delay: Duration::from_millis(self.oauth.failed_renewal_delay_ms),
            oauth_minimum_renewal_interval: Duration::from_millis(
                self.oauth.minimum_renewal_interval_ms,
            ),
            delta_coalescing: self.delivery.delta_coalescing,
            exchange_timeout: Duration::from_millis(self.delivery.exchange_timeout_ms),
            concurrency: self.resources.concurrency,
            request_bytes: self.resources.request_bytes,
            response_bytes: self.resources.response_bytes,
            body_timeout: Duration::from_millis(self.resources.body_timeout_ms),
            history_entries: self.resources.history_entries,
            history_bytes: self.resources.history_bytes,
            history_ttl: Duration::from_millis(self.resources.history_ttl_ms),
            ..Limits::default()
        }
    }
}
