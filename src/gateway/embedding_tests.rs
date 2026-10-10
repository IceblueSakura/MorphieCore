//! Synthetic body and activation checks at the embedding owner; no live egress.
use super::{diagnostics::Trace, speech_tests::source};
use crate::adapter::embeddings::Profile;
use axum::http::HeaderMap;
use bytes::Bytes;
const RESULT: &[u8] = br#"{"object":"list","model":"synthetic","data":[{"object":"embedding","index":0,"embedding":[0.5,1]}],"usage":{"prompt_tokens":1,"total_tokens":1}}"#;

#[tokio::test]
async fn embedding_body_requires_eof_and_rejects_bad_heads_reads_and_limits() {
    let headers = HeaderMap::new();
    let mut trace = Trace::new(None, &headers);
    let (tx, response, dropped) = source(200, "application/json");
    tx.send(Ok(Bytes::from_static(RESULT))).await.unwrap();
    let mut pending = Box::pin(super::embeddings::receive(
        response,
        4096,
        &mut trace,
        Profile::Standard,
    ));
    assert!(futures_util::poll!(&mut pending).is_pending());
    drop(tx);
    assert_eq!(pending.await.unwrap().vectors()[0].values().len(), 2);
    dropped.await.unwrap();
    for case in 0..8 {
        let (tx, mut response, dropped) = source(
            if case == 0 { 500 } else { 200 },
            if case == 1 {
                "text/event-stream"
            } else {
                "application/json"
            },
        );
        match case {
            2 => {
                response
                    .headers_mut()
                    .insert("content-encoding", "gzip".parse().unwrap());
            }
            3 => {
                response
                    .headers_mut()
                    .insert("content-length", "1".parse().unwrap());
            }
            4 => {
                response
                    .headers_mut()
                    .append("content-type", "application/json".parse().unwrap());
            }
            7 => {
                response.headers_mut().insert(
                    "content-type",
                    "application/json;charset=latin1".parse().unwrap(),
                );
            }
            _ => {}
        }
        tx.send(Ok(Bytes::from_static(RESULT))).await.unwrap();
        if case == 5 {
            tx.send(Err(std::io::Error::other("private fixture")))
                .await
                .unwrap();
        }
        drop(tx);
        assert!(
            super::embeddings::receive(
                response,
                if case == 6 { 2 } else { 4096 },
                &mut trace,
                Profile::Standard
            )
            .await
            .is_err()
        );
        dropped.await.unwrap();
    }
}
#[tokio::test]
async fn cancelled_embedding_intake_releases_its_body_without_a_result() {
    let headers = HeaderMap::new();
    let mut trace = Trace::new(None, &headers);
    let (_tx, response, dropped) = source(200, "application/json");
    let mut pending = Box::pin(super::embeddings::receive(
        response,
        4096,
        &mut trace,
        Profile::Standard,
    ));
    assert!(futures_util::poll!(&mut pending).is_pending());
    drop(pending);
    tokio::time::timeout(std::time::Duration::from_secs(1), dropped)
        .await
        .unwrap()
        .unwrap();
}
