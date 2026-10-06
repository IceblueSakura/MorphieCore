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
    pub fn accepts(&self, headers: &HeaderMap) -> bool {
        let mut values = headers.get_all(AUTHORIZATION).iter();
        let Some(value) = values.next() else {
            return false;
        };
        if values.next().is_some() {
            return false;
        }
        let bytes = value.as_bytes();
        if bytes.len() < 8 || bytes.len() > 4103 || !bytes[..7].eq_ignore_ascii_case(b"Bearer ") {
            return false;
        }
        let digest: [u8; 32] = Sha256::digest(&bytes[7..]).into();
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
    }
}
