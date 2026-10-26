//! Semantically blind trusted HTTP transport. No inbound header propagation,
//! redirects or implicit retries. Explicit proxies override system/environment defaults.
#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
use crate::{execution::UpstreamRequest, provider::RecoveryClass};
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
            Self::Timeout => "transport.timeout",
            Self::Send => "transport.send_failed",
            Self::Read => "transport.read_failed",
        }
    }
    /// Preserve recovery policy while retaining a more precise cause.
    pub const fn recovery_class(self) -> RecoveryClass {
        match self {
            Self::Timeout => RecoveryClass::Timeout,
            _ => RecoveryClass::Upstream,
        }
    }
}
#[derive(Clone)]
pub struct HttpTransport {
    client: Client,
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
        })
    }
    pub async fn send(
        &self,
        prepared: UpstreamRequest,
        timeout: Duration,
    ) -> Result<Response, TransportError> {
        let method = reqwest::Method::from_bytes(prepared.method.as_bytes())
            .map_err(|_| TransportError::RequestConstruction)?;
        let mut request = self
            .client
            .request(method, format!("{}{}", prepared.origin, prepared.path))
            .timeout(timeout);
        for (name, value) in prepared.safe_headers {
            let mut value =
                HeaderValue::from_str(&value).map_err(|_| TransportError::RequestConstruction)?;
            // Session grouping is high-cardinality context, not ordinary observable metadata.
            value.set_sensitive(matches!(
                name.as_str(),
                "x-opencode-session" | "chatgpt-account-id"
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
            .map_err(|e| {
                if e.is_timeout() {
                    TransportError::Timeout
                } else if e.is_connect() {
                    TransportError::Connect
                } else if e.is_builder() {
                    TransportError::RequestConstruction
                } else {
                    TransportError::Send
                }
            })
    }
}
