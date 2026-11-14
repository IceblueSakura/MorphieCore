use crate::{
    adapter::transcription::UpstreamAdapter,
    gateway::{diagnostics::Trace, tasks::transcription, tests::support::source},
};
use bytes::Bytes;
const RESULT: &[u8] =
    br#"{"request_id":"synthetic","output":{"text":"Hi."},"usage":{"duration":1}}"#;
#[tokio::test]
async fn transcription_adapters_decode_only_after_eof() {
    let mut trace = Trace::new(None);
    for (profile, result) in [
        (UpstreamAdapter::AliyunFlash, RESULT),
        (UpstreamAdapter::Xiaomi, br#"{"choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"Hi."}}]}"#.as_slice()),
    ] {
    let (tx, response, dropped) = source(200, "application/json");
    tx.send(Ok(Bytes::copy_from_slice(result))).await.unwrap();
    let mut pending = Box::pin(transcription::receive(
        response,
        profile,
        4096,
        &mut trace,
    ));
    assert!(futures_util::poll!(&mut pending).is_pending());
    drop(tx);
    assert_eq!(pending.await.unwrap().text(), "Hi.");
    dropped.await.unwrap();
    }
}
#[tokio::test]
async fn cancelled_transcription_releases_upstream() {
    let mut trace = Trace::new(None);
    let (_tx, response, dropped) = source(200, "application/json");
    let mut pending = Box::pin(transcription::receive(
        response,
        UpstreamAdapter::Xiaomi,
        4096,
        &mut trace,
    ));
    assert!(futures_util::poll!(&mut pending).is_pending());
    drop(pending);
    tokio::time::timeout(std::time::Duration::from_secs(1), dropped)
        .await
        .unwrap()
        .unwrap();
}
