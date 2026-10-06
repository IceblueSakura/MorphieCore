//! Small OAuth mechanisms, not product token defaults or a workflow engine.
use super::{CredentialError as Error, Secret};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use oauth2::{PkceCodeChallenge, PkceCodeVerifier};
use serde::Deserialize;

/// Missing may inherit by the owning grant's contract; explicit null is not missing.
pub(super) fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}
pub(super) fn challenge(verifier: &str) -> Result<PkceCodeChallenge, Error> {
    // oauth2 asserts the RFC length; external verifier data must fail, not panic.
    if !(43..=128).contains(&verifier.len())
        || !verifier
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-._~".contains(&c))
    {
        return Err(Error::Protocol);
    }
    Ok(PkceCodeChallenge::from_code_verifier_sha256(
        &PkceCodeVerifier::new(verifier.into()),
    ))
}
pub(super) fn random_token() -> Result<Secret, Error> {
    // Keep entropy failure fallible instead of using the library's infallible RNG.
    let mut bytes = zeroize::Zeroizing::new([0; 32]);
    getrandom::fill(&mut bytes[..]).map_err(|_| Error::Protocol)?;
    Secret::new(B64.encode(&bytes[..]))
}
/// Shared RFC 6749 code exchange. Paths and registration come from the driver;
/// token schema and error interpretation do not belong to this mechanism.
pub(super) struct CodeExchange<'a> {
    pub client: &'a str,
    pub code: &'a str,
    pub redirect: &'a str,
    pub verifier: &'a str,
}
pub(super) async fn exchange_code(
    http: &super::http::AuthHttp,
    path: &str,
    exchange: CodeExchange<'_>,
    metadata: &[(&str, &str)],
    deadline: tokio::time::Instant,
) -> Result<(u16, super::SecretBytes), Error> {
    http.request_with_metadata(
        path,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", exchange.client),
            ("code", exchange.code),
            ("redirect_uri", exchange.redirect),
            ("code_verifier", exchange.verifier),
        ],
        None,
        metadata,
        deadline,
    )
    .await
}
/// The caller decides whether the first poll waits and how wire errors change interval.
pub(super) async fn wait_for_poll(
    interval: std::time::Duration,
    deadline: tokio::time::Instant,
) -> Result<(), Error> {
    let wake = tokio::time::Instant::now()
        .checked_add(interval)
        .ok_or(Error::Expired)?;
    if wake >= deadline {
        return Err(Error::Expired);
    }
    tokio::time::sleep_until(wake).await;
    Ok(())
}
/// One deadline covers human interaction, exchange and identity verification.
pub(super) async fn login_deadline<T>(
    deadline: tokio::time::Instant,
    operation: impl std::future::Future<Output = Result<T, Error>>,
) -> Result<T, Error> {
    let result = tokio::time::timeout_at(deadline, operation)
        .await
        .map_err(|_| Error::Expired)?;
    if tokio::time::Instant::now() >= deadline {
        return Err(Error::Expired);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pkce_checks_external_lengths_and_alphabet_before_calling_library() {
        for bad in ["x".repeat(42), "x".repeat(129), "!".repeat(43)] {
            assert!(challenge(&bad).is_err());
        }
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")
                .unwrap()
                .as_str(),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
}
