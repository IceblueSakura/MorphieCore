//! Offline HTTP transport checks with two literal-loopback listeners.
use super::*;
use crate::test_proxy_environment as proxy_environment;
use axum::{Router, body::Body, http::StatusCode, response::Response, routing::post};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::net::TcpListener;

#[tokio::test]
async fn environment_proxy_selection_is_isolated_and_explicit_proxy_wins() {
    proxy_environment::matrix("transport::http::tests::environment_proxy_child").await;
}
#[tokio::test]
#[ignore = "invoked only by the isolated proxy matrix"]
async fn environment_proxy_child() {
    let target = std::env::var("MORPHIECORE_TEST_PROXY_TARGET").unwrap();
    let proxy = std::env::var("MORPHIECORE_TEST_PROXY_OVERRIDE").ok();
    let transport = if std::env::var_os("MORPHIECORE_TEST_PROXY_ISOLATED").is_some() {
        HttpTransport::with_environment_proxy(None, false)
    } else {
        HttpTransport::new(proxy.as_deref())
    }
    .unwrap();
    let result = transport
        .client
        .get(target)
        .timeout(Duration::from_secs(2))
        .send()
        .await;
    if proxy_environment::expected_status() == 0 {
        assert!(result.is_err());
    } else {
        assert_eq!(
            result.unwrap().status().as_u16(),
            proxy_environment::expected_status()
        );
    }
}
struct Guard(tokio::task::AbortHandle);
#[tokio::test]
async fn wire_failures_distinguish_interruption_protocol_fault_and_timeout() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for (wire, body, expected) in [
        (&b"HTTP/1.1 200"[..], false, TransportError::Incomplete),
        (
            &b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nshort"[..],
            true,
            TransportError::Interrupted,
        ),
        (
            &b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n9\r\nshort"[..],
            true,
            TransportError::Interrupted,
        ),
        (
            &b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nz\r\ninvalid\r\n"[..],
            true,
            TransportError::Read,
        ),
        (&b"not HTTP\r\n\r\n"[..], false, TransportError::Send),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 8192];
            let mut used = 0;
            while !request[..used].windows(4).any(|w| w == b"\r\n\r\n") {
                let read = socket.read(&mut request[used..]).await.unwrap();
                assert!(read > 0 && used + read < request.len());
                used += read;
            }
            socket.write_all(wire).await.unwrap();
            socket.shutdown().await.unwrap();
        });
        let transport = HttpTransport::with_environment_proxy(None, false).unwrap();
        let result = transport
            .send(
                UpstreamRequest {
                    origin,
                    method: "POST",
                    path: "/".into(),
                    safe_headers: vec![],
                    auth_header: ("authorization".into(), "Bearer synthetic".into()),
                    body: vec![],
                },
                Duration::from_secs(2),
            )
            .await;
        let fact = if body {
            TransportError::from_reqwest(&result.unwrap().bytes().await.unwrap_err())
        } else {
            result.unwrap_err()
        };
        assert_eq!(fact, expected);
        server.await.unwrap();
    }
    // The connection timeout includes a stalled TLS handshake. A timeout after
    // an HTTP send remains ambiguous even when the response head never arrives.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("https://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.unwrap();
        std::future::pending::<()>().await;
    });
    let _server = Guard(server.abort_handle());
    let transport = HttpTransport {
        client: Client::builder()
            .no_proxy()
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_millis(30))
            .build()
            .unwrap(),
        loopback_authority: None,
    };
    let fact = transport
        .send(
            UpstreamRequest {
                origin,
                method: "POST",
                path: "/".into(),
                safe_headers: vec![],
                auth_header: ("authorization".into(), "Bearer synthetic".into()),
                body: vec![],
            },
            Duration::from_secs(2),
        )
        .await
        .unwrap_err();
    assert_eq!(fact, TransportError::ConnectTimeout);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("https://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut hello = [0; 8192];
        assert!(socket.read(&mut hello).await.unwrap() > 0);
        // A fatal bad_certificate alert exercises rustls' typed trust veto.
        socket
            .write_all(b"\x15\x03\x03\x00\x02\x02\x2a")
            .await
            .unwrap();
        socket.shutdown().await.unwrap();
    });
    let transport = HttpTransport::with_environment_proxy(None, false).unwrap();
    let fact = transport
        .send(
            UpstreamRequest {
                origin,
                method: "POST",
                path: "/".into(),
                safe_headers: vec![],
                auth_header: ("authorization".into(), "Bearer synthetic".into()),
                body: vec![],
            },
            Duration::from_secs(2),
        )
        .await
        .unwrap_err();
    assert_eq!(fact, TransportError::Tls);
    assert!(!fact.retryable() && !fact.affects_health());
    server.await.unwrap();
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.abort();
    }
}
#[tokio::test]
async fn redirects_are_not_followed_server_errors_are_not_retried_and_timeouts_are_classified() {
    let trap = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let destination = format!("http://{}/leak", trap.local_addr().unwrap());
    let trap_calls = Arc::new(AtomicUsize::new(0));
    let observed = trap_calls.clone();
    let trap_task = tokio::spawn(async move {
        axum::serve(
            trap,
            Router::new().route(
                "/leak",
                post(move || {
                    let observed = observed.clone();
                    async move {
                        observed.fetch_add(1, Ordering::SeqCst);
                        StatusCode::OK
                    }
                }),
            ),
        )
        .await
        .unwrap();
    });
    let _trap = Guard(trap_task.abort_handle());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let app = Router::new()
        .route(
            "/redirect",
            post(move || {
                let destination = destination.clone();
                async move {
                    Response::builder()
                        .status(307)
                        .header("location", destination)
                        .body(Body::empty())
                        .unwrap()
                }
            }),
        )
        .route(
            "/error",
            post(move || {
                let observed = observed.clone();
                async move {
                    observed.fetch_add(1, Ordering::SeqCst);
                    StatusCode::SERVICE_UNAVAILABLE
                }
            }),
        )
        .route(
            "/stall",
            post(|| async { std::future::pending::<StatusCode>().await }),
        );
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let _server = Guard(server.abort_handle());
    let transport = HttpTransport::with_environment_proxy(None, false).unwrap();
    let request = |path: &str| UpstreamRequest {
        origin: origin.clone(),
        method: "POST",
        path: path.into(),
        safe_headers: vec![("content-type".into(), "application/json".into())],
        auth_header: (
            "authorization".into(),
            "Bearer synthetic-upstream-credential".into(),
        ),
        body: b"{}".to_vec(),
    };
    let redirected = transport
        .send(request("/redirect"), Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(redirected.status(), 307);
    assert_eq!(trap_calls.load(Ordering::SeqCst), 0);
    let error = transport
        .send(request("/error"), Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(error.status(), 503);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        transport
            .send(request("/stall"), Duration::from_millis(30))
            .await
            .unwrap_err(),
        TransportError::Timeout
    );
}
