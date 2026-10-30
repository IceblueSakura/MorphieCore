//! Resolve the conditional source default without rewriting parameter schemas.
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
) -> Result<Option<(GenerationRequest, Vec<Effect>)>, RepresentationError> {
    if !GenerationRequirements::derive(request).conditional_tool_defaults {
        return Ok(None);
    }
    fn resolve(
        tool: &mut FunctionTool,
        strict_carrier: &impl Fn(&SchemaDocument) -> bool,
        loss: &mut bool,
    ) -> Result<(), RepresentationError> {
        if tool.strict != FunctionStrictness::Omitted(StrictDefault::Conditional) {
            return Ok(());
        }
        let schema = tool
            .parameters
            .as_ref()
            .ok_or(RepresentationError::StrictDefault)?;
        let compatible =
            strict_schema::conditional(schema).ok_or(RepresentationError::StrictDefault)?;
        let exact_strict =
            compatible && strict_schema::admits(schema, true) && strict_carrier(schema);
        tool.strict = FunctionStrictness::Explicit(exact_strict);
        *loss |= compatible && !exact_strict;
        Ok(())
    }
    fn resolve_tools(
        tools: &mut [ToolDefinition],
        strict_carrier: &impl Fn(&SchemaDocument) -> bool,
        loss: &mut bool,
    ) -> Result<(), RepresentationError> {
        for tool in tools {
            match tool {
                ToolDefinition::Function(tool) => resolve(tool, strict_carrier, loss)?,
                ToolDefinition::Namespace(group) => {
                    resolve_tools(&mut group.tools, strict_carrier, loss)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    let mut settings = request.settings().clone();
    let mut loss = false;
    resolve_tools(
        settings.tools.as_deref_mut().unwrap_or_default(),
        &strict_carrier,
        &mut loss,
    )?;
    let mut items = request.items().to_vec();
    for (_, item) in &mut items {
        if let Item::ToolChange(ToolChange {
            action: ToolChangeAction::AddDefinition(tool),
            ..
        }) = item
        {
            resolve(tool, &strict_carrier, &mut loss)?;
        }
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
    Ok(Some((
        request.clone().with_settings(settings)?.with_items(items)?,
        effects,
    )))
}
