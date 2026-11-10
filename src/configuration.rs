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
    T::deserialize(value).map_err(|_| {
        ConfigurationError::new(
            domain,
            "$",
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
