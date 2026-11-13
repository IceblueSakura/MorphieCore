//! Semantically blind trusted HTTP transport. No inbound header propagation,
//! redirects or implicit retries. Explicit proxies override system/environment defaults.
#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
use crate::execution::UpstreamRequest;
use reqwest::{
    Client, Response,
    header::{HeaderName, HeaderValue},
};
use std::time::Duration;
/// Safe transport facts. No URL, header value or third-party error text is retained.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TransportError {
    #[error("transport configuration failed")]
    Configuration,
    #[error("trusted request construction failed")]
    RequestConstruction,
    #[error("upstream connection failed")]
    Connect,
    #[error("upstream connection timed out")]
    ConnectTimeout,
    #[error("upstream connection interrupted")]
    Interrupted,
    #[error("upstream HTTP message ended prematurely")]
    Incomplete,
    #[error("upstream TLS failed")]
    Tls,
    #[error("upstream request timed out")]
    Timeout,
    #[error("upstream request failed")]
    Send,
    #[error("upstream body read failed")]
    Read,
}
impl TransportError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Configuration => "transport.configuration",
            Self::RequestConstruction => "transport.request_construction",
            Self::Connect => "transport.connect_failed",
            Self::ConnectTimeout => "transport.connect_timeout",
            Self::Interrupted => "transport.interrupted",
            Self::Incomplete => "transport.incomplete",
            Self::Tls => "transport.tls",
            Self::Timeout => "transport.timeout",
            Self::Send => "transport.send_failed",
            Self::Read => "transport.read_failed",
        }
    }
    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::ConnectTimeout | Self::Interrupted | Self::Incomplete
        )
    }
    pub const fn affects_health(self) -> bool {
        matches!(
            self,
            Self::Timeout | Self::ConnectTimeout | Self::Interrupted | Self::Incomplete
        )
    }
    /// Inspect typed causes only. Neither messages nor URLs enter policy.
    pub fn from_reqwest(error: &reqwest::Error) -> Self {
        fn inspect(
            error: &(dyn std::error::Error + 'static),
            depth: usize,
        ) -> Option<TransportError> {
            if depth == 32 {
                return None;
            }
            if error.is::<rustls::Error>() {
                return Some(TransportError::Tls);
            }
            if let Some(io) = error.downcast_ref::<std::io::Error>() {
                if let Some(inner) = io.get_ref()
                    && let Some(fact) = inspect(inner, depth + 1)
                {
                    return Some(fact);
                }
                use std::io::ErrorKind;
                return Some(match io.kind() {
                    ErrorKind::ConnectionRefused
                    | ErrorKind::ConnectionReset
                    | ErrorKind::ConnectionAborted
                    | ErrorKind::BrokenPipe
                    | ErrorKind::UnexpectedEof => TransportError::Interrupted,
                    _ => TransportError::Read,
                });
            }
            if let Some(hyper) = error.downcast_ref::<hyper::Error>()
                && hyper.is_incomplete_message()
            {
                return Some(TransportError::Incomplete);
            }
            error.source().and_then(|source| inspect(source, depth + 1))
        }
        if error.is_builder() {
            return Self::RequestConstruction;
        }
        let cause = inspect(error, 0);
        if cause == Some(Self::Tls) {
            return Self::Tls;
        }
        if error.is_timeout() {
            return if error.is_connect() {
                Self::ConnectTimeout
            } else {
                Self::Timeout
            };
        }
        cause.unwrap_or_else(|| {
            if error.is_connect() {
                Self::Connect
            } else if error.is_body() || error.is_decode() {
                Self::Read
            } else {
                Self::Send
            }
        })
    }
}
#[derive(Clone)]
pub struct HttpTransport {
    client: Client,
    #[cfg(test)]
    loopback_authority: Option<crate::provider::TrustedOrigin>,
}
impl HttpTransport {
    pub fn new(proxy: Option<&str>) -> Result<Self, TransportError> {
        Self::with_environment_proxy(proxy, true)
    }
    pub(crate) fn with_environment_proxy(
        proxy: Option<&str>,
        environment: bool,
    ) -> Result<Self, TransportError> {
        let mut builder = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(10));
        if !environment || proxy.is_some() {
            builder = builder.no_proxy();
        }
        if let Some(proxy) = proxy {
            builder = builder
                .proxy(reqwest::Proxy::all(proxy).map_err(|_| TransportError::Configuration)?);
        }
        Ok(Self {
            client: builder.build().map_err(|_| TransportError::Configuration)?,
            #[cfg(test)]
            loopback_authority: None,
        })
    }
    /// Test-owned wire authority; production account/target validation is unchanged.
    #[cfg(test)]
    pub(crate) fn with_loopback_authority(mut self, origin: &str) -> Self {
        let origin = crate::provider::TrustedOrigin::parse(origin).expect("test authority");
        assert!(origin.as_str().starts_with("http://"));
        self.loopback_authority = Some(origin);
        self
    }
    pub async fn send(
        &self,
        prepared: UpstreamRequest,
        timeout: Duration,
    ) -> Result<Response, TransportError> {
        let method = reqwest::Method::from_bytes(prepared.method.as_bytes())
            .map_err(|_| TransportError::RequestConstruction)?;
        #[cfg(test)]
        let origin = self
            .loopback_authority
            .as_ref()
            .map_or(prepared.origin.as_str(), |origin| origin.as_str());
        #[cfg(not(test))]
        let origin = prepared.origin.as_str();
        let mut request = self
            .client
            .request(method, format!("{}{}", origin, prepared.path))
            .timeout(timeout);
        for (name, value) in prepared.safe_headers {
            let mut value =
                HeaderValue::from_str(&value).map_err(|_| TransportError::RequestConstruction)?;
            // Session grouping is high-cardinality context, not ordinary observable metadata.
            value.set_sensitive(matches!(
                name.as_str(),
                "x-opencode-session" | "session-id" | "chatgpt-account-id"
            ));
            request = request.header(name, value);
        }
        let name = HeaderName::from_bytes(prepared.auth_header.0.as_bytes())
            .map_err(|_| TransportError::RequestConstruction)?;
        let mut value = HeaderValue::from_str(&prepared.auth_header.1)
            .map_err(|_| TransportError::RequestConstruction)?;
        value.set_sensitive(true);
        request
            .header(name, value)
            .body(prepared.body)
            .send()
            .await
            .map_err(|e| TransportError::from_reqwest(&e))
    }
}
