//! Delivery owner tests use an instrumented HTTP body, not a second Router.
use super::*;
use crate::{
    gateway::{family, tests::gateway},
    protocol::openai::Profile,
};
use futures_util::StreamExt;
use std::{sync::atomic::AtomicUsize, time::Duration};
#[allow(dead_code)]
#[path = "../../tests/support/chat_profile.rs"]
mod wire;
struct Tracked {
    rx: mpsc::Receiver<Result<Bytes, std::io::Error>>,
    reads: Arc<AtomicUsize>,
    dropped: Option<oneshot::Sender<()>>,
}
impl Stream for Tracked {
    type Item = Result<Bytes, std::io::Error>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let result = self.rx.poll_recv(cx);
        if matches!(&result, Poll::Ready(Some(Ok(_)))) {
            self.reads.fetch_add(1, Ordering::SeqCst);
        }
        result
    }
}
impl Drop for Tracked {
    fn drop(&mut self) {
        if let Some(done) = self.dropped.take() {
            let _ = done.send(());
        }
    }
}
type UpstreamProbe = (
    mpsc::Sender<Result<Bytes, std::io::Error>>,
    reqwest::Response,
    oneshot::Receiver<()>,
    Arc<AtomicUsize>,
);
fn upstream(status: u16, media: &str) -> UpstreamProbe {
    let (tx, rx) = mpsc::channel(2);
    let (done, dropped) = oneshot::channel();
    let reads = Arc::new(AtomicUsize::new(0));
    let body = reqwest::Body::wrap_stream(Tracked {
        rx,
        reads: reads.clone(),
        dropped: Some(done),
    });
    let response = reqwest::Response::from(
        axum::http::Response::builder()
            .status(status)
            .header("content-type", media)
            .body(body)
            .unwrap(),
    );
    (tx, response, dropped, reads)
}
fn frames() -> Vec<Bytes> {
    wire::events(2)
        .iter()
        .map(|v| Bytes::from(format!("data: {v}\n\n")))
        .collect()
}
async fn response(
    gate: &crate::gateway::Gateway,
    upstream: reqwest::Response,
    stream: bool,
    deadline: Instant,
) -> Result<Response, ApiError> {
    let entry = gate.state.entries[&(family(Profile::Chat), "deepseek-flash".into())].clone();
    let request=entry.client.decode_request(serde_json::json!({"model":"deepseek-flash","messages":[{"role":"user","content":"hello"}],"stream":stream,"stream_options":if stream {serde_json::json!({"include_usage":true,"include_obfuscation":false})}else{serde_json::Value::Null}}).to_string().as_bytes()).unwrap();
    let mut diagnostic_headers = axum::http::HeaderMap::new();
    diagnostic_headers.insert(
        "x-morphiecore-probe-id",
        "00000000000000000000000000000001:1".parse().unwrap(),
    );
    let candidate = entry.candidates[0].clone();
    respond(
        entry,
        candidate,
        request,
        upstream,
        &gate.state.limits,
        deadline,
        gate.state.shutdown.clone(),
        gate.state.permits.clone().acquire_owned().await.unwrap(),
        super::Trace::new(gate.state.diagnostics.as_ref(), &diagnostic_headers),
    )
    .await
}
#[tokio::test]
async fn diagnostic_status_and_final_intake_are_from_the_original_attempt() {
    let directory = crate::test_files::private_directory();
    let path = directory.path().join("events.jsonl");
    let gate = gateway(Limits::default())
        .with_probe_diagnostics(&path)
        .unwrap();
    for status in [429, 503, 200] {
        let (tx, mut source, _, _) = upstream(status, "application/json");
        source
            .headers_mut()
            .insert("retry-after", "7".parse().unwrap());
        tx.send(Ok(Bytes::from_static(b"synthetic-private-invalid-json")))
            .await
            .unwrap();
        drop(tx);
        let error = response(
            &gate,
            source,
            false,
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(error.status.as_u16(), if status == 429 { 429 } else { 502 });
    }
    gate.flush_probe_diagnostics().await;
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(!raw.contains("synthetic-private"));
    let records: Vec<serde_json::Value> = raw
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 3);
    for (record, status) in records.iter().zip([429, 503, 200]) {
        assert_eq!(record["upstream_status"], status);
        assert_eq!(record["retry_after_seconds"], 7);
        assert_eq!(
            record["stage"],
            if status == 200 {
                "terminal"
            } else {
                "response_head"
            }
        );
    }
}
async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(2), future)
        .await
        .expect("bounded owner check")
}
#[tokio::test]
async fn first_frame_precedes_terminal_and_ack_gates_upstream_progress() {
    let gate = gateway(Limits::default());
    let (tx, upstream, dropped, reads) = upstream(200, "text/event-stream");
    let frames = frames();
    tx.send(Ok(Bytes::from(frames[..2].concat())))
        .await
        .unwrap();
    let response = bounded(response(
        &gate,
        upstream,
        true,
        Instant::now() + Duration::from_secs(3),
    ))
    .await
    .unwrap();
    // The worker is waiting for HTTP-body acknowledgement, not collecting the stream.
    tx.send(Ok(Bytes::from(
        [frames[2..].concat(), b"data: [DONE]\n\n".to_vec()].concat(),
    )))
    .await
    .unwrap();
    drop(tx);
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    let mut body = response.into_body().into_data_stream();
    let first = bounded(body.next()).await.unwrap().unwrap();
    assert!(!String::from_utf8_lossy(&first).contains("[DONE]"));
    let mut all = first.to_vec();
    bounded(async {
        while let Some(chunk) = body.next().await {
            all.extend_from_slice(&chunk.unwrap());
        }
    })
    .await;
    assert!(String::from_utf8_lossy(&all).contains("old "));
    assert_eq!(String::from_utf8_lossy(&all).matches("[DONE]").count(), 1);
    bounded(dropped).await.unwrap();
}
#[tokio::test]
async fn late_truncation_and_trailing_payload_never_emit_success_terminal() {
    for trailing in [false, true] {
        let gate = gateway(Limits::default());
        let (tx, upstream, _, _) = upstream(200, "text/event-stream");
        let mut bytes = frames().concat();
        if trailing {
            bytes.extend_from_slice(b"data: [DONE]\n\ndata: {}\n\n");
        }
        tx.send(Ok(Bytes::from(bytes))).await.unwrap();
        drop(tx);
        let response = bounded(response(
            &gate,
            upstream,
            true,
            Instant::now() + Duration::from_secs(3),
        ))
        .await
        .unwrap();
        let mut body = response.into_body().into_data_stream();
        let mut seen = Vec::new();
        let mut failed = false;
        bounded(async {
            while let Some(next) = body.next().await {
                match next {
                    Ok(bytes) => seen.extend_from_slice(&bytes),
                    Err(_) => {
                        failed = true;
                        break;
                    }
                }
            }
        })
        .await;
        assert!(failed);
        assert!(String::from_utf8_lossy(&seen).contains("old "));
        assert!(!String::from_utf8_lossy(&seen).contains("[DONE]"));
    }
}
#[tokio::test]
async fn drop_deadline_and_shutdown_release_upstream_even_when_body_is_unpolled() {
    for mode in ["drop", "deadline", "shutdown", "server-drop"] {
        let gate = gateway(Limits {
            concurrency: 1,
            ..Limits::default()
        });
        let (tx, upstream, dropped, _) = upstream(200, "text/event-stream");
        tx.send(Ok(Bytes::from(frames()[..2].concat())))
            .await
            .unwrap();
        let deadline = Instant::now()
            + if mode == "deadline" {
                Duration::from_millis(40)
            } else {
                Duration::from_secs(3)
            };
        let response = bounded(response(&gate, upstream, true, deadline))
            .await
            .unwrap();
        if mode == "drop" {
            drop(response);
        } else {
            if mode == "shutdown" {
                gate.shutdown();
            } else if mode == "server-drop" {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let (ready, started) = oneshot::channel();
                let serving = tokio::spawn(gate.clone().serve(listener, async {
                    let _ = ready.send(());
                    std::future::pending::<()>().await;
                }));
                bounded(started).await.unwrap();
                serving.abort();
                let _ = serving.await;
            }
            bounded(dropped).await.unwrap(); // No HTTP-body poll has occurred.
            assert!(
                bounded(response.into_body().into_data_stream().next())
                    .await
                    .unwrap()
                    .is_err()
            );
            let _released = bounded(gate.state.permits.clone().acquire_owned())
                .await
                .unwrap();
            continue;
        }
        bounded(dropped).await.unwrap();
        let _released = bounded(gate.state.permits.clone().acquire_owned())
            .await
            .unwrap();
    }
}
#[tokio::test]
async fn invalid_heads_and_static_overflow_fail_without_exposing_provider_data() {
    for (status, media) in [
        (302, "application/json"),
        (401, "application/json"),
        (503, "application/json"),
        (200, "text/plain"),
        (200, "text/event-stream"),
    ] {
        let gate = gateway(Limits::default());
        let (_tx, upstream, _, reads) = upstream(status, media);
        let error = bounded(response(
            &gate,
            upstream,
            false,
            Instant::now() + Duration::from_secs(1),
        ))
        .await
        .unwrap_err();
        assert_eq!(error.status, 502);
        assert_eq!(reads.load(Ordering::SeqCst), 0);
    }
    let gate = gateway(Limits {
        // Allow the startup discovery view while keeping the upstream body over budget.
        response_bytes: 128,
        ..Limits::default()
    });
    let (tx, upstream, _, _) = upstream(200, "application/json");
    tx.send(Ok(Bytes::from(
        "synthetic-private-upstream-detail".repeat(5),
    )))
    .await
    .unwrap();
    drop(tx);
    assert_eq!(
        bounded(response(
            &gate,
            upstream,
            false,
            Instant::now() + Duration::from_secs(1)
        ))
        .await
        .unwrap_err()
        .status,
        502
    );
}
