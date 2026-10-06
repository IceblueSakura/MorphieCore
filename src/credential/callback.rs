//! A one-transaction, bounded loopback callback. No detached connection workers.
use super::{CredentialError as Error, model::Secret};
use std::{collections::BTreeMap, net::SocketAddr, time::Duration};
use subtle::ConstantTimeEq;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{Instant, timeout_at},
};

const HEADER_LIMIT: usize = 8192;
const ATTEMPT_LIMIT: usize = 64;
const READ_TIMEOUT: Duration = Duration::from_secs(2);
pub(super) struct Callback {
    listener: TcpListener,
    address: SocketAddr,
    path: &'static str,
}
pub(super) enum CallbackDecision {
    Code(CallbackCode),
    Error(Error),
    Reject(u16),
}
pub(super) struct CallbackCode {
    pub code: Secret,
    pub client_id: Option<String>,
}
impl CallbackCode {
    #[cfg(test)]
    pub fn expose(&self) -> &str {
        self.code.expose()
    }
}
impl Callback {
    pub async fn bind(port: u16, path: &'static str) -> Result<Self, Error> {
        if !path.starts_with('/')
            || path.len() > 128
            || !path
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"/-_".contains(&c))
        {
            return Err(Error::InvalidInput);
        }
        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port)))
            .await
            .map_err(|_| Error::Callback)?;
        let address = listener.local_addr().map_err(|_| Error::Callback)?;
        Ok(Self {
            listener,
            address,
            path,
        })
    }
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    pub fn redirect_uri(&self) -> String {
        format!("http://{}{}", self.address(), self.path)
    }
    pub async fn wait(&mut self, state: &str, deadline: Instant) -> Result<Secret, Error> {
        let callback = self.wait_registration(state, deadline).await?;
        if callback.client_id.is_some() {
            return Err(Error::Protocol);
        }
        Ok(callback.code)
    }
    pub async fn wait_registration(
        &mut self,
        state: &str,
        deadline: Instant,
    ) -> Result<CallbackCode, Error> {
        for _ in 0..ATTEMPT_LIMIT {
            let (mut stream, _) = timeout_at(deadline, self.listener.accept())
                .await
                .map_err(|_| Error::Expired)?
                .map_err(|_| Error::Callback)?;
            let limit = deadline.min(Instant::now() + READ_TIMEOUT);
            let decision = match timeout_at(limit, read_request(&mut stream)).await {
                Ok(Ok(bytes)) => classify(&bytes, self.address, state, self.path),
                Ok(Err(status)) => CallbackDecision::Reject(status),
                Err(_) => CallbackDecision::Reject(408),
            };
            let (status, message) = match &decision {
                CallbackDecision::Code(_) => (
                    200,
                    "Callback received. Check the CLI for final login confirmation.",
                ),
                CallbackDecision::Error(_) => (400, "Authorization was not completed."),
                CallbackDecision::Reject(status) => {
                    (*status, "Invalid or unrelated callback request.")
                }
            };
            let reason = match status {
                200 => "OK",
                404 => "Not Found",
                405 => "Method Not Allowed",
                408 => "Request Timeout",
                431 => "Request Header Fields Too Large",
                _ => "Bad Request",
            };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\n\r\n{message}",
                message.len()
            );
            // A disconnected browser does not cause a second code exchange. The
            // response contains no query, code, token or claimed successful login.
            let _ = timeout_at(
                deadline.min(Instant::now() + Duration::from_secs(1)),
                stream.write_all(response.as_bytes()),
            )
            .await;
            drop(stream);
            match decision {
                CallbackDecision::Code(code) => return Ok(code),
                CallbackDecision::Error(error) => return Err(error),
                CallbackDecision::Reject(_) => {}
            }
        }
        Err(Error::Protocol)
    }
}
async fn read_request(stream: &mut TcpStream) -> Result<Vec<u8>, u16> {
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0u8; 512];
        let count = stream.read(&mut chunk).await.map_err(|_| 400u16)?;
        if count == 0 {
            return Err(400);
        }
        if count > HEADER_LIMIT - bytes.len() {
            return Err(431);
        }
        bytes.extend_from_slice(&chunk[..count]);
        let mut headers = [httparse::EMPTY_HEADER; 32];
        match httparse::Request::new(&mut headers).parse(&bytes) {
            Ok(httparse::Status::Complete(_)) => return Ok(bytes),
            Ok(httparse::Status::Partial) if bytes.len() < HEADER_LIMIT => {}
            _ => return Err(431),
        }
    }
}
pub(super) fn classify(
    bytes: &[u8],
    address: SocketAddr,
    expected_state: &str,
    expected_path: &str,
) -> CallbackDecision {
    let reject = CallbackDecision::Reject;
    if bytes.len() > HEADER_LIMIT {
        return reject(431);
    }
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut request = httparse::Request::new(&mut headers);
    let consumed = match request.parse(bytes) {
        Ok(httparse::Status::Complete(count)) => count,
        _ => return reject(400),
    };
    if request.method != Some("GET") {
        return reject(405);
    }
    // One callback, not a general HTTP connection: buffered trailing bytes
    // invalidate the whole attempt instead of being ignored or pipelined.
    if !matches!(request.version, Some(0 | 1)) || consumed != bytes.len() {
        return reject(400);
    }
    let mut host = None;
    for header in request.headers.iter() {
        if header.name.eq_ignore_ascii_case("host") && host.replace(header.value).is_some() {
            return reject(400);
        }
        if header.name.eq_ignore_ascii_case("transfer-encoding")
            || (header.name.eq_ignore_ascii_case("content-length") && header.value != b"0")
        {
            return reject(400);
        }
    }
    let expected_host = address.to_string();
    if host != Some(expected_host.as_bytes()) {
        return reject(400);
    }
    let Some(path) = request.path else {
        return reject(400);
    };
    let Some((path, query)) = path.split_once('?') else {
        return reject(404);
    };
    if path != expected_path || query.contains('#') {
        return reject(404);
    }
    let mut fields = BTreeMap::new();
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        if fields.len() == 16 || fields.insert(key, value).is_some() {
            return reject(400);
        }
    }
    let Some(state) = fields.get("state") else {
        return reject(400);
    };
    if !bool::from(state.as_bytes().ct_eq(expected_state.as_bytes())) {
        return reject(400);
    }
    // Even OAuth errors must be tied to this transaction before terminating it.
    if let Some(error) = fields.get("error") {
        if fields.contains_key("code") {
            return reject(400);
        }
        return CallbackDecision::Error(if error == "access_denied" {
            Error::Denied
        } else {
            Error::Protocol
        });
    }
    let Some(code) = fields.get("code") else {
        return reject(400);
    };
    let client_id = fields.get("client_id").map(|v| v.to_string());
    if client_id
        .as_deref()
        .is_some_and(|v| super::model::valid_client(v).is_err())
    {
        return reject(400);
    }
    match Secret::new(code.to_string()) {
        Ok(code) => CallbackDecision::Code(CallbackCode { code, client_id }),
        Err(_) => reject(400),
    }
}
