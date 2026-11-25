//! Candidate-local reversible tool projection: namespace aliasing and
//! grammar-free custom-text wrapping. Projections never mutate source IR.

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

use crate::{CodecError, ConversionFailure, conversion::generation::RepresentationError};
use morphiecore_semantic::{task::generation::*, value::Text};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OriginalToolTarget {
    pub kind: ToolKind,
    pub namespace: Option<Text>,
    pub name: Text,
    pub wrapped_custom: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ToolDeliveryMap {
    pub entries: BTreeMap<String, OriginalToolTarget>,
}

impl ToolDeliveryMap {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn retained_bytes(&self) -> usize {
        if self.entries.is_empty() {
            return 0;
        }
        std::mem::size_of_val(self)
            + self
                .entries
                .iter()
                .map(|(wire, orig)| {
                    std::mem::size_of::<(String, OriginalToolTarget)>()
                        + wire.capacity()
                        + orig.name.as_str().len()
                        + orig.namespace.as_ref().map_or(0, |ns| ns.as_str().len())
                })
                .sum::<usize>()
    }

    pub fn retain_declared(&mut self, request: &GenerationRequest) {
        if self.entries.is_empty() {
            return;
        }
        let mut declared = BTreeSet::new();
        for (_, tool) in request.tool_declarations() {
            for leaf in tool.leaves() {
                match leaf {
                    ToolDeclaration::Function(t) => {
                        declared.insert((ToolKind::Function, t.name.as_str()));
                    }
                    ToolDeclaration::Custom(t) => {
                        declared.insert((ToolKind::Custom, t.name.as_str()));
                    }
                    ToolDeclaration::Namespace(_) => {}
                }
            }
        }
        self.entries.retain(|wire, orig| {
            let expected_kind = if orig.wrapped_custom {
                ToolKind::Function
            } else {
                orig.kind
            };
            declared.contains(&(expected_kind, wire.as_str()))
        });
    }
}

pub fn deterministic_namespace_alias(namespace: &str, name: &str) -> Result<Text, CodecError> {
    let mut hasher = Sha256::new();
    hasher.update(b"MorphieCore.tool-namespace-alias.v1\0");
    hasher.update((namespace.len() as u64).to_be_bytes());
    hasher.update(namespace.as_bytes());
    hasher.update(b"\0");
    hasher.update((name.len() as u64).to_be_bytes());
    hasher.update(name.as_bytes());
    let digest = hasher.finalize();
    let hex12: String = digest[..6].iter().map(|b| format!("{b:02x}")).collect();
    let hex16: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();

    let mut suffix = String::new();
    for ch in namespace
        .chars()
        .chain(std::iter::once('_'))
        .chain(name.chars())
    {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            suffix.push(ch);
        } else {
            suffix.push('_');
        }
    }
    let trimmed = suffix.trim_matches('_');
    let raw = if trimmed.is_empty() {
        format!("mcns_{hex16}")
    } else {
        let prefix = format!("mcns_{hex12}_");
        let max_len = 64;
        let mut combined = format!("{prefix}{trimmed}");
        if combined.len() > max_len {
            let mut end = max_len;
            while !combined.is_char_boundary(end) {
                end -= 1;
            }
            combined.truncate(end);
        }
        combined
    };
    Text::new(&raw, "tool alias", 64).map_err(|_| CodecError::invalid())
}

fn prepend_namespace_description(
    group_name: &str,
    group_description: &str,
    leaf_description: Option<&str>,
) -> Result<String, CodecError> {
    let header = if group_description.trim().is_empty() {
        format!("[namespace: {group_name}]")
    } else {
        format!("[namespace: {group_name}] {group_description}")
    };
    let combined = match leaf_description {
        Some(desc) if !desc.is_empty() => format!("{header}\n\n{desc}"),
        _ => header,
    };
    if combined.len() > MAX_TEXT_BYTES {
        return Err(CodecError::Limit);
    }
    Ok(combined)
}

fn needs_namespace_aliasing(request: &GenerationRequest) -> bool {
    request
        .tool_declarations()
        .any(|(_, decl)| matches!(decl, ToolDeclaration::Namespace(_)))
        || match request.tool_choice() {
            Some(ToolChoice::Qualified(_)) => true,
            Some(ToolChoice::Allowed { tools, .. }) => tools.iter().any(|r| r.namespace.is_some()),
            _ => false,
        }
        || request.items().iter().any(|(_, item)| match item {
            Item::ToolCall(c) => c.context.namespace.is_some(),
            Item::CustomCall(c) => c.context.namespace.is_some(),
            Item::ToolResult(r) | Item::CustomResult(r) => r.context.namespace.is_some(),
            _ => false,
        })
}

fn rewrite_result_ref(
    result: &mut ToolResult,
    kind: ToolKind,
    call_aliases: &BTreeMap<String, Text>,
    forward: &BTreeMap<(String, ToolKind, String), Text>,
) {
    if let Some(call_id) = &result.call_id
        && let Some(alias) = call_aliases.get(call_id.as_str())
    {
        result.context.namespace = None;
        if result.name.is_some() {
            result.name = Some(alias.clone());
        }
    } else if let (Some(ns), Some(name)) = (&result.context.namespace, &result.name)
        && let Some(alias) = forward.get(&(ns.as_str().to_owned(), kind, name.as_str().to_owned()))
    {
        result.context.namespace = None;
        result.name = Some(alias.clone());
    } else {
        result.context.namespace = None;
    }
}

fn leaf_kind_name(leaf: &ToolDefinition) -> Option<(ToolKind, &str)> {
    match leaf {
        ToolDefinition::Function(t) => Some((ToolKind::Function, t.name.as_str())),
        ToolDefinition::Custom(t) => Some((ToolKind::Custom, t.name.as_str())),
        ToolDefinition::Namespace(_) => None,
    }
}

fn rebuild_candidate_request(
    semantic: &GenerationRequest,
    items: Vec<(morphiecore_semantic::task::generation::ItemId, Item)>,
    settings: GenerationSettings,
) -> Result<GenerationRequest, CodecError> {
    GenerationRequest::from_resources_and_envelopes(
        items,
        settings,
        semantic.resources().clone(),
        semantic.message_envelopes().to_vec(),
    )?
    .with_cached_context(semantic.cached_context().cloned())
    .map_err(CodecError::from)
}

/// Flatten all `ToolNamespace` declarations into collision-checked deterministic
/// leaf aliases on a candidate copy, rewriting choices and history calls/results.
pub fn alias_tool_namespaces<'a>(
    semantic: Cow<'a, GenerationRequest>,
) -> Result<(Cow<'a, GenerationRequest>, ToolDeliveryMap, bool), ConversionFailure> {
    if !needs_namespace_aliasing(semantic.as_ref()) {
        return Ok((semantic, ToolDeliveryMap::default(), false));
    }

    let mut plain_names: BTreeSet<(ToolKind, String)> = BTreeSet::new();
    let mut all_plain_strings: BTreeSet<String> = BTreeSet::new();
    let mut record_plain = |kind: ToolKind, name: &str| {
        plain_names.insert((kind, name.to_owned()));
        all_plain_strings.insert(name.to_owned());
    };

    for (_, decl) in semantic.tool_declarations() {
        match decl {
            ToolDeclaration::Function(t) => record_plain(ToolKind::Function, t.name.as_str()),
            ToolDeclaration::Custom(t) => record_plain(ToolKind::Custom, t.name.as_str()),
            ToolDeclaration::Namespace(_) => {}
        }
    }
    match semantic.tool_choice() {
        Some(ToolChoice::Specific(name)) => record_plain(ToolKind::Function, name.as_str()),
        Some(ToolChoice::Custom(name)) => record_plain(ToolKind::Custom, name.as_str()),
        Some(ToolChoice::Allowed { tools, .. }) => {
            for r in tools {
                if r.namespace.is_none() {
                    record_plain(r.kind, r.name.as_str());
                }
            }
        }
        _ => {}
    }

    type AliasKey = (String, ToolKind, String);
    let mut forward: BTreeMap<AliasKey, Text> = BTreeMap::new();
    let mut delivery = ToolDeliveryMap::default();
    let mut reverse_check: BTreeMap<String, AliasKey> = BTreeMap::new();

    let mut register_leaf =
        |ns: &str, kind: ToolKind, leaf: &str| -> Result<Text, ConversionFailure> {
            let key = (ns.to_owned(), kind, leaf.to_owned());
            if let Some(existing) = forward.get(&key) {
                return Ok(existing.clone());
            }
            let alias = deterministic_namespace_alias(ns, leaf)?;
            let alias_str = alias.as_str().to_owned();
            if all_plain_strings.contains(&alias_str)
                || reverse_check
                    .get(&alias_str)
                    .is_some_and(|other| other != &key)
            {
                return Err(RepresentationError::Admission(Feature::Tools).into());
            }
            reverse_check.insert(alias_str.clone(), key.clone());
            forward.insert(key, alias.clone());
            delivery.entries.insert(
                alias_str,
                OriginalToolTarget {
                    kind,
                    namespace: Some(
                        Text::new(ns, "tool namespace", 128).map_err(|_| CodecError::invalid())?,
                    ),
                    name: Text::new(leaf, "tool name", 128).map_err(|_| CodecError::invalid())?,
                    wrapped_custom: false,
                },
            );
            Ok(alias)
        };

    // Register all declared namespace leaves first.
    for (_, decl) in semantic.tool_declarations() {
        if let ToolDeclaration::Namespace(group) = decl {
            for leaf in &group.tools {
                if let Some((kind, name)) = leaf_kind_name(leaf) {
                    register_leaf(group.name.as_str(), kind, name)?;
                }
            }
        }
    }

    // Also register any namespace-qualified choice or historical call so orphan
    // references are mapped consistently (or remain rejected if undeclared).
    let mut register_ref = |r: &ToolReference| -> Result<(), ConversionFailure> {
        if let Some(ns) = &r.namespace {
            register_leaf(ns.as_str(), r.kind, r.name.as_str())?;
        }
        Ok(())
    };
    match semantic.tool_choice() {
        Some(ToolChoice::Qualified(r)) => register_ref(r)?,
        Some(ToolChoice::Allowed { tools, .. }) => {
            for r in tools {
                register_ref(r)?;
            }
        }
        _ => {}
    }
    for (_, item) in semantic.items() {
        match item {
            Item::ToolCall(c) => {
                if let Some(ns) = &c.context.namespace {
                    register_leaf(ns.as_str(), ToolKind::Function, c.name.as_str())?;
                }
            }
            Item::CustomCall(c) => {
                if let Some(ns) = &c.context.namespace {
                    register_leaf(ns.as_str(), ToolKind::Custom, c.name.as_str())?;
                }
            }
            Item::ToolResult(r) => {
                if let (Some(ns), Some(name)) = (&r.context.namespace, &r.name) {
                    register_leaf(ns.as_str(), ToolKind::Function, name.as_str())?;
                }
            }
            Item::CustomResult(r) => {
                if let (Some(ns), Some(name)) = (&r.context.namespace, &r.name) {
                    register_leaf(ns.as_str(), ToolKind::Custom, name.as_str())?;
                }
            }
            _ => {}
        }
    }

    let flatten_list = |tools: &[ToolDefinition]| -> Result<Vec<ToolDefinition>, CodecError> {
        let mut out = Vec::new();
        for tool in tools {
            match tool {
                ToolDefinition::Namespace(group) => {
                    for leaf in &group.tools {
                        let (kind, name) = leaf_kind_name(leaf).ok_or(CodecError::invalid())?;
                        let alias = forward
                            .get(&(group.name.as_str().to_owned(), kind, name.to_owned()))
                            .cloned()
                            .ok_or(CodecError::invalid())?;
                        match leaf {
                            ToolDefinition::Function(f) => {
                                let mut copy = f.clone();
                                copy.name = alias;
                                copy.description = Some(prepend_namespace_description(
                                    group.name.as_str(),
                                    &group.description,
                                    f.description.as_deref(),
                                )?);
                                out.push(ToolDefinition::Function(copy));
                            }
                            ToolDefinition::Custom(c) => {
                                let mut copy = c.clone();
                                copy.name = alias;
                                copy.description = Some(prepend_namespace_description(
                                    group.name.as_str(),
                                    &group.description,
                                    c.description.as_deref(),
                                )?);
                                out.push(ToolDefinition::Custom(copy));
                            }
                            ToolDefinition::Namespace(_) => return Err(CodecError::invalid()),
                        }
                    }
                }
                other => out.push(other.clone()),
            }
        }
        if out.len() > MAX_TOOLS {
            return Err(CodecError::Limit);
        }
        Ok(out)
    };

    let mut settings = semantic.settings().clone();
    if let Some(tools) = &settings.tools {
        settings.tools = Some(flatten_list(tools)?);
    }
    let map_ref = |r: &ToolReference| -> ToolReference {
        if let Some(ns) = &r.namespace
            && let Some(alias) =
                forward.get(&(ns.as_str().to_owned(), r.kind, r.name.as_str().to_owned()))
        {
            ToolReference {
                kind: r.kind,
                name: alias.clone(),
                namespace: None,
            }
        } else {
            r.clone()
        }
    };
    settings.tool_choice = settings.tool_choice.as_ref().map(|choice| match choice {
        ToolChoice::Qualified(r) => {
            let mapped = map_ref(r);
            match mapped.kind {
                ToolKind::Function => ToolChoice::Specific(mapped.name),
                ToolKind::Custom => ToolChoice::Custom(mapped.name),
            }
        }
        ToolChoice::Allowed { required, tools } => ToolChoice::Allowed {
            required: *required,
            tools: tools.iter().map(map_ref).collect(),
        },
        other => other.clone(),
    });

    let mut items = semantic.items().to_vec();
    let mut call_aliases: BTreeMap<String, Text> = BTreeMap::new();
    for (_, item) in &mut items {
        match item {
            Item::ToolBatch(batch) => {
                batch.tools = flatten_list(&batch.tools)?;
            }
            Item::ToolCall(call) => {
                let ns = call
                    .context
                    .namespace
                    .as_ref()
                    .map(|n| n.as_str())
                    .or_else(|| {
                        (!plain_names
                            .contains(&(ToolKind::Function, call.name.as_str().to_owned()))
                            && forward.contains_key(&(
                                "functions".to_owned(),
                                ToolKind::Function,
                                call.name.as_str().to_owned(),
                            )))
                        .then_some("functions")
                    });
                if let Some(ns) = ns
                    && let Some(alias) = forward.get(&(
                        ns.to_owned(),
                        ToolKind::Function,
                        call.name.as_str().to_owned(),
                    ))
                {
                    call.name = alias.clone();
                    call.context.namespace = None;
                    call_aliases.insert(call.call_id.as_str().to_owned(), alias.clone());
                }
            }
            Item::CustomCall(call) => {
                let ns = call
                    .context
                    .namespace
                    .as_ref()
                    .map(|n| n.as_str())
                    .or_else(|| {
                        (!plain_names.contains(&(ToolKind::Custom, call.name.as_str().to_owned()))
                            && forward.contains_key(&(
                                "functions".to_owned(),
                                ToolKind::Custom,
                                call.name.as_str().to_owned(),
                            )))
                        .then_some("functions")
                    });
                if let Some(ns) = ns
                    && let Some(alias) = forward.get(&(
                        ns.to_owned(),
                        ToolKind::Custom,
                        call.name.as_str().to_owned(),
                    ))
                {
                    call.name = alias.clone();
                    call.context.namespace = None;
                    call_aliases.insert(call.call_id.as_str().to_owned(), alias.clone());
                }
            }
            Item::ToolResult(result) => {
                rewrite_result_ref(result, ToolKind::Function, &call_aliases, &forward);
            }
            Item::CustomResult(result) => {
                rewrite_result_ref(result, ToolKind::Custom, &call_aliases, &forward);
            }
            _ => {}
        }
    }

    let projected = rebuild_candidate_request(semantic.as_ref(), items, settings)?;
    Ok((Cow::Owned(projected), delivery, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ConversionCause;
    use morphiecore_semantic::task::generation::{
        CallContext, ContentPart, FunctionStrictness, GenerationSettings, Item, ItemId, Message,
        MessageRole, Part, PartId, StrictDefault, TextContent, ToolArguments, ToolBatch,
        ToolBatchRole, ToolCall, ToolNamespace, ToolOutput, ToolResult,
    };
    use serde_json::json;

    fn text(s: &str) -> Text {
        Text::new(s, "test text", 128).unwrap()
    }

    fn fn_tool(name: &str, desc: Option<&str>) -> ToolDefinition {
        ToolDefinition::Function(FunctionTool {
            name: text(name),
            description: desc.map(String::from),
            parameters: None,
            input_examples: None,
            strict: FunctionStrictness::Omitted(StrictDefault::NonStrict),
            output_schema: None,
            dispatch: Default::default(),
            cache: None,
            eager_input: None,
        })
    }

    fn custom_tool(name: &str, desc: Option<&str>) -> ToolDefinition {
        ToolDefinition::Custom(CustomTool {
            name: text(name),
            description: desc.map(String::from),
            format: None,
            dispatch: Default::default(),
        })
    }

    fn ns_group(name: &str, desc: &str, tools: Vec<ToolDefinition>) -> ToolDefinition {
        ToolDefinition::Namespace(ToolNamespace {
            name: text(name),
            description: desc.to_owned(),
            tools,
        })
    }

    fn base_request(tools: Vec<ToolDefinition>, items: Vec<(ItemId, Item)>) -> GenerationRequest {
        let settings = GenerationSettings {
            tools: (!tools.is_empty()).then_some(tools),
            ..Default::default()
        };
        let mut all_items = vec![(
            ItemId::new(0),
            Item::Message(Message {
                role: MessageRole::User,
                participant: None,
                phase: None,
                status: ItemLifecycle::Completed,
                parts: vec![Part::new(
                    PartId::new(0),
                    ContentPart::Text(TextContent::new(text("go"), vec![], None).unwrap()),
                )],
            }),
        )];
        all_items.extend(items);
        GenerationRequest::from_settings(all_items, settings).unwrap()
    }

    #[test]
    fn deterministic_alias_is_stable_across_reordering_addition_and_removal() {
        let exec_a = deterministic_namespace_alias("functions", "exec").unwrap();
        let exec_b = deterministic_namespace_alias("functions", "exec").unwrap();
        assert_eq!(exec_a, exec_b);
        assert!(exec_a.as_str().len() <= 64);
        assert_ne!(
            exec_a,
            deterministic_namespace_alias("other", "exec").unwrap()
        );

        let req1 = base_request(
            vec![
                ns_group("functions", "", vec![fn_tool("exec", Some("run"))]),
                fn_tool("plain", None),
            ],
            vec![],
        );
        let req2 = base_request(
            vec![
                fn_tool("extra", None),
                fn_tool("plain", None),
                ns_group(
                    "functions",
                    "",
                    vec![fn_tool("helper", None), fn_tool("exec", Some("run"))],
                ),
            ],
            vec![],
        );
        let (proj1, map1, changed1) = alias_tool_namespaces(Cow::Borrowed(&req1)).unwrap();
        let (_proj2, map2, changed2) = alias_tool_namespaces(Cow::Borrowed(&req2)).unwrap();
        assert!(changed1 && changed2);
        let alias1 = map1
            .entries
            .iter()
            .find(|(_, v)| v.name.as_str() == "exec")
            .unwrap()
            .0
            .clone();
        let alias2 = map2
            .entries
            .iter()
            .find(|(_, v)| v.name.as_str() == "exec")
            .unwrap()
            .0
            .clone();
        assert_eq!(alias1, alias2);
        assert_eq!(alias1, exec_a.as_str());
        assert_ne!(proj1.as_ref(), &req1);
    }

    #[test]
    fn duplicate_leaf_names_and_qualified_or_allowed_choices_map_cleanly() {
        let mut req = base_request(
            vec![
                ns_group("left", "left ops", vec![fn_tool("status", None)]),
                ns_group("right", "right ops", vec![fn_tool("status", None)]),
            ],
            vec![
                (
                    ItemId::new(1),
                    Item::ToolCall(ToolCall {
                        call_id: text("c1"),
                        name: text("status"),
                        arguments: ToolArguments::Raw(serde_json::to_string(&json!({})).unwrap()),
                        status: ItemLifecycle::Completed,
                        context: CallContext {
                            namespace: Some(text("right")),
                            ..Default::default()
                        },
                        cache: None,
                        replay: None,
                    }),
                ),
                (
                    ItemId::new(2),
                    Item::ToolResult(ToolResult {
                        call_id: Some(text("c1")),
                        name: Some(text("status")),
                        output: ToolOutput::Text(String::new()),
                        is_error: Some(false),
                        execution: None,
                        status: None,
                        context: CallContext {
                            namespace: Some(text("right")),
                            ..Default::default()
                        },
                        cache: None,
                    }),
                ),
            ],
        );
        let mut settings = req.settings().clone();
        settings.tool_choice = Some(ToolChoice::Qualified(ToolReference {
            kind: ToolKind::Function,
            name: text("status"),
            namespace: Some(text("right")),
        }));
        req = req.with_settings(settings).unwrap();

        let (projected, map, changed) = alias_tool_namespaces(Cow::Borrowed(&req)).unwrap();
        assert!(changed);
        let right_alias = map
            .entries
            .iter()
            .find(|(_, v)| v.namespace.as_ref().map(Text::as_str) == Some("right"))
            .unwrap()
            .0
            .clone();
        let left_alias = map
            .entries
            .iter()
            .find(|(_, v)| v.namespace.as_ref().map(Text::as_str) == Some("left"))
            .unwrap()
            .0
            .clone();
        assert_ne!(right_alias, left_alias);
        assert_eq!(
            projected.tool_choice(),
            Some(&ToolChoice::Specific(text(&right_alias)))
        );
        match &projected.items()[1].1 {
            Item::ToolCall(call) => {
                assert_eq!(call.name.as_str(), right_alias);
                assert_eq!(call.context.namespace, None);
            }
            other => panic!("unexpected item {other:?}"),
        }
        match &projected.items()[2].1 {
            Item::ToolResult(result) => {
                assert_eq!(
                    result.name.as_ref().map(|t| t.as_str()),
                    Some(right_alias.as_str())
                );
                assert_eq!(result.context.namespace, None);
            }
            other => panic!("unexpected item {other:?}"),
        }
    }

    #[test]
    fn alias_collisions_with_plain_or_mixed_kind_reject_cleanly() {
        let alias = deterministic_namespace_alias("ops", "status").unwrap();
        let colliding_plain = base_request(
            vec![
                fn_tool(alias.as_str(), None),
                ns_group("ops", "ops", vec![fn_tool("status", None)]),
            ],
            vec![],
        );
        assert!(matches!(
            alias_tool_namespaces(Cow::Borrowed(&colliding_plain))
                .unwrap_err()
                .cause,
            ConversionCause::Representation(RepresentationError::Admission(Feature::Tools))
        ));

        let colliding_kinds = base_request(
            vec![ns_group(
                "ops",
                "ops",
                vec![fn_tool("status", None), custom_tool("status", None)],
            )],
            vec![],
        );
        assert!(matches!(
            alias_tool_namespaces(Cow::Borrowed(&colliding_kinds))
                .unwrap_err()
                .cause,
            ConversionCause::Representation(RepresentationError::Admission(Feature::Tools))
        ));
    }

    #[test]
    fn unqualified_default_functions_call_rewrites_only_without_plain_shadow() {
        let req_rewrites = base_request(
            vec![ns_group("functions", "", vec![fn_tool("exec", None)])],
            vec![(
                ItemId::new(1),
                Item::ToolCall(ToolCall {
                    call_id: text("c1"),
                    name: text("exec"),
                    arguments: ToolArguments::Raw("{}".into()),
                    status: ItemLifecycle::Completed,
                    context: CallContext::default(),
                    cache: None,
                    replay: None,
                }),
            )],
        );
        let (proj, map, changed) = alias_tool_namespaces(Cow::Borrowed(&req_rewrites)).unwrap();
        assert!(changed);
        let alias = map.entries.keys().next().unwrap();
        let Item::ToolCall(call) = &proj.items()[1].1 else {
            panic!("expected tool call");
        };
        assert_eq!(call.name.as_str(), alias);

        let req_shadowed = base_request(
            vec![
                fn_tool("exec", None),
                ns_group("functions", "", vec![fn_tool("exec", None)]),
            ],
            vec![(
                ItemId::new(1),
                Item::ToolCall(ToolCall {
                    call_id: text("c1"),
                    name: text("exec"),
                    arguments: ToolArguments::Raw("{}".into()),
                    status: ItemLifecycle::Completed,
                    context: CallContext::default(),
                    cache: None,
                    replay: None,
                }),
            )],
        );
        let (proj_shadowed, _, _) = alias_tool_namespaces(Cow::Borrowed(&req_shadowed)).unwrap();
        let Item::ToolCall(call_shadowed) = &proj_shadowed.items()[1].1 else {
            panic!("expected tool call");
        };
        assert_eq!(call_shadowed.name.as_str(), "exec");

        let mut req_batch = base_request(
            vec![ns_group("functions", "", vec![fn_tool("exec", None)])],
            vec![(
                ItemId::new(1),
                Item::ToolBatch(ToolBatch {
                    role: ToolBatchRole::Developer,
                    tools: vec![ns_group("clock", "", vec![fn_tool("now", None)])],
                }),
            )],
        );
        let mut settings = req_batch.settings().clone();
        settings.tool_choice = Some(ToolChoice::Allowed {
            required: false,
            tools: vec![ToolReference {
                kind: ToolKind::Function,
                name: text("now"),
                namespace: Some(text("clock")),
            }],
        });
        req_batch = req_batch.with_settings(settings).unwrap();
        let (proj_batch, batch_map, _) = alias_tool_namespaces(Cow::Borrowed(&req_batch)).unwrap();
        assert_eq!(batch_map.entries.len(), 2);
        assert!(
            !proj_batch
                .tool_declarations()
                .any(|(_, d)| matches!(d, ToolDeclaration::Namespace(_)))
        );
    }
}
