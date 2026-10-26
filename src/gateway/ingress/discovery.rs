//! Startup-only Generation Text discovery. Pure contract projection, never Provider probing.
mod contract;

use super::models::{response, running};
use crate::{
    gateway::{ApiError, BoundEntry, Runtime, StartupError},
    protocol::ClientProtocol,
    topology::CompiledTopology,
};
use axum::{
    extract::{Path, State, rejection::PathRejection},
    http::StatusCode,
    response::Response,
};
use bytes::Bytes;
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};

const MAX_MODELS: usize = 64;
const MAX_BYTES: usize = 256 << 10;

#[derive(Serialize)]
struct Envelope<T> {
    object: &'static str,
    scope: &'static str,
    coverage: &'static str,
    unlisted: &'static str,
    provider_effectiveness: &'static str,
    full_loop: &'static str,
    data: T,
}
impl<T> Envelope<T> {
    fn new(object: &'static str, data: T) -> Self {
        Self {
            object,
            scope: "generation_text",
            coverage: "partial",
            unlisted: "not_assessed",
            provider_effectiveness: "not_asserted",
            full_loop: "not_asserted",
            data,
        }
    }
}
#[derive(Serialize)]
struct Model<'a> {
    id: &'a str,
    name: &'a str,
    owned_by: &'a str,
    interfaces: Vec<contract::Interface>,
}

pub(in crate::gateway) struct TextModelView {
    pub(in crate::gateway) list: Bytes,
    pub(in crate::gateway) models: BTreeMap<String, Bytes>,
}
impl TextModelView {
    pub(in crate::gateway) fn bind(
        topology: &CompiledTopology,
        entries: &BTreeMap<(ClientProtocol, String), Arc<BoundEntry>>,
        response_bytes: usize,
    ) -> Result<Self, StartupError> {
        if entries.len() > MAX_MODELS * ClientProtocol::ALL.len() {
            return Err(StartupError::Binding);
        }
        let mut descriptors = BTreeMap::new();
        for ((_, label), entry) in entries {
            if entry.candidates.is_empty()
                || entry.candidates.len() > crate::execution::plan::MAX_CANDIDATES
            {
                return Err(StartupError::Binding);
            }
            let metadata = topology
                .model_metadata(&entry.public.canonical_model)
                .ok_or(StartupError::Binding)?;
            let descriptor = descriptors.entry(label.as_str()).or_insert_with(|| Model {
                id: label.as_str(),
                name: label.as_str(),
                owned_by: metadata.developer(),
                interfaces: vec![],
            });
            descriptor
                .interfaces
                .push(contract::Interface::from_bound(entry));
            if descriptors.len() > MAX_MODELS {
                return Err(StartupError::Binding);
            }
        }
        let limit = response_bytes.min(MAX_BYTES);
        let mut models = BTreeMap::new();
        for (label, descriptor) in &descriptors {
            let bytes = encode(&Envelope::new("morphiecore.model", descriptor), limit)?;
            models.insert((*label).to_owned(), bytes);
        }
        let list = encode(
            &Envelope::new(
                "morphiecore.model_list",
                descriptors.values().collect::<Vec<_>>(),
            ),
            limit,
        )?;
        Ok(Self { list, models })
    }
}
fn encode(value: &impl Serialize, limit: usize) -> Result<Bytes, StartupError> {
    let bytes = serde_json::to_vec(value).map_err(|_| StartupError::Binding)?;
    if bytes.len() > limit {
        return Err(StartupError::Limits);
    }
    Ok(bytes.into())
}

pub(in crate::gateway) async fn list(
    State(state): State<Arc<Runtime>>,
) -> Result<Response, ApiError> {
    running(&state)?;
    Ok(response(state.text_models.list.clone()))
}
pub(in crate::gateway) async fn retrieve(
    State(state): State<Arc<Runtime>>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Response, ApiError> {
    running(&state)?;
    let Path(model) = path.map_err(|_| ApiError::invalid())?;
    let bytes = state.text_models.models.get(&model).ok_or(ApiError::new(
        StatusCode::NOT_FOUND,
        crate::gateway::error::ErrorKind::ModelNotFound,
    ))?;
    Ok(response(bytes.clone()))
}
