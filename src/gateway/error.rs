//! Stable client errors never include upstream bodies, URLs, credentials or parser input.
use crate::{execution::failure::RecoveryDecision, protocol::failure::SourceFailureEvidence};
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
/// Gateway failure meaning. Native wire spelling is a presentation, never policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ErrorKind {
    InvalidRequest,
    Upstream,
    Timeout,
    Shutdown,
    ContextOverflow,
    Quota,
    RateLimit,
    Overloaded,
    Unavailable,
    CredentialUnavailable,
    EntropyUnavailable,
    GatewayBusy,
    HistoryCapacity,
    ResponseIdCollision,
    ResponseNotFound,
    UnsupportedMediaType,
    RequestTooLarge,
    RequestTimeout,
    ModelNotFound,
    NotFound,
    MethodNotAllowed,
    Unauthorized,
    ModelDeletionForbidden,
}
impl ErrorKind {
    pub const fn diagnostic_code(self) -> &'static str {
        match self {
            Self::InvalidRequest => "gateway.invalid_request",
            Self::Upstream => "gateway.upstream",
            Self::Timeout => "gateway.timeout",
            Self::Shutdown => "gateway.shutdown",
            Self::ContextOverflow => "gateway.context_overflow",
            Self::Quota => "gateway.quota",
            Self::RateLimit => "gateway.rate_limit",
            Self::Overloaded => "gateway.overloaded",
            Self::Unavailable => "gateway.unavailable",
            Self::CredentialUnavailable => "gateway.credential_unavailable",
            Self::EntropyUnavailable => "gateway.entropy_unavailable",
            Self::GatewayBusy => "gateway.gateway_busy",
            Self::HistoryCapacity => "gateway.history_capacity",
            Self::ResponseIdCollision => "gateway.response_id_collision",
            Self::ResponseNotFound => "gateway.response_not_found",
            Self::UnsupportedMediaType => "gateway.unsupported_media_type",
            Self::RequestTooLarge => "gateway.request_too_large",
            Self::RequestTimeout => "gateway.request_timeout",
            Self::ModelNotFound => "gateway.model_not_found",
            Self::NotFound => "gateway.not_found",
            Self::MethodNotAllowed => "gateway.method_not_allowed",
            Self::Unauthorized => "gateway.unauthorized",
            Self::ModelDeletionForbidden => "gateway.model_deletion_forbidden",
        }
    }
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::Upstream => "upstream_error",
            Self::Timeout => "upstream_timeout",
            Self::Shutdown => "shutting_down",
            Self::ContextOverflow => "context_length_exceeded",
            Self::Quota => "insufficient_quota",
            Self::RateLimit => "rate_limit_exceeded",
            Self::Overloaded => "server_is_overloaded",
            Self::Unavailable => "upstream_unavailable",
            Self::CredentialUnavailable => "credential_unavailable",
            Self::EntropyUnavailable => "entropy_unavailable",
            Self::GatewayBusy => "gateway_busy",
            Self::HistoryCapacity => "history_capacity_exceeded",
            Self::ResponseIdCollision => "response_id_collision",
            Self::ResponseNotFound => "response_not_found",
            Self::UnsupportedMediaType => "unsupported_media_type",
            Self::RequestTooLarge => "request_too_large",
            Self::RequestTimeout => "request_timeout",
            Self::ModelNotFound => "model_not_found",
            Self::NotFound => "not_found",
            Self::MethodNotAllowed => "method_not_allowed",
            Self::Unauthorized => "unauthorized",
            Self::ModelDeletionForbidden => "model_deletion_forbidden",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) struct ApiError {
    pub status: StatusCode,
    pub kind: ErrorKind,
    /// Pure policy decision, independent of protocol presentation.
    pub recovery: RecoveryDecision,
    /// Upstream-reported reset in seconds, sent as `Retry-After` on rate limits.
    pub retry_after: Option<u64>,
}
impl ApiError {
    pub const fn diagnostic_code(&self) -> &'static str {
        self.kind.diagnostic_code()
    }
    pub const fn code(&self) -> &'static str {
        self.kind.code()
    }
    pub const fn new(status: StatusCode, kind: ErrorKind) -> Self {
        Self {
            status,
            kind,
            recovery: RecoveryDecision {
                class: None,
                affects_health: false,
            },
            retry_after: None,
        }
    }
    pub const fn invalid() -> Self {
        Self::new(StatusCode::BAD_REQUEST, ErrorKind::InvalidRequest)
    }
    pub const fn upstream() -> Self {
        Self::new(StatusCode::BAD_GATEWAY, ErrorKind::Upstream)
    }
    pub const fn timeout() -> Self {
        Self {
            recovery: RecoveryDecision {
                class: Some(crate::provider::RecoveryClass::Timeout),
                affects_health: false,
            },
            ..Self::new(StatusCode::GATEWAY_TIMEOUT, ErrorKind::Timeout)
        }
    }
    pub const fn shutdown() -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, ErrorKind::Shutdown)
    }
    pub fn transport(class: crate::provider::RecoveryClass) -> Self {
        let mut error = if class == crate::provider::RecoveryClass::Timeout {
            Self::timeout()
        } else {
            Self::upstream()
        };
        error.recovery = RecoveryDecision::transport(class);
        error
    }
    pub fn status(status: u16) -> Self {
        Self::failure(status, b"", None)
    }
    /// A failed upstream reply, rendered as the class agents act on (roadmap Phase 2).
    pub fn failure(status: u16, body: &[u8], retry_after: Option<u64>) -> Self {
        Self::classified(
            status,
            crate::provider::classify_failure(status, body),
            retry_after,
        )
    }
    /// An in-band stream failure the codec already classified from its envelope.
    pub fn source_failure(source: SourceFailureEvidence) -> Option<Self> {
        let status = source.status_hint?.get();
        Some(Self::classified(status, source.kind, None))
    }
    pub(super) fn classified(
        status: u16,
        kind: crate::provider::SourceFailureKind,
        retry_after: Option<u64>,
    ) -> Self {
        use crate::provider::SourceFailureKind;
        let mut error = match kind {
            SourceFailureKind::ContextOverflow => {
                Self::new(StatusCode::BAD_REQUEST, ErrorKind::ContextOverflow)
            }
            SourceFailureKind::Quota => Self::new(StatusCode::TOO_MANY_REQUESTS, ErrorKind::Quota),
            SourceFailureKind::RateLimit => Self {
                retry_after,
                ..Self::new(StatusCode::TOO_MANY_REQUESTS, ErrorKind::RateLimit)
            },
            SourceFailureKind::Overloaded => {
                Self::new(StatusCode::SERVICE_UNAVAILABLE, ErrorKind::Overloaded)
            }
            SourceFailureKind::Other => Self::upstream(),
        };
        error.recovery = RecoveryDecision::source(status, kind);
        error.retry_after = retry_after;
        error
    }
    pub fn unavailable(seconds: u64) -> Self {
        Self {
            retry_after: Some(seconds),
            ..Self::new(StatusCode::SERVICE_UNAVAILABLE, ErrorKind::Unavailable)
        }
    }
}
impl ApiError {
    /// Classified request failures returned as standard Responses error events.
    pub fn streams_as_failed(&self) -> bool {
        matches!(self.kind, ErrorKind::ContextOverflow | ErrorKind::Quota)
    }
    /// The OpenAI error envelope, for HTTP bodies and stream failure frames.
    pub fn body(&self) -> serde_json::Value {
        // OpenAI's codes and wording; pi matches the message, opencode the body.
        let (kind, message) = match self.kind {
            ErrorKind::ContextOverflow => (
                "invalid_request_error",
                "Your input exceeds the context window of this model. Please adjust your input and try again.".into(),
            ),
            ErrorKind::Quota => (
                "insufficient_quota",
                "The upstream quota is exhausted; check the plan and billing details.".into(),
            ),
            ErrorKind::RateLimit => (
                "rate_limit_error",
                match self.retry_after {
                    Some(seconds) => format!("Rate limit reached. Please try again in {seconds}s."),
                    None => "Rate limit reached. Please try again later.".into(),
                },
            ),
            ErrorKind::Overloaded => (
                "server_error",
                "The upstream is overloaded. Please retry your request.".into(),
            ),
            ErrorKind::ModelNotFound => (
                "invalid_request_error",
                "The requested model does not exist or is not available for this endpoint.".into(),
            ),
            ErrorKind::Unauthorized => (
                "invalid_request_error",
                "Invalid or missing authentication credentials.".into(),
            ),
            ErrorKind::ResponseNotFound => (
                "invalid_request_error",
                "The specified previous response ID was not found.".into(),
            ),
            ErrorKind::HistoryCapacity => (
                "invalid_request_error",
                "Conversation history capacity limit exceeded.".into(),
            ),
            ErrorKind::UnsupportedMediaType => (
                "invalid_request_error",
                "Unsupported media type or Content-Type header.".into(),
            ),
            ErrorKind::RequestTooLarge => (
                "invalid_request_error",
                "Request payload exceeds the maximum permitted size limit.".into(),
            ),
            ErrorKind::RequestTimeout => (
                "invalid_request_error",
                "Request intake timed out before completion.".into(),
            ),
            ErrorKind::Timeout => (
                "api_error",
                "The upstream request timed out.".into(),
            ),
            ErrorKind::Unavailable => (
                "api_error",
                "The upstream provider is temporarily unavailable.".into(),
            ),
            ErrorKind::CredentialUnavailable => (
                "api_error",
                "No valid credentials available for the target route.".into(),
            ),
            ErrorKind::Shutdown => (
                "server_error",
                "The gateway is shutting down and cannot accept new requests.".into(),
            ),
            ErrorKind::GatewayBusy => (
                "server_error",
                "The gateway is currently handling too many concurrent requests.".into(),
            ),
            ErrorKind::NotFound => (
                "invalid_request_error",
                "The requested resource was not found.".into(),
            ),
            ErrorKind::MethodNotAllowed => (
                "invalid_request_error",
                "HTTP method not allowed for this route.".into(),
            ),
            ErrorKind::ModelDeletionForbidden => (
                "invalid_request_error",
                "Model deletion is not permitted.".into(),
            ),
            _ if self.status.is_client_error() => (
                "invalid_request_error",
                format!("The request could not be completed ({}).", self.code()),
            ),
            _ => (
                "api_error",
                format!("The request could not be completed ({}).", self.code()),
            ),
        };
        json!({"error":{"message":message,"type":kind,"param":null,"code":self.code()}})
    }
}
impl ApiError {
    /// Anthropic's error envelope for Messages clients, with the wording Claude
    /// Code reacts to (context overflow reads "prompt is too long").
    pub fn into_messages_response(self) -> Response {
        let (status, body) = self.messages_error();
        let mut response = (status, Json(body)).into_response();
        if status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                "www-authenticate",
                axum::http::HeaderValue::from_static("Bearer"),
            );
        }
        if let Some(seconds) = self.retry_after {
            response
                .headers_mut()
                .insert("retry-after", axum::http::HeaderValue::from(seconds));
        }
        response.headers_mut().insert(
            "cache-control",
            axum::http::HeaderValue::from_static("no-store"),
        );
        response
    }
    /// Anthropic's status and `{"type":"error","error":…}` body.
    pub fn messages_error(&self) -> (StatusCode, serde_json::Value) {
        let (status, kind, message): (StatusCode, &str, String) = match self.kind {
            ErrorKind::ContextOverflow => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "prompt is too long: the input exceeds the context window of this model".into(),
            ),
            ErrorKind::Quota => (
                StatusCode::PAYMENT_REQUIRED,
                "billing_error",
                "The upstream credit balance is too low; check the plan and billing details."
                    .into(),
            ),
            ErrorKind::RateLimit => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limit_error",
                "Rate limit reached. Please retry later.".into(),
            ),
            ErrorKind::Overloaded => (
                StatusCode::from_u16(529).expect("valid status"),
                "overloaded_error",
                "Overloaded".into(),
            ),
            ErrorKind::ModelNotFound => (
                StatusCode::NOT_FOUND,
                "not_found_error",
                "The requested model does not exist or is not available for this endpoint.".into(),
            ),
            ErrorKind::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "authentication_error",
                "Invalid or missing authentication credentials.".into(),
            ),
            _ => {
                let kind = match self.status.as_u16() {
                    401 => "authentication_error",
                    403 => "permission_error",
                    404 => "not_found_error",
                    429 => "rate_limit_error",
                    400..=499 => "invalid_request_error",
                    _ => "api_error",
                };
                // The fixed class is the only diagnosis Messages clients can show.
                (
                    self.status,
                    kind,
                    format!("The request could not be completed ({}).", self.code()),
                )
            }
        };
        (
            status,
            json!({"type":"error","error":{"type":kind,"message":message}}),
        )
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(self.body())).into_response();
        if let Some(seconds) = self.retry_after {
            response
                .headers_mut()
                .insert("retry-after", axum::http::HeaderValue::from(seconds));
        }
        response.headers_mut().insert(
            "cache-control",
            axum::http::HeaderValue::from_static("no-store"),
        );
        if self.status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                "www-authenticate",
                axum::http::HeaderValue::from_static("Bearer"),
            );
        }
        response
    }
}
/// Upstream `Retry-After` in seconds (delta or HTTP date), at most one day.
pub(super) fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    retry_after_at(headers, std::time::SystemTime::now())
}
fn retry_after_at(headers: &reqwest::header::HeaderMap, now: std::time::SystemTime) -> Option<u64> {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty() && v.len() <= 128)
        .and_then(|v| {
            if v.len() <= 5 && v.bytes().all(|b| b.is_ascii_digit()) {
                v.parse::<u64>().ok()
            } else {
                httpdate::parse_http_date(v).ok().map(|date| {
                    let wait = date.duration_since(now).unwrap_or_default();
                    // A dispatch gate must not round a not-before date down.
                    wait.as_secs() + u64::from(wait.subsec_nanos() != 0)
                })
            }
        })
        .filter(|n| *n <= 86400)
}

#[cfg(test)]
#[test]
fn retry_after_dates_never_round_dispatch_earlier_and_stay_bounded() {
    // HTTP dates and delta seconds share the selected dispatch bound.
    use std::time::{Duration, UNIX_EPOCH};
    let mut headers = reqwest::header::HeaderMap::new();
    for (value, expected) in [
        ("0", Some(0)),
        ("86400", Some(86400)),
        ("86401", None),
        ("-1", None),
    ] {
        headers.insert("retry-after", value.parse().unwrap());
        assert_eq!(retry_after_at(&headers, UNIX_EPOCH), expected);
    }
    headers.insert(
        "retry-after",
        "Fri, 02 Jan 1970 00:00:00 GMT".parse().unwrap(),
    );
    assert_eq!(
        retry_after_at(&headers, UNIX_EPOCH + Duration::from_millis(86_398_500)),
        Some(2)
    );
    assert_eq!(
        retry_after_at(&headers, UNIX_EPOCH + Duration::from_secs(86401)),
        Some(0)
    );
}

#[cfg(test)]
#[test]
fn recovery_is_independent_of_native_error_presentation() {
    use crate::{protocol::failure::SourceFailureKind, provider::RecoveryClass};
    for (status, kind, class, messages_status, messages_type) in [
        (
            429,
            SourceFailureKind::Quota,
            RecoveryClass::Quota,
            402,
            "billing_error",
        ),
        (
            429,
            SourceFailureKind::RateLimit,
            RecoveryClass::RateLimit,
            429,
            "rate_limit_error",
        ),
        (
            503,
            SourceFailureKind::Overloaded,
            RecoveryClass::Server,
            529,
            "overloaded_error",
        ),
    ] {
        let error = ApiError::classified(status, kind, Some(7));
        let before = error.recovery;
        assert_eq!(before.class, Some(class));
        let (status, body) = error.messages_error();
        assert_eq!(status.as_u16(), messages_status);
        assert_eq!(body["error"]["type"], messages_type);
        assert_eq!(error.recovery, before);
        assert_eq!(error.retry_after, Some(7));
    }
    for (status, class) in [
        (401, RecoveryClass::Authentication),
        (403, RecoveryClass::PermissionDenied),
    ] {
        let error = ApiError::failure(status, br#"{"error":{"code":"insufficient_quota"}}"#, None);
        assert_eq!(error.recovery.class, Some(class));
        assert!(!error.streams_as_failed());
    }
}
