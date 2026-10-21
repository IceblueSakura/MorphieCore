//! Pure validation and bounded accounting shared by construction, transforms and lowering.
mod controls;
use crate::semantic::task::generation::*;
pub(super) use controls::{output, tools};
use std::collections::{BTreeMap, BTreeSet};
// Sized for long agent sessions that resend whole histories with inline media.
pub const MAX_ITEMS: usize = 65_536;
pub const MAX_TEXT_BYTES: usize = 16 << 20;
pub const MAX_TOTAL_BYTES: usize = 32 << 20;
pub const MAX_TOOLS: usize = 128;
fn add(total: &mut usize, value: &str) -> Result<(), GenerationError> {
    charge(total, value.len())
}
fn charge(total: &mut usize, n: usize) -> Result<(), GenerationError> {
    if n > MAX_TEXT_BYTES {
        return Err(GenerationError::Limit);
    }
    *total = total.checked_add(n).ok_or(GenerationError::Limit)?;
    if *total > MAX_TOTAL_BYTES {
        return Err(GenerationError::Limit);
    }
    Ok(())
}
fn part_id(parts: &mut BTreeSet<PartId>, id: PartId) -> Result<(), GenerationError> {
    if !parts.insert(id) {
        return Err(GenerationError::DuplicatePartId);
    }
    if parts.len() > MAX_ITEMS {
        return Err(GenerationError::Limit);
    }
    Ok(())
}
/// One call-ID namespace across function/custom calls and programs: IDs must not be swapped.
#[derive(Clone, Copy, Eq, PartialEq)]
enum CallKind {
    Function,
    Custom,
    Program,
    Provider,
}
type AliasKey<'a> = (Option<&'a NativeAliasDomain>, &'a str);
fn caller_reference(
    context: &CallContext,
    calls: &BTreeMap<AliasKey<'_>, CallKind>,
) -> Result<(), GenerationError> {
    if let Some(CallOrigin::Program { caller_id }) = &context.caller
        && calls.get(&(None, caller_id.as_str())) != Some(&CallKind::Program)
    {
        return Err(GenerationError::InvalidDependency);
    }
    Ok(())
}
/// Input media is inert URL/inline content of any kind. Opaque references need an
/// issuer/lifecycle contract first.
fn input_part(
    value: &InputPart,
    resources: &ResourceTable,
    bytes: &mut usize,
    file_decoded_bytes: &mut usize,
) -> Result<(), GenerationError> {
    let resource = match value {
        InputPart::Text(text) => return add(bytes, text.as_str()),
        InputPart::Resource(resource) => resource.media(resources)?,
    };
    input_resource(resource, bytes, file_decoded_bytes)
}
fn input_resource(
    resource: ResourceView<'_>,
    bytes: &mut usize,
    file_decoded_bytes: &mut usize,
) -> Result<(), GenerationError> {
    if matches!(
        resource.location,
        ResourceLocation::OpaqueReference(_) | ResourceLocation::NamespacedReference { .. }
    ) {
        return Err(GenerationError::InvalidResource);
    }
    charge(bytes, resource.validate()?)?;
    if resource.kind() == ResourceKind::File {
        *file_decoded_bytes = file_decoded_bytes
            .checked_add(resource.inline_decoded_bytes()?.unwrap_or(0))
            .ok_or(GenerationError::Limit)?;
        if *file_decoded_bytes > MAX_TOTAL_FILE_DECODED_BYTES {
            return Err(GenerationError::Limit);
        }
    }
    Ok(())
}
pub fn items(
    items: &[(ItemId, Item)],
    response: bool,
    resources: &ResourceTable,
) -> Result<usize, GenerationError> {
    if items.is_empty() {
        return Err(GenerationError::EmptyInput);
    }
    if items.len() > MAX_ITEMS {
        return Err(GenerationError::Limit);
    }
    let mut ids = BTreeSet::new();
    let mut parts = BTreeSet::new();
    let mut calls = BTreeMap::new();
    let mut contexts = BTreeMap::new();
    let mut results = BTreeSet::new();
    let mut bytes = 0usize;
    let mut file_decoded_bytes = 0usize;
    let mut citations = 0usize;
    for (position, (id, item)) in items.iter().enumerate() {
        if !ids.insert(*id) {
            return Err(GenerationError::DuplicateItemId);
        }
        if response && item.has_cache_breakpoints() {
            return Err(GenerationError::InvalidResponse);
        }
        charge(&mut bytes, usize::from(item.has_cache_breakpoints()) * 32)?;
        match item {
            Item::Instruction(i) => {
                if response {
                    return Err(GenerationError::InvalidResponse);
                }
                for part in &i.parts {
                    if part.replay.is_some() {
                        return Err(GenerationError::InvalidReplay);
                    }
                    part_id(&mut parts, part.id)?;
                    charge(&mut bytes, usize::from(part.cache.is_some()) * 32)?;
                    add(&mut bytes, part.content.as_str())?;
                }
                if i.parts.is_empty() {
                    return Err(GenerationError::EmptyMessage);
                }
            }
            Item::Message(m) => {
                if response && m.role != MessageRole::Assistant {
                    return Err(GenerationError::InvalidResponse);
                }
                if m.phase.is_some() && m.role != MessageRole::Assistant {
                    return Err(GenerationError::PhaseInUserMessage);
                }
                if m.role == MessageRole::User && m.parts.is_empty() {
                    return Err(GenerationError::EmptyMessage);
                }
                for p in &m.parts {
                    part_id(&mut parts, p.id)?;
                    if let Some(value) = &p.replay {
                        if m.role != MessageRole::Assistant
                            || !matches!(p.content, ContentPart::Text(_))
                        {
                            return Err(GenerationError::InvalidReplay);
                        }
                        value.validate_attachment()?;
                        if m.status == ItemLifecycle::Completed && value.replay_token().is_none() {
                            return Err(GenerationError::InvalidReplay);
                        }
                        add(&mut bytes, value.as_str())?;
                    }
                    charge(&mut bytes, usize::from(p.cache.is_some()) * 32)?;
                    match &p.content {
                        ContentPart::Text(t) => {
                            // Citations and probabilities describe generated text; no
                            // official request carrier attaches them to user input.
                            if m.role != MessageRole::Assistant && !t.is_plain() {
                                return Err(GenerationError::TextMetadataInUserMessage);
                            }
                            t.validate()?;
                            citations += t.annotations().len();
                            if citations > MAX_ITEMS {
                                return Err(GenerationError::Limit);
                            }
                            for a in t.annotations() {
                                a.check_source(resources)?;
                            }
                            charge(&mut bytes, t.bytes())?;
                        }
                        ContentPart::Refusal(t) => {
                            if m.role != MessageRole::Assistant {
                                return Err(GenerationError::RefusalInUserMessage);
                            }
                            t.validate()?;
                            charge(&mut bytes, t.bytes())?;
                        }
                        ContentPart::Audio(audio) => {
                            if m.role != MessageRole::Assistant {
                                return Err(GenerationError::InvalidResource);
                            }
                            audio.validate()?;
                            charge(&mut bytes, audio.bytes())?;
                        }
                        ContentPart::AudioReference(reference) => {
                            if response || m.role != MessageRole::Assistant {
                                return Err(GenerationError::InvalidResource);
                            }
                            charge(&mut bytes, reference.bytes())?;
                        }
                        ContentPart::Resource(resource) => {
                            // Input media belong to user turns. Generated assistant media
                            // need their own output owner, not an input resource use.
                            if m.role == MessageRole::Assistant {
                                return Err(GenerationError::InvalidResource);
                            }
                            input_resource(
                                resource.media(resources)?,
                                &mut bytes,
                                &mut file_decoded_bytes,
                            )?;
                        }
                    }
                }
            }
            Item::ToolCall(c) => {
                if let Some(value) = &c.replay {
                    value.validate_call_attachment()?;
                    if c.status == ItemLifecycle::Completed && value.replay_token().is_none() {
                        return Err(GenerationError::InvalidReplay);
                    }
                    add(&mut bytes, value.as_str())?;
                }
                call_context(&c.context, &mut bytes)?;
                if !response {
                    caller_reference(&c.context, &calls)?;
                }
                contexts.insert(
                    (c.context.alias_domain.as_ref(), c.call_id.as_str()),
                    &c.context,
                );
                validate_call(
                    &mut calls,
                    &mut bytes,
                    c.context.alias_domain.as_ref(),
                    &c.call_id,
                    &c.name,
                    "",
                    CallKind::Function,
                )?;
                charge(&mut bytes, c.arguments.bytes()?)?;
                if c.status == ItemLifecycle::Completed
                    && matches!(c.arguments, ToolArguments::StructuredPartial(_))
                {
                    return Err(GenerationError::InvalidArguments);
                }
            }
            Item::CustomCall(c) => {
                call_context(&c.context, &mut bytes)?;
                if !response {
                    caller_reference(&c.context, &calls)?;
                }
                contexts.insert(
                    (c.context.alias_domain.as_ref(), c.call_id.as_str()),
                    &c.context,
                );
                validate_call(
                    &mut calls,
                    &mut bytes,
                    c.context.alias_domain.as_ref(),
                    &c.call_id,
                    &c.name,
                    &c.input,
                    CallKind::Custom,
                )?;
            }
            Item::ConfigurationUpdate(update) => update.validate()?,
            Item::ToolChange(change) => {
                if response {
                    return Err(GenerationError::InvalidResponse);
                }
                match &change.action {
                    ToolChangeAction::AddDefinition(tool) => charge(
                        &mut bytes,
                        tools(&[ToolDefinition::Function((**tool).clone())], None)?,
                    )?,
                    ToolChangeAction::AddReference(name) | ToolChangeAction::Remove(name) => {
                        if name.as_str().is_empty() || name.as_str().len() > 128 {
                            return Err(GenerationError::InvalidToolDefinition);
                        }
                        add(&mut bytes, name.as_str())?;
                    }
                }
            }
            Item::Compaction(value) => {
                value.validate_compaction()?;
                if !response && value.replay_token().is_none() {
                    return Err(GenerationError::InvalidReplay);
                }
                add(&mut bytes, value.as_str())?;
            }
            Item::Program(p) => {
                if calls
                    .insert((None, p.call_id.as_str()), CallKind::Program)
                    .is_some()
                {
                    return Err(GenerationError::DuplicateCall);
                }
                if p.call_id.as_str().is_empty() || p.call_id.as_str().len() > 256 {
                    return Err(GenerationError::Limit);
                }
                add(&mut bytes, p.call_id.as_str())?;
                add(&mut bytes, &p.code)?;
                add(&mut bytes, &p.fingerprint)?;
            }
            Item::ProgramOutput(o) => {
                // Request replay is the association boundary: outputs must name a
                // preceding program; response items stay self-describing snapshots.
                if o.status == ItemLifecycle::InProgress
                    || !response
                        && calls.get(&(None, o.call_id.as_str())) != Some(&CallKind::Program)
                    || !results.insert((None, o.call_id.as_str()))
                {
                    return Err(GenerationError::InvalidProgramOutput);
                }
                add(&mut bytes, o.call_id.as_str())?;
                add(&mut bytes, &o.result)?;
            }
            Item::Reasoning(r) => {
                if let Some(value) = &r.replay {
                    value.validate_reasoning()?;
                    add(&mut bytes, value.as_str())?;
                }
                for (id, p) in &r.parts {
                    part_id(&mut parts, *id)?;
                    let (ReasoningContent::Summary(t) | ReasoningContent::Text(t)) = p;
                    add(&mut bytes, t.as_str())?;
                }
            }
            Item::ProviderTool(observed) => {
                provider_observation(
                    observed,
                    &mut bytes,
                    &mut parts,
                    resources,
                    &mut file_decoded_bytes,
                )?;
                match &observed.operation {
                    ProviderOperation::Reported {
                        alias: Some(alias), ..
                    } => {
                        if calls
                            .insert((Some(&observed.source), alias.as_str()), CallKind::Provider)
                            .is_some()
                        {
                            return Err(GenerationError::DuplicateCall);
                        }
                    }
                    ProviderOperation::Reference(reference) if !response => {
                        provider_reference(observed, reference, &items[..position])?;
                    }
                    _ => {}
                }
            }
            Item::ToolResult(r) | Item::CustomResult(r) => {
                let kind = if matches!(item, Item::CustomResult(_)) {
                    CallKind::Custom
                } else {
                    CallKind::Function
                };
                if let Some(call_id) = &r.call_id {
                    let reference = (r.context.alias_domain.as_ref(), call_id.as_str());
                    let preceding = calls.get(&reference);
                    let linked = !response || preceding.is_some();
                    if linked && preceding != Some(&kind)
                        || !results.insert(reference)
                        || linked
                            && contexts.get(&reference).is_none_or(|context| {
                                context.caller != r.context.caller
                                    || r.context.namespace.as_ref().is_some_and(|namespace| {
                                        context.namespace.as_ref() != Some(namespace)
                                    })
                            })
                    {
                        return Err(GenerationError::InvalidToolResult);
                    }
                    if let Some(name) = &r.name
                        && items[..position].iter().any(|(_, item)| match item {
                            Item::ToolCall(c) => {
                                c.context.alias_domain == r.context.alias_domain
                                    && c.call_id == *call_id
                                    && c.name != *name
                            }
                            Item::CustomCall(c) => {
                                c.context.alias_domain == r.context.alias_domain
                                    && c.call_id == *call_id
                                    && c.name != *name
                            }
                            _ => false,
                        })
                    {
                        return Err(GenerationError::InvalidToolResult);
                    }
                } else if !response {
                    return Err(GenerationError::InvalidToolResult);
                }
                tool_result(
                    r,
                    &mut bytes,
                    &mut parts,
                    resources,
                    &mut file_decoded_bytes,
                )?;
            }
        }
    }
    Ok(bytes)
}
pub(super) fn tool_result(
    r: &ToolResult,
    bytes: &mut usize,
    parts: &mut BTreeSet<PartId>,
    resources: &ResourceTable,
    file_decoded_bytes: &mut usize,
) -> Result<(), GenerationError> {
    call_context(&r.context, bytes)?;
    for (value, max) in [(&r.call_id, 256), (&r.name, 128)] {
        if let Some(value) = value {
            if value.as_str().is_empty() || value.as_str().len() > max {
                return Err(GenerationError::InvalidToolResult);
            }
            add(bytes, value.as_str())?;
        }
    }
    match &r.output {
        ToolOutput::Text(t) => add(bytes, t)?,
        ToolOutput::Parts(p) => {
            for part in p {
                if part.replay.is_some() {
                    return Err(GenerationError::InvalidReplay);
                }
                part_id(parts, part.id)?;
                charge(bytes, usize::from(part.cache.is_some()) * 32)?;
                input_part(&part.content, resources, bytes, file_decoded_bytes)?;
            }
        }
        ToolOutput::Structured(value) => charge(bytes, value.bytes()?)?,
    }
    if matches!(
        (&r.execution, r.is_error),
        (Some(ToolExecution::Succeeded), Some(true))
            | (Some(ToolExecution::Failed { .. }), Some(false))
    ) {
        return Err(GenerationError::InvalidToolResult);
    }
    if let Some(execution) = &r.execution {
        if r.status == Some(ItemLifecycle::InProgress) {
            return Err(GenerationError::InvalidToolResult);
        }
        if let ToolExecution::Failed { code: Some(code) } = execution {
            if code.as_str().is_empty() || code.as_str().len() > 128 {
                return Err(GenerationError::Limit);
            }
            add(bytes, code.as_str())?;
        }
    }
    Ok(())
}
pub(super) fn provider_observation(
    observed: &ProviderToolObservation,
    bytes: &mut usize,
    parts: &mut BTreeSet<PartId>,
    resources: &ResourceTable,
    file_decoded_bytes: &mut usize,
) -> Result<(), GenerationError> {
    provider_observation_value(observed, bytes, parts, false, resources, file_decoded_bytes)
}
pub(super) fn provider_result_header(
    observed: &ProviderToolObservation,
    bytes: &mut usize,
    parts: &mut BTreeSet<PartId>,
    resources: &ResourceTable,
) -> Result<(), GenerationError> {
    if observed.output.is_some() {
        return Err(GenerationError::InvalidProviderObservation);
    }
    provider_observation_value(observed, bytes, parts, true, resources, &mut 0)
}
fn provider_observation_value(
    observed: &ProviderToolObservation,
    bytes: &mut usize,
    parts: &mut BTreeSet<PartId>,
    building_result: bool,
    resources: &ResourceTable,
    file_decoded_bytes: &mut usize,
) -> Result<(), GenerationError> {
    if observed.source.source.as_str().is_empty() || observed.source.source.as_str().len() > 256 {
        return Err(GenerationError::InvalidProviderObservation);
    }
    add(bytes, observed.source.source.as_str())?;
    match &observed.operation {
        ProviderOperation::Reported {
            tool,
            alias,
            action,
            ..
        } => {
            if tool.as_str().is_empty() || tool.as_str().len() > 128 {
                return Err(GenerationError::InvalidProviderObservation);
            }
            add(bytes, tool.as_str())?;
            if let Some(alias) = alias {
                if alias.as_str().is_empty() || alias.as_str().len() > 256 {
                    return Err(GenerationError::Limit);
                }
                add(bytes, alias.as_str())?;
            }
            if let Some(action) = action {
                match action {
                    ProviderAction::Search { query, queries } => {
                        if let Some(query) = query {
                            add(bytes, query.as_str())?;
                        }
                        if let Some(queries) = queries {
                            if queries.len() > MAX_ITEMS {
                                return Err(GenerationError::Limit);
                            }
                            charge(
                                bytes,
                                queries.len() * std::mem::size_of::<crate::semantic::value::Text>(),
                            )?;
                            for query in queries {
                                add(bytes, query.as_str())?;
                            }
                        }
                    }
                    ProviderAction::OpenPage { url } => {
                        if let Some(url) = url {
                            add(bytes, url.as_str())?;
                        }
                    }
                    ProviderAction::FindInPage { url, pattern } => {
                        add(bytes, url.as_str())?;
                        add(bytes, pattern.as_str())?;
                    }
                }
            }
        }
        ProviderOperation::Reference(reference) => {
            if let ProviderOperationReference::Native(alias) = reference {
                if alias.as_str().is_empty() || alias.as_str().len() > 256 {
                    return Err(GenerationError::Limit);
                }
                add(bytes, alias.as_str())?;
            }
            if !building_result
                && observed.progress.is_none()
                && observed.execution.is_none()
                && observed.output.is_none()
                && observed.artifact_status.is_none()
            {
                return Err(GenerationError::InvalidProviderObservation);
            }
        }
    }
    if observed.progress.is_some()
        && observed
            .execution
            .as_ref()
            .is_some_and(|execution| !matches!(execution, ToolExecution::Unknown))
    {
        return Err(GenerationError::InvalidProviderObservation);
    }
    if let Some(ToolExecution::Failed { code: Some(code) }) = &observed.execution {
        if code.as_str().is_empty() || code.as_str().len() > 128 {
            return Err(GenerationError::Limit);
        }
        add(bytes, code.as_str())?;
    }
    match &observed.output {
        Some(ToolOutput::Text(value)) => add(bytes, value)?,
        Some(ToolOutput::Structured(value)) => charge(bytes, value.bytes()?)?,
        Some(ToolOutput::Parts(values)) => {
            if values.len() > MAX_ITEMS {
                return Err(GenerationError::Limit);
            }
            for part in values {
                if part.cache.is_some() {
                    return Err(GenerationError::InvalidProviderObservation);
                }
                part_id(parts, part.id)?;
                if part.replay.is_some() {
                    return Err(GenerationError::InvalidReplay);
                }
                match &part.content {
                    InputPart::Text(value) => add(bytes, value.as_str())?,
                    InputPart::Resource(resource) => {
                        let resource = resource.media(resources)?;
                        charge(bytes, resource.validate()?)?;
                        if resource.kind() == ResourceKind::File {
                            *file_decoded_bytes = file_decoded_bytes
                                .checked_add(resource.inline_decoded_bytes()?.unwrap_or(0))
                                .ok_or(GenerationError::Limit)?;
                            if *file_decoded_bytes > MAX_TOTAL_FILE_DECODED_BYTES {
                                return Err(GenerationError::Limit);
                            }
                        }
                    }
                }
            }
        }
        None => {}
    }
    Ok(())
}
fn call_context(context: &CallContext, bytes: &mut usize) -> Result<(), GenerationError> {
    if let Some(domain) = &context.alias_domain {
        if domain.source.as_str().is_empty() || domain.source.as_str().len() > 256 {
            return Err(GenerationError::Limit);
        }
        add(bytes, domain.source.as_str())?;
    }
    if let Some(namespace) = &context.namespace {
        if namespace.as_str().is_empty() || namespace.as_str().len() > 128 {
            return Err(GenerationError::InvalidToolDefinition);
        }
        add(bytes, namespace.as_str())?;
    }
    if let Some(CallOrigin::Program { caller_id }) = &context.caller {
        if caller_id.as_str().is_empty() || caller_id.as_str().len() > 256 {
            return Err(GenerationError::InvalidToolDefinition);
        }
        add(bytes, caller_id.as_str())?;
    }
    Ok(())
}
fn validate_call<'a>(
    calls: &mut BTreeMap<AliasKey<'a>, CallKind>,
    bytes: &mut usize,
    domain: Option<&'a NativeAliasDomain>,
    id: &'a crate::semantic::value::Text,
    name: &crate::semantic::value::Text,
    payload: &str,
    kind: CallKind,
) -> Result<(), GenerationError> {
    if calls.insert((domain, id.as_str()), kind).is_some() {
        return Err(GenerationError::DuplicateCall);
    }
    if id.as_str().is_empty()
        || id.as_str().len() > 256
        || name.as_str().is_empty()
        || name.as_str().len() > 128
    {
        return Err(GenerationError::Limit);
    }
    add(bytes, id.as_str())?;
    add(bytes, name.as_str())?;
    add(bytes, payload)
}
/// A reference names exactly one earlier reported operation from the same source.
fn provider_reference(
    observed: &ProviderToolObservation,
    reference: &ProviderOperationReference,
    earlier: &[(ItemId, Item)],
) -> Result<(), GenerationError> {
    let mut matches = earlier.iter().filter(|(id, item)| match reference {
        ProviderOperationReference::Native(value) => {
            crate::semantic::task::generation::identity::call_alias(item).is_some_and(
                |(domain, _, alias)| domain == Some(&observed.source) && alias == value.as_str(),
            )
        }
        ProviderOperationReference::Local(owner) => id == owner,
    });
    match (matches.next(), matches.next()) {
        (Some((_, Item::ProviderTool(found))), None)
            if found.source == observed.source
                && matches!(found.operation, ProviderOperation::Reported { .. }) =>
        {
            Ok(())
        }
        _ => Err(GenerationError::InvalidProviderObservation),
    }
}
