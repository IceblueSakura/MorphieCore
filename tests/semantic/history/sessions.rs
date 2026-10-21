//! Persistent, synthetic multi-turn native wires with independent semantic/history oracles.
#[path = "sessions/facts.rs"]
mod facts;

use morphiecore::{
    adapter::{AdapterError, ClientAdapter, Request, UpstreamAdapter},
    execution::{Attempt, ResponseDelivery, replay::open_from_client},
    protocol::{
        anthropic::messages,
        google::{generate_content, interactions},
        native::ReplayTarget,
        openai::{Adapter, Dialect, Profile, TargetCaps, obfuscation::Obfuscation},
    },
    semantic::task::generation::*,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Component, PathBuf};

const MODEL: &str = "session-model";
const LIMIT: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug)]
enum Api {
    Chat,
    Responses,
    Messages,
    GenerateContent,
    Interactions,
}
impl Api {
    fn name(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Responses => "responses",
            Self::Messages => "messages",
            Self::GenerateContent => "generate-content",
            Self::Interactions => "interactions",
        }
    }
    fn target(self) -> ReplayTarget {
        ReplayTarget::new("synthetic-session", MODEL).unwrap()
    }
    fn openai(self) -> Adapter {
        Adapter::new(
            if matches!(self, Self::Chat) {
                Profile::Chat
            } else {
                Profile::Responses
            },
            Dialect::Standard,
            Some(
                morphiecore::semantic::value::ReplayOrigin::new("synthetic-session-openai")
                    .unwrap(),
            ),
        )
    }
    fn upstream(self) -> UpstreamAdapter {
        match self {
            Self::Chat | Self::Responses => self.openai().into(),
            Self::Messages => UpstreamAdapter::AnthropicMessages {
                profile: messages::Profile::AdaptiveTextTools,
                target: self.target(),
            },
            Self::GenerateContent => UpstreamAdapter::GoogleGenerateContent {
                target: self.target(),
            },
            Self::Interactions => UpstreamAdapter::GoogleInteractions {
                target: self.target(),
            },
        }
    }
    fn client(self) -> ClientAdapter {
        if matches!(self, Self::Messages) {
            return ClientAdapter::Messages;
        }
        ClientAdapter::openai(
            Adapter::new(
                if matches!(self, Self::Chat) {
                    Profile::Chat
                } else {
                    Profile::Responses
                },
                Dialect::MorphieCore,
                None,
            ),
            TargetCaps::full(),
        )
    }
    fn client_history(self) -> &'static str {
        if matches!(self, Self::Chat | Self::Messages) {
            "messages"
        } else {
            "input"
        }
    }
    fn native_history(self) -> &'static str {
        match self {
            Self::Chat | Self::Messages => "messages",
            Self::GenerateContent => "contents",
            _ => "input",
        }
    }
    fn encode(self, request: &Request) -> Result<(Value, Vec<String>), AdapterError> {
        Ok(match self {
            Self::Chat | Self::Responses => {
                let mut caps = TargetCaps::full();
                caps.replay_origin = self.openai().adaptation.scope.clone();
                let (v, r) = self
                    .openai()
                    .encode_request_with_rules(request, MODEL, &caps)?;
                (v, r.into_iter().map(str::to_owned).collect())
            }
            Self::Messages => {
                let p = messages::TargetCaps::new(
                    messages::Profile::AdaptiveTextTools,
                    messages::Dialect::Standard,
                )
                .encode_request(request, &self.target())?;
                (
                    p.value,
                    p.rules.iter().map(|r| r.name().to_owned()).collect(),
                )
            }
            Self::GenerateContent => {
                let (v, r) = generate_content::encode_generation_request(
                    &request.task.semantic,
                    &request.task.fidelity,
                    &self.target(),
                    None,
                )?;
                (v, r.iter().map(|r| r.name().to_owned()).collect())
            }
            Self::Interactions => {
                let (v, r) = interactions::encode_generation_request(
                    &request.task.semantic,
                    &request.task.fidelity,
                    &self.target(),
                )?;
                (v, r.iter().map(|r| r.name().to_owned()).collect())
            }
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Corpus {
    name: String,
    client_request: Value,
    upstream_request: Value,
    turns: Vec<Turn>,
    incomplete: Source,
    #[serde(default)]
    sparse: Option<Source>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Turn {
    name: String,
    source: Source,
    client_append: Vec<Value>,
    upstream_append: Vec<Value>,
    #[serde(default)]
    client_update: Option<Value>,
    #[serde(default)]
    upstream_update: Option<Value>,
    request_rules: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    sse: String,
    json: Option<Value>,
    expect: Value,
    /// Required final SSE frame; the report itself if there is no separate sentinel.
    terminal_marker: String,
}

fn directory(api: Api) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata/repair-and-recall")
        .join(api.name())
}
fn load(api: Api) -> Corpus {
    let c: Corpus =
        serde_json::from_slice(&std::fs::read(directory(api).join("session.json")).unwrap())
            .unwrap();
    assert_eq!(c.name, "repair-and-recall");
    assert_eq!(
        c.turns.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        [
            "inspect", "locate", "patch", "check", "repair", "recheck", "final", "recall"
        ]
    );
    for source in c
        .turns
        .iter()
        .map(|t| &t.source)
        .chain([&c.incomplete])
        .chain(c.sparse.iter())
    {
        assert_eq!(
            source.json.is_some(),
            matches!(api, Api::Chat | Api::Responses | Api::Messages)
        );
    }
    c
}
fn wire(api: Api, source: &Source) -> Vec<u8> {
    assert!(
        PathBuf::from(&source.sse)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    );
    let bytes = std::fs::read(directory(api).join(&source.sse)).unwrap();
    assert!(bytes.len() < LIMIT);
    let text = std::str::from_utf8(&bytes).unwrap();
    assert_eq!(text.matches(&source.terminal_marker).count(), 1);
    bytes
}
fn intake(api: Api, source: &Source, stream: bool, chunk_size: usize) -> Attempt {
    let mut attempt = Attempt::new(api.upstream(), LIMIT, Default::default());
    let (media, bytes) = if !stream && let Some(v) = &source.json {
        ("application/json", serde_json::to_vec(v).unwrap())
    } else {
        ("text/event-stream", wire(api, source))
    };
    attempt.begin(200, media).unwrap();
    for chunk in bytes.chunks(chunk_size) {
        let mut remaining = chunk;
        while !remaining.is_empty() {
            let (used, events) = attempt
                .push(remaining)
                .unwrap_or_else(|error| panic!("{} {}: {error}", api.name(), source.sse));
            assert!(used > 0);
            assert!(
                events
                    .iter()
                    .all(|e| !matches!(e, StreamEvent::Terminal { .. })),
                "EOF owns terminal publication"
            );
            remaining = &remaining[used..];
        }
    }
    assert_eq!(attempt.ignored_events(), 0);
    // Even a complete native terminal/sentinel is not validated transport EOF.
    assert!(attempt.response().is_err());
    assert!(delivery(api).encode_json(&attempt).is_err());
    attempt.finish().unwrap();
    assert_eq!(
        facts::response(&attempt.response().unwrap().semantic),
        source.expect,
        "{} source facts",
        api.name()
    );
    attempt
}
fn delivery(api: Api) -> ResponseDelivery {
    ResponseDelivery::new(
        api.client(),
        MODEL,
        Default::default(),
        Default::default(),
        Obfuscation::Disabled,
    )
}
fn append_delivery(api: Api, body: &mut Value, value: Value) {
    let history = body[api.client_history()].as_array_mut().unwrap();
    match api {
        Api::Chat => history.push(value["choices"][0]["message"].clone()),
        Api::Messages => history.push(json!({"role":"assistant","content":value["content"]})),
        _ => history.extend(value["output"].as_array().unwrap().iter().cloned()),
    }
}
fn update(body: &mut Value, fields: Option<&Value>, history: &str) {
    if let Some(fields) = fields {
        assert!(!fields.as_object().unwrap().contains_key(history));
        body.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
    }
}

fn assert_history(api: Api, request: &Request, turn: usize) {
    let mut calls = vec![];
    let mut results = vec![];
    for (_, item) in request.task.semantic.items() {
        match item {
            Item::ToolCall(c) => calls.push(c.call_id.as_str()),
            Item::ToolResult(r) => {
                let id = r.call_id.as_ref().unwrap().as_str();
                results.push(id);
                assert_eq!(
                    r.is_error,
                    matches!(api, Api::Messages).then_some(matches!(id, "call-a" | "call-check"))
                );
                assert_eq!(
                    r.execution, None,
                    "feedback text never proves execution status"
                );
            }
            _ => {}
        }
    }
    let length = if turn == 0 { 0 } else { (turn + 1).min(7) };
    assert_eq!(
        calls,
        [
            "call-z",
            "call-a",
            "call-locate",
            "call-patch",
            "call-check",
            "call-repair",
            "call-recheck"
        ][..length]
    );
    assert_eq!(
        results,
        [
            "call-a",
            "call-z",
            "call-locate",
            "call-patch",
            "call-check",
            "call-repair",
            "call-recheck"
        ][..length]
    );
    assert_eq!(
        request.task.semantic.settings().controls.max_output_tokens,
        Some(1024)
    );
    assert_eq!(
        request
            .task
            .semantic
            .settings()
            .effort
            .value()
            .unwrap()
            .scope,
        if matches!(api, Api::Messages) {
            EffortScope::Response
        } else {
            EffortScope::Reasoning
        }
    );
    assert_eq!(
        request
            .task
            .semantic
            .settings()
            .effort
            .value()
            .unwrap()
            .level,
        if turn >= 7 {
            EffortLevel::High
        } else {
            EffortLevel::Low
        }
    );
    assert_eq!(
        request
            .task
            .semantic
            .settings()
            .tools
            .as_ref()
            .unwrap()
            .len(),
        if turn >= 7 { 0 } else { 4 }
    );
}

fn run(
    api: Api,
    corpus: &Corpus,
    stream: bool,
    chunk_size: usize,
    final_source: Option<&Source>,
) -> Attempt {
    let mut client = corpus.client_request.clone();
    let mut expected = corpus.upstream_request.clone();
    for (index, turn) in corpus.turns.iter().enumerate() {
        update(
            &mut client,
            turn.client_update.as_ref(),
            api.client_history(),
        );
        update(
            &mut expected,
            turn.upstream_update.as_ref(),
            api.native_history(),
        );
        let mut request = api
            .client()
            .decode_client_request(&serde_json::to_vec(&client).unwrap())
            .unwrap();
        open_from_client(&mut request).unwrap();
        assert_history(api, &request, index);
        let before = request.clone();
        let (actual, rules) = api.encode(&request).unwrap();
        assert_eq!(
            actual,
            expected,
            "{} {} native request",
            api.name(),
            turn.name
        );
        assert_eq!(
            rules,
            turn.request_rules,
            "{} {} request losses",
            api.name(),
            turn.name
        );
        assert_eq!(request, before, "projection must not edit source history");
        let alternate = final_source.filter(|_| turn.name == "final");
        let attempt = intake(api, alternate.unwrap_or(&turn.source), stream, chunk_size);
        let value: Value =
            serde_json::from_slice(&delivery(api).encode_json(&attempt).unwrap()).unwrap();
        if alternate.is_some() {
            return attempt;
        }
        append_delivery(api, &mut client, value);
        client[api.client_history()]
            .as_array_mut()
            .unwrap()
            .extend(turn.client_append.clone());
        expected[api.native_history()]
            .as_array_mut()
            .unwrap()
            .extend(turn.upstream_append.clone());
        if index == corpus.turns.len() - 1 {
            // Also check the last answer's replay; no unused tail oracle.
            let mut request = api
                .client()
                .decode_client_request(&serde_json::to_vec(&client).unwrap())
                .unwrap();
            open_from_client(&mut request).unwrap();
            assert_history(api, &request, index + 1);
            let (actual, rules) = api.encode(&request).unwrap();
            assert_eq!(actual, expected, "{} closed history", api.name());
            assert_eq!(rules, turn.request_rules);
            return attempt;
        }
    }
    unreachable!("the validated corpus has eight turns")
}

fn exercise(api: Api) {
    let corpus = load(api);
    // Native static intake where supported, plus arbitrary byte cuts through UTF-8,
    // escaped/raw arguments and SSE boundaries. Google remains SSE-only upstream.
    for (stream, chunk_size) in [(false, LIMIT), (true, 13), (true, 1)] {
        run(api, &corpus, stream, chunk_size, None);
    }
}

#[test]
fn chat_repair_and_recall() {
    exercise(Api::Chat)
}
#[test]
fn responses_repair_and_recall() {
    exercise(Api::Responses)
}
#[test]
fn messages_repair_and_recall() {
    exercise(Api::Messages)
}
#[test]
fn generate_content_repair_and_recall() {
    exercise(Api::GenerateContent)
}
#[test]
fn interactions_repair_and_recall() {
    exercise(Api::Interactions)
}

#[test]
fn interrupted_final_never_becomes_completed_or_replayable_before_eof() {
    use morphiecore::{
        execution::AttemptError,
        protocol::{CodecError, stream::SseError},
    };
    for api in [
        Api::Chat,
        Api::Responses,
        Api::Messages,
        Api::GenerateContent,
        Api::Interactions,
    ] {
        let corpus = load(api);
        for stream in [true, false]
            .into_iter()
            .filter(|stream| *stream || corpus.incomplete.json.is_some())
        {
            let attempt = run(api, &corpus, stream, 7, Some(&corpus.incomplete));
            assert_eq!(
                attempt.response().unwrap().semantic.outcome(),
                Outcome::Incomplete
            );
            let value: Value =
                serde_json::from_slice(&delivery(api).encode_json(&attempt).unwrap()).unwrap();
            match api {
                Api::Chat => assert_eq!(value["choices"][0]["finish_reason"], "length"),
                Api::Messages => assert_eq!(value["stop_reason"], "max_tokens"),
                _ => assert_eq!(value["status"], "incomplete"),
            }
        }
        let bytes = wire(api, &corpus.incomplete);
        let text = std::str::from_utf8(&bytes).unwrap();
        let end = text.find(&corpus.incomplete.terminal_marker).unwrap();
        assert!(
            end > 0 && text[..end].ends_with("\n\n"),
            "cut only at a complete frame boundary"
        );
        let mut attempt = Attempt::new(api.upstream(), LIMIT, Default::default());
        attempt.begin(200, "text/event-stream").unwrap();
        let mut remaining = &bytes[..end];
        while !remaining.is_empty() {
            let (used, events) = attempt.push(remaining).unwrap();
            assert!(used > 0);
            assert!(
                events
                    .iter()
                    .all(|e| !matches!(e, StreamEvent::Terminal { .. }))
            );
            remaining = &remaining[used..];
        }
        let error = attempt.finish().unwrap_err();
        match api {
            Api::Responses => assert!(
                matches!(
                    error,
                    AttemptError::Sse(SseError::Codec(CodecError::Event(
                        EventError::EofBeforeTerminal
                    )))
                ),
                "{error:?}"
            ),
            _ => {
                let expected = match api {
                    Api::Chat => "EOF before Chat DONE",
                    Api::Messages => "Anthropic stream ordering or closure",
                    Api::GenerateContent => "Gemini stream without finish reason",
                    Api::Interactions => "unfinished Interactions stream",
                    Api::Responses => unreachable!(),
                };
                assert!(
                    matches!(error, AttemptError::Sse(SseError::Codec(CodecError::Invalid(label))) if label==expected),
                    "{error:?}"
                );
            }
        }
        assert!(delivery(api).encode_json(&attempt).is_err());
    }
}

#[test]
fn sparse_interactions_accounting_remains_unknown() {
    let corpus = load(Api::Interactions);
    assert!(corpus.sparse.is_some());
    let attempt = run(Api::Interactions, &corpus, true, 3, corpus.sparse.as_ref());
    let response = &attempt.response().unwrap().semantic;
    let usage = &response.usage_reports().operation().unwrap().counts;
    assert_eq!(usage.cached_input_tokens, None);
    assert_eq!(usage.reasoning_tokens, None);
    let value: Value =
        serde_json::from_slice(&delivery(Api::Interactions).encode_json(&attempt).unwrap())
            .unwrap();
    assert!(
        value["usage"].is_null(),
        "unknown accounting is not a fabricated zero view"
    );
}

#[test]
fn edited_returned_reasoning_cannot_reuse_the_original_replay_authority() {
    use morphiecore::{protocol::CodecError, semantic::value::Text};
    for api in [
        Api::Responses,
        Api::Messages,
        Api::GenerateContent,
        Api::Interactions,
    ] {
        let corpus = load(api);
        let turn = &corpus.turns[0];
        let attempt = intake(api, &turn.source, true, 11);
        let value = serde_json::from_slice(&delivery(api).encode_json(&attempt).unwrap()).unwrap();
        let mut client = corpus.client_request.clone();
        append_delivery(api, &mut client, value);
        client[api.client_history()]
            .as_array_mut()
            .unwrap()
            .extend(turn.client_append.clone());
        let mut request = api
            .client()
            .decode_client_request(&serde_json::to_vec(&client).unwrap())
            .unwrap();
        open_from_client(&mut request).unwrap();
        let mut expected = corpus.upstream_request.clone();
        expected[api.native_history()]
            .as_array_mut()
            .unwrap()
            .extend(turn.upstream_append.clone());
        // The unmodified case reaches and passes this exact target first.
        assert_eq!(api.encode(&request).unwrap().0, expected);
        let mut items = request.task.semantic.items().to_vec();
        let reasoning = items
            .iter_mut()
            .find_map(|(_, item)| match item {
                Item::Reasoning(r) if !r.parts.is_empty() => Some(r),
                _ => None,
            })
            .unwrap();
        reasoning.parts[0].1 = ReasoningContent::Summary(
            Text::new("Edited plan", "synthetic", MAX_TEXT_BYTES).unwrap(),
        );
        request.task.semantic = request.task.semantic.clone().with_items(items).unwrap();
        let error = api.encode(&request).unwrap_err();
        assert!(
            matches!(
                error,
                AdapterError::Codec(CodecError::Invalid("native replay dependency"))
            ),
            "{error:?}"
        );
        assert_eq!(
            facts::response(&attempt.response().unwrap().semantic),
            turn.source.expect
        );
    }
}
