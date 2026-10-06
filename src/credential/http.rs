//! Bounded authority HTTP, with no redirects, ambient proxies or automatic retries.
use super::{CredentialError as Error, SecretBytes};
use futures_util::StreamExt;
use reqwest::{Client, Method, header::HeaderValue};
use std::time::Duration;
use tokio::time::Instant;

pub(super) const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const BODY_LIMIT: usize = 65536;
#[derive(Clone)]
pub(super) struct AuthHttp {
    client: Client,
    origin: String,
}
impl AuthHttp {
    pub fn with_user_agent(
        origin: &'static str,
        proxy: Option<&str>,
        agent: &str,
    ) -> Result<Self, Error> {
        Self::build(origin.into(), proxy, Some(agent))
    }
    #[cfg(test)]
    /// Test-only client without default metadata.
    pub fn raw(origin: &'static str, proxy: Option<&str>) -> Result<Self, Error> {
        Self::build(origin.into(), proxy, None)
    }
    fn build(origin: String, proxy: Option<&str>, agent: Option<&str>) -> Result<Self, Error> {
        let mut builder = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(10));
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
        Self::build(origin.into(), None, agent)
    }
    pub async fn request(
        &self,
        path: &str,
        fields: &[(&str, &str)],
        bearer: Option<&str>,
        deadline: Instant,
    ) -> Result<(u16, SecretBytes), Error> {
        self.request_with_metadata(path, fields, bearer, &[], deadline)
            .await
    }
    pub async fn request_with_metadata(
        &self,
        path: &str,
        fields: &[(&str, &str)],
        bearer: Option<&str>,
        metadata: &[(&str, &str)],
        deadline: Instant,
    ) -> Result<(u16, SecretBytes), Error> {
        if let Some(token) = bearer {
            return self.send(path, None, Some(token), metadata, deadline).await;
        }
        let body = {
            let mut form = url::form_urlencoded::Serializer::new(String::new());
            form.extend_pairs(fields.iter().copied());
            form.finish()
        };
        self.send(
            path,
            Some(("application/x-www-form-urlencoded", body)),
            None,
            metadata,
            deadline,
        )
        .await
    }
    pub async fn get(&self, path: &str, deadline: Instant) -> Result<(u16, SecretBytes), Error> {
        self.send(path, None, None, &[], deadline).await
    }
    async fn send(
        &self,
        path: &str,
        body: Option<(&str, String)>,
        bearer: Option<&str>,
        metadata: &[(&str, &str)],
        deadline: Instant,
    ) -> Result<(u16, SecretBytes), Error> {
        let deadline = deadline.min(Instant::now() + REQUEST_TIMEOUT);
        if deadline <= Instant::now() {
            return Err(Error::Timeout);
        }
        let mut request = self
            .client
            .request(
                if body.is_some() {
                    Method::POST
                } else {
                    Method::GET
                },
                format!("{}{path}", self.origin),
            )
            .header("accept", "application/json");
        if let Some((content_type, body)) = body {
            if body.len() > BODY_LIMIT {
                return Err(Error::Protocol);
            }
            request = request.header("content-type", content_type).body(body);
        }
        if let Some(token) = bearer {
            let mut value =
                HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| Error::Protocol)?;
            value.set_sensitive(true);
            request = request.header("authorization", value);
        }
        // Only trusted drivers supply metadata, never response fields or business input.
        for (name, value) in metadata {
            request = request.header(*name, *value);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
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
            let call = http.get("/jwks", Instant::now() + Duration::from_secs(2));
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
