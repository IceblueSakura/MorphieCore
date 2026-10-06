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
}
impl ApiError {
    pub const fn new(status: StatusCode, code: &'static str) -> Self {
        Self {
            status,
            code,
            fallback: None,
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
        error
    }
    pub fn status(status: u16) -> Self {
        let mut error = if status == 429 {
            Self::new(StatusCode::TOO_MANY_REQUESTS, "upstream_rate_limit")
        } else {
            Self::upstream()
        };
        if let crate::provider::StatusClass::Failure(class) =
            crate::provider::classify_status(status)
        {
            error.fallback = Some(class);
        }
        error
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let kind = if self.status.is_client_error() {
            "invalid_request_error"
        } else {
            "api_error"
        };
        let mut response=(self.status,Json(json!({"error":{"message":"The request could not be completed.","type":kind,"param":null,"code":self.code}}))).into_response();
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
