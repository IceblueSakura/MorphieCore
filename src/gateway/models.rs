//! Immutable, bounded discovery of activated public labels; never contacts an upstream.
use super::{ApiError, Runtime, StartupError};
use crate::topology::CompiledTopology;
use axum::{
    body::Body,
    extract::{Path, State, rejection::PathRejection},
    http::StatusCode,
    response::Response,
};
use bytes::Bytes;
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Serialize)]
struct Model<'a> {
    id: &'a str,
    object: &'static str,
    created: u64,
    owned_by: &'a str,
}
#[derive(Serialize)]
struct List<'a> {
    object: &'static str,
    data: Vec<Model<'a>>,
}
pub(super) struct ModelView {
    pub(super) list: Bytes,
    pub(super) models: BTreeMap<String, Bytes>,
}
impl ModelView {
    pub(super) fn bind<'a>(
        topology: &CompiledTopology,
        labels: impl IntoIterator<Item = &'a str>,
        response_bytes: usize,
    ) -> Result<Self, StartupError> {
        // Bounds apply before serialization, including embedded callers.
        let mut selected = std::collections::BTreeSet::new();
        for label in labels {
            selected.insert(label);
            if selected.len() > 64 {
                return Err(StartupError::Binding);
            }
        }
        let mut models = BTreeMap::new();
        let mut data = Vec::with_capacity(selected.len());
        for label in selected {
            let canonical = topology
                .model(label)
                .map(|m| &m.canonical_model)
                .or_else(|| topology.image_route(label).map(|r| &r.canonical_model))
                .or_else(|| topology.speech_route(label).map(|r| &r.canonical_model))
                .or_else(|| {
                    topology
                        .transcription_route(label)
                        .map(|r| &r.canonical_model)
                })
                .ok_or(StartupError::Binding)?;
            let metadata = topology
                .model_metadata(canonical)
                .ok_or(StartupError::Binding)?;
            let model = Model {
                id: label,
                object: "model",
                created: metadata.released_at(),
                owned_by: metadata.developer(),
            };
            let bytes = serde_json::to_vec(&model).map_err(|_| StartupError::Binding)?;
            models.insert(label.into(), Bytes::from(bytes));
            data.push(model);
        }
        // No pagination is defined by Models; never return a truncated successful list.
        // Source: https://developers.openai.com/api/reference/resources/models/methods/list
        let list = serde_json::to_vec(&List {
            object: "list",
            data,
        })
        .map_err(|_| StartupError::Binding)?;
        if list.len() > response_bytes.min(32 << 10) {
            return Err(StartupError::Limits);
        }
        Ok(Self {
            list: Bytes::from(list),
            models,
        })
    }
}

fn response(bytes: Bytes) -> Response {
    Response::builder()
        .header("content-type", "application/json")
        .header("cache-control", "no-store")
        .body(Body::from(bytes))
        .expect("static discovery headers")
}
fn running(state: &Runtime) -> Result<(), ApiError> {
    if state.shutdown.is_cancelled() {
        return Err(ApiError::shutdown());
    }
    Ok(())
}
pub(super) async fn list(State(state): State<Arc<Runtime>>) -> Result<Response, ApiError> {
    running(&state)?;
    Ok(response(state.models.list.clone()))
}
pub(super) async fn retrieve(
    State(state): State<Arc<Runtime>>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Response, ApiError> {
    running(&state)?;
    let Path(model) = path.map_err(|_| ApiError::invalid())?;
    let bytes = state
        .models
        .models
        .get(&model)
        .ok_or(ApiError::new(StatusCode::NOT_FOUND, "model_not_found"))?;
    Ok(response(bytes.clone()))
}
pub(super) async fn delete(
    State(state): State<Arc<Runtime>>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Response, ApiError> {
    running(&state)?;
    let Path(model) = path.map_err(|_| ApiError::invalid())?;
    if !state.models.models.contains_key(&model) {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "model_not_found"));
    }
    // The standard operation deletes owned fine-tuned models, not catalog entries.
    // This instance has no such ownership or deletion capability.
    // Source: https://developers.openai.com/api/reference/resources/models/methods/delete
    Err(ApiError::new(
        StatusCode::FORBIDDEN,
        "model_deletion_forbidden",
    ))
}
