//! Native-only controlled matrix. Bodies/signatures stay in-process; shared ledger owns quota.
use morphiecore::{
    adapter::Request,
    credential::{CredentialManager, PoolMember},
    execution::{Attempt, AttemptError, admit, prepare},
    protocol::{
        DecodedRequest, DecodedResponse,
        anthropic::{Profile, ReplayTarget},
    },
    provider::SecretMaterial,
    semantic::{
        context::ConversationContext,
        task::generation::*,
        value::{Presence, Text},
    },
    topology::{Endpoint, PublicModel, catalog},
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
const TIMEOUT: Duration = Duration::from_secs(120);
const BYTES: usize = 2 << 20;
const MODEL: &str = "claude-haiku-5-5";
const GROUPS: [(&str, bool, usize); 8] = [
    ("text-json", false, 1),
    ("text-sse", true, 1),
    ("tool-json", false, 2),
    ("tool-sse", true, 2),
    ("error", false, 3),
    ("signature-json", false, 2),
    ("signature-sse", true, 2),
    ("length", false, 1),
];
struct Call<'a> {
    endpoint: &'a Endpoint,
    public: &'a PublicModel,
    secret: &'a SecretMaterial,
    client: &'a reqwest::Client,
    group: &'a str,
    ordinal: u64,
}
struct Observed {
    decoded: DecodedResponse,
    bytes: usize,
    status: u16,
}
fn text(response: &GenerationResponse) -> String {
    response
        .items()
        .iter()
        .filter_map(|(_, i)| {
            if let Item::Message(m) = i {
                Some(m)
            } else {
                None
            }
        })
        .flat_map(|m| &m.parts)
        .filter_map(|p| {
            if let ContentPart::Text(t) = &p.content {
                Some(t.as_str())
            } else {
                None
            }
        })
        .collect()
}
fn signatures(response: &GenerationResponse) -> usize {
    response.items().iter().filter(|(_,i)|matches!(i,Item::Reasoning(r) if r.replay.as_ref().and_then(ReplayValue::replay_token).is_some())).count()
}
fn tool(response: &GenerationResponse, key: &str) -> Option<Text> {
    let calls = response
        .items()
        .iter()
        .filter_map(|(_, i)| {
            if let Item::ToolCall(c) = i {
                Some(c)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if calls.len() != 1 || calls[0].name.as_str() != "lookup" {
        return None;
    }
    let ToolArguments::Structured(args) = &calls[0].arguments else {
        return None;
    };
    if args.value().get("key").and_then(Value::as_str) != Some(key)
        || args.value().as_object()?.len() != 1
    {
        return None;
    }
    Some(calls[0].call_id.clone())
}
async fn exchange(
    ctx: &Call<'_>,
    source: &DecodedRequest,
    stream: bool,
    attempt_id: &str,
    metrics: &mut Value,
) -> Result<Observed, &'static str> {
    let mut request = Request::from_generation(source.clone(), MODEL).map_err(|_| "setup")?;
    request.delivery.stream = Presence::Value(stream);
    request.conversation = Some(ConversationContext::conversation(ctx.group).map_err(|_| "setup")?);
    admit(ctx.public, &request).map_err(|_| "setup")?;
    let provider = catalog::MESSAGES_BINDINGS[0].provider();
    let prepared = prepare(ctx.endpoint, &provider, ctx.secret, &request).map_err(|_| "setup")?;
    let target = ReplayTarget::new(provider.id.as_str(), &ctx.endpoint.upstream_model)
        .map_err(|_| "setup")?;
    let mut attempt = Attempt::native_messages(
        source.clone(),
        target,
        LocalScope::new(ctx.ordinal + 2),
        BYTES,
    )
    .map_err(|_| "setup")?;
    let mut call = ctx
        .client
        .post(format!("{}{}", prepared.origin, prepared.path));
    for (name, value) in prepared.safe_headers {
        call = call.header(name, value);
    }
    let mut auth =
        reqwest::header::HeaderValue::from_str(&prepared.auth_header.1).map_err(|_| "setup")?;
    auth.set_sensitive(true);
    call = call
        .header(&prepared.auth_header.0, auth)
        .body(prepared.body);
    super::probe_control::call("dispatched", json!({"attempt":attempt_id})).await;
    let mut response = call.send().await.map_err(|_| "transport")?;
    metrics["http"] = json!(response.status().as_u16());
    if response.status().as_u16() != 200 {
        return Err("http");
    }
    let content_types = response
        .headers()
        .get_all(reqwest::header::CONTENT_TYPE)
        .iter()
        .collect::<Vec<_>>();
    if content_types.len() != 1 {
        return Err("wire");
    }
    let content_type = content_types[0].to_str().map_err(|_| "wire")?.to_owned();
    if stream
        != content_type
            .split(';')
            .next()
            .is_some_and(|s| s.trim().eq_ignore_ascii_case("text/event-stream"))
    {
        return Err("wire");
    }
    attempt.begin(200, &content_type).map_err(|_| "wire")?;
    let mut bytes = 0usize;
    while let Some(chunk) = response.chunk().await.map_err(|_| "transport")? {
        bytes = bytes
            .checked_add(chunk.len())
            .filter(|n| *n <= BYTES)
            .ok_or("budget")?;
        let mut offset = 0;
        while offset < chunk.len() {
            let (used, _) = attempt.push(&chunk[offset..]).map_err(|_| "wire")?;
            if used == 0 {
                return Err("wire");
            }
            offset += used;
        }
    }
    let decoded = attempt
        .finish()
        .map_err(|error| {
            metrics["decode_failure"] = json!(match error {
                AttemptError::Codec(morphiecore::protocol::CodecError::Invalid(
                    "native model binding",
                )) => "metadata_model_changed",
                AttemptError::Codec(morphiecore::protocol::CodecError::Unsupported(category)) =>
                    match category.as_str() {
                        "Anthropic cache_creation" => "native_cache_creation",
                        "Anthropic output_tokens_details" => "native_output_tokens_details",
                        "Anthropic service_tier" => "native_service_tier",
                        "Anthropic inference_geo" => "native_inference_geo",
                        "Anthropic server_tool_use" => "native_server_tool_use",
                        "Anthropic container" => "native_container",
                        "Anthropic stop_details" => "native_stop_details",
                        "Anthropic input_transformations" => "native_input_transformations",
                        _ => "unsupported",
                    },
                _ => "other",
            });
            "wire"
        })?
        .clone();
    Ok(Observed {
        decoded,
        bytes,
        status: 200,
    })
}
fn initial(group: &str, cap: u64) -> Result<DecodedRequest, &'static str> {
    let tool_case = group.starts_with("tool") || group == "error";
    let prompt = if tool_case {
        "Use lookup to read synthetic configuration key alpha. If it fails, call lookup for beta once. Then reply with exactly the numeric result."
    } else if group == "length" {
        "Write the synthetic code token alpha 200 times, without summarizing."
    } else {
        "In synthetic code, x = 41. Reply with exactly the integer x, without words or punctuation."
    };
    let mut wire =
        json!({"model":MODEL,"max_tokens":cap,"messages":[{"role":"user","content":prompt}]});
    if tool_case {
        wire["tools"] = json!([{"name":"lookup","input_schema":{"type":"object","properties":{"key":{"type":"string","enum":["alpha","beta"]}},"required":["key"],"additionalProperties":false}}]);
        wire["tool_choice"] = json!({"type":"auto"});
    }
    Profile::AdaptiveTextTools
        .decode_generation_request(
            &wire.to_string().into_bytes(),
            LocalScope::new(1),
            &ReplayTarget::new("opencode-go", MODEL).map_err(|_| "setup")?,
        )
        .map_err(|_| "setup")
}
fn successor(
    source: &DecodedRequest,
    response: &DecodedResponse,
    group: &str,
    round: usize,
) -> Result<DecodedRequest, &'static str> {
    let id = ItemId::scoped(LocalScope::new(10_000 + round as u64), 1);
    let error = group == "error" && round == 1;
    let item = if group.starts_with("tool") || group == "error" {
        Item::ToolResult(ToolResult {
            call_id: tool(
                &response.semantic,
                if round == 1 { "alpha" } else { "beta" },
            )
            .ok_or("oracle")?,
            output: ToolOutput::Text(
                if error {
                    "Synthetic lookup refusal for alpha. Try beta."
                } else {
                    "42"
                }
                .into(),
            ),
            is_error: Some(error),
            execution: Some(if error {
                ToolExecution::NotExecuted
            } else {
                ToolExecution::Succeeded
            }),
            status: None,
            context: Default::default(),
        })
    } else {
        Item::Message(Message {
            role: MessageRole::User,
            parts: vec![Part {
                id: PartId::scoped(id.scope(), 1),
                content: ContentPart::Text(
                    Text::new(
                        "Add 1 to the previous synthetic x. Reply with exactly the integer.",
                        "text",
                        256,
                    )
                    .map_err(|_| "setup")?
                    .into(),
                ),
                replay: None,
            }],
            status: ItemLifecycle::Completed,
            phase: None,
        })
    };
    let semantic = ClientManaged::new(source.semantic.settings().clone())
        .map_err(|_| "setup")?
        .select_request(&source.semantic)
        .map_err(|_| "setup")?
        .select_response(&response.semantic)
        .map_err(|_| "setup")?
        .append_items(vec![(id, item)])
        .map_err(|_| "setup")?
        .build(&[])
        .map_err(|_| "setup")?;
    let mut groups = semantic.message_envelopes().to_vec();
    groups.push(
        MessageEnvelope::new(
            GroupId::new(id.scope(), 1),
            MessageEnvelopeRole::User,
            vec![id],
        )
        .map_err(|_| "setup")?,
    );
    Ok(DecodedRequest {
        semantic: semantic
            .with_message_envelopes(groups)
            .map_err(|_| "setup")?,
        fidelity: response.fidelity.clone(),
    })
}
pub async fn run() -> Result<(), &'static str> {
    if std::env::var("MORPHIECORE_PROBE_MODEL").as_deref() != Ok(MODEL) {
        return Err("explicit native model required");
    }
    let cap = std::env::var("MORPHIECORE_PROBE_MAX_TOKENS")
        .ok()
        .and_then(|n| n.parse::<u64>().ok())
        .filter(|n| (1..=2048).contains(n))
        .ok_or("explicit bounded cap required")?;
    super::probe_control::call("check", json!({"model":MODEL})).await;
    let filter = std::env::var("MORPHIECORE_PROBE_CASE").ok();
    if filter
        .as_ref()
        .is_some_and(|s| !GROUPS.iter().any(|(name, _, _)| name == s))
    {
        return Err("native case selection");
    }
    let cases = GROUPS
        .iter()
        .filter(|(name, _, _)| filter.as_ref().is_none_or(|s| s == name))
        .flat_map(|(name, _, rounds)| {
            (1..=*rounds).map(move |r| {
                json!([
                    MODEL,
                    format!("native:{MODEL}:{name}:{r}"),
                    if *name == "length" { 8 } else { cap }
                ])
            })
        })
        .collect::<Vec<_>>();
    super::probe_control::call("register", json!({"cases":cases})).await;
    if std::env::var("MORPHIECORE_PROBE_DRY_RUN").as_deref() == Ok("1") {
        println!("{}", json!({"requests":cases.len(),"tokens":cap}));
        return Ok(());
    }
    let directory = std::env::var("MORPHIECORE_PROBE_CREDENTIALS_DIR")
        .map_err(|_| "explicit store required")?;
    let manager = CredentialManager::new(directory, vec![]).map_err(|_| "credential setup")?;
    let (_, _, mut status) = manager
        .pools()
        .map_err(|_| "credential setup")?
        .into_iter()
        .find(|(p, b, _)| p == "opencode-go" && b == "opencode-go-api-key")
        .ok_or("credential setup")?;
    status.config.max_attempts = 1;
    status.config.fallback = false;
    let pool = manager
        .bind_pool("opencode-go", &status.config)
        .map_err(|_| "credential setup")?;
    let Some(PoolMember::ApiKey(key)) = pool.members.first() else {
        return Err("credential kind");
    };
    let binding = &catalog::MESSAGES_BINDINGS[0];
    let topology = binding.topology().map_err(|_| "binding")?;
    let endpoint = binding.endpoint();
    let public = topology.model(MODEL).ok_or("binding")?;
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(Duration::from_secs(30))
        .timeout(TIMEOUT);
    for variable in [
        "https_proxy",
        "HTTPS_PROXY",
        "http_proxy",
        "HTTP_PROXY",
        "all_proxy",
        "ALL_PROXY",
    ] {
        if let Ok(value) = std::env::var(variable) {
            builder = builder.proxy(reqwest::Proxy::all(&value).map_err(|_| "proxy")?);
            break;
        }
    }
    let client = builder.build().map_err(|_| "transport setup")?;
    let start = Instant::now();
    let mut ordinal = 0u64;
    let mut failed = false;
    for (group, first_stream, rounds) in GROUPS {
        if filter.as_ref().is_some_and(|s| s != group) {
            continue;
        }
        let cap = if group == "length" { 8 } else { cap };
        let mut source = initial(group, cap)?;
        let conversation = format!(
            "{}:{group}",
            std::env::var("MORPHIECORE_PROBE_RUN")
                .map_err(|_| "run")?
                .rsplit('/')
                .next()
                .ok_or("run")?
        );
        for round in 1..=rounds {
            if start.elapsed() > Duration::from_secs(1800) {
                return Err("run deadline");
            }
            let scenario = format!("native:{MODEL}:{group}:{round}");
            let id = super::probe_control::call(
                "reserve",
                json!({"model":MODEL,"scenario":scenario,"tokens":cap}),
            )
            .await;
            ordinal += 1;
            let before = Instant::now();
            let mut metrics = json!({"history_ok":true});
            let secret = match key.borrow().and_then(|material| {
                SecretMaterial::new(material.expose())
                    .map_err(|_| morphiecore::credential::CredentialError::KeyUnavailable)
            }) {
                Ok(secret) => secret,
                Err(_) => {
                    super::probe_control::call(
                        "finish",
                        json!({"attempt":id,"state":"failed",
                        "metrics":{"failure":"setup","gateway_error":"credential_unavailable"}}),
                    )
                    .await;
                    return Err("credential unavailable");
                }
            };
            let ctx = Call {
                endpoint: &endpoint,
                public,
                secret: &secret,
                client: &client,
                group: &conversation,
                ordinal,
            };
            let stream = if round == 1 {
                first_stream
            } else {
                !first_stream
            };
            let observed =
                tokio::time::timeout(TIMEOUT, exchange(&ctx, &source, stream, &id, &mut metrics))
                    .await
                    .unwrap_or(Err("transport"));
            metrics["elapsed_ms"] =
                json!(before.elapsed().as_millis().min(u64::MAX as u128) as u64);
            let state = match &observed {
                Ok(result) => {
                    let r = &result.decoded.semantic;
                    let wants_call = (group.starts_with("tool") || group == "error")
                        && (round == 1 || group == "error" && round == 2);
                    let ok = if group == "length" {
                        r.outcome() == Outcome::Incomplete
                    } else if wants_call {
                        r.progress() == InteractionProgress::AwaitingToolResults
                            && tool(r, if round == 1 { "alpha" } else { "beta" }).is_some()
                    } else {
                        r.outcome() == Outcome::Completed
                            && text(r).trim() == if round == 1 { "41" } else { "42" }
                            && (!group.starts_with("signature") || signatures(r) > 0)
                    };
                    metrics["http"] = json!(result.status);
                    metrics["wire_bytes"] = json!(result.bytes);
                    metrics["wire_closed"] = json!(true);
                    metrics["native_consumed"] = json!(true);
                    metrics["content_ok"] = json!(ok);
                    metrics["reported_opaque_items"] = json!(signatures(r));
                    metrics["text_chars"] = json!(text(r).chars().count());
                    metrics["reported_output_tokens"] =
                        json!(r.usage().and_then(|u| u.output_tokens));
                    metrics["reported_cached_tokens"] =
                        json!(r.usage().and_then(|u| u.cached_input_tokens));
                    if ok {
                        "passed"
                    } else {
                        metrics["failure"] = json!("oracle");
                        metrics["oracle_failure"] = json!("other");
                        "oracle_failed"
                    }
                }
                Err(failure) => {
                    metrics["failure"] = json!(failure);
                    metrics["native_consumed"] = json!(false);
                    "failed"
                }
            };
            super::probe_control::call(
                "finish",
                json!({"attempt":id,"state":state,"metrics":metrics}),
            )
            .await;
            println!("{}", json!({"scenario":scenario,"state":state}));
            if state != "passed" {
                failed = true;
                if state == "failed" {
                    return Err("native transport/protocol failure");
                }
                break;
            }
            if round < rounds {
                source = successor(
                    &source,
                    &observed.map_err(|_| "wire")?.decoded,
                    group,
                    round,
                )?;
            }
        }
    }
    if failed {
        Err("native oracle failure")
    } else {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_typed_history_and_error_feedback_keep_signature_and_execution_fact() {
        let p = Profile::AdaptiveTextTools;
        let target = ReplayTarget::new("opencode-go", MODEL).unwrap();
        let source = initial("error", 32).unwrap();
        let response=p.decode_generation_response(br#"{"id":"msg","type":"message","role":"assistant","model":"claude-haiku-5-5","content":[{"type":"thinking","thinking":"think","signature":"synthetic-sig"},{"type":"tool_use","id":"toolu_actual","name":"lookup","input":{"key":"alpha"}}],"stop_reason":"tool_use","usage":{"input_tokens":11,"output_tokens":12}}"#,
            LocalScope::new(2),&source,&target).unwrap();
        let next = successor(&source, &response, "error", 1).unwrap();
        assert!(matches!(
            next.semantic.items().last().unwrap().1,
            Item::ToolResult(ToolResult {
                is_error: Some(true),
                execution: Some(ToolExecution::NotExecuted),
                ..
            })
        ));
        let wire = p
            .encode_generation_request(&next.semantic, &next.fidelity, &target)
            .unwrap();
        assert_eq!(wire.omitted_execution.len(), 1);
        assert_eq!(
            wire.value["messages"][1]["content"][0]["signature"],
            "synthetic-sig"
        );
        assert_eq!(
            wire.value["messages"][2]["content"][0]["tool_use_id"],
            "toolu_actual"
        );
        assert_eq!(wire.value["messages"][2]["content"][0]["is_error"], true);
        assert_eq!(
            GROUPS.iter().map(|(_, _, rounds)| rounds).sum::<usize>(),
            14
        );
        assert!(text(&response.semantic).is_empty());
        assert_eq!(signatures(&response.semantic), 1);
    }
}
