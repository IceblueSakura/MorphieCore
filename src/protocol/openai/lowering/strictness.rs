//! Resolve the conditional source default without rewriting parameter schemas.
use super::RepresentationError;
use crate::{
    protocol::{
        conversion::generation::{Change, ConversionBoundary, Effect, Field},
        openai::strict_schema,
    },
    semantic::task::generation::*,
};

pub(in crate::protocol::openai) fn project(
    request: &GenerationRequest,
    strict_carrier: bool,
) -> Result<Option<(GenerationRequest, Vec<Effect>)>, RepresentationError> {
    if !request
        .tools()
        .iter()
        .flat_map(ToolDefinition::leaves)
        .any(|tool| {
            matches!(tool, ToolDefinition::Function(tool)
            if tool.strict == FunctionStrictness::Omitted(StrictDefault::Conditional))
        })
    {
        return Ok(None);
    }
    fn resolve(
        tools: &mut [ToolDefinition],
        strict_carrier: bool,
        changed: &mut bool,
        loss: &mut bool,
    ) -> Result<(), RepresentationError> {
        for tool in tools {
            match tool {
                ToolDefinition::Function(tool)
                    if tool.strict == FunctionStrictness::Omitted(StrictDefault::Conditional) =>
                {
                    let schema = tool
                        .parameters
                        .as_ref()
                        .ok_or(RepresentationError::StrictDefault)?;
                    let compatible = strict_schema::conditional(schema)
                        .ok_or(RepresentationError::StrictDefault)?;
                    let exact_strict =
                        compatible && strict_carrier && strict_schema::admits(schema, true);
                    tool.strict = FunctionStrictness::Explicit(exact_strict);
                    *changed = true;
                    *loss |= compatible && !exact_strict;
                }
                ToolDefinition::Namespace(group) => {
                    resolve(&mut group.tools, strict_carrier, changed, loss)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    let mut settings = request.settings().clone();
    let mut changed = false;
    let mut loss = false;
    resolve(
        settings.tools.as_deref_mut().unwrap_or_default(),
        strict_carrier,
        &mut changed,
        &mut loss,
    )?;
    if !changed {
        return Ok(None);
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
    Ok(Some((request.clone().with_settings(settings)?, effects)))
}
