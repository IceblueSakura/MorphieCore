//! Private normalized application capture. Only sanitized bounded units enter the writer.
use axum::{
    body::{Body, Bytes},
    http::HeaderMap,
};
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    task::{Context, Poll},
};

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Selection {
    pub headers: bool,
    pub body: bool,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureConfig {
    pub path: PathBuf,
    #[serde(default)]
    pub client_request: Selection,
    #[serde(default)]
    pub upstream_request: Selection,
    #[serde(default)]
    pub upstream_response: Selection,
    #[serde(default)]
    pub client_response: Selection,
    #[serde(default = "unit_limit")]
    pub max_unit_bytes: usize,
    #[serde(default = "exchange_limit")]
    pub max_exchange_bytes: usize,
    #[serde(default = "total_limit")]
    pub max_total_bytes: usize,
}
fn unit_limit() -> usize {
    256 << 10
}
fn exchange_limit() -> usize {
    4 << 20
}
fn total_limit() -> usize {
    64 << 20
}
#[derive(Clone, Copy)]
pub(super) enum Boundary {
    ClientRequest,
    UpstreamRequest,
    UpstreamResponse,
    ClientResponse,
}
impl Boundary {
    fn label(self) -> &'static str {
        match self {
            Self::ClientRequest => "client_request",
            Self::UpstreamRequest => "upstream_request",
            Self::UpstreamResponse => "upstream_response",
            Self::ClientResponse => "client_response",
        }
    }
}
impl CaptureConfig {
    pub(super) fn enabled(&self) -> bool {
        [
            self.client_request,
            self.upstream_request,
            self.upstream_response,
            self.client_response,
        ]
        .iter()
        .any(|s| s.headers || s.body)
    }
    fn selection(&self, boundary: Boundary) -> Selection {
        match boundary {
            Boundary::ClientRequest => self.client_request,
            Boundary::UpstreamRequest => self.upstream_request,
            Boundary::UpstreamResponse => self.upstream_response,
            Boundary::ClientResponse => self.client_response,
        }
    }
}
#[derive(Default, Serialize)]
pub struct CaptureStatus {
    pub queue_dropped: u64,
    pub retention_dropped: u64,
    pub write_failed: bool,
}
#[derive(Default)]
struct Counters {
    queue: AtomicU64,
    retention: AtomicU64,
    failed: AtomicU64,
}
enum Message {
    Unit(Vec<u8>, Arc<AtomicU64>),
    Flush(tokio::sync::oneshot::Sender<()>),
}
struct Writer {
    config: CaptureConfig,
    tx: mpsc::SyncSender<Message>,
    next: AtomicU64,
    counts: Arc<Counters>,
}
#[derive(Clone)]
pub(super) struct Sink(Arc<Writer>);
impl Sink {
    pub(super) fn open(config: CaptureConfig) -> std::io::Result<Self> {
        if !(1024..=1 << 20).contains(&config.max_unit_bytes)
            || config.max_exchange_bytes < config.max_unit_bytes
            || config.max_exchange_bytes > 64 << 20
            || config.max_total_bytes < config.max_exchange_bytes
            || config.max_total_bytes > 1 << 30
        {
            return Err(std::io::Error::other("invalid capture bounds"));
        }
        let parent = config
            .path
            .parent()
            .ok_or_else(|| std::io::Error::other("private capture directory required"))?;
        let mut builder = fs_mistrust::Mistrust::builder();
        builder.ignore_environment();
        #[cfg(unix)]
        builder.trust_no_group_id();
        builder
            .build()
            .map_err(|_| std::io::Error::other("capture directory"))?
            .verifier()
            .require_directory()
            .check(parent)
            .map_err(|_| std::io::Error::other("capture directory must be private"))?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&config.path)?;
        let (tx, rx) = mpsc::sync_channel(32);
        let counts = Arc::new(Counters::default());
        let observed = counts.clone();
        let limit = config.max_total_bytes;
        std::thread::Builder::new()
            .name("gateway-capture".into())
            .spawn(move || {
                let mut used = 0usize;
                for message in rx {
                    match message {
                        Message::Unit(bytes, dropped) => {
                            if observed.failed.load(Ordering::Relaxed) != 0
                                || bytes.len() > limit.saturating_sub(used)
                            {
                                observed.retention.fetch_add(1, Ordering::Relaxed);
                                dropped.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }
                            if file.write_all(&bytes).is_err() {
                                observed.failed.store(1, Ordering::Relaxed);
                                dropped.fetch_add(1, Ordering::Relaxed);
                            } else {
                                used += bytes.len();
                            }
                        }
                        Message::Flush(done) => {
                            if file.flush().is_err() {
                                observed.failed.store(1, Ordering::Relaxed);
                            }
                            let _ = done.send(());
                        }
                    }
                }
            })?;
        Ok(Self(Arc::new(Writer {
            config,
            tx,
            next: AtomicU64::new(1),
            counts,
        })))
    }
    pub(super) fn exchange(&self) -> Exchange {
        Exchange(Arc::new(ExchangeState {
            sink: self.clone(),
            id: self.0.next.fetch_add(1, Ordering::Relaxed),
            used: AtomicUsize::new(0),
            sequence: AtomicU64::new(0),
            dropped: Arc::new(AtomicU64::new(0)),
        }))
    }
    pub(super) async fn flush(&self) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if self.0.tx.try_send(Message::Flush(tx)).is_ok() {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), rx).await;
        }
    }
    pub(super) fn status(&self) -> CaptureStatus {
        CaptureStatus {
            queue_dropped: self.0.counts.queue.load(Ordering::Relaxed),
            retention_dropped: self.0.counts.retention.load(Ordering::Relaxed),
            write_failed: self.0.counts.failed.load(Ordering::Relaxed) != 0,
        }
    }
}
struct ExchangeState {
    dropped: Arc<AtomicU64>,
    sink: Sink,
    id: u64,
    used: AtomicUsize,
    sequence: AtomicU64,
}
#[derive(Clone)]
pub(super) struct Exchange(Arc<ExchangeState>);
fn authentication(name: &str) -> bool {
    let name: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    name.contains("authorization")
        || name.contains("apikey")
        || name.contains("password")
        || name.contains("secret")
        || name.contains("credential")
        || name.contains("privatekey")
        || name.contains("signingkey")
        || name.ends_with("token")
        || matches!(
            name.as_str(),
            "cookie"
                | "setcookie"
                | "key"
                | "jwt"
                | "auth"
                | "authentication"
                | "bearer"
                | "clientkey"
                | "accesskey"
                | "accountkey"
                | "wwwauthenticate"
                | "proxyauthenticate"
                | "authenticationinfo"
                | "proxyauthenticationinfo"
        )
}
fn authentication_query(name: &str) -> bool {
    authentication(name)
        || matches!(
            name.to_ascii_lowercase().as_str(),
            "sig" | "signature" | "x-amz-signature" | "x-goog-signature" | "code"
        )
}
fn capture_url(text: &str) -> Option<(url::Url, bool)> {
    if text.starts_with('/') || text.starts_with('?') {
        let base = url::Url::parse("https://capture.invalid/").ok()?;
        return url::Url::options()
            .base_url(Some(&base))
            .parse(text)
            .ok()
            .map(|url| (url, true));
    }
    url::Url::parse(text)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .map(|url| (url, false))
}
fn sanitize(value: &mut Value, redacted: &mut bool, depth: usize) -> bool {
    if depth > 64 {
        return false;
    }
    match value {
        Value::Object(values) => {
            for (name, value) in values {
                if authentication(name) {
                    *value = json!("[REDACTED]");
                    *redacted = true;
                } else if !sanitize(value, redacted, depth + 1) {
                    return false;
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                if !sanitize(value, redacted, depth + 1) {
                    return false;
                }
            }
        }
        Value::String(text) => {
            if text == "[REDACTED]" {
                return true;
            }
            // Function arguments/results can contain serialized authentication objects.
            if matches!(text.trim_start().as_bytes().first(), Some(b'{' | b'[')) {
                let Ok(mut nested) = crate::protocol::json::decode(text.as_bytes()) else {
                    return false;
                };
                let mut nested_redacted = false;
                if !sanitize(&mut nested, &mut nested_redacted, depth + 1) {
                    return false;
                }
                if nested_redacted {
                    let Ok(encoded) = serde_json::to_string(&nested) else {
                        return false;
                    };
                    *text = encoded;
                    *redacted = true;
                }
            } else if let Some((mut url, relative)) = capture_url(text) {
                let mut changed = !url.username().is_empty() || url.password().is_some();
                let _ = url.set_username("");
                let _ = url.set_password(None);
                let query: Vec<_> = url
                    .query_pairs()
                    .map(|(key, value)| {
                        let value = if authentication_query(&key) {
                            changed = true;
                            "[REDACTED]".into()
                        } else {
                            value.into_owned()
                        };
                        (key.into_owned(), value)
                    })
                    .collect();
                if url.fragment().is_some_and(|fragment| {
                    url::form_urlencoded::parse(fragment.as_bytes())
                        .any(|(key, _)| authentication_query(&key))
                }) {
                    url.set_fragment(None);
                    changed = true;
                }
                if changed {
                    if url.query().is_some() {
                        url.query_pairs_mut().clear().extend_pairs(query);
                    }
                    *text = if relative {
                        url[url::Position::BeforePath..].into()
                    } else {
                        url.into()
                    };
                    *redacted = true;
                }
            }
        }
        _ => {}
    }
    true
}
impl Exchange {
    pub(super) fn id(&self) -> u64 {
        self.0.id
    }
    pub(super) fn upstream_request(
        &self,
        prepared: &crate::execution::UpstreamRequest,
        attempt: usize,
    ) {
        let mut headers = HeaderMap::new();
        for (name, value) in &prepared.safe_headers {
            let (Ok(name), Ok(value)) = (
                name.parse::<axum::http::header::HeaderName>(),
                value.parse::<axum::http::HeaderValue>(),
            ) else {
                self.emit(
                    Boundary::UpstreamRequest,
                    Some(attempt),
                    "unsafe_headers",
                    None,
                    false,
                );
                return;
            };
            headers.append(name, value);
        }
        if let Ok(name) = prepared
            .auth_header
            .0
            .parse::<axum::http::header::HeaderName>()
        {
            let mut redacted = axum::http::HeaderValue::from_static("[REDACTED]");
            redacted.set_sensitive(true);
            headers.insert(name, redacted);
        }
        self.head(
            Boundary::UpstreamRequest,
            Some(attempt),
            &headers,
            prepared.path.split_once('?').map(|(_, query)| query),
        );
        self.json(Boundary::UpstreamRequest, Some(attempt), &prepared.body);
    }
    fn emit(
        &self,
        boundary: Boundary,
        attempt: Option<usize>,
        status: &'static str,
        value: Option<Value>,
        redacted: bool,
    ) {
        let writer = &self.0.sink.0;
        let record = json!({"exchange":self.0.id,"attempt":attempt,"boundary":boundary.label(),
            "sequence":self.0.sequence.fetch_add(1, Ordering::Relaxed),"status":status,
            "redacted":redacted,"dropped_before":self.0.dropped.load(Ordering::Relaxed),"value":value});
        let Ok(mut bytes) = serde_json::to_vec(&record) else {
            return;
        };
        bytes.push(b'\n');
        if bytes.len() > writer.config.max_unit_bytes
            || self
                .0
                .used
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                    used.checked_add(bytes.len())
                        .filter(|next| *next <= writer.config.max_exchange_bytes)
                })
                .is_err()
        {
            writer.counts.retention.fetch_add(1, Ordering::Relaxed);
            self.0.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        if writer
            .tx
            .try_send(Message::Unit(bytes, self.0.dropped.clone()))
            .is_err()
        {
            writer.counts.queue.fetch_add(1, Ordering::Relaxed);
            self.0.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub(super) fn head(
        &self,
        boundary: Boundary,
        attempt: Option<usize>,
        headers: &HeaderMap,
        query: Option<&str>,
    ) {
        if !self.0.sink.0.config.selection(boundary).headers {
            return;
        }
        let mut value = serde_json::Map::new();
        let mut redacted = false;
        let mut used = 0;
        for (name, header) in headers {
            if matches!(name.as_str(), "link" | "refresh")
                || matches!(name.as_str(), "location" | "content-location")
                    && !header
                        .to_str()
                        .is_ok_and(|v| v.starts_with("https://") || v.starts_with("http://"))
            {
                self.emit(boundary, attempt, "unsafe_uri_header", None, false);
                continue;
            }
            used += name.as_str().len() + header.len();
            if used > self.0.sink.0.config.max_unit_bytes / 2 {
                self.emit(boundary, attempt, "headers_truncated", None, false);
                return;
            }
            let text = if authentication(name.as_str()) || header.is_sensitive() {
                redacted = true;
                "[REDACTED]"
            } else if let Ok(text) = header.to_str() {
                text
            } else {
                self.emit(boundary, attempt, "unsafe_headers", None, false);
                return;
            };
            value
                .entry(name.as_str())
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .expect("array")
                .push(json!(text));
        }
        if query.is_some_and(|query| query.len() > self.0.sink.0.config.max_unit_bytes / 4) {
            self.emit(boundary, attempt, "query_truncated", None, false);
            return;
        }
        let query: Value = query
            .map(|query| {
                Value::Array(
                    url::form_urlencoded::parse(query.as_bytes())
                        .map(|(key, value)| {
                            if authentication_query(&key) {
                                redacted = true;
                                json!([key, "[REDACTED]"])
                            } else {
                                json!([key, value])
                            }
                        })
                        .collect(),
                )
            })
            .unwrap_or(Value::Null);
        let mut value = json!({"headers":value,"query":query});
        if !sanitize(&mut value, &mut redacted, 0) {
            self.emit(boundary, attempt, "unsafe_headers", None, false);
            return;
        }
        self.emit(boundary, attempt, "headers", Some(value), redacted);
    }
    pub(super) fn json(&self, boundary: Boundary, attempt: Option<usize>, bytes: &[u8]) -> bool {
        if !self.0.sink.0.config.selection(boundary).body {
            return true;
        }
        if bytes.len() > self.0.sink.0.config.max_unit_bytes / 2 {
            self.emit(boundary, attempt, "body_truncated", None, false);
            return false;
        }
        let Ok(mut value) = crate::protocol::json::decode(bytes) else {
            self.emit(boundary, attempt, "unsafe_body", None, false);
            return false;
        };
        // A fragment of serialized arguments is not an independently sanitizable unit.
        let kind = value
            .get("type")
            .or_else(|| value.get("event_type"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if kind.contains("arguments.delta")
            || kind == "arguments_delta"
            || value.pointer("/delta/type").and_then(Value::as_str) == Some("input_json_delta")
            || value.pointer("/delta/type").and_then(Value::as_str) == Some("arguments_delta")
            || value
                .get("choices")
                .and_then(Value::as_array)
                .is_some_and(|choices| {
                    choices.iter().any(|choice| {
                        choice.pointer("/delta/tool_calls").is_some()
                            || choice.pointer("/delta/function_call").is_some()
                    })
                })
        {
            self.emit(boundary, attempt, "unsafe_fragment", None, false);
            return false;
        }
        let mut redacted = false;
        if !sanitize(&mut value, &mut redacted, 0) {
            self.emit(boundary, attempt, "unsafe_body", None, false);
            return false;
        }
        self.emit(boundary, attempt, "body_unit", Some(value), redacted);
        true
    }
    pub(super) fn body(
        &self,
        boundary: Boundary,
        attempt: Option<usize>,
        content_type: &str,
    ) -> Option<BodyCapture> {
        if !self.0.sink.0.config.selection(boundary).body {
            return None;
        }
        let mode = if content_type.starts_with("text/event-stream") {
            Mode::Sse(crate::transport::sse::SseDecoder::new(
                self.0.sink.0.config.max_unit_bytes / 2,
            ))
        } else if content_type.starts_with("application/json") {
            if matches!(boundary, Boundary::ClientRequest) {
                Mode::JsonRequest(vec![])
            } else {
                Mode::Json
            }
        } else {
            Mode::Unsupported
        };
        Some(BodyCapture {
            exchange: self.clone(),
            boundary,
            attempt,
            mode,
            ended: false,
            rejected: false,
        })
    }
}
enum Mode {
    Json,
    JsonRequest(Vec<u8>),
    Sse(crate::transport::sse::SseDecoder),
    Unsupported,
}
pub(super) struct BodyCapture {
    exchange: Exchange,
    boundary: Boundary,
    attempt: Option<usize>,
    mode: Mode,
    ended: bool,
    rejected: bool,
}
impl BodyCapture {
    pub(super) fn feed(&mut self, bytes: &[u8]) {
        if self.rejected {
            return;
        }
        match &mut self.mode {
            Mode::Json => {
                self.rejected = !self.exchange.json(self.boundary, self.attempt, bytes);
            }
            Mode::JsonRequest(buffer) => {
                if bytes.len()
                    > (self.exchange.0.sink.0.config.max_unit_bytes / 2)
                        .saturating_sub(buffer.len())
                {
                    buffer.clear();
                    self.rejected = true;
                    self.exchange
                        .emit(self.boundary, self.attempt, "body_truncated", None, false);
                } else {
                    buffer.extend_from_slice(bytes);
                }
            }
            Mode::Sse(decoder) => {
                let mut rest = bytes;
                while !rest.is_empty() {
                    match decoder.push_until_event(rest) {
                        Ok((event, used)) if used > 0 => {
                            rest = &rest[used..];
                            if let Some(event) = event
                                && event.data() != "[DONE]"
                            {
                                self.rejected |= !self.exchange.json(
                                    self.boundary,
                                    self.attempt,
                                    event.data().as_bytes(),
                                );
                                if self.rejected {
                                    break;
                                }
                            }
                        }
                        _ => {
                            self.rejected = true;
                            self.exchange.emit(
                                self.boundary,
                                self.attempt,
                                "unsafe_stream",
                                None,
                                false,
                            );
                            break;
                        }
                    }
                }
            }
            Mode::Unsupported => {
                self.rejected = true;
                self.exchange
                    .emit(self.boundary, self.attempt, "unsupported_body", None, false);
            }
        }
    }
    pub(super) fn finish(&mut self) {
        if self.ended {
            return;
        }
        self.ended = true;
        if let Mode::JsonRequest(buffer) = &self.mode
            && !self.rejected
        {
            self.rejected = !self.exchange.json(self.boundary, self.attempt, buffer);
        }
        if let Mode::Sse(decoder) = &mut self.mode
            && decoder.finish_strict().is_err()
        {
            self.rejected = true;
        }
        self.exchange.emit(
            self.boundary,
            self.attempt,
            if self.rejected {
                "incomplete"
            } else {
                "source_eof"
            },
            None,
            false,
        );
    }
}
impl Drop for BodyCapture {
    fn drop(&mut self) {
        if !self.ended {
            self.exchange
                .emit(self.boundary, self.attempt, "incomplete", None, false);
        }
    }
}
struct ObservedBody {
    stream: Pin<Box<dyn Stream<Item = Result<Bytes, axum::Error>> + Send>>,
    capture: BodyCapture,
}
impl Stream for ObservedBody {
    type Item = Result<Bytes, axum::Error>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.stream.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                self.capture.feed(&bytes);
                Poll::Ready(Some(Ok(bytes)))
            }
            Poll::Ready(Some(Err(error))) => {
                self.capture.rejected = true;
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(None) => {
                self.capture.finish();
                Poll::Ready(None)
            }
            other => other,
        }
    }
}
pub(super) fn observe(body: Body, capture: BodyCapture) -> Body {
    Body::from_stream(ObservedBody {
        stream: Box::pin(body.into_data_stream()),
        capture,
    })
}
