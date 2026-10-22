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
    /// Exhausted quota or credit: scoped to the credential, so another may answer.
    Quota,
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

/// What an agent loop does next with a failed turn: compact, stop, wait or retry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureKind {
    ContextOverflow,
    Quota,
    RateLimit,
    Overloaded,
    Other,
}

// Lowercase fragments of bound Providers' error codes, types and messages; the
// same fragments pi (`utils/overflow.ts`, `utils/retry.ts`) and opencode match.
const OVERFLOW: &[&str] = &[
    "context_length_exceeded",
    "context length exceeded",
    "exceeds the context window",
    "maximum context length",
    "context window exceeds limit",
    "model_context_window_exceeded",
    "prompt is too long",
    "prompt too long",
    "request_too_large",
    "input is too long",
    "maximum prompt length",
    "exceeded model token limit",
    "range of input length should be",
    // Gemini: "The input token count (N) exceeds the maximum number of tokens allowed (M)."
    "exceeds the maximum number of tokens allowed",
];
const QUOTA: &[&str] = &[
    "insufficient_quota",
    "quota exceeded",
    "billing",
    "credit balance",
    "insufficient balance",
    // OpenCode Go and Zen subscription limits (`GoUsageLimitError`).
    "usagelimiterror",
];

/// Classify a failed upstream reply from its status and the `code`, `type` and
/// `message` of an OpenAI-compatible or Anthropic error body. Overflow wording
/// counts only on request errors, so throttling text never reads as overflow.
pub fn classify_failure(status: u16, body: &[u8]) -> FailureKind {
    let value: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let error = match value.get("error") {
        Some(serde_json::Value::String(message)) => serde_json::json!({ "message": message }),
        Some(error) => error.clone(),
        // vLLM-style servers report the fields at the top level.
        None => value,
    };
    let text = ["code", "type", "message"]
        .iter()
        .filter_map(|key| error.get(key).and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let has = |fragments: &[&str]| fragments.iter().any(|f| text.contains(f));
    match status {
        401 | 403 => FailureKind::Other,
        402 => FailureKind::Quota,
        400..=499 if has(QUOTA) => FailureKind::Quota,
        429 => FailureKind::RateLimit,
        400 | 413 | 422 if has(OVERFLOW) => FailureKind::ContextOverflow,
        503 | 529 => FailureKind::Overloaded,
        500..=599 if has(&["overloaded"]) => FailureKind::Overloaded,
        _ => FailureKind::Other,
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
