//! Bounded authority HTTP; explicit proxies override environment proxy selection.
use crate::credential::{CredentialError as Error, SecretBytes};
use futures_util::StreamExt;
use reqwest::{Client, RequestBuilder, header::HeaderValue};
use std::time::Duration;
use tokio::time::Instant;

pub(in crate::credential) const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const BODY_LIMIT: usize = 65536;
#[derive(Clone)]
pub(in crate::credential) struct AuthHttp {
    client: Client,
    origin: String,
}
impl AuthHttp {
    pub fn with_user_agent(
        origin: &'static str,
        proxy: Option<&str>,
        agent: &str,
        environment: bool,
    ) -> Result<Self, Error> {
        Self::build_with_environment(origin.into(), proxy, Some(agent), environment)
    }
    #[cfg(test)]
    /// Test-only client without default metadata.
    pub fn raw(origin: &'static str, proxy: Option<&str>) -> Result<Self, Error> {
        Self::build(origin.into(), proxy, None)
    }
    #[cfg(test)]
    fn build(origin: String, proxy: Option<&str>, agent: Option<&str>) -> Result<Self, Error> {
        Self::build_with_environment(origin, proxy, agent, true)
    }
    fn build_with_environment(
        origin: String,
        proxy: Option<&str>,
        agent: Option<&str>,
        environment: bool,
    ) -> Result<Self, Error> {
        let mut builder = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(10));
        if !environment || proxy.is_some() {
            builder = builder.no_proxy();
        }
        if let Some(agent) = agent {
            builder = builder.user_agent(agent);
        }
        if let Some(proxy) = proxy {
            let url = url::Url::parse(proxy).map_err(|_| Error::InvalidInput)?;
            if !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.path() != "/"
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(Error::InvalidInput);
            }
            builder =
                builder.proxy(reqwest::Proxy::all(url.as_str()).map_err(|_| Error::InvalidInput)?);
        }
        let client = builder.build().map_err(|_| Error::Network)?;
        Ok(Self { client, origin })
    }
    #[cfg(test)]
    pub fn synthetic(origin: &str, agent: &str) -> Result<Self, Error> {
        Self::synthetic_with_agent(origin, Some(agent))
    }
    #[cfg(test)]
    fn synthetic_with_agent(origin: &str, agent: Option<&str>) -> Result<Self, Error> {
        let url = url::Url::parse(origin).map_err(|_| Error::InvalidInput)?;
        if url.scheme() != "http"
            || url.host_str() != Some("127.0.0.1")
            || url.path() != "/"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(Error::InvalidInput);
        }
        Self::build_with_environment(origin.into(), None, agent, false)
    }
    pub async fn post_form(
        &self,
        path: &str,
        fields: &[(&str, &str)],
        metadata: &[(&str, &str)],
        deadline: Instant,
    ) -> Result<(u16, SecretBytes), Error> {
        let body = {
            let mut form = url::form_urlencoded::Serializer::new(String::new());
            form.extend_pairs(fields.iter().copied());
            form.finish()
        };
        let deadline = bounded_deadline(deadline)?;
        if body.len() > BODY_LIMIT {
            return Err(Error::Protocol);
        }
        let mut request = self
            .client
            .post(format!("{}{path}", self.origin))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(body);
        // Only trusted drivers supply metadata, never response fields or business input.
        for (name, value) in metadata {
            request = request.header(*name, *value);
        }
        self.execute(request, deadline).await
    }
    pub async fn get(
        &self,
        path: &str,
        bearer: Option<&str>,
        deadline: Instant,
    ) -> Result<(u16, SecretBytes), Error> {
        let deadline = bounded_deadline(deadline)?;
        let mut request = self.client.get(format!("{}{path}", self.origin));
        if let Some(token) = bearer {
            let mut value =
                HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| Error::Protocol)?;
            value.set_sensitive(true);
            request = request.header("authorization", value);
        }
        self.execute(request, deadline).await
    }
    async fn execute(
        &self,
        request: RequestBuilder,
        deadline: Instant,
    ) -> Result<(u16, SecretBytes), Error> {
        let request = request.header("accept", "application/json");
        tokio::time::timeout_at(deadline, async {
            let response = request.send().await.map_err(|_| Error::Network)?;
            let status = response.status().as_u16();
            if (300..400).contains(&status) {
                return Err(Error::Protocol);
            }
            if response
                .content_length()
                .is_some_and(|len| len > BODY_LIMIT as u64)
            {
                return Err(Error::Protocol);
            }
            let mut stream = response.bytes_stream();
            let mut bytes = SecretBytes::new(Vec::new());
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|_| Error::Network)?;
                if chunk.len() > BODY_LIMIT - bytes.len() {
                    return Err(Error::Protocol);
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok((status, bytes))
        })
        .await
        .map_err(|_| Error::Timeout)?
    }
}
fn bounded_deadline(deadline: Instant) -> Result<Instant, Error> {
    let deadline = deadline.min(Instant::now() + REQUEST_TIMEOUT);
    if deadline <= Instant::now() {
        return Err(Error::Timeout);
    }
    Ok(deadline)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_proxy_environment as proxy_environment;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    #[tokio::test]
    async fn environment_proxy_selection_is_isolated_and_explicit_proxy_wins() {
        proxy_environment::matrix("credential::oauth::http::tests::environment_proxy_child").await;
    }
    #[tokio::test]
    #[ignore = "invoked only by the isolated proxy matrix"]
    async fn environment_proxy_child() {
        let target = std::env::var("MORPHIECORE_TEST_PROXY_TARGET").unwrap();
        let proxy = std::env::var("MORPHIECORE_TEST_PROXY_OVERRIDE").ok();
        let http = if std::env::var_os("MORPHIECORE_TEST_PROXY_ISOLATED").is_some() {
            AuthHttp::synthetic_with_agent(target.trim_end_matches("/fixture"), None)
        } else {
            AuthHttp::build(
                target.trim_end_matches("/fixture").into(),
                proxy.as_deref(),
                None,
            )
        }
        .unwrap();
        let result = http
            .get("/fixture", None, Instant::now() + Duration::from_secs(2))
            .await;
        if proxy_environment::expected_status() == 0 {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap().0, proxy_environment::expected_status());
        }
    }
    #[test]
    fn explicit_proxy_rejects_credentials_and_non_transport_fields() {
        for proxy in [
            "socks5://127.0.0.1:1",
            "http://user:password@127.0.0.1:1",
            "http://127.0.0.1:1/path",
            "http://127.0.0.1:1/?token=synthetic",
            "http://127.0.0.1:1/#fragment",
        ] {
            assert!(matches!(
                AuthHttp::raw("https://authority.invalid", Some(proxy)),
                Err(Error::InvalidInput)
            ));
        }
    }
    #[tokio::test]
    async fn an_explicit_loopback_proxy_is_used_without_direct_fallback() {
        let proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_url = format!("http://{}", proxy.local_addr().unwrap());
        let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", target.local_addr().unwrap());
        let http = AuthHttp::build(origin.clone(), Some(&proxy_url), None).unwrap();
        let exchange = async {
            let call = http.get("/jwks", None, Instant::now() + Duration::from_secs(2));
            let serve = async {
                let (mut stream, _) = proxy.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut part = [0; 1024];
                    let count = stream.read(&mut part).await.unwrap();
                    assert!(count > 0 && request.len() + count <= 8192);
                    request.extend_from_slice(&part[..count]);
                    if request.windows(4).any(|part| part == b"\r\n\r\n") {
                        break;
                    }
                }
                assert!(
                    String::from_utf8(request)
                        .unwrap()
                        .starts_with(&format!("GET {origin}/jwks HTTP/1.1\r\n"))
                );
                stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.unwrap();
            };
            let (result, ()) = tokio::join!(call, serve);
            assert_eq!(result.unwrap().0, 502);
            assert!(
                tokio::time::timeout(Duration::from_millis(20), target.accept())
                    .await
                    .is_err()
            );
            assert!(
                tokio::time::timeout(Duration::from_millis(20), proxy.accept())
                    .await
                    .is_err()
            );
        };
        tokio::time::timeout(Duration::from_secs(3), exchange)
            .await
            .unwrap();
    }
}
