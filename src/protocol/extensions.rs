//! Provider-scoped extension carriers over stateless envelopes, never Task IR.
//!
//! Lifecycle classes follow the Codex HTTP header baseline: session/cache scope,
//! thread identity, context window, turn state, routing hint, lineage and agent
//! labels are distinct owners, so one value never becomes a universal session ID.
//! Credential, account-locator and transport headers are owned by the credential
//! and transport layers and never enter this carrier. Server-issued turn state
//! only round-trips; nothing here derives or extends it.
use super::openai::CodecError;
use crate::semantic::value::Presence;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

const MAX_EXTRA_HEADERS: usize = 32;
const MAX_HEADER_NAME: usize = 64;
const MAX_HEADER_VALUE: usize = 2048;
const MAX_TURN_METADATA: usize = 8192;
const MAX_FIELD: usize = 256;
const MAX_SECTION_ENTRIES: usize = 32;
const MAX_SECTION_KEY: usize = 64;
const MAX_SECTION_BYTES: usize = 16 * 1024;

/// Credentials, account locators and transport framing are never context metadata.
const FORBIDDEN_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
    "api-key",
    "x-api-key",
    "chatgpt-account-id",
    "x-openai-fedramp",
    "x-oai-attestation",
    "accept",
    "content-type",
    "content-encoding",
    "content-length",
    "host",
    "connection",
    "transfer-encoding",
    "upgrade",
    "te",
    "trailer",
    "keep-alive",
];
/// Body sections with explicit admission; other unknown body keys stay rejected.
pub const SECTION_NAMES: &[&str] = &["client_metadata"];

/// Typed routing hint (`model=<model>` or `model=<model>;tier=<tier>`), never sticky state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingHint {
    pub model: String,
    pub tier: Option<String>,
}
impl RoutingHint {
    fn parse(value: &str) -> Result<Self, CodecError> {
        let mut parts = value.split(';');
        let invalid = || CodecError::Invalid("routing hint");
        let model = parts
            .next()
            .and_then(|p| p.strip_prefix("model="))
            .filter(|m| !m.is_empty() && m.len() <= MAX_FIELD)
            .ok_or_else(invalid)?;
        let tier = match parts.next() {
            None => None,
            Some(t) => Some(
                t.strip_prefix("tier=")
                    .filter(|t| !t.is_empty() && t.len() <= 64)
                    .ok_or_else(invalid)?,
            ),
        };
        if parts.next().is_some() {
            return Err(invalid());
        }
        Ok(Self {
            model: model.into(),
            tier: tier.map(Into::into),
        })
    }
    fn render(&self) -> String {
        match &self.tier {
            Some(tier) => format!("model={};tier={}", self.model, tier),
            None => format!("model={}", self.model),
        }
    }
    fn validate(&self) -> Result<(), CodecError> {
        if self.model.is_empty() || self.model.len() > MAX_FIELD {
            return Err(CodecError::Invalid("routing hint"));
        }
        if self
            .tier
            .as_ref()
            .is_some_and(|t| t.is_empty() || t.len() > 64)
        {
            return Err(CodecError::Invalid("routing hint"));
        }
        Ok(())
    }
}

/// Codex HTTP header projection of provider-scoped execution context.
/// Fields are decoded only from their own wire names; unmodeled safe headers
/// round-trip opaquely inside the same budget.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CodexHeaders {
    /// `session-id`: session/cache scope carrier (ChatGPT cache affinity), not a logical session ID.
    pub session: Presence<String>,
    /// `thread-id` and `x-client-request-id`: one thread identity in the Codex baseline.
    pub thread: Presence<String>,
    /// `x-codex-window-id`: context-window identity, distinct from the thread.
    pub window: Presence<String>,
    /// `x-codex-turn-state`: server-issued opaque token, replayed verbatim or dropped.
    pub turn_state: Presence<String>,
    /// `x-codex-routing-hint`: model/tier routing hint, never sticky state.
    pub routing: Presence<RoutingHint>,
    /// `x-codex-parent-thread-id`: agent lineage, only when explicitly present.
    pub parent_thread: Presence<String>,
    /// `x-openai-subagent`: agent/worker classification, only when explicitly present.
    pub subagent: Presence<String>,
    /// `x-codex-turn-metadata`: bounded opaque compatibility projection of turn metadata.
    pub turn_metadata: Presence<String>,
    /// Unmodeled safe headers preserved verbatim within budget.
    pub extra: BTreeMap<String, String>,
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_HEADER_NAME
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}
fn valid_value(value: &str, max: usize) -> bool {
    // Header injection and control bytes are rejected at the codec boundary.
    value.len() <= max
        && !value
            .bytes()
            .any(|b| b == b'\r' || b == b'\n' || b == 0 || b < 0x20)
}

impl CodexHeaders {
    pub fn decode(headers: &[(String, String)]) -> Result<Self, CodecError> {
        let mut out = Self::default();
        let mut names = BTreeSet::new();
        for (name, value) in headers {
            if !valid_name(name) {
                return Err(CodecError::Invalid("header name"));
            }
            if FORBIDDEN_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
                return Err(CodecError::Unsupported(format!("header {name}")));
            }
            if !names.insert(name.to_ascii_lowercase()) {
                return Err(CodecError::Invalid("duplicate header"));
            }
            let lower = name.to_ascii_lowercase();
            let set = |slot: &mut Presence<String>, max: usize| -> Result<(), CodecError> {
                if !valid_value(value, max) {
                    return Err(CodecError::Invalid("header value"));
                }
                match slot {
                    Presence::Value(old) if old != value => {
                        Err(CodecError::Invalid("header identity"))
                    }
                    Presence::Value(_) => Ok(()),
                    other => {
                        *other = Presence::Value(value.clone());
                        Ok(())
                    }
                }
            };
            match lower.as_str() {
                "session-id" => set(&mut out.session, MAX_FIELD)?,
                "thread-id" | "x-client-request-id" => set(&mut out.thread, MAX_FIELD)?,
                "x-codex-window-id" => set(&mut out.window, MAX_FIELD)?,
                "x-codex-turn-state" => set(&mut out.turn_state, MAX_HEADER_VALUE)?,
                "x-codex-routing-hint" => {
                    if !valid_value(value, MAX_HEADER_VALUE) {
                        return Err(CodecError::Invalid("header value"));
                    }
                    if out.routing.value().is_some() {
                        return Err(CodecError::Invalid("duplicate header"));
                    }
                    out.routing = Presence::Value(RoutingHint::parse(value)?);
                }
                "x-codex-parent-thread-id" => set(&mut out.parent_thread, MAX_FIELD)?,
                "x-openai-subagent" => set(&mut out.subagent, MAX_FIELD)?,
                "x-codex-turn-metadata" => set(&mut out.turn_metadata, MAX_TURN_METADATA)?,
                _ => {
                    if !valid_value(value, MAX_HEADER_VALUE) {
                        return Err(CodecError::Invalid("header value"));
                    }
                    if out.extra.len() >= MAX_EXTRA_HEADERS {
                        return Err(CodecError::Limit);
                    }
                    out.extra.insert(name.clone(), value.clone());
                }
            }
        }
        out.validate()?;
        Ok(out)
    }
    pub fn validate(&self) -> Result<(), CodecError> {
        for (value, max) in [
            (&self.session, MAX_FIELD),
            (&self.thread, MAX_FIELD),
            (&self.window, MAX_FIELD),
            (&self.turn_state, MAX_HEADER_VALUE),
            (&self.parent_thread, MAX_FIELD),
            (&self.subagent, MAX_FIELD),
            (&self.turn_metadata, MAX_TURN_METADATA),
        ] {
            if value
                .value()
                .is_some_and(|s| !valid_value(s, max) || s.is_empty())
            {
                return Err(CodecError::Invalid("header value"));
            }
        }
        if self.routing.value().is_some_and(|r| r.validate().is_err()) {
            return Err(CodecError::Invalid("routing hint"));
        }
        if self.extra.len() > MAX_EXTRA_HEADERS {
            return Err(CodecError::Limit);
        }
        for (name, value) in &self.extra {
            if !valid_name(name) || !valid_value(value, MAX_HEADER_VALUE) {
                return Err(CodecError::Invalid("header"));
            }
        }
        Ok(())
    }
    /// Canonical projection: `thread` renders into both thread headers because the
    /// Codex baseline sends one thread identity through both names.
    pub fn encode(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        push(&mut out, "session-id", &self.session);
        push(&mut out, "thread-id", &self.thread);
        push(&mut out, "x-client-request-id", &self.thread);
        push(&mut out, "x-codex-window-id", &self.window);
        push(&mut out, "x-codex-turn-state", &self.turn_state);
        if let Presence::Value(hint) = &self.routing {
            out.push(("x-codex-routing-hint".to_string(), hint.render()));
        }
        push(&mut out, "x-codex-parent-thread-id", &self.parent_thread);
        push(&mut out, "x-openai-subagent", &self.subagent);
        push(&mut out, "x-codex-turn-metadata", &self.turn_metadata);
        out.extend(self.extra.iter().map(|(k, v)| (k.clone(), v.clone())));
        out
    }
}
fn push(out: &mut Vec<(String, String)>, name: &str, slot: &Presence<String>) {
    if let Presence::Value(v) = slot {
        out.push((name.to_string(), v.clone()));
    }
}

/// Bounded opaque body sections (`client_metadata` today) that ride beside the
/// standard envelope fields. Section values keep their source shape; the codec
/// never interprets provider-private metadata schemas.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CustomSections {
    pub sections: Map<String, Value>,
}
impl CustomSections {
    /// Strict standalone decode: every key must be an admitted section name.
    pub fn decode(sections: &Map<String, Value>) -> Result<Self, CodecError> {
        for (name, value) in sections {
            if !SECTION_NAMES.contains(&name.as_str()) {
                return Err(CodecError::Unsupported(format!("extension section {name}")));
            }
            let Value::Object(entries) = value else {
                return Err(CodecError::Invalid("extension section"));
            };
            if entries.len() > MAX_SECTION_ENTRIES
                || entries
                    .keys()
                    .any(|k| k.is_empty() || k.chars().count() > MAX_SECTION_KEY)
            {
                return Err(CodecError::Limit);
            }
            if crate::semantic::value::json_size(value, MAX_SECTION_BYTES).is_err() {
                return Err(CodecError::Limit);
            }
        }
        Ok(Self {
            sections: sections.clone(),
        })
    }
    /// Extract admitted sections from an envelope body; unknown keys stay the
    /// envelope whitelist's responsibility.
    pub fn read(body: &Map<String, Value>) -> Result<Self, CodecError> {
        let picked: Map<_, _> = body
            .iter()
            .filter(|(k, _)| SECTION_NAMES.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Self::decode(&picked)
    }
    pub fn write(&self, body: &mut Map<String, Value>) -> Result<(), CodecError> {
        let mut probe = Map::new();
        for (k, v) in &self.sections {
            probe.insert(k.clone(), v.clone());
        }
        Self::decode(&probe)?;
        body.extend(self.sections.clone());
        Ok(())
    }
}
