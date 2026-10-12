//! Stateless Google Interactions v1 request projection.
//! Source: <https://ai.google.dev/api/interactions-api-v1>.
mod request;
mod signature;
pub use request::RequestRule;

use crate::{
    protocol::{
        CodecError,
        fidelity::FidelityRecords,
        native::{ReplayTarget, group_client_history},
    },
    semantic::{task::generation::GenerationRequest, value::json_size},
};
use serde_json::Value;

/// Full history as steps, always streamed and never stored by the upstream.
pub fn encode_generation_request(
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
) -> Result<(Value, Vec<RequestRule>), CodecError> {
    let request = group_client_history(request)?;
    request.validate()?;
    fidelity.require_reasoning_replay(&request, &target.origin())?;
    let mut rules = Default::default();
    let value = request::project(&request, fidelity, target, &mut rules)?;
    json_size(&value, crate::semantic::task::generation::MAX_TOTAL_BYTES)
        .map_err(|_| CodecError::Limit)?;
    Ok((value, rules.into_iter().collect()))
}
