//! Bounded loopback body lifecycle; no MorphieCore service, Provider, or private config.
use crate::wire;
use axum::{
    Router,
    body::{Body, Bytes},
    http::header::CONTENT_TYPE,
    routing::get,
};
use futures_util::stream;
use morphiecore::protocol::openai::sse::{ResponsesSseDecoder, SseLimits, encode_frame};
use std::{convert::Infallible, time::Duration};
use tokio::{net::TcpListener, sync::mpsc};

fn body(rx: mpsc::Receiver<Bytes>) -> Body {
    Body::from_stream(stream::unfold(rx, |mut rx| async move {
        rx.recv()
            .await
            .map(|frame| (Ok::<_, Infallible>(frame), rx))
    }))
}

#[tokio::test]
async fn malformed_or_oversize_frame_after_commit_cannot_become_success() {
    let initial = encode_frame(&wire::events(2)[0], SseLimits::default().max_event_bytes).unwrap();
    let terminal = encode_frame(
        wire::events(2).last().unwrap(),
        SseLimits::default().max_event_bytes,
    )
    .unwrap();
    for oversize in [false, true] {
        let limits = SseLimits {
            max_event_bytes: initial.len() + 4,
            ..SseLimits::default()
        };
        let bad = if oversize {
            Bytes::from(format!("data: {}\n\n", "x".repeat(limits.max_event_bytes)))
        } else {
            Bytes::from_static(b"event: response.completed\ndata: invalid-json\n\n")
        };
        let first = initial.clone();
        let last = terminal.clone();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route(
                    "/broken",
                    get(move || {
                        let frames = [first.clone(), bad.clone(), last.clone()];
                        async move {
                            let (tx, rx) = mpsc::channel(1);
                            tokio::spawn(async move {
                                for frame in frames {
                                    if tx.send(frame).await.is_err() {
                                        return;
                                    }
                                }
                            });
                            ([(CONTENT_TYPE, "text/event-stream")], body(rx))
                        }
                    }),
                ),
            )
            .await
            .unwrap();
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let mut response = tokio::time::timeout(
            Duration::from_secs(3),
            client.get(format!("http://{address}/broken")).send(),
        )
        .await
        .unwrap()
        .unwrap();
        let mut decoder = ResponsesSseDecoder::new(200, "text/event-stream", limits, None).unwrap();
        let mut started = 0;
        let mut rejected = false;
        while let Some(chunk) = tokio::time::timeout(Duration::from_secs(3), response.chunk())
            .await
            .unwrap()
            .unwrap()
        {
            let mut rest = chunk.as_ref();
            while !rest.is_empty() {
                match decoder.consume(rest) {
                    Ok((n, events)) => {
                        started += events.len();
                        rest = &rest[n..];
                    }
                    Err(_) => {
                        rejected = true;
                        break;
                    }
                }
            }
            if rejected {
                break;
            }
        }
        assert_eq!(started, 1);
        assert!(rejected);
        assert!(decoder.finish().is_err());
        assert!(decoder.materialize().is_err());
        drop(response);
        server.abort();
        let _ = server.await;
    }
}

#[tokio::test]
async fn truncated_http_body_never_becomes_a_successful_responses_terminal() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/truncated",
                get(|| async {
                    let (tx, rx) = mpsc::channel(1);
                    tokio::spawn(async move {
                        let frame =
                            encode_frame(&wire::events(2)[0], SseLimits::default().max_event_bytes)
                                .unwrap();
                        let _ = tx.send(frame).await;
                    });
                    ([(CONTENT_TYPE, "text/event-stream")], body(rx))
                }),
            ),
        )
        .await
        .unwrap();
    });
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let mut response = tokio::time::timeout(
        Duration::from_secs(3),
        client.get(format!("http://{address}/truncated")).send(),
    )
    .await
    .unwrap()
    .unwrap();
    let mut decoder = ResponsesSseDecoder::new(
        response.status().as_u16(),
        response.headers()[CONTENT_TYPE].to_str().unwrap(),
        SseLimits::default(),
        None,
    )
    .unwrap();
    let mut started = 0;
    while let Some(chunk) = tokio::time::timeout(Duration::from_secs(3), response.chunk())
        .await
        .unwrap()
        .unwrap()
    {
        let mut rest = chunk.as_ref();
        while !rest.is_empty() {
            let (n, events) = decoder.consume(rest).unwrap();
            started += events.len();
            rest = &rest[n..];
        }
    }
    assert_eq!(started, 1);
    assert!(decoder.finish().is_err());
    assert!(decoder.materialize().is_err());
    server.abort();
    let _ = server.await;
}
