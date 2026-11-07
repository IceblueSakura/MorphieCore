//! Private application configuration: bounded reads and value-free diagnostics.
use crate::semantic::value::{JsonLimits, parse_json};
use serde::de::DeserializeOwned;
use std::path::Path;

#[derive(Debug, thiserror::Error, serde::Serialize)]
#[error("{domain}.{field}: {reason}")]
pub struct ConfigurationError {
    pub domain: &'static str,
    pub field: String,
    pub reason: &'static str,
}

impl ConfigurationError {
    pub fn new(domain: &'static str, field: impl Into<String>, reason: &'static str) -> Self {
        Self {
            domain,
            field: field.into(),
            reason,
        }
    }
}

pub(crate) fn read<T: DeserializeOwned>(
    path: &Path,
    domain: &'static str,
) -> Result<T, ConfigurationError> {
    let bytes = crate::credential::read_private_file(path, 65536).map_err(|_| {
        ConfigurationError::new(
            domain,
            "$",
            "file unavailable, oversized or unsafe; check path and private permissions",
        )
    })?;
    let value = parse_json(
        &bytes,
        JsonLimits {
            bytes: 65536,
            depth: 16,
            nodes: 16384,
        },
    )
    .map_err(|_| {
        ConfigurationError::new(domain, "$", "invalid or excessive JSON, or duplicate keys")
    })?;
    serde_path_to_error::deserialize(value).map_err(|error| {
        // Map keys can contain secrets. Report only schema-owned names and sequence indices,
        // never the raw Serde error (which may include rejected values).
        let mut field = String::new();
        for segment in error.path() {
            match segment {
                serde_path_to_error::Segment::Seq { index } => {
                    field.push_str(&format!("[{index}]"))
                }
                serde_path_to_error::Segment::Map { key } => {
                    if !field.is_empty() {
                        field.push('.');
                    }
                    field.push_str(if FIELDS.contains(&key.as_str()) {
                        key
                    } else {
                        "*"
                    });
                }
                _ => {}
            }
        }
        ConfigurationError::new(
            domain,
            if field.is_empty() { "$".into() } else { field },
            "invalid field or type; use the current schema and omit unspecified fields",
        )
    })
}

/// Optional means absent, not null. Explicit clearing is reserved for named nullable fields.
pub(crate) fn optional<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

const FIELDS: &[&str] = &[
    "version",
    "auth_file",
    "bind",
    "credentials_dir",
    "models_file",
    "accounts_file",
    "proxy",
    "diagnostics",
    "capture",
    "routing",
    "oauth",
    "delivery",
    "resources",
    "concurrency",
    "request_bytes",
    "response_bytes",
    "body_timeout_ms",
    "history_entries",
    "history_bytes",
    "history_ttl_ms",
    "exchange_timeout_ms",
    "delta_coalescing",
    "max_delay_ms",
    "max_pending_bytes",
    "implicit_cache_affinity",
    "affinity_ttl_ms",
    "affinity_capacity",
    "max_wait_ms",
    "retry_hint_horizon_ms",
    "scan_interval_ms",
    "refresh_ahead_seconds",
    "renewal_timeout_ms",
    "failed_renewal_delay_ms",
    "minimum_renewal_interval_ms",
    "path",
    "client_request",
    "upstream_request",
    "upstream_response",
    "client_response",
    "headers",
    "body",
    "max_unit_bytes",
    "max_exchange_bytes",
    "max_total_bytes",
    "agents",
    "agent_id",
    "key",
    "name",
    "desc",
    "providers",
    "accounts",
    "enabled",
    "credential",
    "alias",
    "base_url",
    "recovery",
    "recovery_timeout_ms",
    "transient_cooldown_ms",
    "transient_multiplier",
    "transient_max_cooldown_ms",
    "rate_limit_cooldown_ms",
    "rate_limit_multiplier",
    "rate_limit_max_cooldown_ms",
    "reset_hint_floor_ms",
    "rate_limit_reset_max_ms",
    "quota_reset_max_ms",
    "quota_cooldown_ms",
    "auth_cooldown_ms",
    "permission_cooldown_ms",
    "failure_free_reset_ms",
    "models",
    "binding",
    "account",
    "candidates",
    "total_timeout_ms",
    "thinking_tokens",
    "chat_profile",
    "allowed_effort",
    "context_window_tokens",
    "maximum_input_tokens",
    "maximum_output_tokens",
    "default_output_tokens",
    "upstream_model",
    "reasoning_as_text",
    "thinking_conversion",
    "minimal",
    "low",
    "medium",
    "high",
    "xhigh",
    "max",
];
