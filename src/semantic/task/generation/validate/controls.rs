//! Tool definitions/selection and output constraints, separate from ordered history checks.
use super::*;

pub(in crate::semantic::task::generation) fn output(
    value: &OutputConstraint,
) -> Result<usize, GenerationError> {
    match value {
        OutputConstraint::Text | OutputConstraint::JsonObject => Ok(0),
        OutputConstraint::JsonSchema {
            name,
            description,
            schema,
            ..
        } => {
            if name.as_ref().is_some_and(|name| {
                name.as_str().is_empty()
                    || name.as_str().len() > 64
                    || !name
                        .as_str()
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            }) {
                return Err(GenerationError::InvalidControl);
            }
            let mut bytes = schema.validate()?;
            if let Some(name) = name {
                add(&mut bytes, name.as_str())?;
            }
            if let Some(d) = description {
                add(&mut bytes, d.as_str())?;
            }
            Ok(bytes)
        }
    }
}
pub(in crate::semantic::task::generation) fn tools(
    tools: &[ToolDefinition],
) -> Result<usize, GenerationError> {
    declarations(tools.iter().map(ToolDeclaration::from))
}
pub(in crate::semantic::task::generation) fn declarations<'a>(
    tools: impl ExactSizeIterator<Item = ToolDeclaration<'a>>,
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
    let mut names = BTreeSet::new();
    let mut groups = BTreeSet::new();
    let mut count = tools.len();
    let mut bytes = tools.len() * std::mem::size_of::<ToolDefinition>();
    for t in tools {
        let (n, kind) = match t {
            ToolDeclaration::Function(tool) => (tool.name.as_str(), Some(ToolKind::Function)),
            ToolDeclaration::Custom(tool) => (tool.name.as_str(), Some(ToolKind::Custom)),
            ToolDeclaration::Namespace(tool) => (tool.name.as_str(), None),
        };
        if n.is_empty() || n.len() > 128 {
            return Err(GenerationError::InvalidToolDefinition);
        }
        add(&mut bytes, n)?;
        if let ToolDeclaration::Namespace(group) = t {
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
            charge(&mut bytes, self::tools(&group.tools)?)?;
            add(&mut bytes, &group.description)?;
            continue;
        }
        if !names.insert((kind.expect("leaf"), n)) {
            return Err(GenerationError::InvalidToolDefinition);
        }
        match t {
            ToolDeclaration::Function(t) => {
                charge(&mut bytes, t.dispatch.validate()?)?;
                charge(
                    &mut bytes,
                    usize::from(t.cache.is_some()) * std::mem::size_of::<PrefixCacheTtl>(),
                )?;
                if let Some(d) = &t.description {
                    add(&mut bytes, d)?;
                }
                for schema in t.parameters.iter().chain(&t.output_schema) {
                    schema_bytes(&mut bytes, schema.validate()?)?;
                }
                if let Some(examples) = &t.input_examples {
                    if examples.len() > MAX_ITEMS {
                        return Err(GenerationError::Limit);
                    }
                    for example in examples {
                        if !example.value().is_object() {
                            return Err(GenerationError::InvalidToolDefinition);
                        }
                        charge(&mut bytes, example.bytes()?)?;
                    }
                }
            }
            ToolDeclaration::Custom(t) => {
                charge(&mut bytes, t.dispatch.validate()?)?;
                if let Some(d) = &t.description {
                    add(&mut bytes, d)?;
                }
                if let Some(CustomFormat::Grammar { definition, .. }) = &t.format {
                    add(&mut bytes, definition.as_str())?;
                }
            }
            ToolDeclaration::Namespace(_) => unreachable!("handled above"),
        }
    }
    Ok(bytes)
}

type ToolKey<'a> = (Option<&'a str>, ToolKind, &'a str);

/// Resolve present selection independently of historical calls and their schemas.
pub(in crate::semantic::task::generation) fn tool_choice(
    root: &[ToolDefinition],
    items: &[(ItemId, Item)],
    choice: Option<&ToolChoice>,
) -> Result<usize, GenerationError> {
    let mut known = BTreeMap::new();
    let mut active = 0usize;
    for definition in root {
        let namespace = match definition {
            ToolDefinition::Namespace(group) => Some(group.name.as_str()),
            _ => None,
        };
        for leaf in definition.leaves() {
            let offered = match leaf {
                ToolDefinition::Function(tool) => !tool.dispatch.defer_loading,
                ToolDefinition::Custom(tool) => !tool.dispatch.defer_loading,
                ToolDefinition::Namespace(_) => unreachable!("validated leaf"),
            };
            known.insert(
                (namespace, leaf.kind().expect("leaf"), leaf.name().as_str()),
                offered,
            );
            active += usize::from(offered);
        }
    }
    for (_, item) in items {
        if let Item::ToolBatch(batch) = item {
            for definition in &batch.tools {
                let namespace = match definition {
                    ToolDefinition::Namespace(group) => Some(group.name.as_str()),
                    _ => None,
                };
                for leaf in definition.leaves() {
                    let offered = match leaf {
                        ToolDefinition::Function(tool) => !tool.dispatch.defer_loading,
                        ToolDefinition::Custom(tool) => !tool.dispatch.defer_loading,
                        ToolDefinition::Namespace(_) => unreachable!("validated leaf"),
                    };
                    let key = (namespace, leaf.kind().expect("leaf"), leaf.name().as_str());
                    // A batch does not select a same-name schema winner. Membership
                    // admits a possible native choice; collision-dependent lowering
                    // must qualify precedence before projecting this history.
                    let previous = known.entry(key).or_insert(false);
                    if offered && !*previous {
                        active += 1;
                        *previous = true;
                    }
                }
            }
            if active > MAX_TOOLS {
                return Err(GenerationError::Limit);
            }
            continue;
        }
        let Item::ToolChange(change) = item else {
            continue;
        };
        // Reference/removal currently name unqualified function definitions.
        let key = (None, ToolKind::Function, change.name().as_str());
        let (previous, next) = match &change.action {
            ToolChangeAction::AddDefinition(tool) => {
                let next = !tool.dispatch.defer_loading;
                (known.insert(key, next).unwrap_or(false), next)
            }
            ToolChangeAction::AddReference(_) | ToolChangeAction::Remove(_) => {
                let offered = known
                    .get_mut(&key)
                    .ok_or(GenerationError::InvalidToolDefinition)?;
                let next = matches!(change.action, ToolChangeAction::AddReference(_));
                (std::mem::replace(offered, next), next)
            }
        };
        active = active - usize::from(previous) + usize::from(next);
        if active > MAX_TOOLS {
            return Err(GenerationError::Limit);
        }
    }
    let available = known
        .into_iter()
        .filter_map(|(key, offered)| offered.then_some(key))
        .collect();
    validate_choice(&available, choice)?;
    Ok(available.len())
}

fn validate_choice(
    available: &BTreeSet<ToolKey<'_>>,
    choice: Option<&ToolChoice>,
) -> Result<(), GenerationError> {
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
        Some(ToolChoice::Required) if available.is_empty() => {
            return Err(GenerationError::InvalidToolChoice);
        }
        Some(ToolChoice::Allowed { tools, .. }) => {
            for r in tools {
                let reference = (
                    r.namespace.as_ref().map(|n| n.as_str()),
                    r.kind,
                    r.name.as_str(),
                );
                if !available.contains(&reference) {
                    return Err(GenerationError::InvalidToolChoice);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

pub(in crate::semantic::task::generation) fn tool_choice_shape(
    choice: Option<&ToolChoice>,
) -> Result<usize, GenerationError> {
    let mut bytes = 0;
    let mut reference = |r: &ToolReference| -> Result<(), GenerationError> {
        if !nonempty_within(r.name.as_str(), MAX_NAME_BYTES) {
            return Err(GenerationError::InvalidToolChoice);
        }
        add(&mut bytes, r.name.as_str())?;
        if let Some(namespace) = &r.namespace {
            if !nonempty_within(namespace.as_str(), MAX_NAME_BYTES) {
                return Err(GenerationError::InvalidToolChoice);
            }
            add(&mut bytes, namespace.as_str())?;
        }
        Ok(())
    };
    match choice {
        Some(ToolChoice::Specific(name) | ToolChoice::Custom(name)) => {
            if !nonempty_within(name.as_str(), MAX_NAME_BYTES) {
                return Err(GenerationError::InvalidToolChoice);
            }
            add(&mut bytes, name.as_str())?;
        }
        Some(ToolChoice::Qualified(r)) => {
            if r.namespace.is_none() {
                return Err(GenerationError::InvalidToolChoice);
            }
            reference(r)?;
        }
        Some(ToolChoice::Allowed { tools, .. }) => {
            if tools.is_empty() || tools.len() > MAX_TOOLS {
                return Err(GenerationError::InvalidToolChoice);
            }
            let mut seen = BTreeSet::new();
            for r in tools {
                reference(r)?;
                if !seen.insert((r.namespace.as_ref(), r.kind, &r.name)) {
                    return Err(GenerationError::InvalidToolChoice);
                }
            }
        }
        _ => {}
    }
    Ok(bytes)
}
