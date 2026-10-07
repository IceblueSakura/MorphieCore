//! Pure validation and bounded accounting shared by construction, transforms and lowering.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
pub const MAX_ITEMS: usize = 1024;
pub const MAX_TEXT_BYTES: usize = 1 << 20;
pub const MAX_TOTAL_BYTES: usize = 4 << 20;
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
pub fn items(items: &[(ItemId, Item)], response: bool) -> Result<usize, GenerationError> {
    if items.is_empty() {
        return Err(GenerationError::EmptyInput);
    }
    if items.len() > MAX_ITEMS {
        return Err(GenerationError::Limit);
    }
    let mut ids = BTreeSet::new();
    let mut parts = BTreeSet::new();
    let mut calls = BTreeMap::new();
    let mut namespaces = BTreeMap::new();
    let mut results = BTreeSet::new();
    let mut bytes = super::configuration::validate_bindings(items)?;
    let mut file_decoded_bytes = 0usize;
    for (position, (id, item)) in items.iter().enumerate() {
        if !ids.insert(*id) {
            return Err(GenerationError::DuplicateItemId);
        }
        match item {
            Item::Instruction(i) => {
                if response {
                    return Err(GenerationError::InvalidResponse);
                }
                for (id, t) in &i.parts {
                    part_id(&mut parts, *id)?;
                    add(&mut bytes, t.as_str())?;
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
                            || value.format() != ReplayFormat::GoogleGenerateContentPart
                            || !matches!(
                                &p.content,
                                ContentPart::Text(_)
                                    | ContentPart::Resource(Resource {
                                        description: ResourceDescription::Image { .. },
                                        ..
                                    })
                            )
                        {
                            return Err(GenerationError::InvalidReplay);
                        }
                        value.validate()?;
                        add(&mut bytes, value.as_str())?;
                    }
                    match &p.content {
                        ContentPart::Text(t) => {
                            t.validate()?;
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
                            // Assistant images are observations, not public carrier admission.
                            if m.role == MessageRole::Assistant
                                && resource.kind() != ResourceKind::Image
                            {
                                return Err(GenerationError::InvalidResource);
                            }
                            charge(&mut bytes, resource.validate()?)?;
                            if resource.kind() == ResourceKind::File {
                                file_decoded_bytes = file_decoded_bytes
                                    .checked_add(resource.inline_decoded_bytes()?.unwrap_or(0))
                                    .ok_or(GenerationError::Limit)?;
                                if file_decoded_bytes > MAX_TOTAL_FILE_DECODED_BYTES {
                                    return Err(GenerationError::Limit);
                                }
                            }
                        }
                    }
                }
            }
            Item::ToolCall(c) => {
                call_context(&c.context, &mut bytes)?;
                if let Some(value) = &c.context.replay {
                    if !matches!(
                        value.format(),
                        ReplayFormat::GoogleGenerateContentPart
                            | ReplayFormat::GoogleInteractionsV1Step
                    ) {
                        return Err(GenerationError::InvalidReplay);
                    }
                    value.validate()?;
                    add(&mut bytes, value.as_str())?;
                }
                namespaces.insert(
                    (c.context.alias_domain.as_ref(), c.call_id.as_str()),
                    c.context.namespace.as_ref(),
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
                if c.context.replay.is_some() {
                    return Err(GenerationError::InvalidReplay);
                }
                call_context(&c.context, &mut bytes)?;
                namespaces.insert(
                    (c.context.alias_domain.as_ref(), c.call_id.as_str()),
                    c.context.namespace.as_ref(),
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
            Item::ConfigurationUpdate(_) => {}
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
                provider_observation(observed, &mut bytes, &mut parts)?;
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
                    ProviderOperation::Reference(_) if !response => {
                        observed
                            .resolve(&items[..position])
                            .map_err(|_| GenerationError::InvalidProviderObservation)?;
                    }
                    _ => {}
                }
            }
            Item::ToolResult(r) | Item::CustomResult(r) => {
                if r.context.replay.is_some() {
                    return Err(GenerationError::InvalidReplay);
                }
                call_context(&r.context, &mut bytes)?;
                let reference = (r.context.alias_domain.as_ref(), r.call_id.as_str());
                if r.context.namespace.as_ref().is_some_and(|namespace| {
                    namespaces.get(&reference).copied().flatten() != Some(namespace)
                }) {
                    return Err(GenerationError::InvalidToolResult);
                }
                if response {
                    return Err(GenerationError::InvalidResponse);
                }
                let kind = if matches!(item, Item::CustomResult(_)) {
                    CallKind::Custom
                } else {
                    CallKind::Function
                };
                if calls.get(&reference) != Some(&kind) || !results.insert(reference) {
                    return Err(GenerationError::InvalidToolResult);
                }
                add(&mut bytes, r.call_id.as_str())?;
                match &r.output {
                    ToolOutput::Text(t) => add(&mut bytes, t)?,
                    ToolOutput::Parts(p) => {
                        for (id, value) in p {
                            part_id(&mut parts, *id)?;
                            match value {
                                ToolResultPart::Text(text) => add(&mut bytes, text.as_str())?,
                                ToolResultPart::Resource(resource) => {
                                    // Selected tool media is inert URL/inline image content.
                                    // Opaque references need an issuer/lifecycle contract first.
                                    if resource.kind() != ResourceKind::Image
                                        || matches!(
                                            resource.location,
                                            ResourceLocation::OpaqueReference(_)
                                        )
                                    {
                                        return Err(GenerationError::InvalidResource);
                                    }
                                    charge(&mut bytes, resource.validate()?)?;
                                }
                            }
                        }
                    }
                    ToolOutput::Structured(value) => charge(&mut bytes, value.bytes()?)?,
                }
                if let Some(execution) = &r.execution {
                    if r.status == Some(ItemLifecycle::InProgress) {
                        return Err(GenerationError::InvalidToolResult);
                    }
                    if let ToolExecution::Failed { code: Some(code) } = execution {
                        if code.as_str().is_empty() || code.as_str().len() > 128 {
                            return Err(GenerationError::Limit);
                        }
                        add(&mut bytes, code.as_str())?;
                    }
                }
            }
        }
    }
    Ok(bytes)
}
pub(super) fn provider_observation(
    observed: &ProviderToolObservation,
    bytes: &mut usize,
    parts: &mut BTreeSet<PartId>,
) -> Result<(), GenerationError> {
    provider_observation_value(observed, bytes, parts, false)
}
pub(super) fn provider_result_header(
    observed: &ProviderToolObservation,
    bytes: &mut usize,
    parts: &mut BTreeSet<PartId>,
) -> Result<(), GenerationError> {
    if observed.output.is_some() {
        return Err(GenerationError::InvalidProviderObservation);
    }
    provider_observation_value(observed, bytes, parts, true)
}
fn provider_observation_value(
    observed: &ProviderToolObservation,
    bytes: &mut usize,
    parts: &mut BTreeSet<PartId>,
    building_result: bool,
) -> Result<(), GenerationError> {
    if observed.source.source.as_str().is_empty() || observed.source.source.as_str().len() > 256 {
        return Err(GenerationError::InvalidProviderObservation);
    }
    add(bytes, observed.source.source.as_str())?;
    if let Some(value) = &observed.replay {
        if value.format() != ReplayFormat::GoogleInteractionsV1Step {
            return Err(GenerationError::InvalidReplay);
        }
        value.validate()?;
        add(bytes, value.as_str())?;
    }
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
            for (id, value) in values {
                part_id(parts, *id)?;
                match value {
                    ToolResultPart::Text(value) => add(bytes, value.as_str())?,
                    ToolResultPart::Resource(resource) => charge(bytes, resource.validate()?)?,
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
pub fn output(value: &OutputConstraint) -> Result<usize, GenerationError> {
    match value {
        OutputConstraint::Text | OutputConstraint::JsonObject => Ok(0),
        OutputConstraint::JsonSchema {
            name,
            description,
            schema: s,
            strict,
        } => {
            if name.as_str().is_empty()
                || name.as_str().len() > 64
                || !name
                    .as_str()
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                return Err(GenerationError::InvalidControl);
            }
            let mode = if *strict == Some(true) {
                super::schema::Mode::Explicit
            } else {
                super::schema::Mode::General
            };
            let mut bytes = super::schema::validate(s, mode)?;
            add(&mut bytes, name.as_str())?;
            if let Some(d) = description {
                add(&mut bytes, d.as_str())?;
            }
            Ok(bytes)
        }
    }
}
pub fn tools(
    tools: &[ToolDefinition],
    choice: Option<&ToolChoice>,
) -> Result<usize, GenerationError> {
    fn schema_bytes(total: &mut usize, bytes: usize) -> Result<(), GenerationError> {
        *total = total.checked_add(bytes).ok_or(GenerationError::Limit)?;
        if *total > MAX_TOTAL_BYTES {
            return Err(GenerationError::Limit);
        }
        Ok(())
    }
    if tools.len() > MAX_TOOLS {
        return Err(GenerationError::Limit);
    }
    let mut available = BTreeSet::new();
    let mut groups = BTreeSet::new();
    let mut count = tools.len();
    let mut bytes = 0;
    for t in tools {
        let n = t.name().as_str();
        if n.is_empty() || n.len() > 128 {
            return Err(GenerationError::InvalidToolDefinition);
        }
        add(&mut bytes, n)?;
        if let ToolDefinition::Namespace(group) = t {
            if !groups.insert(n)
                || group.tools.is_empty()
                || group.tools.iter().any(|t| t.kind().is_none())
            {
                return Err(GenerationError::InvalidToolDefinition);
            }
            count = count
                .checked_add(group.tools.len())
                .ok_or(GenerationError::Limit)?;
            if count > MAX_TOOLS {
                return Err(GenerationError::Limit);
            }
            charge(&mut bytes, self::tools(&group.tools, None)?)?;
            add(&mut bytes, &group.description)?;
            for leaf in &group.tools {
                available.insert((Some(n), leaf.kind().expect("leaf"), leaf.name().as_str()));
            }
            continue;
        }
        if !available.insert((None, t.kind().expect("leaf"), n)) {
            return Err(GenerationError::InvalidToolDefinition);
        }
        match t {
            ToolDefinition::Function(t) => {
                if let Some(d) = &t.description {
                    add(&mut bytes, d)?;
                }
                if let Some(s) = &t.parameters {
                    let mode = match t.strict {
                        FunctionStrictness::Explicit(true) => super::schema::Mode::Explicit,
                        FunctionStrictness::Omitted(StrictDefault::NormalizeSchema) => {
                            super::schema::Mode::Normalize
                        }
                        _ => super::schema::Mode::General,
                    };
                    schema_bytes(&mut bytes, super::schema::validate(s, mode)?)?;
                }
                if let Some(s) = &t.output_schema {
                    schema_bytes(
                        &mut bytes,
                        super::schema::validate(s, super::schema::Mode::General)?,
                    )?;
                }
            }
            ToolDefinition::Custom(t) => {
                if let Some(d) = &t.description {
                    add(&mut bytes, d)?;
                }
                if let Some(CustomFormat::Grammar { definition, .. }) = &t.format {
                    add(&mut bytes, definition.as_str())?;
                }
            }
            ToolDefinition::Namespace(_) => unreachable!("handled above"),
        }
    }
    match choice {
        Some(ToolChoice::Specific(n))
            if !available.contains(&(None, ToolKind::Function, n.as_str())) =>
        {
            return Err(GenerationError::InvalidToolChoice);
        }
        Some(ToolChoice::Custom(n))
            if !available.contains(&(None, ToolKind::Custom, n.as_str())) =>
        {
            return Err(GenerationError::InvalidToolChoice);
        }
        Some(ToolChoice::Qualified(r))
            if r.namespace.is_none()
                || !available.contains(&(
                    r.namespace.as_ref().map(|n| n.as_str()),
                    r.kind,
                    r.name.as_str(),
                )) =>
        {
            return Err(GenerationError::InvalidToolChoice);
        }
        Some(ToolChoice::Required) if tools.is_empty() => {
            return Err(GenerationError::InvalidToolChoice);
        }
        Some(ToolChoice::Allowed { tools, .. }) => {
            if tools.is_empty() || tools.len() > MAX_TOOLS {
                return Err(GenerationError::InvalidToolChoice);
            }
            let mut seen = BTreeSet::new();
            for r in tools {
                let reference = (
                    r.namespace.as_ref().map(|n| n.as_str()),
                    r.kind,
                    r.name.as_str(),
                );
                if !available.contains(&reference) || !seen.insert(reference) {
                    return Err(GenerationError::InvalidToolChoice);
                }
            }
        }
        _ => {}
    }
    Ok(bytes)
}
