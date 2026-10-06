//! Fixed provider identity, trusted origin and relative entry paths.

use crate::provider::{auth::AuthScheme, errors::ProviderError, ident_ok};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn new(id: &str) -> Result<Self, ProviderError> {
        if ident_ok(id) {
            Ok(Self(id.into()))
        } else {
            Err(ProviderError::InvalidId)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Normalized `scheme://host[:port]` with no path, query, fragment or userinfo.
///
/// HTTPS is required except for loopback test origins, where plaintext HTTP is
/// contained to `localhost`/`127.0.0.1`/`[::1]`. Credentials in the URL are
/// rejected outright.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustedOrigin(String);

impl TrustedOrigin {
    pub fn parse(s: &str) -> Result<Self, ProviderError> {
        let invalid = || ProviderError::InvalidOrigin;
        let (_, authority) = s.split_once("://").ok_or_else(invalid)?;
        // Reject raw syntax before URL normalization can strip controls, decode a
        // host escape or turn a backslash into a path delimiter.
        if s.len() > 8192
            || s.bytes()
                .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
            || authority.contains(['/', '?', '#', '@', '\\', '%'])
        {
            return Err(invalid());
        }
        let url = url::Url::parse(s).map_err(|_| invalid())?;
        if !matches!(url.scheme(), "https" | "http") {
            return Err(invalid());
        }
        let host = url.host_str().ok_or_else(invalid)?;
        // Url canonicalizes default ports away. Retain an explicitly configured
        // port, but let the standard parser validate its syntax and range first.
        let (raw_host, port) = if !authority.ends_with(']')
            && let Some((host, port)) = authority.rsplit_once(':')
        {
            let port: u16 = port.parse().map_err(|_| invalid())?;
            if port == 0 {
                return Err(invalid());
            }
            (host, Some(port))
        } else {
            (authority, None)
        };
        if url.scheme() == "http"
            && !["localhost", "127.0.0.1", "[::1]"]
                .iter()
                .any(|allowed| raw_host.eq_ignore_ascii_case(allowed))
        {
            return Err(invalid());
        }
        let mut normalized = format!("{}://{host}", url.scheme());
        if let Some(port) = port {
            normalized.push_str(&format!(":{port}"));
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Relative entry path bound to one protocol profile of a provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointPath(String);

impl EndpointPath {
    pub fn new(path: &str) -> Result<Self, ProviderError> {
        if !path.starts_with('/')
            || path.len() > 256
            || path.contains(['?', '#', ' ', '\t'])
            || path.ends_with('/')
        {
            return Err(ProviderError::InvalidPath);
        }
        let mut segments = path.split('/').skip(1).peekable();
        while let Some(segment) = segments.next() {
            if segment.is_empty() || matches!(segment, "." | "..") {
                return Err(ProviderError::InvalidPath);
            }
            if segments.peek().is_none() && !segment.is_ascii() {
                return Err(ProviderError::InvalidPath);
            }
        }
        Ok(Self(path.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Trusted provider facts: origin, per-protocol relative entries and the auth
/// scheme for one credential kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderDefinition {
    pub id: ProviderId,
    pub origin: TrustedOrigin,
    /// Absent when this provider has no admitted native Chat entry.
    pub chat_completions: Option<EndpointPath>,
    /// Absent when this provider has no admitted native Responses entry.
    pub responses: Option<EndpointPath>,
    pub auth: AuthScheme,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    proptest! {
        #![proptest_config(crate::test_properties::config())]
        #[test]
        fn property_origin_raw_delimiters_and_controls_cannot_be_normalized_away(
            host in "[a-z]{1,20}", port in 1u16..=65535,
            bad in prop::sample::select(vec!['\n', '\r', '\t', '\\', '@', '%', '?', '#', '/']), position in 0usize..70
        ) {
            let mut origin = format!("https://{host}.invalid:{port}");
            prop_assert!(TrustedOrigin::parse(&origin).is_ok());
            origin.insert(position % (origin.len()+1), bad);
            prop_assert!(TrustedOrigin::parse(&origin).is_err());
        }
        #[test]
        fn property_origin_numeric_http_aliases_never_become_literal_loopback(host in any::<u32>()) {
            let origin = format!("http://{host}");
            prop_assert!(TrustedOrigin::parse(&origin).is_err());
        }
    }

    #[test]
    fn trusted_origin_accepts_https_and_loopback_http_only() {
        for ok in [
            "https://api.deepseek.com",
            "https://api.xiaomimimo.com",
            "https://api.deepseek.com:8443",
            "http://127.0.0.1:39217",
            "http://localhost:39217",
            "http://[::1]:39217",
        ] {
            assert!(TrustedOrigin::parse(ok).is_ok(), "expected ok: {ok}");
        }
        for bad in [
            "http://api.deepseek.com",
            "ftp://api.deepseek.com",
            "https://user:pass@api.deepseek.com",
            "https://api.deepseek.com/path",
            "https://api.deepseek.com?x=1",
            "https://api.deepseek.com#frag",
            "https://",
            "https://api.deepseek.com:0",
            "api.deepseek.com",
        ] {
            assert!(TrustedOrigin::parse(bad).is_err(), "expected err: {bad}");
        }
    }

    #[test]
    fn origin_rejects_invalid_ip_and_normalization_bypasses() {
        for value in [
            "https://[::::]",
            "https://api.invalid\n",
            "https://host\\other",
            "https://%65xample.com",
            "http://127.1",
            "http://2130706433",
            "http://[0:0:0:0:0:0:0:1]",
            "https://host:",
            "https://host:65536",
        ] {
            assert!(TrustedOrigin::parse(value).is_err(), "{value:?}");
        }
        for (input, expected) in [
            ("https://EXAMPLE.com:443", "https://example.com:443"),
            ("http://LOCALHOST:80", "http://localhost:80"),
            ("https://[2001:0db8::1]:444", "https://[2001:db8::1]:444"),
        ] {
            assert_eq!(TrustedOrigin::parse(input).unwrap().as_str(), expected);
        }
    }
    #[test]
    fn trusted_origin_normalizes_case_and_keeps_port() {
        let origin = TrustedOrigin::parse("HTTPS://API.DeepSeek.COM").unwrap();
        assert_eq!(origin.as_str(), "https://api.deepseek.com");
        let origin = TrustedOrigin::parse("http://127.0.0.1:39217").unwrap();
        assert_eq!(origin.as_str(), "http://127.0.0.1:39217");
    }

    #[test]
    fn endpoint_paths_are_concrete_relative_entries() {
        for ok in ["/chat/completions", "/responses", "/v1/responses"] {
            assert!(EndpointPath::new(ok).is_ok(), "expected ok: {ok}");
        }
        for bad in [
            "chat/completions",
            "/",
            "/v1//responses",
            "/v1/../secret",
            "/responses?beta=1",
            "/responses#frag",
            "/responses/",
        ] {
            assert!(EndpointPath::new(bad).is_err(), "expected err: {bad}");
        }
    }

    #[test]
    fn provider_ids_are_bounded_identifiers() {
        assert_eq!(ProviderId::new("deepseek").unwrap().as_str(), "deepseek");
        assert!(ProviderId::new("Deep Seek").is_err());
        assert!(ProviderId::new("").is_err());
    }
}
