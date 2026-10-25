//! Startup-fixed Agent keys. Only digests are retained; comparisons are constant-time.
use crate::gateway::StartupError;
use crate::provider::SecretMaterial;
use axum::http::{HeaderMap, header::AUTHORIZATION};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};
use subtle::ConstantTimeEq;

/// Trusted admission identity, never supplied by a business header or body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::gateway) struct AuthenticatedAgent {
    pub agent_id: Arc<str>,
}

struct AgentKey {
    agent: AuthenticatedAgent,
    digest: [u8; 32],
}

pub(in crate::gateway) struct Auth {
    /// Private configuration namespace, not an Agent or replay authority.
    pub digest: [u8; 32],
    keys: Vec<AgentKey>,
}
impl Auth {
    pub fn new(agents: BTreeMap<String, SecretMaterial>) -> Result<Self, StartupError> {
        if agents.is_empty() || agents.len() > 256 {
            return Err(StartupError::Credentials);
        }
        let mut keys: Vec<AgentKey> = Vec::with_capacity(agents.len());
        let mut namespace = Sha256::new();
        namespace.update(b"morphiecore.agent-keys.v1");
        for (id, secret) in agents {
            let valid_id = (1..=128).contains(&id.len())
                && id.as_bytes()[0].is_ascii_alphanumeric()
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c));
            if !valid_id || !(32..=4096).contains(&secret.expose().len()) {
                return Err(StartupError::Credentials);
            }
            let digest: [u8; 32] = Sha256::digest(secret.expose().as_bytes()).into();
            if keys.iter().any(|key| bool::from(digest.ct_eq(&key.digest))) {
                return Err(StartupError::Credentials);
            }
            namespace.update((id.len() as u64).to_le_bytes());
            namespace.update(id.as_bytes());
            namespace.update(digest);
            keys.push(AgentKey {
                agent: AuthenticatedAgent {
                    agent_id: id.into(),
                },
                digest,
            });
        }
        Ok(Self {
            digest: namespace.finalize().into(),
            keys,
        })
    }
    /// Exactly one credential: `Authorization: Bearer`, or `x-api-key` as Anthropic
    /// clients (Claude Code with `ANTHROPIC_API_KEY`) send it.
    pub fn authenticate(&self, headers: &HeaderMap) -> Option<AuthenticatedAgent> {
        let bearer: Vec<_> = headers.get_all(AUTHORIZATION).iter().collect();
        let key: Vec<_> = headers.get_all("x-api-key").iter().collect();
        let secret = match (bearer.as_slice(), key.as_slice()) {
            ([value], []) => {
                let bytes = value.as_bytes();
                if bytes.len() < 8 || !bytes[..7].eq_ignore_ascii_case(b"Bearer ") {
                    return None;
                }
                &bytes[7..]
            }
            ([], [value]) => value.as_bytes(),
            _ => return None,
        };
        if secret.len() > 4096 {
            return None;
        }
        let digest: [u8; 32] = Sha256::digest(secret).into();
        let mut matched = None;
        // Compare the complete bounded table, without an early successful return.
        for key in &self.keys {
            if bool::from(digest.ct_eq(&key.digest)) {
                matched = Some(key.agent.clone());
            }
        }
        matched
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn auth() -> Auth {
        Auth::new(BTreeMap::from([(
            "agent".into(),
            SecretMaterial::new("synthetic-client-identity-00000001").unwrap(),
        )]))
        .unwrap()
    }
    #[test]
    fn identities_and_keys_are_bounded_and_unambiguous() {
        assert!(Auth::new(BTreeMap::new()).is_err());
        for id in [
            "",
            ".agent",
            "agent name",
            "agent/one",
            "客户端",
            &"a".repeat(129),
        ] {
            assert!(
                Auth::new(BTreeMap::from([(
                    id.into(),
                    SecretMaterial::new(&"x".repeat(32)).unwrap()
                )]))
                .is_err()
            );
        }
        for length in [31, 32, 4096, 4097] {
            assert_eq!(
                Auth::new(BTreeMap::from([(
                    "Agent_1.local-test".into(),
                    SecretMaterial::new(&"x".repeat(length)).unwrap()
                )]))
                .is_ok(),
                (32..=4096).contains(&length)
            );
        }
        assert!(
            Auth::new(BTreeMap::from([
                ("one".into(), SecretMaterial::new(&"x".repeat(32)).unwrap()),
                ("two".into(), SecretMaterial::new(&"x".repeat(32)).unwrap())
            ]))
            .is_err()
        );
        for count in [256, 257] {
            let agents = (0..count)
                .map(|i| {
                    (
                        format!("agent-{i}"),
                        SecretMaterial::new(&format!("synthetic-agent-key-{i:032}")).unwrap(),
                    )
                })
                .collect();
            assert_eq!(Auth::new(agents).is_ok(), count == 256);
        }
        assert!(
            Auth::new(BTreeMap::from([(
                "a".repeat(128),
                SecretMaterial::new(&"x".repeat(32)).unwrap()
            )]))
            .is_ok()
        );
    }
    #[test]
    fn rejects_duplicates_and_never_accepts_prefixes_or_alternate_headers() {
        let auth = auth();
        let mut h = HeaderMap::new();
        assert!(auth.authenticate(&h).is_none());
        h.insert(
            AUTHORIZATION,
            "Bearer synthetic-client-identity-00000001".parse().unwrap(),
        );
        assert_eq!(auth.authenticate(&h).unwrap().agent_id.as_ref(), "agent");
        h.append(
            AUTHORIZATION,
            "Bearer synthetic-client-identity-00000001".parse().unwrap(),
        );
        assert!(auth.authenticate(&h).is_none());
        h.remove(AUTHORIZATION);
        h.insert(
            "api-key",
            "synthetic-client-identity-00000001".parse().unwrap(),
        );
        assert!(auth.authenticate(&h).is_none());
        h.insert(
            AUTHORIZATION,
            "Bearer synthetic-client-identity-00000001-extra"
                .parse()
                .unwrap(),
        );
        assert!(auth.authenticate(&h).is_none());
        h.clear();
        h.insert(
            "x-api-key",
            "synthetic-client-identity-00000001".parse().unwrap(),
        );
        assert_eq!(auth.authenticate(&h).unwrap().agent_id.as_ref(), "agent");
        h.append(
            "x-api-key",
            "synthetic-client-identity-00000001".parse().unwrap(),
        );
        assert!(auth.authenticate(&h).is_none());
        h.remove("x-api-key");
        h.insert(
            "x-api-key",
            "synthetic-client-identity-00000001".parse().unwrap(),
        );
        // Two credentials are ambiguous even when both are valid.
        h.insert(
            AUTHORIZATION,
            "Bearer synthetic-client-identity-00000001".parse().unwrap(),
        );
        assert!(auth.authenticate(&h).is_none());
    }
}
