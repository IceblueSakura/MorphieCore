//! Single bootstrap principal. Only a digest is retained; comparison is constant-time.
use super::StartupError;
use crate::provider::SecretMaterial;
use axum::http::{HeaderMap, header::AUTHORIZATION};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
pub(super) struct Auth {
    pub digest: [u8; 32],
}
impl Auth {
    pub fn new(secret: SecretMaterial) -> Result<Self, StartupError> {
        if !(32..=4096).contains(&secret.expose().len()) {
            return Err(StartupError::Credentials);
        }
        Ok(Self {
            digest: Sha256::digest(secret.expose().as_bytes()).into(),
        })
    }
    /// Exactly one credential: `Authorization: Bearer`, or `x-api-key` as Anthropic
    /// clients (Claude Code with `ANTHROPIC_API_KEY`) send it.
    pub fn accepts(&self, headers: &HeaderMap) -> bool {
        let bearer: Vec<_> = headers.get_all(AUTHORIZATION).iter().collect();
        let key: Vec<_> = headers.get_all("x-api-key").iter().collect();
        let secret = match (bearer.as_slice(), key.as_slice()) {
            ([value], []) => {
                let bytes = value.as_bytes();
                if bytes.len() < 8 || !bytes[..7].eq_ignore_ascii_case(b"Bearer ") {
                    return false;
                }
                &bytes[7..]
            }
            ([], [value]) => value.as_bytes(),
            _ => return false,
        };
        if secret.len() > 4096 {
            return false;
        }
        let digest: [u8; 32] = Sha256::digest(secret).into();
        bool::from(digest.ct_eq(&self.digest))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ingress_key_limit_is_independent_of_oauth_access_size() {
        assert!(Auth::new(SecretMaterial::new(&"x".repeat(4097)).unwrap()).is_err());
    }
    #[test]
    fn rejects_duplicates_and_never_accepts_prefixes_or_alternate_headers() {
        let auth =
            Auth::new(SecretMaterial::new("synthetic-client-identity-00000001").unwrap()).unwrap();
        let mut h = HeaderMap::new();
        assert!(!auth.accepts(&h));
        h.insert(
            AUTHORIZATION,
            "Bearer synthetic-client-identity-00000001".parse().unwrap(),
        );
        assert!(auth.accepts(&h));
        h.append(
            AUTHORIZATION,
            "Bearer synthetic-client-identity-00000001".parse().unwrap(),
        );
        assert!(!auth.accepts(&h));
        h.remove(AUTHORIZATION);
        h.insert(
            "api-key",
            "synthetic-client-identity-00000001".parse().unwrap(),
        );
        assert!(!auth.accepts(&h));
        h.insert(
            AUTHORIZATION,
            "Bearer synthetic-client-identity-00000001-extra"
                .parse()
                .unwrap(),
        );
        assert!(!auth.accepts(&h));
        h.clear();
        h.insert(
            "x-api-key",
            "synthetic-client-identity-00000001".parse().unwrap(),
        );
        assert!(auth.accepts(&h));
        // Two credentials are ambiguous even when both are valid.
        h.insert(
            AUTHORIZATION,
            "Bearer synthetic-client-identity-00000001".parse().unwrap(),
        );
        assert!(!auth.accepts(&h));
    }
}
