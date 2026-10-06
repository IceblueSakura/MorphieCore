use super::{diagnostics::Trace, speech_tests::source, transcription};
use axum::http::HeaderMap;
use bytes::Bytes;
const RESULT: &[u8] =
    br#"{"request_id":"synthetic","output":{"text":"Hi."},"usage":{"duration":1}}"#;
#[tokio::test]
async fn transcription_json_requires_eof_and_rejects_bad_heads_and_reads() {
    let headers = HeaderMap::new();
    let mut trace = Trace::new(None, &headers);
    let (tx, response, dropped) = source(200, "application/json");
    tx.send(Ok(Bytes::from_static(RESULT))).await.unwrap();
    let mut pending = Box::pin(transcription::receive(response, 4096, &mut trace));
    assert!(futures_util::poll!(&mut pending).is_pending());
    drop(tx);
    assert_eq!(pending.await.unwrap().text(), "Hi.");
    dropped.await.unwrap();
    for case in 0..6 {
        let (tx, mut response, dropped) = source(
            200,
            if case == 0 {
                "text/event-stream"
            } else {
                "application/json"
            },
        );
        match case {
            1 => {
                response
                    .headers_mut()
                    .insert("content-encoding", "gzip".parse().unwrap());
            }
            2 => {
                response
                    .headers_mut()
                    .insert("content-length", "1".parse().unwrap());
            }
            3 => {
                response
                    .headers_mut()
                    .append("content-type", "application/json".parse().unwrap());
            }
            _ => {}
        }
        tx.send(Ok(Bytes::from_static(RESULT))).await.unwrap();
        if case == 4 {
            tx.send(Err(std::io::Error::other("private fixture")))
                .await
                .unwrap();
        }
        drop(tx);
        assert!(
            transcription::receive(response, if case == 5 { 2 } else { 4096 }, &mut trace)
                .await
                .is_err()
        );
        dropped.await.unwrap();
    }
}
#[tokio::test]
async fn cancelled_transcription_releases_upstream() {
    let headers = HeaderMap::new();
    let mut trace = Trace::new(None, &headers);
    let (_tx, response, dropped) = source(200, "application/json");
    let mut pending = Box::pin(transcription::receive(response, 4096, &mut trace));
    assert!(futures_util::poll!(&mut pending).is_pending());
    drop(pending);
    tokio::time::timeout(std::time::Duration::from_secs(1), dropped)
        .await
        .unwrap()
        .unwrap();
}
