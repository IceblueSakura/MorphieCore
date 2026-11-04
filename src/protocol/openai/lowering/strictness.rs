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
    if !request.function_tools().any(is_conditional) {
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
        let exact_strict = !force_non_strict
            && strict_schema::admits_conditional(schema)
            && strict_carrier(schema);
        tool.strict = FunctionStrictness::Explicit(exact_strict);
        *loss |= !exact_strict;
    }
    let resolve_request = |force_non_strict| -> Result<_, RepresentationError> {
        let mut settings = request.settings().clone();
        let mut loss = false;
        let mut items = request.items().to_vec();
        for_each_function_tool_mut(&mut settings, &mut items, |_, tool| {
            resolve(tool, &strict_carrier, force_non_strict, &mut loss);
        });
        Ok((
            request.clone().with_items_and_settings(items, settings)?,
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
