//! Binary intake checks use synthetic bodies, not additional production Routers.
use super::*;
use crate::adapter::speech::Profile;
use crate::semantic::task::speech_synthesis::SpeechRequest;
use axum::body::Bytes;
use futures_util::Stream;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::{mpsc, oneshot};

struct Tracked {
    rx: mpsc::Receiver<Result<Bytes, std::io::Error>>,
    dropped: Option<oneshot::Sender<()>>,
}
impl Stream for Tracked {
    type Item = Result<Bytes, std::io::Error>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}
impl Drop for Tracked {
    fn drop(&mut self) {
        if let Some(tx) = self.dropped.take() {
            let _ = tx.send(());
        }
    }
}
type Probe = (
    mpsc::Sender<Result<Bytes, std::io::Error>>,
    reqwest::Response,
    oneshot::Receiver<()>,
);
pub(super) fn source(status: u16, media: &str) -> Probe {
    let (tx, rx) = mpsc::channel(2);
    let (done, dropped) = oneshot::channel();
    let body = reqwest::Body::wrap_stream(Tracked {
        rx,
        dropped: Some(done),
    });
    (
        tx,
        reqwest::Response::from(
            axum::http::Response::builder()
                .status(status)
                .header("content-type", media)
                .body(body)
                .unwrap(),
        ),
        dropped,
    )
}
#[tokio::test]
async fn cancelling_binary_collection_drops_unfinished_upstream() {
    let task = SpeechRequest::new("synthetic", "alloy").unwrap();
    let mut trace = diagnostics::Trace::new(None);
    let (_tx, response, dropped) = source(200, "audio/mpeg");
    let mut pending = Box::pin(speech::receive(
        response,
        &task,
        Profile::Standard,
        4,
        &mut trace,
    ));
    assert!(futures_util::poll!(&mut pending).is_pending());
    drop(pending);
    tokio::time::timeout(std::time::Duration::from_secs(1), dropped)
        .await
        .unwrap()
        .unwrap();
}
fn native_sse() -> Vec<u8> {
    use serde_json::json;
    let mut bytes = Vec::new();
    for (kind, data) in [
        ("sentence-begin", ""),
        ("sentence-synthesis", "YWJj"),
        ("sentence-end", ""),
    ] {
        let value = json!({"request_id":"native-fixture","output":{
            "finish_reason":"null","type":kind,"sentence":{"index":0,"words":[]},
            "audio":{"data":data}}});
        bytes.extend_from_slice(format!("data: {value}\n\n").as_bytes());
    }
    bytes.extend_from_slice(b"data: {\"request_id\":\"native-fixture\",\"output\":{\"finish_reason\":\"stop\",\"audio\":{\"data\":\"\"}}}\n\n");
    bytes
}
#[tokio::test]
async fn native_speech_uses_strict_sse_and_eof_not_just_stop() {
    let mut trace = diagnostics::Trace::new(None);
    let (tx, response, dropped) = source(200, "text/event-stream");
    tx.send(Ok(Bytes::from(native_sse()))).await.unwrap();
    let mut pending = Box::pin(aliyun_speech::receive(
        response, 64, 8192, 2048, 16, &mut trace,
    ));
    assert!(futures_util::poll!(&mut pending).is_pending());
    drop(tx);
    assert_eq!(pending.await.unwrap().audio().data().as_ref(), b"abc");
    dropped.await.unwrap();
    for case in 0..7 {
        let (tx, response, dropped) = source(
            200,
            if case == 0 {
                "application/json"
            } else {
                "text/event-stream"
            },
        );
        let mut bytes = native_sse();
        match case {
            1 => {
                bytes.truncate(bytes.len() - 2);
            }
            2 => bytes.extend_from_slice(b"data: {}\n\n"),
            6 => bytes.extend_from_slice(b"data:\n\n"),
            _ => {}
        }
        tx.send(Ok(Bytes::from(bytes))).await.unwrap();
        if case == 3 {
            tx.send(Err(std::io::Error::other("synthetic private error")))
                .await
                .unwrap();
        }
        drop(tx);
        assert!(
            aliyun_speech::receive(
                response,
                if case == 4 { 2 } else { 64 },
                8192,
                2048,
                if case == 5 { 2 } else { 16 },
                &mut trace
            )
            .await
            .is_err()
        );
        dropped.await.unwrap();
    }
}
