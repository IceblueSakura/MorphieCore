//! Startup-only Generation Text discovery. Pure contract projection, never Provider probing.
pub(super) mod contract;
pub(crate) mod defaults;
mod media;

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

pub(in crate::gateway) struct Contracts {
    pub(in crate::gateway) interfaces: BTreeMap<(ClientProtocol, String), contract::Interface>,
    preferences: BTreeMap<String, Preference>,
}

#[derive(Serialize)]
struct Preference {
    state: &'static str,
    protocol: Option<&'static str>,
    basis: &'static str,
    uniform: Option<bool>,
}
impl Preference {
    fn from_bound(
        entry: &BoundEntry,
        entries: &BTreeMap<(ClientProtocol, String), Arc<BoundEntry>>,
        contracts: &Contracts,
    ) -> Self {
        let mut preference = Self {
            state: "not_assessed",
            protocol: None,
            basis: "primary_candidate_native_protocol_and_agent_default_preflight",
            uniform: None,
        };
        use crate::topology::GenerationProtocol;
        let primary = entry.candidates[0].endpoint.protocol;
        let (protocol, name) = match primary {
            GenerationProtocol::OpenAiChat => (ClientProtocol::Chat, "chat_completions"),
            GenerationProtocol::OpenAiResponses => (ClientProtocol::Responses, "responses"),
            GenerationProtocol::AnthropicMessages => (ClientProtocol::Messages, "messages"),
            _ => return preference,
        };
        let key = (protocol, entry.public.id.as_str().into());
        let Some(native) = entries.get(&key) else {
            return preference;
        };
        let same = |a: &crate::gateway::runtime::BoundCandidate,
                    b: &crate::gateway::runtime::BoundCandidate| {
            a.endpoint.id == b.endpoint.id && a.cache_scope == b.cache_scope
        };
        if !same(&entry.candidates[0], &native.candidates[0]) {
            return preference;
        }
        let interface = &contracts.interfaces[&key];
        let qualified = interface.qualified_candidates();
        if qualified.first() != Some(&true) {
            return preference;
        }
        preference.state = "assessed";
        preference.protocol = Some(name);
        preference.uniform = Some(
            qualified.iter().all(|qualified| *qualified)
                && entry.candidates.len() == native.candidates.len()
                && entry
                    .candidates
                    .iter()
                    .zip(&native.candidates)
                    .all(|(a, b)| same(a, b))
                && entry
                    .candidates
                    .iter()
                    .all(|candidate| candidate.endpoint.protocol == primary),
        );
        preference
    }
}

pub(in crate::gateway) fn compile(
    entries: &BTreeMap<(ClientProtocol, String), Arc<BoundEntry>>,
) -> Result<Contracts, StartupError> {
    if entries.len() > MAX_MODELS * ClientProtocol::ALL.len()
        || entries.values().any(|entry| {
            entry.candidates.is_empty()
                || entry.candidates.len() > crate::execution::plan::MAX_CANDIDATES
        })
    {
        return Err(StartupError::Binding);
    }
    let mut contracts = Contracts {
        interfaces: entries
            .iter()
            .map(|(key, entry)| (key.clone(), contract::Interface::from_bound(entry)))
            .collect(),
        preferences: BTreeMap::new(),
    };
    for ((_, label), entry) in entries {
        if !contracts.preferences.contains_key(label) {
            let preference = Preference::from_bound(entry, entries, &contracts);
            contracts.preferences.insert(label.clone(), preference);
        }
    }
    Ok(contracts)
}

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
    preferred_interface: &'a Preference,
    interfaces: Vec<&'a contract::Interface>,
}

pub(in crate::gateway) struct TextModelView {
    pub(in crate::gateway) list: Bytes,
    pub(in crate::gateway) models: BTreeMap<String, Bytes>,
}
impl TextModelView {
    #[cfg(test)]
    pub(in crate::gateway) fn bind(
        topology: &CompiledTopology,
        entries: &BTreeMap<(ClientProtocol, String), Arc<BoundEntry>>,
        response_bytes: usize,
    ) -> Result<Self, StartupError> {
        Self::from_contracts(topology, entries, &compile(entries)?, response_bytes)
    }
    pub(in crate::gateway) fn from_contracts(
        topology: &CompiledTopology,
        entries: &BTreeMap<(ClientProtocol, String), Arc<BoundEntry>>,
        contracts: &Contracts,
        response_bytes: usize,
    ) -> Result<Self, StartupError> {
        let mut descriptors = BTreeMap::new();
        for (key @ (_, label), entry) in entries {
            let metadata = topology
                .model_metadata(&entry.public.canonical_model)
                .ok_or(StartupError::Binding)?;
            let descriptor = descriptors.entry(label.as_str()).or_insert_with(|| Model {
                id: label.as_str(),
                name: label.as_str(),
                owned_by: metadata.developer(),
                preferred_interface: &contracts.preferences[label],
                interfaces: vec![],
            });
            descriptor
                .interfaces
                .push(contracts.interfaces.get(key).ok_or(StartupError::Binding)?);
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
