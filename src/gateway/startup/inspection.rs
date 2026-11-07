//! Credential-free catalog inspection and redacted effective binding descriptions.
use super::error::StartupError;
use crate::{
    lowering::target::TargetContract,
    topology::{Endpoint, catalog},
};
use serde_json::{Value, json};

pub(super) fn endpoint(endpoint: &Endpoint) -> Value {
    let limits = endpoint.representation.model_limits();
    let default = match &endpoint.representation {
        TargetContract::AnthropicMessages(caps) => caps.default_output_tokens,
        _ => None,
    };
    json!({
        "provider":endpoint.provider.as_str(),
        "protocol":format!("{:?}", endpoint.protocol),
        "planning":{"context_window_tokens":limits.context_window_tokens,
                    "maximum_input_tokens":limits.maximum_input_tokens},
        "maximum_output_tokens":limits.maximum_output_tokens,
        "default_output_tokens":default,
        "allowed_effort":limits.allowed_effort.as_ref().map(|levels| levels.iter()
            .map(|level| <&'static str>::from(*level)).collect::<Vec<_>>()),
        "endpoint_timeout_ms":endpoint.execution.timeout_ms,
        "request_bytes":endpoint.execution.request_body_limit,
        "response_bytes":endpoint.execution.response_body_limit,
    })
}

pub(super) fn task(
    model: &str,
    provider: &str,
    binding: &str,
    policy: &crate::topology::ExecutionContract,
    task: &str,
) -> Value {
    json!({"model":model,"provider":provider,"binding":binding,"task":task,
        "selection":if task == "transcription" {"account; binding required when ambiguous"} else {"account"},
        "endpoint_timeout_ms":policy.timeout_ms,
        "request_bytes":policy.request_body_limit,"response_bytes":policy.response_body_limit})
}

/// Reads only compiled declarations, without home-directory lookup or credential access.
pub fn catalog() -> Result<Value, StartupError> {
    let topology = catalog::default_topology().map_err(|_| StartupError::Binding)?;
    let mut models = Vec::new();
    for model in topology.models() {
        let candidates: Vec<_> = topology
            .route_endpoints(&model.route)
            .iter()
            .map(|target| {
                let mut row = endpoint(target);
                row["binding"] = json!(target.id.as_str());
                row
            })
            .collect();
        models.push(json!({"model":model.id.as_str(),"task":"generation","candidates":candidates}));
    }
    for binding in catalog::IMAGE_BINDINGS {
        let route = binding.route();
        models.push(task(
            binding.model,
            route.endpoint.provider.as_str(),
            route.endpoint.id.as_str(),
            &route.endpoint.execution,
            "images",
        ));
    }
    for binding in catalog::SPEECH_BINDINGS {
        let route = binding.route();
        models.push(task(
            binding.model,
            route.endpoint.provider.as_str(),
            route.endpoint.id.as_str(),
            &route.endpoint.execution,
            "speech",
        ));
    }
    for binding in catalog::EMBEDDING_BINDINGS {
        let route = binding.route();
        models.push(task(
            binding.model,
            route.endpoint.provider.as_str(),
            route.endpoint.id.as_str(),
            &route.endpoint.execution,
            "embeddings",
        ));
    }
    for binding in catalog::TRANSCRIPTION_BINDINGS {
        let route = binding.route();
        models.push(task(
            binding.model,
            route.endpoint.provider.as_str(),
            route.endpoint.id.as_str(),
            &route.endpoint.execution,
            "transcription",
        ));
    }
    Ok(json!({"models":models,
        "semantics":"Context/input are planning metadata. Output maximum clamps a present ceiling; only Messages defaults fill absence. Endpoint ceilings intersect with deployment budgets."}))
}
