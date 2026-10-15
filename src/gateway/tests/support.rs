//! Synthetic gateway and upstream body fixtures, without test cases.
use super::topology::topology;
use crate::gateway::{Entry, Gateway, Limits};
use crate::{
    protocol::openai::Profile,
    provider::{CredentialBindingId, SecretMaterial},
    topology::EndpointId,
};
use bytes::Bytes;
use futures_util::Stream;
use std::{
    collections::BTreeMap,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::sync::{mpsc, oneshot};
pub(in crate::gateway) const KEY: &str = "synthetic-gateway-client-token-0001";
pub(in crate::gateway) fn gateway(limits: Limits) -> Gateway {
    Gateway::new(
        topology(),
        crate::gateway::TaskEntries {
            generation: vec![Entry {
                model: "fixture-model".into(),
                protocol: Profile::Chat,
                endpoint: EndpointId::new("fixture-chat").unwrap(),
            }],
            ..Default::default()
        },
        BTreeMap::from([(
            CredentialBindingId::new("fixture-key").unwrap(),
            Arc::new(SecretMaterial::new("synthetic-upstream-credential-0001").unwrap()),
        )]),
        SecretMaterial::new(KEY).unwrap(),
        limits,
        None,
    )
    .unwrap()
}
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
pub(in crate::gateway) fn source(status: u16, media: &str) -> Probe {
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
