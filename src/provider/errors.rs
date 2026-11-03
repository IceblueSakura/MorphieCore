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

pub use crate::protocol::failure::{SourceFailureKind, classify_failure};

/// Meaning belongs to the configured service, not its wire skin or hostname.
pub(crate) fn classify_source(
    provider: &str,
    status: u16,
    source: crate::protocol::failure::SourceFailureEvidence,
    observed_http: bool,
) -> (
    crate::execution::failure::RecoveryDecision,
    crate::protocol::failure::SourceFailureEvidence,
) {
    use crate::protocol::failure::FailureCode as C;
    use RecoveryClass as R;
    let known = match provider {
        "openai" => match source.code.or(source.error_type) {
            Some(C::InsufficientQuota) => Some(R::Quota),
            Some(C::RateLimit | C::RateLimitType) => Some(R::RateLimit),
            Some(C::Server | C::OverloadedCode) => Some(R::Transient),
            _ => None,
        },
        "chatgpt" => match source.code.or(source.error_type) {
            Some(C::SharingQuota) => Some(R::Quota),
            Some(C::SharingUsageUnavailable | C::SharingUserUnavailable | C::Server) => {
                Some(R::Transient)
            }
            Some(C::SharingInvalidUser) => Some(R::Auth),
            Some(C::SharingCapability) => Some(R::Request),
            Some(C::SharingEligibility | C::SharingRoute | C::SharingScope | C::SharingContext) => {
                Some(R::Permission)
            }
            Some(C::RateLimit) => Some(R::RateLimit),
            _ => None,
        },
        "aliyun-dashscope-cn" => match source.code {
            Some(C::InsufficientQuota | C::AllocationQuota) => Some(R::RateLimit),
            _ => None,
        },
        "aliyun-tokenplan-cn" => match source.code {
            Some(C::InsufficientQuota) => Some(R::Quota),
            _ => None,
        },
        "opencode-go" => match source.error_type {
            Some(
                C::GoCredits
                | C::GoMonthlyLimit
                | C::GoUserLimit
                | C::GoGoUsageLimit
                | C::GoFreeUsageLimit
                | C::GoBlackUsageLimit,
            ) => Some(R::Quota),
            Some(C::GoRateLimit) => Some(R::RateLimit),
            Some(C::GoAuth) => Some(R::Auth),
            Some(C::GoModel) => Some(R::Request),
            Some(C::GoRegion | C::GoDataPolicy) => Some(R::Permission),
            _ => None,
        },
        "openrouter" => match source.canonical {
            Some(C::RouterAuthentication) => Some(R::Auth),
            Some(C::RouterPaymentRequired) => Some(R::Quota),
            Some(C::RouterPermissionDenied) => Some(R::Permission),
            Some(C::RateLimit) => Some(R::RateLimit),
            Some(
                C::RouterProviderOverloaded
                | C::RouterProviderUnavailable
                | C::RouterServer
                | C::RouterTimeout,
            ) => Some(R::Transient),
            Some(
                C::RouterContextLengthExceeded
                | C::RouterInvalidRequest
                | C::RouterContentPolicyViolation,
            ) => Some(R::Request),
            _ => None,
        },
        _ => None,
    };
    let class = known.unwrap_or_else(|| {
        if source.kind == SourceFailureKind::ContextOverflow {
            R::Request
        } else {
            match classify_status(status) {
                StatusClass::Failure(class) => class,
                StatusClass::Success => R::Transient,
            }
        }
    });
    let kind = match class {
        R::Quota => SourceFailureKind::Quota,
        R::RateLimit => SourceFailureKind::RateLimit,
        R::Request if source.kind == SourceFailureKind::ContextOverflow => source.kind,
        R::Transient if matches!(status, 503 | 529) => SourceFailureKind::Overloaded,
        _ => SourceFailureKind::Other,
    };
    let transient = matches!(class, R::Transient);
    let equivalent = match provider {
        "openai" => matches!(
            source.code.or(source.error_type),
            Some(C::Server | C::OverloadedCode)
        ),
        "chatgpt" => matches!(
            source.code,
            Some(C::Server | C::SharingUsageUnavailable | C::SharingUserUnavailable)
        ),
        "openrouter" => matches!(
            source.canonical,
            Some(C::RouterProviderOverloaded | C::RouterProviderUnavailable | C::RouterServer)
        ),
        _ => false,
    };
    // A codec's synthetic 500 cannot turn an unknown event into a resend.
    let numeric = source.numeric_status;
    let retryable = transient
        && (equivalent
            || ((observed_http || numeric) && matches!(status, 408 | 500 | 502 | 503 | 504)));
    (
        crate::execution::failure::RecoveryDecision {
            class: Some(class),
            affects_health: matches!(
                class,
                R::Auth | R::Permission | R::Quota | R::RateLimit | R::Transient
            ),
            retryable,
        },
        {
            let mut evidence = source;
            evidence.kind = kind;
            evidence
        },
    )
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
    use crate::protocol::failure::{Observation, SourceFailureEvidence};

    #[test]
    fn service_codes_override_status_without_global_message_or_protocol_inference() {
        for (provider, status, body, class, retry) in [
            (
                "openai",
                429,
                r#"{"error":{"code":"insufficient_quota"}}"#,
                RecoveryClass::Quota,
                false,
            ),
            (
                "aliyun-dashscope-cn",
                429,
                r#"{"code":"insufficient_quota"}"#,
                RecoveryClass::RateLimit,
                false,
            ),
            (
                "aliyun-tokenplan-cn",
                429,
                r#"{"code":"insufficient_quota"}"#,
                RecoveryClass::Quota,
                false,
            ),
            (
                "opencode-go",
                401,
                r#"{"type":"error","error":{"type":"CreditsError"}}"#,
                RecoveryClass::Quota,
                false,
            ),
            (
                "opencode-go",
                401,
                r#"{"error":{"type":"ModelError"}}"#,
                RecoveryClass::Request,
                false,
            ),
            (
                "chatgpt",
                503,
                r#"{"error":{"code":"subscription_sharing_usage_limit_exceeded"}}"#,
                RecoveryClass::Quota,
                false,
            ),
            (
                "chatgpt",
                503,
                r#"{"error":{"code":"subscription_sharing_usage_unavailable"}}"#,
                RecoveryClass::Transient,
                true,
            ),
            (
                "openrouter",
                500,
                r#"{"error":{"code":"server_error"},"error_type":"authentication"}"#,
                RecoveryClass::Auth,
                false,
            ),
            (
                "openrouter",
                403,
                r#"{"error":{"type":"permission_error","error_type":"content_policy_violation"}}"#,
                RecoveryClass::Request,
                false,
            ),
            (
                "openrouter",
                500,
                r#"{"error":{"metadata":{"error_type":"payment_required"}}}"#,
                RecoveryClass::Quota,
                false,
            ),
            (
                "deepseek",
                429,
                r#"{"error":{"message":"billing credit balance insufficient_quota"}}"#,
                RecoveryClass::RateLimit,
                false,
            ),
            (
                "deepseek",
                419,
                r#"{"error":{"code":"unknown"}}"#,
                RecoveryClass::Request,
                false,
            ),
        ] {
            let source =
                SourceFailureEvidence::http(status, body.as_bytes(), Observation::Complete);
            let (decision, _) = classify_source(provider, status, source, true);
            assert_eq!(decision.class, Some(class), "{provider}: {body}");
            assert_eq!(decision.retryable, retry, "{provider}: {body}");
        }
    }

    #[test]
    fn server_allowlist_and_positive_in_band_evidence_are_independent_of_health() {
        for (code, kind) in [
            (
                Some("server_is_overloaded"),
                Some("service_unavailable_error"),
            ),
            (None, Some("server_error")),
        ] {
            let source = crate::protocol::openai::terminal::reported_failure(
                None,
                code,
                kind,
                Some("synthetic failure"),
            );
            let status = source.status_hint.unwrap().get();
            assert!(classify_source("openai", status, source, false).0.retryable);
            assert!(
                !classify_source("deepseek", status, source, false)
                    .0
                    .retryable
            );
        }
        for code in [
            "subscription_sharing_usage_unavailable",
            "subscription_sharing_user_unavailable",
        ] {
            let source =
                crate::protocol::openai::terminal::reported_failure(None, Some(code), None, None);
            assert!(classify_source("chatgpt", 500, source, false).0.retryable);
            assert!(!classify_source("openai", 500, source, false).0.retryable);
        }
        for status in [408, 500, 502, 503, 504, 501, 507, 529] {
            let source = SourceFailureEvidence::http(status, b"", Observation::Complete);
            let (decision, _) = classify_source("openai", status, source, true);
            assert_eq!(decision.class, Some(RecoveryClass::Transient));
            assert_eq!(
                decision.retryable,
                matches!(status, 408 | 500 | 502 | 503 | 504)
            );
        }
        let unknown = crate::protocol::openai::terminal::reported_failure(
            None,
            Some("unknown"),
            None,
            Some("overloaded billing"),
        );
        assert!(!classify_source("openai", 500, unknown, false).0.retryable);
        let numeric =
            crate::protocol::openai::terminal::reported_failure(Some(503), None, None, None);
        assert!(
            classify_source("openrouter", 503, numeric, false)
                .0
                .retryable
        );
        let quota = SourceFailureEvidence::http(
            503,
            br#"{"error":{"code":"insufficient_quota"}}"#,
            Observation::Complete,
        );
        assert!(!classify_source("openai", 503, quota, true).0.retryable);
    }

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
