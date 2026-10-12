//! Real-Provider gateway checks, ignored by default: `cargo test --test live -- --ignored`.
//! The key comes from the default credential store through the project loader; each
//! check runs serially with a fixed request count, output cap and timeout and no retries.
//! Failures print only status and error codes, never bodies or opaque values.
use morphiecore::{
    credential::{CredentialManager, PoolMember, builtin_drivers, resolve_store_directory},
    gateway::{Entry, Gateway, Limits},
    protocol::openai::Profile,
    provider::{CredentialBindingId, SecretMaterial},
    topology::catalog::{self, GOOGLE_BINDINGS},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::net::TcpListener;

const CLIENT_KEY: &str = "live-check-loopback-client-key-0001";
const PROMPT: &str = "Use the read tool to read notes.txt, then reply with the exact secret word it contains and nothing else.";
const FILE: &str = "The secret word is: marigold-42";
const SECRET_WORD: &str = "marigold-42";

/// Borrows the pool's first key through the loader; the value never leaves this process.
fn stored_key(provider: &str, binding: &str) -> SecretMaterial {
    let directory = resolve_store_directory(None).expect("credential store");
    let manager =
        CredentialManager::new(&directory, builtin_drivers(None).expect("drivers")).expect("store");
    let (_, _, status) = manager
        .pools()
        .expect("pools")
        .into_iter()
        .find(|(p, b, _)| p == provider && b == binding)
        .unwrap_or_else(|| panic!("no {provider}/{binding} pool"));
    let access = manager.bind_pool(provider, &status.config).expect("pool");
    let Some(PoolMember::ApiKey(key)) = access.members.first() else {
        panic!("{binding} needs an API key member")
    };
    SecretMaterial::new(key.borrow().expect("enabled key").expose()).expect("key material")
}

struct Live {
    base: String,
    client: reqwest::Client,
    model: &'static str,
    gateway: Gateway,
    diagnostics: tempfile::TempDir,
    server: tokio::task::JoinHandle<()>,
}
impl Drop for Live {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl Live {
    async fn google() -> Self {
        let binding = &GOOGLE_BINDINGS[0];
        let endpoint = binding.endpoint().id;
        let gateway = Gateway::new(
            catalog::topology(&Default::default()).expect("catalog"),
            [Profile::Responses, Profile::Chat]
                .map(|protocol| Entry {
                    model: binding.model.into(),
                    protocol,
                    endpoint: endpoint.clone(),
                })
                .into(),
            BTreeMap::from([(
                CredentialBindingId::new(binding.credential).unwrap(),
                Arc::new(stored_key("google", binding.credential)),
            )]),
            SecretMaterial::new(CLIENT_KEY).unwrap(),
            Limits::default(),
            None,
        )
        .expect("gateway");
        let diagnostics = tempfile::tempdir().unwrap();
        let gateway = gateway
            .with_diagnostics(&diagnostics.path().join("diagnostics.jsonl"))
            .expect("diagnostics");
        let owner = gateway.clone();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            gateway
                .serve(listener, std::future::pending())
                .await
                .unwrap()
        });
        Self {
            base,
            client: reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(90))
                .build()
                .unwrap(),
            model: binding.model,
            gateway: owner,
            diagnostics,
            server,
        }
    }
    /// Body-free failure facts from the gateway's own diagnostics record.
    async fn last_diagnostic(&self) -> String {
        self.gateway.flush_diagnostics().await;
        let records =
            std::fs::read_to_string(self.diagnostics.path().join("diagnostics.jsonl")).unwrap();
        let last: Value = serde_json::from_str(records.lines().last().unwrap_or("{}")).unwrap();
        let candidate = &last["candidates"][0];
        format!(
            "stage={} outcome={} upstream={} decode={} projection={} candidate_error={} rules={}",
            last["stage"],
            last["outcome"],
            last["upstream_status"],
            last["decode_failure"],
            last["projection_failure"],
            candidate["error_code"],
            candidate["request_rules"]
        )
    }
    /// One request; SSE bodies are returned as their parsed `data` frames.
    async fn post(&self, path: &str, body: &Value, stream: bool) -> Result<Value, String> {
        let response = self
            .client
            .post(format!("{}{path}", self.base))
            .bearer_auth(CLIENT_KEY)
            .header("x-morphiecore-conversation-id", "live-check")
            .json(body)
            .send()
            .await
            .map_err(|e| format!("transport: {}", e.is_timeout()))?;
        let status = response.status().as_u16();
        let text = match response.text().await {
            Ok(text) => text,
            Err(_) => return Err(format!("aborted body; {}", self.last_diagnostic().await)),
        };
        if status != 200 {
            let error: Value = serde_json::from_str(&text).unwrap_or_default();
            return Err(format!(
                "HTTP {status} type={} code={}; {}",
                error["error"]["type"],
                error["error"]["code"],
                self.last_diagnostic().await
            ));
        }
        Ok(if stream {
            Value::Array(
                text.lines()
                    .filter_map(|line| line.strip_prefix("data: "))
                    .filter(|data| *data != "[DONE]")
                    .map(|data| serde_json::from_str(data).expect("SSE JSON"))
                    .collect(),
            )
        } else {
            serde_json::from_str(&text).expect("JSON body")
        })
    }
}

fn tool() -> Value {
    json!({"name":"read","description":"Read a file and return its contents.",
        "parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}})
}

/// Turn one must call the tool with a signed reasoning carrier; turn two replays both
/// and must answer from the tool result.
async fn responses(live: &Live, stream: bool) -> Result<(), String> {
    let mut input = vec![json!({"role":"user","content":PROMPT})];
    for turn in 0..2 {
        let mut tool = tool();
        tool["type"] = json!("function");
        let body = json!({"model":live.model,"input":input,"tools":[tool],"store":false,
            "include":["reasoning.encrypted_content"],"reasoning":{"effort":"low","summary":"auto"},
            "max_output_tokens":2048,"stream":stream});
        let out = live.post("/v1/responses", &body, stream).await?;
        let response = if stream {
            out.as_array()
                .unwrap()
                .iter()
                .rev()
                .find(|e| e["type"] == "response.completed")
                .ok_or("no response.completed")?["response"]
                .clone()
        } else {
            out
        };
        let output = response["output"].as_array().ok_or("output")?.clone();
        let calls: Vec<_> = output
            .iter()
            .filter(|o| o["type"] == "function_call")
            .collect();
        if turn == 0 {
            if calls.is_empty()
                || !output
                    .iter()
                    .any(|o| o["type"] == "reasoning" && o["encrypted_content"].is_string())
            {
                return Err("turn 0: no signed tool call".into());
            }
            let ids: Vec<_> = calls.iter().map(|c| c["call_id"].clone()).collect();
            input.extend(output);
            for id in ids {
                input.push(json!({"type":"function_call_output","call_id":id,"output":FILE}));
            }
        } else if !calls.is_empty() || !response.to_string().contains(SECRET_WORD) {
            return Err("turn 1: no answer".into());
        }
    }
    Ok(())
}

fn fold_chat(chunks: &Value) -> Value {
    let mut message =
        json!({"role":"assistant","content":"","reasoning_details":[],"tool_calls":[]});
    for chunk in chunks.as_array().unwrap() {
        let Some(delta) = chunk["choices"][0].get("delta") else {
            continue;
        };
        if let Some(text) = delta["content"].as_str() {
            let joined = format!("{}{text}", message["content"].as_str().unwrap());
            message["content"] = json!(joined);
        }
        if let Some(details) = delta["reasoning_details"].as_array() {
            message["reasoning_details"]
                .as_array_mut()
                .unwrap()
                .extend(details.iter().cloned());
        }
        for call in delta["tool_calls"].as_array().into_iter().flatten() {
            let calls = message["tool_calls"].as_array_mut().unwrap();
            let index = call["index"].as_u64().unwrap() as usize;
            if calls.len() <= index {
                calls.push(json!({"id":call["id"],"type":"function",
                    "function":{"name":call["function"]["name"],"arguments":""}}));
            }
            let arguments = format!(
                "{}{}",
                calls[index]["function"]["arguments"].as_str().unwrap(),
                call["function"]["arguments"].as_str().unwrap_or("")
            );
            calls[index]["function"]["arguments"] = json!(arguments);
        }
    }
    message
}

async fn chat(live: &Live, stream: bool) -> Result<(), String> {
    let mut messages = vec![json!({"role":"user","content":PROMPT})];
    for turn in 0..2 {
        let body = json!({"model":live.model,"messages":messages,
            "tools":[{"type":"function","function":tool()}],
            "max_completion_tokens":2048,"reasoning_effort":"low","stream":stream});
        let out = live.post("/v1/chat/completions", &body, stream).await?;
        let message = if stream {
            fold_chat(&out)
        } else {
            out["choices"][0]["message"].clone()
        };
        let calls = message["tool_calls"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if turn == 0 {
            let signed = message["reasoning_details"]
                .as_array()
                .is_some_and(|d| d.iter().any(|d| d["type"] == "reasoning.encrypted"));
            if calls.is_empty() || !signed {
                return Err("turn 0: no signed tool call".into());
            }
            messages.push(message);
            for call in calls {
                messages.push(json!({"role":"tool","tool_call_id":call["id"],"content":FILE}));
            }
        } else if !calls.is_empty() || !message["content"].to_string().contains(SECRET_WORD) {
            return Err("turn 1: no answer".into());
        }
    }
    Ok(())
}

fn fold_messages(events: &Value) -> Value {
    let mut content: Vec<Value> = vec![];
    let mut json_input = BTreeMap::<usize, String>::new();
    for event in events.as_array().unwrap() {
        let index = event["index"].as_u64().unwrap_or(0) as usize;
        match event["type"].as_str().unwrap_or("") {
            "content_block_start" => content.push(event["content_block"].clone()),
            "content_block_delta" => {
                let delta = &event["delta"];
                let block = &mut content[index];
                let append = |block: &mut Value, key: &str, value: &Value| {
                    let joined = format!(
                        "{}{}",
                        block[key].as_str().unwrap_or(""),
                        value.as_str().unwrap()
                    );
                    block[key] = json!(joined);
                };
                match delta["type"].as_str().unwrap() {
                    "text_delta" => append(block, "text", &delta["text"]),
                    "thinking_delta" => append(block, "thinking", &delta["thinking"]),
                    "signature_delta" => append(block, "signature", &delta["signature"]),
                    "input_json_delta" => json_input
                        .entry(index)
                        .or_default()
                        .push_str(delta["partial_json"].as_str().unwrap()),
                    _ => {}
                }
            }
            "content_block_stop" => {
                if let Some(input) = json_input.remove(&index) {
                    content[index]["input"] = serde_json::from_str(&input).unwrap();
                }
            }
            _ => {}
        }
    }
    json!({"content": content})
}

async fn messages(live: &Live, stream: bool) -> Result<(), String> {
    let mut history = vec![json!({"role":"user","content":PROMPT})];
    let tool = tool();
    let tool = json!({"name":tool["name"],"description":tool["description"],"input_schema":tool["parameters"]});
    for turn in 0..2 {
        let body = json!({"model":live.model,"max_tokens":2048,"messages":history,
            "tools":[tool],"stream":stream});
        let out = live.post("/v1/messages", &body, stream).await?;
        let message = if stream { fold_messages(&out) } else { out };
        let content = message["content"].as_array().ok_or("content")?.clone();
        let calls: Vec<_> = content.iter().filter(|b| b["type"] == "tool_use").collect();
        if turn == 0 {
            let signed = content.iter().any(|b| {
                b["type"] == "thinking" && b["signature"].as_str().is_some_and(|s| !s.is_empty())
            });
            if calls.is_empty() || !signed {
                return Err("turn 0: no signed tool call".into());
            }
            let results: Vec<_> = calls
                .iter()
                .map(|c| json!({"type":"tool_result","tool_use_id":c["id"],"content":FILE}))
                .collect();
            history.push(json!({"role":"assistant","content":content}));
            history.push(json!({"role":"user","content":results}));
        } else if !calls.is_empty() || !message.to_string().contains(SECRET_WORD) {
            return Err("turn 1: no answer".into());
        }
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "real Provider calls; run explicitly"]
async fn gemini_tool_loops_replay_signatures_for_every_client_protocol() {
    let live = Live::google().await;
    let mut failures = vec![];
    for stream in [false, true] {
        for (name, result) in [
            ("responses", responses(&live, stream).await),
            ("chat", chat(&live, stream).await),
            ("messages", messages(&live, stream).await),
        ] {
            println!("{name} stream={stream}: {result:?}");
            if let Err(error) = result {
                failures.push(format!("{name} stream={stream}: {error}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
