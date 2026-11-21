//! Listener bounds that apply before any request head is admitted.
use crate::ingress::http::{ConnectionLimits, serve_connections};
use axum::{Router, routing::get};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};

const PARTIAL_HEAD: &[u8] = b"GET / HTTP/1.1\r\nHost: localhost\r\nX-Partial: ";
const REQUEST: &[u8] = b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";

async fn listener() -> (TcpListener, std::net::SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    (listener, address)
}
fn router() -> Router {
    Router::new().route("/", get(|| async { "ok" }))
}
async fn bounded<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("bounded wait")
}
/// Reads until the peer closes; an error also means the connection ended.
async fn read_to_close(stream: &mut TcpStream) -> Vec<u8> {
    let mut received = vec![];
    let _ = stream.read_to_end(&mut received).await;
    received
}

#[tokio::test]
async fn partial_request_heads_are_closed_after_the_header_timeout() {
    let (listener, address) = listener().await;
    let limits = ConnectionLimits {
        header_read: Duration::from_millis(200),
        ..ConnectionLimits::DEFAULT
    };
    tokio::spawn(serve_connections(
        listener,
        router(),
        limits,
        std::future::pending(),
    ));
    let mut stalled = TcpStream::connect(address).await.unwrap();
    stalled.write_all(PARTIAL_HEAD).await.unwrap();
    let response = bounded(read_to_close(&mut stalled)).await;
    assert!(!response.starts_with(b"HTTP/1.1 200"));
    let mut complete = TcpStream::connect(address).await.unwrap();
    complete.write_all(REQUEST).await.unwrap();
    assert!(
        bounded(read_to_close(&mut complete))
            .await
            .starts_with(b"HTTP/1.1 200")
    );
}

#[tokio::test]
async fn shutdown_drops_connections_left_after_the_drain_bound() {
    let (listener, address) = listener().await;
    let limits = ConnectionLimits {
        drain: Duration::from_millis(200),
        ..ConnectionLimits::DEFAULT
    };
    let (stop, stopped) = oneshot::channel::<()>();
    let server = tokio::spawn(serve_connections(listener, router(), limits, async {
        let _ = stopped.await;
    }));
    let mut stalled = TcpStream::connect(address).await.unwrap();
    stalled.write_all(PARTIAL_HEAD).await.unwrap();
    // Let the connection task start reading the head before shutdown.
    tokio::time::sleep(Duration::from_millis(50)).await;
    stop.send(()).unwrap();
    bounded(server).await.unwrap();
    bounded(read_to_close(&mut stalled)).await;
}

#[tokio::test]
async fn connections_beyond_the_cap_wait_until_a_slot_is_released() {
    let (listener, address) = listener().await;
    let limits = ConnectionLimits {
        connections: 1,
        ..ConnectionLimits::DEFAULT
    };
    tokio::spawn(serve_connections(
        listener,
        router(),
        limits,
        std::future::pending(),
    ));
    let mut first = TcpStream::connect(address).await.unwrap();
    first.write_all(PARTIAL_HEAD).await.unwrap();
    let mut second = TcpStream::connect(address).await.unwrap();
    second.write_all(REQUEST).await.unwrap();
    let mut byte = [0; 1];
    assert!(
        tokio::time::timeout(Duration::from_millis(300), second.read(&mut byte))
            .await
            .is_err(),
        "a connection beyond the cap was served"
    );
    drop(first);
    assert!(
        bounded(read_to_close(&mut second))
            .await
            .starts_with(b"HTTP/1.1 200")
    );
}
