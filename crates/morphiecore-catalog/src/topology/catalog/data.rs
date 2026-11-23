//! Strict, immutable inputs to the existing topology compiler.
use super::*;
use crate::{
    protocol::{
        openai::dialect::adaptation::WireRules,
        reasoning::{ReasoningConversion, ReasoningEffortMapping, ReasoningRequestPolicy},
    },
    provider::{GenerationOperation, ProviderDefinition, ProviderOperation},
    semantic::{
        task::generation::{EffortLevel, EffortScope},
        value::{JsonLimits, parse_json},
    },
};
use serde::Deserialize;
use std::{collections::BTreeSet, sync::OnceLock};

#[derive(Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "'de: 'static"))]
pub(super) struct Catalog {
    models: Box<[models::ModelDefinition]>,
    api_key: Box<[ApiKeyBinding]>,
    subscriptions: Box<[SubscriptionBinding]>,
    messages: Box<[MessagesBinding]>,
    google: Box<[GoogleBinding]>,
    images: Box<[ImageBinding]>,
    speech: Box<[SpeechBinding]>,
    transcriptions: Box<[TranscriptionBinding]>,
    transcription_models: Box<[Publication]>,
    embeddings: Box<[EmbeddingBinding]>,
    effort_mappings: Box<[EffortMapping]>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Publication {
    model: &'static str,
    released_at: u64,
    developer: &'static str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EffortMapping {
    canonical_model: &'static str,
    rule: &'static str,
    scope: EffortScope,
    native_levels: Box<[EffortLevel]>,
    encode: Box<[(EffortLevel, EffortLevel)]>,
    decode: Box<[(EffortLevel, EffortLevel)]>,
    #[serde(skip)]
    mapping: OnceLock<ReasoningEffortMapping>,
}

impl EffortMapping {
    fn mapping(&'static self) -> &'static ReasoningEffortMapping {
        self.mapping.get_or_init(|| ReasoningEffortMapping {
            canonical_model: self.canonical_model,
            rule: self.rule,
            scope: self.scope,
            native_levels: &self.native_levels,
            encode: &self.encode,
            decode: &self.decode,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ScopeEffort {
    rule: &'static str,
    from: EffortScope,
    to: EffortScope,
    levels: Box<[(EffortLevel, EffortLevel)]>,
}

impl ScopeEffort {
    pub(super) fn policy(&'static self, model: &'static str) -> ReasoningRequestPolicy {
        ReasoningRequestPolicy {
            canonical_model: model.into(),
            rule: self.rule,
            conversion: ReasoningConversion::Scope {
                from: self.from,
                to: self.to,
                levels: &self.levels,
            },
        }
    }
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireOverrides {
    reject_reasoning_controls: Option<bool>,
    chat_video_url: Option<bool>,
    auto_tool_choice_only: Option<bool>,
    reasoning_alias: Option<bool>,
    responses_summary_text_alias: Option<bool>,
    qwen4_responses_reasoning: Option<bool>,
}

impl WireOverrides {
    pub(super) fn apply(self, rules: &mut WireRules) {
        if let Some(value) = self.reject_reasoning_controls {
            rules.request.reject_reasoning_controls = value;
        }
        if let Some(value) = self.chat_video_url {
            rules.request.chat_video_url = value;
        }
        if let Some(value) = self.auto_tool_choice_only {
            rules.request.auto_tool_choice_only = value;
        }
        if let Some(value) = self.reasoning_alias {
            rules.reasoning.reasoning_alias = value;
        }
        if let Some(value) = self.responses_summary_text_alias {
            rules.reasoning.responses_summary_text_alias = value;
        }
        if let Some(value) = self.qwen4_responses_reasoning {
            rules.reasoning.qwen4_responses_reasoning = value;
        }
    }
}

fn parse(bytes: &'static [u8]) -> Result<Catalog, TopologyError> {
    // Use strict bounded admission before borrowing strings from the embedded
    // source. Unlike Value deserialization, this retains static string views.
    parse_json(bytes, JsonLimits::ENVELOPE).map_err(|_| TopologyError::InvalidCatalog)?;
    serde_json::from_slice(bytes).map_err(|_| TopologyError::InvalidCatalog)
}

pub(super) fn checked() -> Result<&'static Catalog, TopologyError> {
    static CATALOG: OnceLock<Result<Catalog, TopologyError>> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            let catalog = parse(include_bytes!("topology.json"))?;
            catalog.validate()?;
            Ok(catalog)
        })
        .as_ref()
        .map_err(|error| *error)
}

pub(super) fn provider(id: &str) -> &'static ProviderDefinition {
    catalog::get(id).expect("validated embedded Provider reference")
}

pub(super) fn openai_model_contract(id: &str) -> Dispositions {
    checked()
        .expect("validated embedded topology")
        .openai_model_contract(id)
        .expect("validated OpenAI-compatible model reference")
}

pub(super) fn effort_mapping(rule: &str) -> &'static ReasoningEffortMapping {
    checked()
        .expect("validated embedded topology")
        .effort_mappings
        .iter()
        .find(|m| m.rule == rule)
        .expect("validated embedded effort reference")
        .mapping()
}

fn operation(
    provider: &str,
    operation: ProviderOperation,
    credential: CredentialKind,
) -> Result<(), TopologyError> {
    let provider = catalog::get(provider).ok_or(TopologyError::UnknownProvider)?;
    let entry = provider
        .entry(operation)
        .ok_or(TopologyError::TargetMismatch)?;
    if entry.auth.kind() != credential {
        return Err(TopologyError::TargetMismatch);
    }
    Ok(())
}

fn publication(model: &str, released_at: u64, developer: &str) -> Result<(), TopologyError> {
    ModelId::new(model).map_err(|_| TopologyError::InvalidModelBinding)?;
    ModelMetadata::new(released_at, developer).map_err(|_| TopologyError::InvalidModelMetadata)?;
    Ok(())
}

fn members<T>(
    protocols: &[GenerationProtocol],
    entries: &[(GenerationProtocol, T)],
) -> Result<(), TopologyError> {
    for (index, (protocol, _)) in entries.iter().enumerate() {
        if !protocols.contains(protocol) || entries[..index].iter().any(|(p, _)| p == protocol) {
            return Err(TopologyError::InvalidModelBinding);
        }
    }
    Ok(())
}

fn unique<T: PartialEq>(values: &[T]) -> bool {
    values
        .iter()
        .enumerate()
        .all(|(index, value)| !values[..index].contains(value))
}

impl Catalog {
    // Canonical identity alone does not establish an OpenAI-compatible
    // contract. Messages contributes its public admission, not native carriage.
    fn openai_model_contract(&self, id: &str) -> Result<Dispositions, TopologyError> {
        if let Some(model) = self.models.iter().find(|m| m.id == id) {
            return Ok(model.contract());
        }
        if let Some(model) = self.messages.iter().find(|m| m.model == id) {
            return Ok(model.public_contract());
        }
        Err(TopologyError::UnknownCanonicalModel)
    }

    fn validate(&self) -> Result<(), TopologyError> {
        let mut canonical = BTreeSet::new();
        for (id, released_at, developer) in self
            .models
            .iter()
            .map(|m| (m.id, m.released_at, m.developer))
            .chain(
                self.messages
                    .iter()
                    .map(|m| (m.model, m.released_at, m.developer)),
            )
            .chain(
                self.google
                    .iter()
                    .map(|m| (m.model, m.released_at, m.developer)),
            )
        {
            publication(id, released_at, developer)?;
            if !canonical.insert(id) {
                return Err(TopologyError::DuplicateCanonicalModel);
            }
        }
        let mut mapping_rules = BTreeSet::new();
        for mapping in &self.effort_mappings {
            if !canonical.contains(mapping.canonical_model)
                || mapping.rule.is_empty()
                || !mapping_rules.insert(mapping.rule)
                || mapping.native_levels.is_empty()
                || !unique(&mapping.native_levels)
                || mapping.encode.is_empty()
                || mapping.decode.is_empty()
                || !unique(
                    &mapping
                        .encode
                        .iter()
                        .map(|(from, _)| *from)
                        .collect::<Vec<_>>(),
                )
                || !unique(
                    &mapping
                        .decode
                        .iter()
                        .map(|(from, _)| *from)
                        .collect::<Vec<_>>(),
                )
                || mapping
                    .encode
                    .iter()
                    .any(|(_, to)| !mapping.native_levels.contains(to))
                || mapping.decode.iter().any(|(from, to)| {
                    !mapping.native_levels.contains(from) || !mapping.native_levels.contains(to)
                })
            {
                return Err(TopologyError::InvalidModelBinding);
            }
        }
        for b in &self.api_key {
            self.openai_model_contract(b.canonical_model)?;
            ModelId::new(b.model).map_err(|_| TopologyError::InvalidModelBinding)?;
            RouteId::new(&format!("{}-generation", b.endpoint_prefix))
                .map_err(|_| TopologyError::InvalidModelBinding)?;
            if b.protocols.is_empty() || !unique(&b.protocols) {
                return Err(TopologyError::InvalidModelBinding);
            }
            members(&b.protocols, &b.reasoning_history)?;
            members(&b.protocols, &b.effort_mappings)?;
            members(&b.protocols, &b.wire_overrides)?;
            for &p in &b.protocols {
                if !matches!(
                    p,
                    GenerationProtocol::OpenAiChat | GenerationProtocol::OpenAiResponses
                ) {
                    return Err(TopologyError::InvalidModelBinding);
                }
                let suffix = if p == GenerationProtocol::OpenAiChat {
                    "chat"
                } else {
                    "responses"
                };
                EndpointId::new(&format!("{}-{suffix}", b.endpoint_prefix))
                    .map_err(|_| TopologyError::InvalidModelBinding)?;
                operation(b.provider, p.operation().into(), CredentialKind::ApiKey)?;
            }
            for (_, rule) in &b.effort_mappings {
                if !self
                    .effort_mappings
                    .iter()
                    .any(|m| m.rule == *rule && m.canonical_model == b.canonical_model)
                {
                    return Err(TopologyError::InvalidModelBinding);
                }
            }
            b.limits
                .validate()
                .map_err(|_| TopologyError::InvalidModelBinding)?;
        }
        for b in &self.subscriptions {
            self.openai_model_contract(b.model)?;
            RouteId::new(&format!("{}-generation", b.identity))
                .map_err(|_| TopologyError::InvalidModelBinding)?;
            EndpointId::new(&format!("{}-responses", b.identity))
                .map_err(|_| TopologyError::InvalidModelBinding)?;
            operation(
                b.provider,
                GenerationOperation::Responses.into(),
                CredentialKind::OAuth(b.profile),
            )?;
            b.limits
                .validate()
                .map_err(|_| TopologyError::InvalidModelBinding)?;
        }
        // Only reviewed bindings can inherit the code-owned Messages and Images contracts.
        if self.messages.len() > 1
            || self
                .messages
                .iter()
                .any(|b| (b.provider, b.model) != messages::REVIEWED)
            || self.images.iter().any(|b| !b.reviewed())
        {
            return Err(TopologyError::InvalidModelBinding);
        }
        for b in &self.messages {
            operation(
                b.provider,
                GenerationOperation::AnthropicMessages.into(),
                CredentialKind::ApiKey,
            )?;
            b.limits
                .validate()
                .map_err(|_| TopologyError::InvalidModelBinding)?;
        }
        for b in &self.google {
            b.endpoint_ids()?;
            b.route_id()?;
            for p in [
                GenerationOperation::GoogleGenerateContent,
                GenerationOperation::GoogleInteractions,
            ] {
                operation(b.provider, p.into(), CredentialKind::ApiKey)?;
                let provider = provider(b.provider);
                let entry = provider.generation_entry(p).expect("validated entry");
                p.resolve_path(entry.path, b.upstream, true)
                    .map_err(|_| TopologyError::InvalidModelBinding)?;
            }
            if let Some(effort) = &b.scope_effort
                && (effort.rule.is_empty()
                    || effort.from == effort.to
                    || effort.levels.is_empty()
                    || !unique(
                        &effort
                            .levels
                            .iter()
                            .map(|(from, _)| *from)
                            .collect::<Vec<_>>(),
                    ))
            {
                return Err(TopologyError::InvalidModelBinding);
            }
            b.limits
                .validate()
                .map_err(|_| TopologyError::InvalidModelBinding)?;
        }
        for b in &self.images {
            publication(b.model, b.released_at, b.developer)?;
            for provider in &b.providers {
                operation(
                    provider,
                    ProviderOperation::Images,
                    if *provider == "grok" {
                        CredentialKind::OAuth("grok")
                    } else {
                        CredentialKind::ApiKey
                    },
                )?;
            }
        }
        for b in &self.speech {
            publication(b.model, b.released_at, b.developer)?;
            EndpointId::new(b.identity).map_err(|_| TopologyError::InvalidModelBinding)?;
            b.voices()?;
            operation(
                b.provider,
                ProviderOperation::Speech,
                CredentialKind::ApiKey,
            )?;
        }
        let mut transcription_models = BTreeSet::new();
        for m in &self.transcription_models {
            publication(m.model, m.released_at, m.developer)?;
            if !transcription_models.insert(m.model) {
                return Err(TopologyError::DuplicateModelMetadata);
            }
        }
        for b in &self.transcriptions {
            if !transcription_models.contains(b.canonical_model) {
                return Err(TopologyError::UnknownCanonicalModel);
            }
            ModelId::new(b.model).map_err(|_| TopologyError::InvalidModelBinding)?;
            EndpointId::new(b.endpoint_id).map_err(|_| TopologyError::InvalidModelBinding)?;
            operation(
                b.provider,
                ProviderOperation::Transcription,
                CredentialKind::ApiKey,
            )?;
        }
        for b in &self.embeddings {
            publication(b.model, b.released_at, b.developer)?;
            b.ids()?;
            operation(
                b.provider,
                ProviderOperation::Embeddings,
                CredentialKind::ApiKey,
            )?;
        }
        Ok(())
    }
}

macro_rules! bindings {
    ($($accessor:ident : $kind:ty => $field:ident),+ $(,)?) => {
        $(pub fn $accessor() -> &'static [$kind] {
            &checked().expect("validated embedded topology").$field
        })+
    };
}

bindings! {
    api_key_bindings: ApiKeyBinding => api_key,
    subscription_bindings: SubscriptionBinding => subscriptions,
    messages_bindings: MessagesBinding => messages,
    google_bindings: GoogleBinding => google,
    image_bindings: ImageBinding => images,
    speech_bindings: SpeechBinding => speech,
    transcription_bindings: TranscriptionBinding => transcriptions,
    embedding_bindings: EmbeddingBinding => embeddings,
}

pub(super) fn models() -> &'static [models::ModelDefinition] {
    &checked().expect("validated embedded topology").models
}

pub(super) fn transcription_metadata() -> impl Iterator<Item = (ModelId, ModelMetadata)> {
    checked()
        .expect("validated embedded topology")
        .transcription_models
        .iter()
        .map(|m| {
            (
                ModelId::new(m.model).expect("validated model identity"),
                ModelMetadata::new(m.released_at, m.developer).expect("validated publication"),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! fixture {
        ($provider:literal, $canonical:literal, $effort:literal, $extra:literal) => {
            concat!(
                r#"{"models":[{"id":"fixture","images":false,"files":false,"strict_tools":false,
                    "custom_tools":false,"video":false,"developer":"Fixture","released_at":1}],
                    "subscriptions":[],"messages":[],"google":[],"images":[],"speech":[],
                    "transcriptions":[],"transcription_models":[],"embeddings":[],"effort_mappings":[],
                    "api_key":[{"model":"fixture","upstream":"fixture","endpoint_prefix":"fixture",
                        "dialect":"standard","public_slice":"canonical","endpoint_slice":"canonical",
                        "protocols":["open_ai_chat"],"reasoning_history":[],"effort_mappings":"#,
                $effort,
                r#","provider":""#, $provider, r#"","canonical_model":""#, $canonical, r#"""#,
                $extra, r#"}]}"#
            ).as_bytes()
        };
    }

    #[test]
    fn invalid_embedded_references_fail_before_endpoint_materialization() {
        assert_eq!(
            parse(fixture!("unknown-provider", "fixture", "[]", ""))
                .unwrap()
                .validate(),
            Err(TopologyError::UnknownProvider)
        );
        assert_eq!(
            parse(fixture!("openai", "missing-model", "[]", ""))
                .unwrap()
                .validate(),
            Err(TopologyError::UnknownCanonicalModel)
        );
        assert_eq!(
            parse(fixture!("openai", "fixture", "[]", ""))
                .unwrap()
                .validate(),
            Err(TopologyError::TargetMismatch)
        );
        assert_eq!(
            parse(fixture!(
                "deepseek",
                "fixture",
                r#"[["open_ai_chat","missing-rule"]]"#,
                ""
            ))
            .unwrap()
            .validate(),
            Err(TopologyError::InvalidModelBinding)
        );
    }

    #[test]
    fn openai_bindings_cannot_reference_a_google_only_canonical_model() {
        let mut catalog = parse(fixture!("deepseek", "fixture", "[]", "")).unwrap();
        catalog.google = Box::new([serde_json::from_str(
            r#"{"model":"google-fixture","upstream":"google-fixture",
                    "provider":"google","released_at":1,"developer":"Fixture"}"#,
        )
        .unwrap()]);
        assert_eq!(catalog.validate(), Ok(()));
        catalog.api_key[0].canonical_model = "google-fixture";
        assert_eq!(
            catalog.validate(),
            Err(TopologyError::UnknownCanonicalModel)
        );
    }

    #[test]
    fn valid_public_model_label_must_also_form_valid_endpoint_ids() {
        let mut catalog = parse(fixture!("deepseek", "fixture", "[]", "")).unwrap();
        catalog.google = Box::new([serde_json::from_str(
            r#"{"model":"Google-Fixture","upstream":"google-fixture",
                    "provider":"google","released_at":1,"developer":"Fixture"}"#,
        )
        .unwrap()]);
        assert!(ModelId::new(catalog.google[0].model).is_ok());
        assert_eq!(catalog.validate(), Err(TopologyError::InvalidModelBinding));
    }

    #[test]
    fn code_owned_messages_and_images_contracts_accept_only_their_reviewed_binding() {
        let mut catalog = parse(fixture!("deepseek", "fixture", "[]", "")).unwrap();
        catalog.messages = Box::new([serde_json::from_str(
            r#"{"model":"claude-haiku-5-5","upstream":"claude-haiku-5-5",
                    "provider":"opencode-go","released_at":1,"developer":"Fixture"}"#,
        )
        .unwrap()]);
        catalog.images = Box::new([serde_json::from_str(
            r#"{"model":"gpt-image-2.6-flare",
                    "providers":["openai"],"released_at":1,"developer":"Fixture"}"#,
        )
        .unwrap()]);
        assert_eq!(catalog.validate(), Ok(()));
        let mut other = parse(fixture!("deepseek", "fixture", "[]", "")).unwrap();
        other.messages = Box::new([serde_json::from_str(
            r#"{"model":"claude-sonnet-5-5","upstream":"claude-sonnet-5-5",
                    "provider":"opencode-go","released_at":1,"developer":"Fixture"}"#,
        )
        .unwrap()]);
        assert_eq!(other.validate(), Err(TopologyError::InvalidModelBinding));
        let mut other = parse(fixture!("deepseek", "fixture", "[]", "")).unwrap();
        other.images = Box::new([serde_json::from_str(
            r#"{"model":"gpt-image-2.6-flare",
                    "providers":["aliyun-dashscope-cn"],"released_at":1,"developer":"Fixture"}"#,
        )
        .unwrap()]);
        assert_eq!(other.validate(), Err(TopologyError::InvalidModelBinding));
    }

    #[test]
    fn fixed_speech_codec_voices_cannot_be_redeclared_in_topology() {
        let mut catalog = parse(fixture!("deepseek", "fixture", "[]", "")).unwrap();
        catalog.speech = Box::new([serde_json::from_str(
            r#"{"provider":"xiaomi","identity":"fixture-speech","profile":"xiaomi",
                    "model":"fixture-tts","upstream":"fixture-tts",
                    "released_at":1,"developer":"Fixture"}"#,
        )
        .unwrap()]);
        assert_eq!(catalog.validate(), Ok(()));
        catalog.speech = Box::new([serde_json::from_str(
            r#"{"provider":"xiaomi","identity":"fixture-speech","profile":"xiaomi",
                    "model":"fixture-tts","upstream":"fixture-tts","voices":["fixture-voice"],
                    "released_at":1,"developer":"Fixture"}"#,
        )
        .unwrap()]);
        assert_eq!(catalog.validate(), Err(TopologyError::InvalidModelBinding));
    }

    #[test]
    fn binding_fields_cannot_override_trusted_origins_or_repeat_identity() {
        assert!(matches!(
            parse(fixture!(
                "openai",
                "fixture",
                "[]",
                r#","origin":"https://fixture.invalid""#
            )),
            Err(TopologyError::InvalidCatalog)
        ));
        assert!(matches!(
            parse(fixture!(
                "openai",
                "fixture",
                "[]",
                r#","provider":"openai""#
            )),
            Err(TopologyError::InvalidCatalog)
        ));
    }
}
