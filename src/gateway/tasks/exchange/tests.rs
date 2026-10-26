use super::*;
use crate::gateway::{tasks, tests::support::source};
use bytes::Bytes;
use std::time::Duration;

#[tokio::test]
async fn every_task_preserves_retry_after_and_releases_a_failed_head() {
    let speech =
        crate::semantic::task::speech_synthesis::SpeechRequest::new("synthetic", "alloy").unwrap();
    for task in 0..5 {
        let (_sender, mut upstream, dropped) = source(429, "application/json");
        upstream
            .headers_mut()
            .insert("retry-after", "7".parse().unwrap());
        let mut trace = Trace::new(None);
        let result = tokio::time::timeout(Duration::from_secs(1), async {
            match task {
                0 => tasks::embeddings::receive(
                    upstream,
                    4096,
                    &mut trace,
                    crate::adapter::embeddings::UpstreamAdapter::Standard,
                    crate::protocol::embedding::Encoding::Float,
                )
                .await
                .map(|_| ()),
                1 => tasks::images::receive(
                    upstream,
                    crate::adapter::images::UpstreamAdapter::GptImage,
                    4096,
                    &mut trace,
                )
                .await
                .map(|_| ()),
                2 => tasks::speech::receive(
                    upstream,
                    &speech,
                    crate::adapter::speech::UpstreamAdapter::Standard,
                    4096,
                    &mut trace,
                )
                .await
                .map(|_| ()),
                3 => tasks::speech::aliyun::receive(upstream, 4096, 4096, 4096, 16, &mut trace)
                    .await
                    .map(|_| ()),
                _ => tasks::transcription::receive(
                    upstream,
                    crate::adapter::transcription::UpstreamAdapter::AliyunFlash,
                    4096,
                    &mut trace,
                )
                .await
                .map(|_| ()),
            }
        })
        .await
        .unwrap();
        let error = result.unwrap_err();
        assert_eq!(error.code(), "rate_limit_exceeded");
        assert_eq!(error.retry_after, Some(7));
        tokio::time::timeout(Duration::from_secs(1), dropped)
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn shared_receiver_validates_declared_lengths_and_actual_eof() {
    for length in ["", "no", "0", "5", "18446744073709551616"] {
        let (_sender, mut upstream, dropped) = source(200, "application/json");
        upstream
            .headers_mut()
            .insert("content-length", length.parse().unwrap());
        assert!(Receiver::new(upstream, 4, &mut Trace::new(None)).is_err());
        dropped.await.unwrap();
    }
    let (_sender, mut upstream, dropped) = source(200, "application/json");
    upstream
        .headers_mut()
        .append("content-length", "2".parse().unwrap());
    upstream
        .headers_mut()
        .append("content-length", "2".parse().unwrap());
    assert!(Receiver::new(upstream, 4, &mut Trace::new(None)).is_err());
    dropped.await.unwrap();
    for (length, expected) in [(None, true), (Some("2"), true), (Some("3"), false)] {
        let (sender, mut upstream, dropped) = source(200, "application/json");
        if let Some(length) = length {
            upstream
                .headers_mut()
                .insert("content-length", length.parse().unwrap());
        }
        sender.send(Ok(Bytes::from_static(b"{}"))).await.unwrap();
        let mut trace = Trace::new(None);
        let receiver = Receiver::new(upstream, 4, &mut trace).unwrap();
        let mut read = Box::pin(receiver.collect(&mut trace));
        assert!(futures_util::poll!(&mut read).is_pending());
        drop(sender);
        assert_eq!(read.await.is_ok(), expected);
        dropped.await.unwrap();
    }
}

#[tokio::test]
async fn shared_capture_redacts_json_and_declares_binary_omission() {
    let directory = crate::test_files::private_directory();
    let path = directory.path().join("exchange.jsonl");
    let sink = crate::gateway::capture::Sink::open(
        serde_json::from_value(serde_json::json!({
            "path":path, "upstream_response":{"body":true}
        }))
        .unwrap(),
    )
    .unwrap();
    for (media, bytes) in [
        (
            "Application/JSON",
            br#"{"value":"synthetic","api_key":"synthetic-secret"}"#.as_slice(),
        ),
        (
            "Text/Event-Stream",
            b"data: {\"value\":\"synthetic\",\"api_key\":\"synthetic-secret\"}\n\n".as_slice(),
        ),
        ("audio/mpeg", b"synthetic-binary-secret".as_slice()),
    ] {
        let mut trace = Trace::new(None);
        trace.capture = Some(sink.exchange(
            None,
            &crate::gateway::ingress::auth::AuthenticatedAgent {
                agent_id: "fixture-agent".into(),
            },
        ));
        let (sender, upstream, dropped) = source(200, media);
        sender
            .send(Ok(Bytes::copy_from_slice(bytes)))
            .await
            .unwrap();
        drop(sender);
        let receiver = Receiver::new(upstream, 4096, &mut trace).unwrap();
        assert_eq!(receiver.collect(&mut trace).await.unwrap(), bytes);
        dropped.await.unwrap();
    }
    sink.flush().await;
    let captured = std::fs::read_to_string(path).unwrap();
    assert_eq!(captured.matches("\"status\":\"body_unit\"").count(), 2);
    assert_eq!(captured.matches("\"status\":\"source_eof\"").count(), 2);
    assert!(captured.contains("unsupported_body"));
    assert!(
        !captured.contains("synthetic-secret") && !captured.contains("synthetic-binary-secret")
    );
}

#[tokio::test]
async fn image_read_timeout_keeps_its_wire_policy_and_precise_internal_cause() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (release, wait) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0; 1024];
        assert!(stream.read(&mut request).await.unwrap() > 0);
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n",
            )
            .await
            .unwrap();
        let _ = wait.await;
    });
    let upstream = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("http://{address}/"))
        .timeout(Duration::from_millis(150))
        .send()
        .await
        .unwrap();
    let mut trace = Trace::new(None);
    trace.begin_candidate(1);
    let error = tokio::time::timeout(
        Duration::from_secs(2),
        tasks::images::receive(
            upstream,
            crate::adapter::images::UpstreamAdapter::GptImage,
            4096,
            &mut trace,
        ),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert_eq!(error.status.as_u16(), 502);
    assert_eq!(error.code(), "upstream_error");
    assert_eq!(error.recovery, Default::default());
    assert_eq!(
        trace.report.attempt().failure.unwrap().cause.code(),
        "transport.timeout"
    );
    drop(release);
    server.await.unwrap();
}
