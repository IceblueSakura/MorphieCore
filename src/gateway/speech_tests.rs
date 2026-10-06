//! Binary intake checks use synthetic bodies, not additional production Routers.
use super::*;
use crate::adapter::speech::Profile;
use crate::semantic::{task::speech_synthesis::SpeechRequest, value::AudioEncoding};
use axum::{body::Bytes, http::HeaderMap};
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
async fn speech_waits_for_eof_and_never_infers_format_from_request() {
    let task = SpeechRequest::new("synthetic", "alloy").unwrap();
    let headers = HeaderMap::new();
    let mut trace = diagnostics::Trace::new(None, &headers);
    let (tx, response, dropped) = source(200, "application/octet-stream");
    tx.send(Ok(Bytes::from_static(b"ab"))).await.unwrap();
    let mut pending = Box::pin(speech::receive(
        response,
        &task,
        Profile::Standard,
        4,
        &mut trace,
    ));
    assert!(
        futures_util::poll!(&mut pending).is_pending(),
        "bytes without EOF cannot be published"
    );
    tx.send(Ok(Bytes::from_static(b"cd"))).await.unwrap();
    drop(tx);
    let artifact = pending.await.unwrap();
    assert_eq!(artifact.data().as_ref(), b"abcd");
    assert_eq!(artifact.encoding(), None);
    dropped.await.unwrap();
    let (tx, response, _) = source(200, "audio/mpeg");
    drop(tx);
    assert!(
        speech::receive(response, &task, Profile::Standard, 4, &mut trace)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn speech_intake_rejects_bad_heads_lengths_overflow_and_read_errors() {
    let mut task = SpeechRequest::new("synthetic", "alloy").unwrap();
    task.format = Some(AudioEncoding::Wav);
    for case in 0..10 {
        let headers = HeaderMap::new();
        let mut trace = diagnostics::Trace::new(None, &headers);
        let (tx, mut response, dropped) = source(
            if case == 0 { 302 } else { 200 },
            if case == 1 {
                "application/json"
            } else {
                "audio/wav"
            },
        );
        match case {
            2 => {
                response
                    .headers_mut()
                    .append("content-type", "audio/wav".parse().unwrap());
            }
            3 => {
                response
                    .headers_mut()
                    .insert("content-encoding", "gzip".parse().unwrap());
            }
            4 => {
                response
                    .headers_mut()
                    .insert("content-length", "3".parse().unwrap());
            }
            5 => {
                response
                    .headers_mut()
                    .insert("content-length", "1".parse().unwrap());
            }
            6 => {
                response
                    .headers_mut()
                    .insert("content-length", "5000".parse().unwrap());
            }
            7 => {
                response
                    .headers_mut()
                    .insert("content-length", "invalid".parse().unwrap());
            }
            8 => {
                response
                    .headers_mut()
                    .insert("content-type", "audio/mpeg".parse().unwrap());
            }
            _ => {}
        }
        tx.send(Ok(Bytes::from_static(b"ab"))).await.unwrap();
        if case == 9 {
            tx.send(Err(std::io::Error::other("synthetic private body")))
                .await
                .unwrap();
        }
        drop(tx);
        let error = speech::receive(response, &task, Profile::Standard, 4, &mut trace)
            .await
            .unwrap_err();
        assert_eq!(error.status.as_u16(), 502, "{case}");
        assert!(!format!("{error:?}").contains("private body"));
        dropped.await.unwrap();
    }
    let headers = HeaderMap::new();
    let mut trace = diagnostics::Trace::new(None, &headers);
    let (tx, response, _) = source(200, "audio/wav");
    tx.send(Ok(Bytes::from_static(b"abcde"))).await.unwrap();
    drop(tx);
    assert!(
        speech::receive(response, &task, Profile::Standard, 4, &mut trace)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn cancelling_binary_collection_drops_unfinished_upstream() {
    let task = SpeechRequest::new("synthetic", "alloy").unwrap();
    let headers = HeaderMap::new();
    let mut trace = diagnostics::Trace::new(None, &headers);
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
    let headers = HeaderMap::new();
    let mut trace = diagnostics::Trace::new(None, &headers);
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
#[tokio::test]
async fn cancelling_native_speech_intake_drops_source() {
    let headers = HeaderMap::new();
    let mut trace = diagnostics::Trace::new(None, &headers);
    let (_tx, response, dropped) = source(200, "text/event-stream");
    let mut pending = Box::pin(aliyun_speech::receive(
        response, 64, 8192, 2048, 16, &mut trace,
    ));
    assert!(futures_util::poll!(&mut pending).is_pending());
    drop(pending);
    tokio::time::timeout(std::time::Duration::from_secs(1), dropped)
        .await
        .unwrap()
        .unwrap();
}
