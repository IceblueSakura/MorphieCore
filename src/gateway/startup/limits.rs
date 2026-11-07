//! Request, response, history and time bounds shared by every task.
use std::time::Duration;
#[derive(Clone, Debug)]
pub struct Limits {
    /// Synthetic intake experiment only; never exposed through configuration.
    #[cfg(test)]
    pub(crate) force_upstream_stream: bool,
    /// Bounded text delivery batching; None explicitly selects immediate encoding.
    pub delta_coalescing: Option<crate::execution::delivery::DeltaCoalescing>,
    /// Disposable implicit affinity only; explicit grouping remains available when disabled.
    pub implicit_cache_affinity: bool,
    /// Success-only account binding retention, independent of prefix hints.
    pub affinity_ttl: Duration,
    pub affinity_capacity: usize,
    pub max_wait: Duration,
    pub retry_hint_horizon: Duration,
    pub oauth_scan_interval: Duration,
    pub oauth_refresh_ahead_seconds: u64,
    pub oauth_renewal_timeout: Duration,
    /// Scheduling only; lifecycle evidence still decides material reuse.
    pub oauth_failed_renewal_delay: Duration,
    /// Preserve short-grant storm protection independently of failed renewal.
    pub oauth_minimum_renewal_interval: Duration,
    /// Normalized Provider-owned settings from the shared account declaration.
    pub provider_recovery: std::collections::BTreeMap<String, crate::provider::RecoveryPolicy>,
    pub request_bytes: usize,
    pub response_bytes: usize,
    /// Decoded output bytes per image; cannot enlarge response or endpoint ceilings.
    pub image_bytes: usize,
    /// Aggregate decoded output bytes, independent of count and JSON limits.
    pub images_bytes: usize,
    /// Complete binary audio bytes, also bounded by endpoint and response ceilings.
    pub speech_bytes: usize,
    /// Whole multipart envelope, independently bounded from JSON ingress.
    pub transcription_request_bytes: usize,
    pub event_bytes: usize,
    pub max_events: usize,
    pub concurrency: usize,
    /// Opt-in response snapshots; payload bytes include retained JSON and typed history.
    pub history_entries: usize,
    pub history_bytes: usize,
    pub history_ttl: Duration,
    pub body_timeout: Duration,
    pub exchange_timeout: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            #[cfg(test)]
            force_upstream_stream: false,
            delta_coalescing: Self::default_delta_coalescing(),
            implicit_cache_affinity: true,
            affinity_ttl: Duration::from_secs(3600),
            affinity_capacity: 4096,
            max_wait: Duration::from_secs(10),
            retry_hint_horizon: Duration::from_secs(60),
            oauth_scan_interval: Duration::from_secs(15),
            oauth_refresh_ahead_seconds: 120,
            oauth_renewal_timeout: Duration::from_secs(90),
            oauth_failed_renewal_delay: Duration::from_secs(60),
            oauth_minimum_renewal_interval: Duration::from_secs(60),
            provider_recovery: Default::default(),
            request_bytes: 32 << 20,
            response_bytes: 64 << 20,
            image_bytes: 2 << 20,
            images_bytes: 2 << 20,
            speech_bytes: 8 << 20,
            transcription_request_bytes: crate::protocol::openai::transcription::MAX_UPLOAD_BYTES,
            event_bytes: 16 << 20,
            max_events: 1_000_000,
            concurrency: 16,
            history_entries: 1024,
            history_bytes: 256 << 20,
            history_ttl: Duration::from_secs(30 * 60),
            body_timeout: Duration::from_secs(60),
            exchange_timeout: Duration::from_secs(60 * 60),
        }
    }
}
impl Limits {
    pub(super) fn default_delta_coalescing() -> Option<crate::execution::delivery::DeltaCoalescing>
    {
        Some(crate::execution::delivery::DeltaCoalescing {
            max_delay_ms: 50,
            max_pending_bytes: 16 << 10,
        })
    }

    pub(super) fn validate(&self) -> Result<(), super::error::StartupError> {
        let checks = [
            (
                "delivery.delta_coalescing",
                self.delta_coalescing.is_none_or(|p| p.valid()),
            ),
            (
                "resources.request_bytes",
                (1..=64 << 20).contains(&self.request_bytes),
            ),
            (
                "resources.response_bytes",
                (1..=64 << 20).contains(&self.response_bytes),
            ),
            (
                "image_bytes",
                (1..=crate::semantic::task::image_generation::MAX_IMAGE_BYTES)
                    .contains(&self.image_bytes),
            ),
            (
                "images_bytes",
                (1..=crate::semantic::task::image_generation::MAX_IMAGES_BYTES)
                    .contains(&self.images_bytes),
            ),
            (
                "speech_bytes",
                (1..=crate::semantic::value::MAX_AUDIO_BYTES).contains(&self.speech_bytes),
            ),
            (
                "transcription_request_bytes",
                (1..=crate::protocol::openai::transcription::MAX_UPLOAD_BYTES)
                    .contains(&self.transcription_request_bytes),
            ),
            ("event_bytes", (1..=64 << 20).contains(&self.event_bytes)),
            ("max_events", (1..=1_000_000).contains(&self.max_events)),
            (
                "resources.concurrency",
                (1..=256).contains(&self.concurrency),
            ),
            (
                "resources.history_entries",
                (1..=4096).contains(&self.history_entries),
            ),
            (
                "resources.history_bytes",
                (1..=1 << 30).contains(&self.history_bytes),
            ),
            (
                "resources.history_ttl_ms",
                !self.history_ttl.is_zero()
                    && self.history_ttl <= Duration::from_secs(24 * 60 * 60),
            ),
            (
                "resources.body_timeout_ms",
                !self.body_timeout.is_zero() && self.body_timeout <= Duration::from_secs(60),
            ),
            (
                "delivery.exchange_timeout_ms",
                !self.exchange_timeout.is_zero()
                    && self.exchange_timeout <= Duration::from_secs(24 * 60 * 60),
            ),
        ];
        if let Some((field, _)) = checks.into_iter().find(|(_, valid)| !valid) {
            return Err(crate::configuration::ConfigurationError::new(
                "gateway", field, "outside supported safety bounds; check the documented resource and delivery limits").into());
        }
        Ok(())
    }
}
