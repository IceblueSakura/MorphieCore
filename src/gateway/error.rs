//! Stable client errors never include upstream bodies, URLs, credentials or parser input.
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
#[derive(Clone, Copy, Debug)]
pub(super) struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    /// Execution-only classification; never part of the downstream error body.
    pub fallback: Option<crate::provider::ErrorClass>,
    /// Only upstream/local-credential evidence may affect executor health.
    pub health: bool,
    /// Upstream-reported reset in seconds, sent as `Retry-After` on rate limits.
    pub retry_after: Option<u64>,
}
impl ApiError {
    pub const fn new(status: StatusCode, code: &'static str) -> Self {
        Self {
            status,
            code,
            fallback: None,
            health: false,
            retry_after: None,
        }
    }
    pub const fn invalid() -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request")
    }
    pub const fn upstream() -> Self {
        Self::new(StatusCode::BAD_GATEWAY, "upstream_error")
    }
    pub const fn timeout() -> Self {
        Self {
            fallback: Some(crate::provider::ErrorClass::Timeout),
            ..Self::new(StatusCode::GATEWAY_TIMEOUT, "upstream_timeout")
        }
    }
    pub const fn shutdown() -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, "shutting_down")
    }
    pub fn transport(class: crate::provider::ErrorClass) -> Self {
        let mut error = if class == crate::provider::ErrorClass::Timeout {
            Self::timeout()
        } else {
            Self::upstream()
        };
        error.fallback = Some(class);
        error.health = true;
        error
    }
    pub fn status(status: u16) -> Self {
        Self::failure(status, b"", None)
    }
    /// A failed upstream reply, rendered as the class agents act on (roadmap Phase 2).
    pub fn failure(status: u16, body: &[u8], retry_after: Option<u64>) -> Self {
        use crate::provider::FailureKind;
        let mut error = match crate::provider::classify_failure(status, body) {
            FailureKind::ContextOverflow => {
                Self::new(StatusCode::BAD_REQUEST, "context_length_exceeded")
            }
            FailureKind::Quota => Self::new(StatusCode::TOO_MANY_REQUESTS, "insufficient_quota"),
            FailureKind::RateLimit => Self {
                retry_after,
                ..Self::new(StatusCode::TOO_MANY_REQUESTS, "rate_limit_exceeded")
            },
            FailureKind::Overloaded => {
                Self::new(StatusCode::SERVICE_UNAVAILABLE, "server_is_overloaded")
            }
            FailureKind::Other => Self::upstream(),
        };
        if let crate::provider::StatusClass::Failure(class) =
            crate::provider::classify_status(status)
        {
            error.fallback = Some(class);
        }
        if error.code == "insufficient_quota" {
            error.fallback = Some(crate::provider::ErrorClass::Quota);
        }
        error.retry_after = retry_after;
        error.health = true;
        error
    }
    pub fn unavailable(seconds: u64) -> Self {
        Self {
            retry_after: Some(seconds),
            ..Self::new(StatusCode::SERVICE_UNAVAILABLE, "upstream_unavailable")
        }
    }
}
impl ApiError {
    /// Classified request failures returned as standard Responses error events.
    pub fn streams_as_failed(&self) -> bool {
        matches!(self.code, "context_length_exceeded" | "insufficient_quota")
    }
    /// The OpenAI error envelope, for HTTP bodies and stream failure frames.
    pub fn body(&self) -> serde_json::Value {
        // OpenAI's codes and wording; pi matches the message, opencode the body.
        let (kind, message) = match self.code {
            "context_length_exceeded" => (
                "invalid_request_error",
                "Your input exceeds the context window of this model. Please adjust your input and try again.".into(),
            ),
            "insufficient_quota" => (
                "insufficient_quota",
                "The upstream quota is exhausted; check the plan and billing details.".into(),
            ),
            "rate_limit_exceeded" => (
                "rate_limit_error",
                match self.retry_after {
                    Some(seconds) => format!("Rate limit reached. Please try again in {seconds}s."),
                    None => "Rate limit reached. Please try again later.".into(),
                },
            ),
            "server_is_overloaded" => (
                "server_error",
                "The upstream is overloaded. Please retry your request.".into(),
            ),
            _ if self.status.is_client_error() => (
                "invalid_request_error",
                "The request could not be completed.".into(),
            ),
            _ => ("api_error", "The request could not be completed.".to_string()),
        };
        json!({"error":{"message":message,"type":kind,"param":null,"code":self.code}})
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
        let (status, kind, message): (StatusCode, &str, String) = match self.code {
            "context_length_exceeded" => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "prompt is too long: the input exceeds the context window of this model".into(),
            ),
            "insufficient_quota" => (
                StatusCode::PAYMENT_REQUIRED,
                "billing_error",
                "The upstream credit balance is too low; check the plan and billing details."
                    .into(),
            ),
            "rate_limit_exceeded" => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limit_error",
                "Rate limit reached. Please retry later.".into(),
            ),
            "server_is_overloaded" => (
                StatusCode::from_u16(529).expect("valid status"),
                "overloaded_error",
                "Overloaded".into(),
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
                    format!("The request could not be completed ({}).", self.code),
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
