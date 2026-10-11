//! Target-only projection of native Messages facts onto the OpenAI client carriers.
use crate::{
    protocol::{CodecError, DecodedResponse, openai::Profile},
    semantic::task::generation::*,
};
mod events;
pub(crate) use events::EventProjection;

fn client_usage(usage: Usage) -> Result<Option<Usage>, CodecError> {
    let derived = usage.derive(UsageFormula::InputPlusCacheReadAndWrite)?;
    Ok(
        if let (Some(input), Some(output)) = (derived, usage.output_tokens) {
            let input = input.tokens();
            let total = input.checked_add(output).ok_or(CodecError::Limit)?;
            Some(Usage {
                cached_input_tokens: usage.cached_input_tokens,
                input_cache_write_tokens: usage.input_cache_write_tokens,
                ..Usage::operation(input, output, total)
            })
        } else {
            None
        },
    )
}

/// The caller supplies a private delivery copy; the native observation stays intact.
/// Container boundaries are omitted, object arguments are serialized exactly, and
/// stop progress is represented by OpenAI outcome plus the preserved tool calls.
pub(crate) fn project(decoded: &mut DecodedResponse, profile: Profile) -> Result<(), CodecError> {
    let source = &decoded.semantic;
    match source.progress() {
        InteractionProgress::Unreported | InteractionProgress::TurnFinished => {}
        InteractionProgress::AwaitingToolResults
            if source
                .items()
                .iter()
                .any(|(_, item)| matches!(item, Item::ToolCall(_))) => {}
        _ => return Err(CodecError::Unsupported("native progress projection".into())),
    }
    let mut items = source.items().to_vec();
    for (_, item) in &mut items {
        if let Item::ToolCall(call) = item {
            let ToolArguments::Structured(value) = &call.arguments else {
                return Err(CodecError::Invalid("native tool arguments"));
            };
            call.arguments = ToolArguments::Raw(
                serde_json::to_string(value.value())
                    .map_err(|_| CodecError::Invalid("native tool arguments"))?,
            );
        }
    }
    // Build a target representation rather than editing an observed call under
    // its source dependency receipts. JSON serialization above preserves its value.
    let mut projected =
        GenerationResponse::from_resources(items, source.outcome(), source.resources().clone())?
            .with_details(source.details().clone())?
            .with_usage_reports(source.usage_reports().to_vec())?;
    if let Some(usage) = source.usage() {
        // Derivation requires all disjoint input categories. Missing is not zero.
        let reports = client_usage(usage)?.into_iter().collect();
        projected = projected.with_usage_reports(reports)?;
    }
    if profile == Profile::Chat {
        let status = if source.outcome() == Outcome::Incomplete {
            ItemLifecycle::Incomplete
        } else {
            ItemLifecycle::Completed
        };
        let mut reasoning = vec![];
        let mut parts = vec![];
        let mut calls = vec![];
        let mut owner = None;
        for (id, item) in projected.items() {
            match item {
                Item::Reasoning(_)
                    if parts.is_empty() && calls.is_empty() && reasoning.is_empty() =>
                {
                    reasoning.push((*id, item.clone()))
                }
                Item::Message(message)
                    if calls.is_empty() && message.role == MessageRole::Assistant =>
                {
                    owner.get_or_insert(*id);
                    parts.extend(message.parts.clone());
                }
                Item::ToolCall(call) => {
                    let mut call = call.clone();
                    call.status = status;
                    calls.push((*id, Item::ToolCall(call)));
                }
                _ => return Err(CodecError::Unsupported("native block order on Chat".into())),
            }
        }
        let owner = match owner {
            Some(owner) => owner,
            None => ItemId::scoped(
                projected
                    .items()
                    .first()
                    .map_or(LocalScope::new(3), |(id, _)| id.scope()),
                projected
                    .items()
                    .iter()
                    .map(|(id, _)| id.get())
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or(CodecError::Limit)?,
            ),
        };
        let message = Item::Message(Message {
            role: MessageRole::Assistant,
            parts,
            status,
            phase: None,
        });
        let mut members = vec![owner];
        members.extend(calls.iter().map(|(id, _)| *id));
        reasoning.push((owner, message));
        reasoning.extend(calls);
        projected = GenerationResponse::from_resources(
            reasoning,
            projected.outcome(),
            projected.resources().clone(),
        )?
        .with_details(projected.details().clone())?
        .with_usage_reports(projected.usage_reports().to_vec())?
        .with_message_envelopes(vec![MessageEnvelope::new(
            GroupId::new(owner.scope(), owner.get()),
            MessageEnvelopeRole::Assistant,
            members,
        )?])?;
    }
    decoded.semantic = projected;
    decoded.fidelity.retain_owners(decoded.semantic.items());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{anthropic, replay_envelope};

    fn fixture() -> DecodedResponse {
        let profile = anthropic::Profile::AdaptiveTextTools;
        let target = anthropic::ReplayTarget::new("provider", "model").unwrap();
        let source = profile
            .decode_generation_request(
                br#"{"model":"model","max_tokens":64,"messages":[{"role":"user","content":"go"}]}"#,
                LocalScope::new(1),
                &target,
            )
            .unwrap();
        profile.decode_generation_response(
            br#"{"id":"native","type":"message","role":"assistant","model":"model","content":[{"type":"thinking","thinking":"plan","signature":"synthetic-signature"},{"type":"tool_use","id":"c","name":"lookup","input":{"n":9007199254740993.125}}],"stop_reason":"tool_use","usage":{"input_tokens":11,"cache_read_input_tokens":3,"cache_creation_input_tokens":2,"output_tokens":7}}"#,
            LocalScope::new(3), &source, &target,
        ).unwrap()
    }
    #[test]
    fn projection_keeps_native_observation_and_derives_only_the_client_view() {
        let source = fixture();
        let before = source.clone();
        for profile in [Profile::Responses, Profile::Chat] {
            let mut client = source.clone();
            crate::execution::replay::seal_for_client(
                &mut client,
                &crate::execution::replay::CLIENT_ORIGIN,
            )
            .unwrap();
            project(&mut client, profile).unwrap();
            client.metadata.created = Some(1.into());
            let adapter =
                crate::adapter::Adapter::new(profile, crate::adapter::Dialect::MorphieCore, None);
            let contract = crate::lowering::generation::GenerationRepresentationContract {
                replay_origin: Some(crate::execution::replay::CLIENT_ORIGIN.clone()),
                ..crate::lowering::generation::GenerationRepresentationContract::full()
            };
            let wire = adapter.encode_response(&client, &contract).unwrap();
            let args = if profile == Profile::Responses {
                &wire["output"][1]["arguments"]
            } else {
                &wire["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"]
            };
            assert_eq!(args, "{\"n\":9007199254740993.125}");
            assert_eq!(wire["usage"]["total_tokens"], 23);
            assert_eq!(client.semantic.progress(), InteractionProgress::Unreported);
            assert_eq!(source, before);
            assert_eq!(source.metadata.created, None);
            assert_eq!(source.semantic.usage().unwrap().total_tokens, None);
            let Item::Reasoning(r) = &client.semantic.items()[0].1 else {
                panic!()
            };
            assert_eq!(
                replay_envelope::open(r.replay.as_ref().unwrap().as_str())
                    .unwrap()
                    .2
                    .as_str(),
                "synthetic-signature"
            );
        }
    }
    #[test]
    fn missing_cache_report_is_unknown_and_overflow_is_not_a_fallback() {
        let mut decoded = fixture();
        let mut usage = decoded.semantic.usage().unwrap();
        usage.cached_input_tokens = None;
        decoded.semantic = decoded.semantic.with_usage(usage).unwrap();
        project(&mut decoded, Profile::Responses).unwrap();
        assert_eq!(decoded.semantic.usage(), None);
        let mut decoded = fixture();
        let mut usage = decoded.semantic.usage().unwrap();
        usage.input_tokens = Some(u64::MAX);
        decoded.semantic = decoded.semantic.with_usage(usage).unwrap();
        assert!(project(&mut decoded, Profile::Responses).is_err());
    }
}
