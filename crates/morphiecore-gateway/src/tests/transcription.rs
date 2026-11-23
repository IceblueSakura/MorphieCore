use crate::{
    adapter::transcription::UpstreamAdapter, diagnostics::Trace,
    semantic::task::speech_recognition::OutputFormat, tasks::transcription, tests::support::source,
};
use bytes::Bytes;
const RESULT: &[u8] =
    br#"{"request_id":"synthetic","output":{"text":"Hi."},"usage":{"duration":1}}"#;
#[tokio::test]
async fn transcription_adapters_decode_only_after_eof() {
    let mut trace = Trace::new(None);
    for (profile, format, media, result) in [
        (UpstreamAdapter::Standard, OutputFormat::Json, "application/json", br#"{"text":"Hi."}"#.as_slice()),
        (UpstreamAdapter::Standard, OutputFormat::Text, "text/plain; charset=utf-8", b"Hi.".as_slice()),
        (UpstreamAdapter::AliyunFlash, OutputFormat::Json, "application/json", RESULT),
        (UpstreamAdapter::Xiaomi, OutputFormat::Json, "application/json", br#"{"choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"Hi."}}]}"#.as_slice()),
    ] {
    let (tx, response, dropped) = source(200, media);
    tx.send(Ok(Bytes::copy_from_slice(result))).await.unwrap();
    let mut pending = Box::pin(transcription::receive_for_test(
        response,
        profile,
        format,
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
    let mut pending = Box::pin(transcription::receive_for_test(
        response,
        UpstreamAdapter::Xiaomi,
        OutputFormat::Json,
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
