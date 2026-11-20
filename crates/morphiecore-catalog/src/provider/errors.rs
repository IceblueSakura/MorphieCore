//! HTTP status and upstream failure classification for provider endpoints.

/// Failure classes shared by provider error handling and downstream reporting.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryClass {
    Request,
    Malformed,
    Transient,
    RateLimit,
    Quota,
    Auth,
    Permission,
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
        300..=399 => StatusClass::Failure(RecoveryClass::Request),
        401 => StatusClass::Failure(RecoveryClass::Auth),
        402 => StatusClass::Failure(RecoveryClass::Quota),
        408 => StatusClass::Failure(RecoveryClass::Transient),
        403 => StatusClass::Failure(RecoveryClass::Permission),
        404 => StatusClass::Failure(RecoveryClass::Request),
        429 => StatusClass::Failure(RecoveryClass::RateLimit),
        400..=499 => StatusClass::Failure(RecoveryClass::Request),
        500..=599 => StatusClass::Failure(RecoveryClass::Transient),
        _ => StatusClass::Failure(RecoveryClass::Transient),
    }
}

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
            (400, StatusClass::Failure(RecoveryClass::Request)),
            (401, StatusClass::Failure(RecoveryClass::Auth)),
            (403, StatusClass::Failure(RecoveryClass::Permission)),
            (404, StatusClass::Failure(RecoveryClass::Request)),
            (413, StatusClass::Failure(RecoveryClass::Request)),
            (422, StatusClass::Failure(RecoveryClass::Request)),
            (429, StatusClass::Failure(RecoveryClass::RateLimit)),
            (500, StatusClass::Failure(RecoveryClass::Transient)),
            (503, StatusClass::Failure(RecoveryClass::Transient)),
            (302, StatusClass::Failure(RecoveryClass::Request)),
        ] {
            assert_eq!(classify_status(status), class, "status {status}");
        }
    }
}
