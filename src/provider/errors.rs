//! HTTP status and upstream failure classification for provider endpoints.

/// Failure classes shared by provider error handling and downstream reporting.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryClass {
    /// Local unavailable material, not an upstream 401 or permission denial.
    CredentialUnavailable,
    Authentication,
    PermissionDenied,
    InvalidRequest,
    NotFound,
    Redirect,
    RateLimit,
    /// Exhausted quota or credit: scoped to the credential, so another may answer.
    Quota,
    Server,
    Timeout,
    Upstream,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusClass {
    Success,
    Failure(RecoveryClass),
}

/// Map an HTTP status to a semantic failure class.
///
/// Redirects are failures: transport must not follow them into untrusted
/// origins. `Timeout` and `Upstream` come from the transport edge, not statuses.
pub fn classify_status(status: u16) -> StatusClass {
    match status {
        200..=299 => StatusClass::Success,
        300..=399 => StatusClass::Failure(RecoveryClass::Redirect),
        401 => StatusClass::Failure(RecoveryClass::Authentication),
        403 => StatusClass::Failure(RecoveryClass::PermissionDenied),
        404 => StatusClass::Failure(RecoveryClass::NotFound),
        429 => StatusClass::Failure(RecoveryClass::RateLimit),
        400..=499 => StatusClass::Failure(RecoveryClass::InvalidRequest),
        500..=599 => StatusClass::Failure(RecoveryClass::Server),
        _ => StatusClass::Failure(RecoveryClass::Upstream),
    }
}

pub use crate::protocol::failure::{SourceFailureKind, classify_failure};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProviderError {
    #[error("invalid embedded provider catalog")]
    InvalidCatalog,
    #[error("invalid provider identity")]
    InvalidId,
    #[error("invalid trusted origin")]
    InvalidOrigin,
    #[error("invalid endpoint path")]
    InvalidPath,
    #[error("invalid credential binding")]
    InvalidBinding,
    #[error("invalid credential material")]
    InvalidSecret,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_classification_is_stable_and_rejects_redirects() {
        for (status, class) in [
            (200, StatusClass::Success),
            (204, StatusClass::Success),
            (400, StatusClass::Failure(RecoveryClass::InvalidRequest)),
            (401, StatusClass::Failure(RecoveryClass::Authentication)),
            (403, StatusClass::Failure(RecoveryClass::PermissionDenied)),
            (404, StatusClass::Failure(RecoveryClass::NotFound)),
            (413, StatusClass::Failure(RecoveryClass::InvalidRequest)),
            (422, StatusClass::Failure(RecoveryClass::InvalidRequest)),
            (429, StatusClass::Failure(RecoveryClass::RateLimit)),
            (500, StatusClass::Failure(RecoveryClass::Server)),
            (503, StatusClass::Failure(RecoveryClass::Server)),
            (302, StatusClass::Failure(RecoveryClass::Redirect)),
        ] {
            assert_eq!(classify_status(status), class, "status {status}");
        }
    }
}
