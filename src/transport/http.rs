//! Semantically blind trusted HTTP transport. No inbound header propagation,
//! redirects, implicit retries or ambient proxy discovery.
#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
use crate::{execution::UpstreamRequest, provider::ErrorClass};
use reqwest::{
    Client, Response,
    header::{HeaderName, HeaderValue},
};
use std::time::Duration;
#[derive(Clone)]
pub struct HttpTransport {
    client: Client,
}
impl HttpTransport {
    pub fn new(proxy: Option<&str>) -> Result<Self, ErrorClass> {
        let mut builder = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(10));
        if let Some(proxy) = proxy {
            builder = builder.proxy(reqwest::Proxy::all(proxy).map_err(|_| ErrorClass::Upstream)?);
        }
        Ok(Self {
            client: builder.build().map_err(|_| ErrorClass::Upstream)?,
        })
    }
    pub async fn send(
        &self,
        prepared: UpstreamRequest,
        timeout: Duration,
    ) -> Result<Response, ErrorClass> {
        let method = reqwest::Method::from_bytes(prepared.method.as_bytes())
            .map_err(|_| ErrorClass::Upstream)?;
        let mut request = self
            .client
            .request(method, format!("{}{}", prepared.origin, prepared.path))
            .timeout(timeout);
        for (name, value) in prepared.safe_headers {
            let mut value = HeaderValue::from_str(&value).map_err(|_| ErrorClass::Upstream)?;
            // Session grouping is high-cardinality context, not ordinary observable metadata.
            value.set_sensitive(matches!(
                name.as_str(),
                "x-opencode-session" | "chatgpt-account-id"
            ));
            request = request.header(name, value);
        }
        let name = HeaderName::from_bytes(prepared.auth_header.0.as_bytes())
            .map_err(|_| ErrorClass::Upstream)?;
        let mut value =
            HeaderValue::from_str(&prepared.auth_header.1).map_err(|_| ErrorClass::Upstream)?;
        value.set_sensitive(true);
        request
            .header(name, value)
            .body(prepared.body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ErrorClass::Timeout
                } else {
                    ErrorClass::Upstream
                }
            })
    }
}
