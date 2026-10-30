//! Resolve conditional strictness on a private request without rewriting supplied schemas.
use super::RepresentationError;
use crate::{
    protocol::{
        conversion::generation::{Change, ConversionBoundary, Effect, Field},
        openai::strict_schema,
    },
    semantic::task::generation::*,
};

pub(crate) fn project(
    request: &GenerationRequest,
    strict_carrier: impl Fn(&SchemaDocument) -> bool,
    fits_request: impl Fn(&GenerationRequest) -> bool,
) -> Result<Option<(GenerationRequest, Vec<Effect>)>, RepresentationError> {
    let functions = || {
        request
            .tools()
            .iter()
            .flat_map(ToolDefinition::leaves)
            .filter_map(|tool| match tool {
                ToolDefinition::Function(tool) => Some(tool),
                _ => None,
            })
            .chain(request.items().iter().filter_map(|(_, item)| match item {
                Item::ToolChange(ToolChange {
                    action: ToolChangeAction::AddDefinition(tool),
                    ..
                }) => Some(tool.as_ref()),
                _ => None,
            }))
    };
    if !functions().any(is_conditional) {
        return Ok(None);
    }
    fn resolve(
        tool: &mut FunctionTool,
        strict_carrier: &impl Fn(&SchemaDocument) -> bool,
        force_non_strict: bool,
        loss: &mut bool,
    ) {
        // An omitted parameter list declares a parameterless function. Materialize
        // its equivalent empty-object carrier only on the candidate copy.
        if !is_conditional(tool) {
            return;
        }
        let schema = tool.parameters.get_or_insert_with(empty_parameters);
        let exact_strict =
            !force_non_strict && strict_schema::admits(schema) && strict_carrier(schema);
        tool.strict = FunctionStrictness::Explicit(exact_strict);
        *loss |= !exact_strict;
    }
    fn resolve_tools(
        tools: &mut [ToolDefinition],
        strict_carrier: &impl Fn(&SchemaDocument) -> bool,
        force_non_strict: bool,
        loss: &mut bool,
    ) {
        for tool in tools {
            match tool {
                ToolDefinition::Function(tool) => {
                    resolve(tool, strict_carrier, force_non_strict, loss);
                }
                ToolDefinition::Namespace(group) => {
                    resolve_tools(&mut group.tools, strict_carrier, force_non_strict, loss);
                }
                _ => {}
            }
        }
    }
    let resolve_request = |force_non_strict| -> Result<_, RepresentationError> {
        let mut settings = request.settings().clone();
        let mut loss = false;
        resolve_tools(
            settings.tools.as_deref_mut().unwrap_or_default(),
            &strict_carrier,
            force_non_strict,
            &mut loss,
        );
        let mut items = request.items().to_vec();
        for (_, item) in &mut items {
            if let Item::ToolChange(ToolChange {
                action: ToolChangeAction::AddDefinition(tool),
                ..
            }) = item
            {
                resolve(tool, &strict_carrier, force_non_strict, &mut loss);
            }
        }
        Ok((
            request.clone().with_settings(settings)?.with_items(items)?,
            loss,
        ))
    };
    let (mut projected, mut loss) = resolve_request(false)?;
    if !fits_request(&projected) {
        // Start from source intent again: every conditional definition relaxes,
        // including inline additions. Explicit guarantees remain unchanged.
        (projected, loss) = resolve_request(true)?;
    }
    let effects = if loss {
        vec![Effect::new(
            "openai.omit-conditional-tool-strictness.v1",
            Field::Tools,
            ConversionBoundary::RequestProject,
            Change::Omit,
        )]
    } else {
        vec![]
    };
    Ok(Some((projected, effects)))
}

fn is_conditional(tool: &FunctionTool) -> bool {
    tool.strict == FunctionStrictness::Omitted(StrictDefault::Conditional)
}

pub(crate) fn empty_parameters() -> SchemaDocument {
    // Omitting parameters declares an empty parameter list, not an arbitrary object.
    serde_json::json!({"type":"object","properties":{},"additionalProperties":false}).into()
}
