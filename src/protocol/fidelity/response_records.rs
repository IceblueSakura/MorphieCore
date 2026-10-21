//! Output-only creator, response and routing reports bound to verified owners.
use super::*;

impl FidelityRecords {
    /// Output-only report, never caller authority or a request-history control.
    pub fn observe_created_by(
        &mut self,
        owner: ItemId,
        item: &Item,
        resources: &ResourceTable,
        value: Option<&Value>,
    ) -> Result<(), CodecError> {
        if !matches!(
            item,
            Item::ToolResult(_) | Item::CustomResult(_) | Item::Compaction(_)
        ) {
            if value.is_some_and(|v| !v.is_null()) {
                return Err(CodecError::Unsupported("created_by owner".into()));
            }
            return Ok(());
        }
        let actor = value
            .filter(|v| !v.is_null())
            .map(|v| {
                Text::allowing_empty(
                    v.as_str().ok_or(CodecError::Invalid("created_by"))?,
                    "created_by",
                    256,
                )
                .map_err(|_| CodecError::Limit)
            })
            .transpose()?;
        let previous = self.reported_creators.get(&owner);
        if actor
            .as_ref()
            .zip(previous)
            .is_some_and(|(actor, old)| actor != &old.actor)
        {
            return Err(CodecError::Invalid("created_by changed"));
        }
        let actor = actor.or_else(|| previous.map(|old| old.actor.clone()));
        if let Some(actor) = actor {
            if !self.reported_creators.contains_key(&owner)
                && self.reported_creators.len() >= MAX_ITEMS
            {
                return Err(CodecError::Limit);
            }
            self.reported_creators.insert(
                owner,
                CreatorBinding {
                    actor,
                    dependency: creator_dependency(item, resources),
                },
            );
        }
        Ok(())
    }
    pub fn created_by(
        &self,
        owner: ItemId,
        item: &Item,
        resources: &ResourceTable,
    ) -> Option<&str> {
        self.reported_creators
            .get(&owner)
            .filter(|report| report.dependency == creator_dependency(item, resources))
            .map(|report| report.actor.as_str())
    }
    pub(crate) fn merge_item_reports(
        &mut self,
        source: &Self,
        state: &StreamState,
    ) -> Result<(), CodecError> {
        for owner in source.reported_creators.keys() {
            if let Ok(item) = state.item(*owner) {
                let snapshot = item.snapshot()?;
                if let Some(actor) = source.created_by(*owner, &snapshot, state.resources()) {
                    self.observe_created_by(
                        *owner,
                        &snapshot,
                        state.resources(),
                        Some(&Value::String(actor.into())),
                    )?;
                }
            }
        }
        let owners = self.reported_creators.keys().copied().collect::<Vec<_>>();
        for owner in owners {
            let snapshot = state.item(owner)?.snapshot()?;
            self.observe_created_by(owner, &snapshot, state.resources(), None)?;
        }
        Ok(())
    }
    /// Partial snapshots are checked but cannot bind response-wide facts.
    pub(crate) fn capture_response_extras(
        &mut self,
        protocol: Profile,
        adaptation: &Adaptation,
        o: &Map<String, Value>,
        semantic: Option<&GenerationResponse>,
    ) -> Result<(), CodecError> {
        let mut values = BTreeMap::new();
        let mut used = 0usize;
        for name in declared_response_extras(protocol, adaptation) {
            if let Some(value) = o.get(*name) {
                used = used.saturating_add(
                    crate::semantic::value::json_size(value, RESPONSE_EXTRAS_BUDGET)
                        .map_err(|_| CodecError::Limit)?,
                );
                if used > RESPONSE_EXTRAS_BUDGET {
                    return Err(CodecError::Limit);
                }
                values.insert((*name).into(), value.clone());
            }
        }
        if let Some(semantic) = semantic {
            self.response_extras = if values.is_empty() {
                None
            } else {
                Some(ResponseExtras {
                    protocol,
                    profile: adaptation.profile_id,
                    origin: adaptation
                        .scope
                        .clone()
                        .ok_or(CodecError::Invalid("unbound response extras"))?,
                    response_id: Text::new(
                        o.get("id")
                            .and_then(Value::as_str)
                            .ok_or(CodecError::Invalid("response identity"))?,
                        "response identity",
                        256,
                    )
                    .map_err(|_| CodecError::Limit)?,
                    dependency: response_dependency(semantic),
                    values,
                })
            };
        }
        Ok(())
    }
    pub(crate) fn projected_response_extras(
        &self,
        protocol: Profile,
        adaptation: &Adaptation,
        semantic: &GenerationResponse,
        response_id: &str,
    ) -> Option<&BTreeMap<String, Value>> {
        self.response_extras
            .as_ref()
            .filter(|r| {
                r.protocol == protocol
                    && r.profile == adaptation.profile_id
                    && Some(&r.origin) == adaptation.scope.as_ref()
                    && r.response_id.as_str() == response_id
                    && r.dependency == response_dependency(semantic)
                    && adaptation.rules.response_extras
            })
            .map(|r| &r.values)
    }
    pub(crate) fn capture_routing_extras(
        &mut self,
        protocol: Profile,
        adaptation: &Adaptation,
        values: crate::protocol::openai::dialect::adapter_shapes::Extras,
        semantic: &GenerationResponse,
        response_id: &str,
    ) -> Result<(), CodecError> {
        crate::protocol::openai::dialect::adapter_shapes::check_budget(&values)?;
        self.routing_extras = if values.is_empty() {
            None
        } else {
            if !(adaptation.rules.routing_extras
                || adaptation.rules.chat_stop_diagnostics
                || adaptation.rules.reported_request_id
                || adaptation.rules.responses_access_programs
                || adaptation.rules.responses_zero_tool_accounting
                || adaptation.rules.responses_inactive_reports
                || adaptation.rules.responses_usage_attribution
                || adaptation.rules.responses_context_accounting)
            {
                return Err(CodecError::Unsupported("routing extras".into()));
            }
            Some(ResponseExtras {
                protocol,
                profile: adaptation.profile_id,
                origin: adaptation
                    .scope
                    .clone()
                    .ok_or(CodecError::Invalid("unbound routing extras"))?,
                response_id: Text::new(response_id, "response identity", 256)
                    .map_err(|_| CodecError::Limit)?,
                dependency: response_dependency(semantic),
                values,
            })
        };
        Ok(())
    }
    pub(crate) fn projected_routing_extras(
        &self,
        protocol: Profile,
        adaptation: &Adaptation,
        semantic: &GenerationResponse,
        response_id: &str,
    ) -> Option<&BTreeMap<String, Value>> {
        self.routing_extras
            .as_ref()
            .filter(|r| {
                (adaptation.rules.routing_extras
                    || adaptation.rules.chat_stop_diagnostics
                    || adaptation.rules.reported_request_id
                    || adaptation.rules.responses_access_programs
                    || adaptation.rules.responses_zero_tool_accounting
                    || adaptation.rules.responses_inactive_reports
                    || adaptation.rules.responses_usage_attribution
                    || adaptation.rules.responses_context_accounting)
                    && r.protocol == protocol
                    && r.profile == adaptation.profile_id
                    && Some(&r.origin) == adaptation.scope.as_ref()
                    && r.response_id.as_str() == response_id
                    && r.dependency == response_dependency(semantic)
            })
            .map(|r| &r.values)
    }
    pub(crate) fn copy_response_records(&mut self, source: &Self) {
        self.response_extras = source.response_extras.clone();
        self.routing_extras = source.routing_extras.clone();
        self.normalizations = source.normalizations.clone();
    }
    /// Intake only, after independent native snapshot equality and terminal materialization.
    /// Local source IDs may differ between static and interleaved event parsing.
    pub(crate) fn bind_verified_snapshot_records(
        &mut self,
        source: &Self,
        semantic: &GenerationResponse,
    ) {
        self.copy_response_records(source);
        for (owner, item) in semantic.items() {
            let wire = self.response_item_id(*owner);
            let source_owner = source
                .response_item_ids
                .iter()
                .find(|(_, id)| Some(id.as_str()) == wire)
                .map(|(id, _)| *id);
            if let Some(report) = source_owner.and_then(|id| source.reported_creators.get(&id)) {
                self.reported_creators.insert(
                    *owner,
                    CreatorBinding {
                        actor: report.actor.clone(),
                        dependency: creator_dependency(item, semantic.resources()),
                    },
                );
            }
        }
        let dependency = response_dependency(semantic);
        for record in [&mut self.response_extras, &mut self.routing_extras]
            .into_iter()
            .flatten()
        {
            record.dependency = dependency;
        }
    }
}

fn creator_dependency(item: &Item, resources: &ResourceTable) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(format!("{item:?}").as_bytes());
    if let Item::Compaction(value) = item {
        hash.update(value.fingerprint());
    }
    if let Item::ToolResult(result) | Item::CustomResult(result) = item
        && let ToolOutput::Parts(parts) = &result.output
    {
        for part in parts {
            if let InputPart::Resource(resource) = &part.content {
                if let Some(location) = resources.get(resource.id) {
                    hash.update(location.fingerprint());
                }
                if let ResourceDescription::File(file) = &resource.description {
                    match &file.filename {
                        None => hash.update([0]),
                        Some(name) => {
                            hash.update([1]);
                            hash.update((name.as_str().len() as u64).to_le_bytes());
                            hash.update(name.as_str().as_bytes());
                        }
                    }
                    match &file.context {
                        crate::semantic::value::Presence::Absent => hash.update([0]),
                        crate::semantic::value::Presence::Null => hash.update([1]),
                        crate::semantic::value::Presence::Value(context) => {
                            hash.update([2]);
                            hash.update((context.as_str().len() as u64).to_le_bytes());
                            hash.update(context.as_str().as_bytes());
                        }
                    }
                }
            }
        }
    }
    hash.finalize().into()
}
// This in-process fingerprint is not persisted or exposed as a wire identity.
// Hashing the complete typed response conservatively invalidates extras after edits.
fn response_dependency(response: &GenerationResponse) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(format!("{response:?}").as_bytes());
    if let Some(details) = response.details().refusal.value() {
        hash.update(details.fingerprint());
    }
    for (id, value) in response.resources().iter() {
        hash.update(id.scope().get().to_le_bytes());
        hash.update(id.get().to_le_bytes());
        hash.update(value.fingerprint());
    }
    // Opaque Debug is redacted; bind its bytes separately without formatting them.
    for (id, item) in response.items() {
        let value = match item {
            Item::Reasoning(r) => r.replay.as_ref(),
            Item::Compaction(value) => Some(value),
            Item::ToolCall(call) => call.replay.as_ref(),
            _ => None,
        };
        if let Some(value) = value {
            hash.update(id.scope().get().to_le_bytes());
            hash.update(id.get().to_le_bytes());
            hash.update(value.fingerprint());
        }
    }
    for (_, item) in response.items() {
        if matches!(item, Item::ToolResult(_) | Item::CustomResult(_)) {
            hash.update(creator_dependency(item, response.resources()));
        }
        if let Item::Message(m) = item {
            for p in &m.parts {
                if let Some(value) = &p.replay {
                    hash.update(value.fingerprint());
                }
                match &p.content {
                    ContentPart::Text(t) => {
                        for a in t.annotations() {
                            hash.update(a.fingerprint());
                        }
                    }
                    ContentPart::Audio(a) => hash.update(a.fingerprint()),
                    ContentPart::AudioReference(a) => hash.update(a.fingerprint()),
                    _ => {}
                }
            }
        }
    }
    hash.finalize().into()
}
