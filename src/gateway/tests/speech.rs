//! Binary intake checks use synthetic bodies, not additional production Routers.
use crate::{
    adapter::speech::UpstreamAdapter,
    gateway::{
        tasks::speech::{self, aliyun as aliyun_speech},
        tests::support::source,
        *,
    },
    semantic::task::speech_synthesis::SpeechRequest,
};
use axum::body::Bytes;

#[tokio::test]
async fn cancelling_binary_collection_drops_unfinished_upstream() {
    let task = SpeechRequest::new("synthetic", "alloy").unwrap();
    for (profile, media) in [
        (UpstreamAdapter::Standard, "audio/mpeg"),
        (UpstreamAdapter::Xiaomi, "application/json"),
    ] {
        let mut trace = diagnostics::Trace::new(None);
        let (_tx, response, dropped) = source(200, media);
        let mut pending = Box::pin(speech::receive(response, &task, profile, 4, &mut trace));
        assert!(futures_util::poll!(&mut pending).is_pending());
        drop(pending);
        tokio::time::timeout(std::time::Duration::from_secs(1), dropped)
            .await
            .unwrap()
            .unwrap();
    }
}
#[tokio::test]
async fn json_speech_waits_for_eof_before_decoding_audio() {
    let task = SpeechRequest::new("synthetic", "Chloe").unwrap();
    let mut trace = diagnostics::Trace::new(None);
    let (tx, response, dropped) = source(200, "application/json");
    tx.send(Ok(Bytes::from_static(br#"{"choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","audio":{"data":"YWJj"}}}]}"#))).await.unwrap();
    let mut pending = Box::pin(speech::receive(
        response,
        &task,
        UpstreamAdapter::Xiaomi,
        4096,
        &mut trace,
    ));
    assert!(futures_util::poll!(&mut pending).is_pending());
    drop(tx);
    assert_eq!(pending.await.unwrap().data().as_ref(), b"abc");
    dropped.await.unwrap();
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
