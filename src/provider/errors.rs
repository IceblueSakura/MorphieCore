//! HTTP status and upstream failure classification for provider endpoints.

/// Failure classes shared by provider error handling and downstream reporting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorClass {
    /// Local unavailable material, not an upstream 401 or permission denial.
    CredentialUnavailable,
    Authentication,
    PermissionDenied,
    InvalidRequest,
    NotFound,
    Redirect,
    RateLimit,
    Server,
    Timeout,
    Upstream,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusClass {
    Success,
    Failure(ErrorClass),
}

/// Map an HTTP status to a semantic failure class.
///
/// Redirects are failures: transport must not follow them into untrusted
/// origins. `Timeout` and `Upstream` come from the transport edge, not statuses.
pub fn classify_status(status: u16) -> StatusClass {
    match status {
        200..=299 => StatusClass::Success,
        300..=399 => StatusClass::Failure(ErrorClass::Redirect),
        401 => StatusClass::Failure(ErrorClass::Authentication),
        403 => StatusClass::Failure(ErrorClass::PermissionDenied),
        404 => StatusClass::Failure(ErrorClass::NotFound),
        429 => StatusClass::Failure(ErrorClass::RateLimit),
        400..=499 => StatusClass::Failure(ErrorClass::InvalidRequest),
        500..=599 => StatusClass::Failure(ErrorClass::Server),
        _ => StatusClass::Failure(ErrorClass::Upstream),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProviderError {
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
            (400, StatusClass::Failure(ErrorClass::InvalidRequest)),
            (401, StatusClass::Failure(ErrorClass::Authentication)),
            (403, StatusClass::Failure(ErrorClass::PermissionDenied)),
            (404, StatusClass::Failure(ErrorClass::NotFound)),
            (413, StatusClass::Failure(ErrorClass::InvalidRequest)),
            (422, StatusClass::Failure(ErrorClass::InvalidRequest)),
            (429, StatusClass::Failure(ErrorClass::RateLimit)),
            (500, StatusClass::Failure(ErrorClass::Server)),
            (503, StatusClass::Failure(ErrorClass::Server)),
            (302, StatusClass::Failure(ErrorClass::Redirect)),
        ] {
            assert_eq!(classify_status(status), class, "status {status}");
        }
    }
}
