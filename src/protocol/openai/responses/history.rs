//! Typed managed-history composition. No public-wire round trip or token opening.
use crate::{
    protocol::{
        CodecError, DecodedRequest, DecodedResponse,
        openai::{content::intake::Items, shape::DecodeDirection},
    },
    semantic::{task::generation::*, value::Text},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn seed<'a>(
    input: &DecodedRequest,
    output: &DecodedResponse,
    direction: DecodeDirection<'a>,
) -> Result<(Items<'a>, Vec<MessageEnvelope>), CodecError> {
    input.semantic.validate()?;
    input
        .fidelity
        .check_attachment_bindings(input.semantic.items())?;
    if !matches!(
        output.semantic.outcome(),
        Outcome::Completed | Outcome::Incomplete
    ) {
        return Err(CodecError::Invalid("non-replayable response"));
    }
    output
        .fidelity
        .check_attachment_bindings(output.semantic.items())?;
    let mut used: BTreeSet<_> = input
        .semantic
        .items()
        .iter()
        .map(|(id, _)| id.scope())
        .collect();
    used.extend(
        input
            .semantic
            .items()
            .iter()
            .flat_map(|(_, item)| history_parts(item).map(PartId::scope)),
    );
    used.extend(input.semantic.resources().iter().map(|(id, _)| id.scope()));
    used.extend(
        input
            .semantic
            .message_envelopes()
            .iter()
            .map(|g| g.id().scope()),
    );
    let mut fresh = || -> Result<LocalScope, CodecError> {
        for n in 1..=used.len() as u64 + 1 {
            let scope = LocalScope::new(n);
            if used.insert(scope) {
                return Ok(scope);
            }
        }
        Err(CodecError::Limit)
    };
    let scope = fresh()?;
    let next_scope = fresh()?;
    let owners: BTreeMap<_, _> = output
        .semantic
        .items()
        .iter()
        .enumerate()
        .map(|(n, (id, _))| (*id, ItemId::scoped(scope, n as u64 + 1)))
        .collect();
    let parts: BTreeMap<_, _> = output
        .semantic
        .items()
        .iter()
        .flat_map(|(_, item)| history_parts(item))
        .enumerate()
        .map(|(n, id)| (id, PartId::scoped(scope, n as u64 + 1)))
        .collect();
    let resources: BTreeMap<_, _> = output
        .semantic
        .resources()
        .iter()
        .enumerate()
        .map(|(n, (id, _))| (id, ResourceId::scoped(scope, n as u64 + 1)))
        .collect();
    let imported_resources = ResourceTable::new(
        output
            .semantic
            .resources()
            .iter()
            .map(|(id, value)| (resources[&id], value.clone()))
            .collect(),
    )?;
    let mut b = Items::in_scope(direction, next_scope);
    b.items = input.semantic.items().to_vec();
    b.resources = input
        .semantic
        .resources()
        .clone()
        .merge(&imported_resources)?;
    b.fidelity = input.fidelity.clone();
    let mut groups = input.semantic.message_envelopes().to_vec();
    for (old_id, old) in output.semantic.items() {
        let mut item = old.clone();
        match &mut item {
            Item::Instruction(value) => {
                for p in &mut value.parts {
                    p.id = parts[&p.id];
                }
            }
            Item::Message(value) => {
                for p in &mut value.parts {
                    p.id = parts[&p.id];
                    match &mut p.content {
                        ContentPart::Resource(r) => r.id = resources[&r.id],
                        ContentPart::Text(text) if !text.annotations().is_empty() => {
                            let annotations = text
                                .annotations()
                                .iter()
                                .map(|annotation| {
                                    annotation.check_source(output.semantic.resources())?;
                                    let mut citation = annotation.value().clone();
                                    citation.source = resources[&citation.source];
                                    Annotation::new(citation, &imported_resources)
                                })
                                .collect::<Result<_, _>>()?;
                            *text = TextContent::new(
                                Text::allowing_empty(text.as_str(), "history text", MAX_TEXT_BYTES)
                                    .map_err(|_| CodecError::Limit)?,
                                annotations,
                                text.logprobs().clone(),
                            )?;
                        }
                        _ => {}
                    }
                }
            }
            Item::Reasoning(value) => {
                for (id, _) in &mut value.parts {
                    *id = parts[id];
                }
            }
            Item::ToolResult(value) | Item::CustomResult(value) => {
                remap_output(&mut value.output, &parts, &resources)
            }
            Item::ProviderTool(value) => {
                if let ProviderOperation::Reference(ProviderOperationReference::Local(id)) =
                    &mut value.operation
                {
                    *id = *owners
                        .get(id)
                        .ok_or(CodecError::Invalid("history operation reference"))?;
                }
                if let Some(output) = &mut value.output {
                    remap_output(output, &parts, &resources);
                }
            }
            Item::ToolCall(_)
            | Item::CustomCall(_)
            | Item::Compaction(_)
            | Item::ConfigurationUpdate(_)
            | Item::ToolChange(_)
            | Item::Program(_)
            | Item::ProgramOutput(_) => {}
        }
        let new_id = owners[old_id];
        b.fidelity
            .import_history_item(&output.fidelity, *old_id, old, new_id, &item)?;
        b.items.push((new_id, item));
    }
    for (n, group) in output.semantic.message_envelopes().iter().enumerate() {
        let id = GroupId::new(scope, n as u64 + 1);
        groups.push(if group.members().is_empty() {
            MessageEnvelope::empty(id, group.role(), group.before().map(|owner| owners[&owner]))
        } else {
            MessageEnvelope::new(
                id,
                group.role(),
                group.members().iter().map(|owner| owners[owner]).collect(),
            )?
        });
    }
    if b.items.len() > MAX_ITEMS {
        return Err(CodecError::Limit);
    }
    Ok((b, groups))
}

fn history_parts(item: &Item) -> impl Iterator<Item = PartId> + '_ {
    let provider_parts: &[Part<InputPart>] = match item {
        Item::ProviderTool(value) => match &value.output {
            Some(ToolOutput::Parts(parts)) => parts,
            _ => &[],
        },
        _ => &[],
    };
    item.part_ids().chain(provider_parts.iter().map(|p| p.id))
}

fn remap_output(
    output: &mut ToolOutput,
    parts: &BTreeMap<PartId, PartId>,
    resources: &BTreeMap<ResourceId, ResourceId>,
) {
    if let ToolOutput::Parts(values) = output {
        for p in values {
            p.id = parts[&p.id];
            if let InputPart::Resource(r) = &mut p.content {
                r.id = resources[&r.id];
            }
        }
    }
}
