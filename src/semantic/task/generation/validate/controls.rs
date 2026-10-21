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
            ToolDefinition::Custom(t) => {
                charge(&mut bytes, t.dispatch.validate()?)?;
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
