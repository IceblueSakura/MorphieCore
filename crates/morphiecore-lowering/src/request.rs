//! Candidate-local semantic request transformations. Carrier adapters interleave
//! their own validation and envelope operations without entering this layer.
use std::borrow::Cow;

use crate::{
    CodecError, ConversionFailure,
    capabilities::TargetCapabilities,
    conversion::generation::{ConversionBoundary, EvidenceDelta, Owner},
    disposition::{Disposition, Dispositions},
    failure::CarrierReason,
    reasoning::ReasoningRequestPolicy,
};
use morphiecore_semantic::{task::generation::*, value::Presence};

pub fn omit_option<T>(
    value: &mut Option<T>,
    feature: Feature,
    table: &Dispositions,
    evidence: &mut EvidenceDelta,
) {
    if let Disposition::Omit(rule) = table.get(feature)
        && value.take().is_some()
    {
        evidence.push(rule.effect(ConversionBoundary::RequestProject));
    }
}
pub fn apply_option<T>(
    value: &mut Option<T>,
    feature: Feature,
    table: &Dispositions,
    evidence: &mut EvidenceDelta,
) -> Result<(), ConversionFailure> {
    if value.is_none() {
        return Ok(());
    }
    match table.get(feature) {
        Disposition::Omit(rule) => {
            *value = None;
            evidence.push(rule.effect(ConversionBoundary::RequestProject));
        }
        Disposition::Equivalent => *value = None,
        Disposition::Reject(_) => {
            return Err(
                crate::conversion::generation::RepresentationError::Admission(feature).into(),
            );
        }
        _ => {}
    }
    Ok(())
}
pub fn apply_presence<T>(
    value: &mut Presence<T>,
    feature: Feature,
    table: &Dispositions,
    evidence: &mut EvidenceDelta,
) -> Result<(), ConversionFailure> {
    if value.is_absent() {
        return Ok(());
    }
    match table.get(feature) {
        Disposition::Omit(rule) => {
            if value.value().is_some() {
                evidence.push(rule.effect(ConversionBoundary::RequestProject));
            }
            *value = Presence::Absent;
        }
        Disposition::Equivalent => *value = Presence::Absent,
        Disposition::Reject(_) => {
            return Err(
                crate::conversion::generation::RepresentationError::Admission(feature).into(),
            );
        }
        _ => {}
    }
    Ok(())
}

fn ordered_tool_declarations(items: &[(ItemId, Item)]) -> bool {
    items
        .iter()
        .any(|(_, item)| matches!(item, Item::ToolBatch(_) | Item::ToolChange(_)))
}

pub fn collapse_configuration<'a>(
    source: &'a GenerationRequest,
    original: &GenerationRequest,
    semantic: Cow<'a, GenerationRequest>,
    evidence: &mut EvidenceDelta,
) -> Result<Cow<'a, GenerationRequest>, CodecError> {
    // Carrier-dependent history omission cannot redefine source timing.
    // Even a native marker must collapse if its activation interval changed.
    let resolved = original.resolved_configuration().active;
    let mut settings = source.settings().clone();
    settings.effort = resolved.effort;
    settings.reasoning.activation = resolved.activation;
    if resolved.activation.is_some() || resolved.effort.is_some() {
        settings.reasoning.presence = ReasoningPresence::Present;
    }
    if resolved.activation == Some(ReasoningActivation::Disabled) {
        settings.reasoning.mode = Presence::Absent;
        settings.reasoning.budget_tokens = None;
        if settings.reasoning.presentation.visibility == Some(ReasoningDisplay::Summary) {
            settings.reasoning.presentation = ReasoningPresentation::default();
        }
    }
    let items = source
        .items()
        .iter()
        .filter(|(_, item)| !matches!(item, Item::ConfigurationUpdate(_)))
        .cloned()
        .collect();
    let semantic = Cow::Owned(
        semantic
            .into_owned()
            .with_items_and_settings(items, settings)
            .map_err(CodecError::from)?,
    );
    evidence.push(super::disposition::CONFIGURATION.effect(ConversionBoundary::RequestProject));
    evidence
        .push(super::disposition::CONFIGURATION_PREFIX.effect(ConversionBoundary::RequestProject));
    Ok(semantic)
}

pub struct ControlProjection<'a, 'p> {
    pub value: Cow<'a, GenerationRequest>,
    pub evidence: EvidenceDelta,
    pub reasoning: ReasoningReceipts<'p>,
}

pub struct ReasoningReceipts<'p> {
    policy: Option<&'p ReasoningRequestPolicy>,
    effort_rule: Option<&'static str>,
}

pub fn project_controls<'a, 'p>(
    mut semantic: Cow<'a, GenerationRequest>,
    source: &GenerationRequest,
    target: TargetCapabilities<'p>,
    table: &mut Dispositions,
    mut evidence: EvidenceDelta,
) -> Result<ControlProjection<'a, 'p>, ConversionFailure> {
    let mut initial = semantic.settings().clone();
    if !table.accepts(Feature::ReasoningActivation)
        && semantic
            .features()
            .contains_key(&Feature::ReasoningActivation)
    {
        return Err(
            crate::conversion::generation::RepresentationError::Admission(
                Feature::ReasoningActivation,
            )
            .into(),
        );
    }
    if initial.reasoning.replay_mismatch.is_some() && !table.accepts(Feature::ReplayMismatch) {
        return Err(
            crate::conversion::generation::RepresentationError::Admission(Feature::ReplayMismatch)
                .into(),
        );
    }
    omit_option(
        &mut initial.reasoning.replay_mismatch,
        Feature::ReplayMismatch,
        table,
        &mut evidence,
    );
    if initial.reasoning.budget_tokens.is_some()
        && initial.effort.is_some()
        && (!target.adaptive_budget || !target.budget_fits(&initial))
    {
        initial.reasoning.budget_tokens = None;
        if initial.reasoning.mode.value() == Some(&ReasoningMode::Budgeted) {
            initial.reasoning.mode = if target.adaptive_budget {
                Presence::Value(ReasoningMode::Adaptive)
            } else {
                Presence::Absent
            };
        }
        evidence.push(
            super::disposition::BUDGET_WITH_EFFORT.effect(ConversionBoundary::RequestProject),
        );
    }
    if initial.reasoning.activation.as_ref() == Some(&ReasoningActivation::Disabled)
        && initial.effort.is_some()
        && target.effort_scope() == EffortScope::Reasoning
    {
        initial.effort = None;
        evidence.push(
            super::disposition::unsupported(Feature::Effort)
                .rule()
                .expect("effort omission rule")
                .effect(ConversionBoundary::RequestProject),
        );
    }
    if &initial != semantic.settings() {
        semantic = Cow::Owned(
            semantic
                .into_owned()
                .with_settings(initial)
                .map_err(CodecError::from)?,
        );
    }
    let mut applied_policy = None;
    if let Some(policy) = target.policy()
        && let Some(copy) = policy.project_request(&semantic, |s| {
            if s.reasoning.budget_tokens.is_some() {
                target.budget_fits(s)
                    || s.effort.is_some()
                        && matches!(
                            policy.conversion,
                            crate::reasoning::ReasoningConversion::BudgetToEffort { .. }
                        )
            } else {
                s.effort
                    .as_ref()
                    .is_some_and(|e| target.has_effort() && e.scope == target.effort_scope())
            }
        })?
    {
        applied_policy = Some(policy);
        semantic = Cow::Owned(copy);
    }
    let mut settings = semantic.settings().clone();
    if target.named_tool_subsets && settings.parallel_tool_calls != Some(false) {
        let reference = match &settings.tool_choice {
            Some(ToolChoice::Specific(name)) => Some((ToolKind::Function, name)),
            Some(ToolChoice::Custom(name)) => Some((ToolKind::Custom, name)),
            _ => None,
        };
        if let Some((kind, name)) = reference {
            // Resolve the one-or-more carrier before target admission. Writers
            // must not introduce an unchecked allowed-subset spelling.
            settings.tool_choice = Some(ToolChoice::Allowed {
                required: true,
                tools: vec![ToolReference {
                    kind,
                    name: name.clone(),
                    namespace: None,
                }],
            });
            if table.get(Feature::ToolChoice) == Disposition::Native {
                table.set(Feature::ToolChoice, Disposition::Equivalent);
            }
            evidence.push(
                super::disposition::NAMED_TOOL_SUBSET.effect(ConversionBoundary::RequestProject),
            );
        }
    }
    // Selecting the entire inventory needs no subset carrier or visibility loss.
    if target.whole_inventory_choice
        && let Some(ToolChoice::Allowed { required, tools }) = &settings.tool_choice
        && semantic.available_tool_count() == Ok(tools.len())
        && tools
            .iter()
            .all(|r| r.kind == ToolKind::Function && r.namespace.is_none())
    {
        settings.tool_choice = Some(if *required {
            ToolChoice::Required
        } else {
            ToolChoice::Auto
        });
        evidence.push(
            super::disposition::FULL_TOOL_INVENTORY_CHOICE
                .effect(ConversionBoundary::RequestProject),
        );
    }
    if target.omit_explicit_text_format && settings.text.format == Some(OutputConstraint::Text) {
        settings.text.format = None;
    }
    if table.get(Feature::ReasoningMode) == Disposition::Equivalent {
        settings.reasoning.mode = Presence::Absent;
    }
    if target.adaptive_budget
        && !target.has_budget()
        && source.reasoning().budget_tokens.is_some()
        && settings.reasoning.budget_tokens.is_none()
    {
        settings.reasoning.mode = Presence::Value(ReasoningMode::Adaptive);
    }
    let original_controls = settings.controls.clone();
    let mut controls = original_controls
        .clone()
        .without_sampling()
        .without_penalties();
    for (feature, value) in [
        (Feature::Temperature, original_controls.temperature()),
        (Feature::TopP, original_controls.top_p()),
        (
            Feature::PresencePenalty,
            original_controls.presence_penalty(),
        ),
        (
            Feature::FrequencyPenalty,
            original_controls.frequency_penalty(),
        ),
    ] {
        if let Some(value) = value {
            if let Disposition::Omit(rule) = table.get(feature) {
                evidence.push(rule.effect(ConversionBoundary::RequestProject));
            } else {
                controls = match feature {
                    Feature::Temperature => controls.with_temperature(value),
                    Feature::TopP => controls.with_top_p(value),
                    Feature::PresencePenalty => controls.with_presence_penalty(value),
                    Feature::FrequencyPenalty => controls.with_frequency_penalty(value),
                    _ => unreachable!(),
                }
                .map_err(CodecError::from)?;
            }
        }
    }
    omit_option(&mut controls.top_k, Feature::TopK, table, &mut evidence);
    omit_option(&mut controls.seed, Feature::Seed, table, &mut evidence);
    if controls.logprobs == Some(false)
        && controls.top_logprobs.is_none()
        && matches!(table.get(Feature::Logprobs), Disposition::Omit(_))
    {
        controls.logprobs = None;
    }
    omit_option(
        &mut controls.logprobs,
        Feature::Logprobs,
        table,
        &mut evidence,
    );
    if matches!(table.get(Feature::Logprobs), Disposition::Omit(_))
        && controls.top_logprobs.take().is_some()
        && let Some(rule) = table.get(Feature::Logprobs).rule()
    {
        evidence.push(rule.effect(ConversionBoundary::RequestProject));
    }
    omit_option(
        &mut controls.truncation,
        Feature::Truncation,
        table,
        &mut evidence,
    );
    omit_option(
        &mut settings.text.verbosity,
        Feature::Verbosity,
        table,
        &mut evidence,
    );
    omit_option(
        &mut settings.prediction,
        Feature::Prediction,
        table,
        &mut evidence,
    );
    omit_option(
        &mut settings.automatic_prefix_cache,
        Feature::AutomaticPrefixCache,
        table,
        &mut evidence,
    );
    if settings.parallel_tool_calls == Some(true) {
        match table.get(Feature::ParallelTools) {
            Disposition::Equivalent => settings.parallel_tool_calls = None,
            Disposition::Omit(rule) => {
                settings.parallel_tool_calls = None;
                evidence.push(rule.effect(ConversionBoundary::RequestProject));
            }
            _ => {}
        }
    }
    if settings.parallel_tool_calls == Some(false) && !table.accepts(Feature::SerialTools) {
        return Err(crate::conversion::generation::RepresentationError::Tools.into());
    }
    if settings.reasoning.presence == ReasoningPresence::Null {
        settings.reasoning.presence = ReasoningPresence::Absent;
    }
    if !target.has_budget() && settings.reasoning.budget_tokens.is_some() {
        let budget = settings
            .reasoning
            .budget_tokens
            .take()
            .expect("present budget");
        let rule = if settings.effort.as_ref().is_some() {
            super::disposition::BUDGET_WITH_EFFORT
        } else {
            super::disposition::BUDGET_TO_EFFORT
        };
        if settings.effort.is_none() && target.has_effort() {
            let (level, _) = crate::reasoning::SHARED_BUDGETS.nearest(budget)?;
            settings.effort = Some(EffortControl {
                scope: target.effort_scope(),
                level,
            });
        }
        settings.reasoning.mode = if target.adaptive_budget {
            Presence::Value(ReasoningMode::Adaptive)
        } else {
            Presence::Absent
        };
        evidence.push(rule.effect(ConversionBoundary::RequestProject));
    }
    let effort_levels = std::cell::LazyCell::new(|| target.levels());
    let mut effort_rule = None;
    if let Some(effort) = settings.effort {
        if target.has_effort() {
            if target.omit_unavailable_allocation_mode
                && !target.has_budget()
                && matches!(
                    settings.reasoning.mode.value(),
                    Some(ReasoningMode::Adaptive | ReasoningMode::Budgeted)
                )
            {
                settings.reasoning.mode = Presence::Absent;
                effort_rule = Some(super::disposition::EFFORT.id);
            }
            let (mapped, rule) = target.map_effort(effort, &effort_levels)?;
            settings.effort = Some(mapped);
            if mapped != effort {
                effort_rule = Some(rule);
            }
        } else {
            settings.effort = None;
            if let Some(rule) = table.get(Feature::Effort).rule() {
                evidence.push(rule.effect(ConversionBoundary::RequestProject));
            }
        }
    }
    apply_option(
        &mut settings.reasoning.activation,
        Feature::ReasoningActivation,
        table,
        &mut evidence,
    )?;
    apply_presence(
        &mut settings.reasoning.mode,
        Feature::ReasoningMode,
        table,
        &mut evidence,
    )?;
    // Allocation conversion may change whether a native summarizer is active.
    target.presentation_rows(&settings, table);
    apply_option(
        &mut settings.reasoning.presentation.visibility,
        Feature::ReasoningDisplay,
        table,
        &mut evidence,
    )?;
    if let Disposition::Approximate(rule) = table.get(Feature::ReasoningSummary) {
        settings.reasoning.presentation.summary = None;
        evidence.push(rule.effect(ConversionBoundary::RequestProject));
    } else {
        apply_option(
            &mut settings.reasoning.presentation.summary,
            Feature::ReasoningSummary,
            table,
            &mut evidence,
        )?;
    }
    if settings.reasoning.encrypted_output() {
        match table.get(Feature::EncryptedReasoning) {
            Disposition::Equivalent => {
                settings.reasoning = settings.reasoning.with_encrypted_output(false);
            }
            Disposition::Omit(rule) => {
                settings.reasoning = settings.reasoning.with_encrypted_output(false);
                evidence.push(rule.effect(ConversionBoundary::RequestProject));
            }
            _ => {}
        }
    }
    if target.require_output_ceiling {
        if controls.max_output_tokens.is_none() {
            let default = target
                .default_output_tokens
                .ok_or(CodecError::Unsupported(CarrierReason::Unavailable))?;
            controls.max_output_tokens = Some(
                target
                    .limits
                    .maximum_output_tokens
                    .map_or(default, |maximum| default.min(maximum)),
            );
        }
        if settings.automatic_prefix_cache.is_none()
            && !settings
                .instructions
                .as_ref()
                .is_some_and(|root| root.parts.iter().any(|part| part.cache.is_some()))
            && !semantic
                .items()
                .iter()
                .any(|(_, item)| item.has_cache_breakpoints())
            && !semantic
                .tools()
                .iter()
                .flat_map(ToolDefinition::leaves)
                .any(|tool| matches!(tool, ToolDefinition::Function(t) if t.cache.is_some()))
        {
            settings.automatic_prefix_cache = target.automatic_cache_default;
        }
    }
    if let Some(maximum) = target.limits().maximum_output_tokens
        && controls.max_output_tokens.is_some_and(|n| n > maximum)
    {
        controls.max_output_tokens = Some(maximum);
        evidence.push(super::disposition::CLAMP_OUTPUT.effect(ConversionBoundary::RequestProject));
    }
    omit_option(
        &mut controls.max_output_tokens,
        Feature::MaxOutputTokens,
        table,
        &mut evidence,
    );
    if controls.max_output_tokens.is_some()
        && let Disposition::Approximate(rule) = table.get(Feature::MaxOutputTokens)
    {
        evidence.push(rule.effect(ConversionBoundary::RequestProject));
    }
    if settings
        .reasoning
        .budget_tokens
        .zip(controls.max_output_tokens)
        .is_some_and(|(budget, ceiling)| {
            budget > ceiling || target.adaptive_budget && budget == ceiling
        })
    {
        return Err(
            crate::conversion::generation::RepresentationError::Admission(Feature::ReasoningBudget)
                .into(),
        );
    }
    settings.controls = controls;
    {
        let mut reasoning = settings.reasoning.clone();
        reasoning.presence = ReasoningPresence::Absent;
        if reasoning == ReasoningRequest::default() {
            settings.reasoning = reasoning;
        }
    }
    if settings.text.format.is_none() && settings.text.verbosity.is_none() {
        settings.text.presence = None;
    }
    let mut items = semantic.items().to_vec();
    if let Disposition::Omit(rule) = table.get(Feature::EagerToolInput) {
        let mut owners = std::collections::BTreeSet::new();
        for_each_function_tool_mut(&mut settings, &mut items, |owner, tool| {
            if tool.eager_input.take().is_some() {
                owners.insert(owner);
            }
        });
        for owner in owners {
            let mut effect = rule.effect(ConversionBoundary::RequestProject);
            if let Some(id) = owner {
                effect = effect.at(Owner::item(id));
            }
            evidence.push(effect);
        }
    }
    for (id, item) in &mut items {
        if let Item::ConfigurationUpdate(update) = item
            && let Some(effort) = update.effort
        {
            if !target.has_effort() {
                update.effort = None;
                if let Some(rule) = table.get(Feature::Effort).rule() {
                    evidence.push(
                        rule.effect(ConversionBoundary::RequestProject)
                            .at(crate::conversion::generation::Owner::item(*id)),
                    );
                }
                continue;
            }
            let (mapped, rule) = target.map_effort(effort, &effort_levels)?;
            if mapped != effort {
                update.effort = Some(mapped);
                evidence.push(
                    crate::conversion::generation::Effect::new(
                        rule,
                        crate::conversion::generation::Field::Effort,
                        ConversionBoundary::RequestProject,
                        crate::conversion::generation::Change::Approximate,
                    )
                    .at(crate::conversion::generation::Owner::item(*id)),
                );
            }
        }
    }
    if let Disposition::Omit(rule) = table.get(Feature::ToolExecutionReport) {
        for (id, item) in &mut items {
            if let Item::ToolResult(result) | Item::CustomResult(result) = item
                && result.execution.take().is_some()
            {
                evidence.push(
                    rule.effect(ConversionBoundary::RequestProject)
                        .at(crate::conversion::generation::Owner::item(*id)),
                );
            }
        }
    }
    if target.require_tool_description {
        for_each_function_tool_mut(&mut settings, &mut items, |_, tool| {
            tool.description.get_or_insert_with(String::new);
        });
    }
    for (_, item) in &mut items {
        fn detail(
            use_: &mut ResourceUse,
            target: TargetCapabilities<'_>,
            evidence: &mut EvidenceDelta,
        ) {
            let ResourceDescription::Image { detail, .. } = &mut use_.description else {
                return;
            };
            let Some(value) = *detail else { return };
            let carried = target.image_details.contains(&value)
                && (value != ImageDetail::Original || target.original_image_detail);
            if !carried {
                *detail = None;
                let rule = if value == ImageDetail::Auto {
                    super::disposition::IMAGE_AUTO_DEFAULT
                } else {
                    super::disposition::OMIT_IMAGE_DETAIL
                };
                evidence.push(rule.effect(ConversionBoundary::RequestProject));
            }
        }
        match item {
            Item::Message(message) => {
                for part in &mut message.parts {
                    if let ContentPart::Resource(use_) = &mut part.content {
                        detail(use_, target, &mut evidence);
                    }
                }
            }
            Item::ToolResult(result) | Item::CustomResult(result) => {
                if let ToolOutput::Parts(parts) = &mut result.output {
                    for part in parts {
                        if let InputPart::Resource(use_) = &mut part.content {
                            detail(use_, target, &mut evidence);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    project_cache(&mut settings, &mut items, table, &mut evidence);
    if let Disposition::Omit(rule) = table.get(Feature::MessagePhase) {
        let mut changed = false;
        for (_, item) in &mut items {
            if let Item::Message(message) = item {
                changed |= message.phase.take().is_some();
            }
        }
        if changed {
            evidence.push(rule.effect(ConversionBoundary::RequestProject));
        }
    }
    if &settings != semantic.settings() || items != semantic.items() {
        let envelopes = if items.len() != semantic.items().len() {
            semantic
                .clone()
                .into_owned()
                .with_items(items.clone())
                .map_err(CodecError::from)?
                .message_envelopes()
                .to_vec()
        } else {
            semantic.message_envelopes().to_vec()
        };
        semantic = Cow::Owned(
            GenerationRequest::from_resources_and_envelopes(
                items,
                settings,
                semantic.resources().clone(),
                envelopes,
            )
            .map_err(CodecError::from)?
            .with_cached_context(semantic.cached_context().cloned())
            .map_err(CodecError::from)?,
        );
    }
    Ok(ControlProjection {
        value: semantic,
        evidence,
        reasoning: ReasoningReceipts {
            policy: applied_policy,
            effort_rule,
        },
    })
}

pub fn project_tool_inventory<'a>(
    mut semantic: Cow<'a, GenerationRequest>,
    target: TargetCapabilities<'_>,
    table: &mut Dispositions,
    evidence: &mut EvidenceDelta,
) -> Result<
    (
        Cow<'a, GenerationRequest>,
        crate::tool_projection::ToolDeliveryMap,
    ),
    ConversionFailure,
> {
    // The selected bounded tool-set projections share one owner: namespace
    // aliasing, retained native additions, the effective active-set collapse
    // and allowed-set inventory filtering. Projections act on this candidate copy only.
    let mut tool_delivery = crate::tool_projection::ToolDeliveryMap::default();
    if target.alias_tool_namespaces {
        let (aliased, map, changed) = crate::tool_projection::alias_tool_namespaces(semantic)?;
        semantic = aliased;
        tool_delivery = map;
        if changed {
            evidence.push(
                super::disposition::ALIAS_TOOL_NAMESPACES
                    .effect(ConversionBoundary::RequestProject),
            );
        }
    }
    let ordered = ordered_tool_declarations(semantic.items());
    let allowed_choice = matches!(semantic.tool_choice(), Some(ToolChoice::Allowed { .. }));
    // The required carrier can need a singleton request inventory, not only
    // a singleton allowed list. Reuse the selected declaration-visibility loss.
    let singleton_subset = target.filter_singleton_inventory
        && semantic.available_tool_count().is_ok_and(|count| count > 1)
        && match semantic.tool_choice() {
            Some(ToolChoice::Specific(_)) => true,
            Some(ToolChoice::Allowed {
                required: true,
                tools,
            }) => {
                matches!(tools.as_slice(), [reference]
                    if reference.kind == ToolKind::Function && reference.namespace.is_none())
            }
            _ => false,
        };
    if ordered && !table.accepts(Feature::ToolChanges) {
        return Err(
            crate::conversion::generation::RepresentationError::Admission(Feature::ToolChanges)
                .into(),
        );
    }
    if ordered || target.filter_allowed_inventory && allowed_choice || singleton_subset {
        let carrier = if singleton_subset || target.filter_allowed_inventory && allowed_choice {
            super::tool_set::OrderedCarrier::None
        } else {
            target.ordered_tools
        };
        let projection = match super::tool_set::project(carrier, semantic.tools(), semantic.items())
        {
            Ok(projection) => projection,
            Err(super::tool_set::Rejection::Collision) => {
                table.set(
                    Feature::ToolChanges,
                    Disposition::Reject(
                        "an ordered redeclaration needs an unknown same-name winner",
                    ),
                );
                return Err(
                    crate::conversion::generation::RepresentationError::Admission(
                        Feature::ToolChanges,
                    )
                    .into(),
                );
            }
            Err(super::tool_set::Rejection::UnknownChange) => {
                return Err(CodecError::invalid().into());
            }
        };
        let mut settings = semantic.settings().clone();
        let mut items = Cow::Borrowed(semantic.items());
        match projection {
            super::tool_set::Projection::Native => {}
            super::tool_set::Projection::Retained {
                items: projected,
                root_additions,
            } => {
                items = Cow::Owned(projected);
                if !root_additions.is_empty() {
                    let mut tools = semantic.tools().to_vec();
                    tools.extend(root_additions);
                    settings.tools = Some(tools);
                }
                evidence.push(
                    super::disposition::RETAIN_ORDERED_ADDITIONS
                        .effect(ConversionBoundary::RequestProject),
                );
            }
            super::tool_set::Projection::ActiveSet {
                items: projected,
                tools,
            } => {
                items = Cow::Owned(projected);
                settings.tools = (!tools.is_empty()).then_some(tools);
                evidence.push(
                    super::disposition::TOOL_CHANGES.effect(ConversionBoundary::RequestProject),
                );
            }
        }
        if target.filter_allowed_inventory && allowed_choice || singleton_subset {
            let (required, references) = match settings.tool_choice.as_ref() {
                Some(ToolChoice::Specific(name)) => (
                    true,
                    vec![ToolReference {
                        kind: ToolKind::Function,
                        name: name.clone(),
                        namespace: None,
                    }],
                ),
                Some(ToolChoice::Allowed { required, tools }) => (*required, tools.clone()),
                _ => unreachable!("inventory filtering requires a selected subset"),
            };
            if references.iter().any(|r| r.namespace.is_some()) {
                return Err(
                    crate::conversion::generation::RepresentationError::Admission(
                        Feature::ToolChoice,
                    )
                    .into(),
                );
            }
            // Allowed-set inventory filtering: the restriction and required
            // invocation survive while filtered definitions lose visibility.
            let filtered = super::tool_set::filter_allowed(
                settings.tools.take().unwrap_or_default(),
                &references,
            );
            settings.tools = (!filtered.is_empty()).then_some(filtered);
            // An empty intersection keeps the required meaning instead of
            // silently becoming `auto`; revalidation rejects it.
            if target.filter_allowed_inventory {
                settings.tool_choice = Some(if required {
                    ToolChoice::Required
                } else {
                    ToolChoice::Auto
                });
            }
            evidence.push(
                super::disposition::FILTER_ALLOWED_INVENTORY
                    .effect(ConversionBoundary::RequestProject),
            );
        }
        if semantic.items() != items.as_ref() || *semantic.settings() != settings {
            let items = items.into_owned();
            semantic = Cow::Owned(
                semantic
                    .into_owned()
                    .with_items_and_settings(items, settings)
                    .map_err(CodecError::from)?,
            );
        }
    }
    tool_delivery.retain_declared(semantic.as_ref());
    Ok((semantic, tool_delivery))
}

pub fn relax_tool_strictness<'a>(
    mut semantic: Cow<'a, GenerationRequest>,
    table: &Dispositions,
    evidence: &mut EvidenceDelta,
) -> Result<Cow<'a, GenerationRequest>, CodecError> {
    if let Disposition::Approximate(rule) = table.get(Feature::StrictTools)
        && semantic
            .function_tools()
            .any(|tool| tool.strict != FunctionStrictness::Omitted(StrictDefault::NonStrict))
    {
        let mut settings = semantic.settings().clone();
        let mut items = semantic.items().to_vec();
        let mut relaxed = false;
        for_each_function_tool_mut(&mut settings, &mut items, |_, tool| {
            relaxed |= matches!(
                tool.strict,
                FunctionStrictness::Explicit(true)
                    | FunctionStrictness::Omitted(StrictDefault::Conditional)
            );
            tool.strict = FunctionStrictness::Omitted(StrictDefault::NonStrict);
        });
        semantic = Cow::Owned(
            semantic
                .into_owned()
                .with_items_and_settings(items, settings)
                .map_err(CodecError::from)?,
        );
        if relaxed {
            evidence.push(rule.effect(ConversionBoundary::RequestProject));
        }
    }
    Ok(semantic)
}

pub fn project_tool_result_errors<'a>(
    mut semantic: Cow<'a, GenerationRequest>,
    table: &Dispositions,
    evidence: &mut EvidenceDelta,
) -> Result<Cow<'a, GenerationRequest>, ConversionFailure> {
    if let Disposition::Approximate(rule) = table.get(Feature::ToolResultError) {
        let (copy, changed) = with_tool_error_status(semantic)?;
        semantic = copy;
        if changed {
            evidence.push(rule.effect(ConversionBoundary::RequestProject));
        }
    }
    Ok(semantic)
}

impl ReasoningReceipts<'_> {
    pub fn record(
        self,
        source: &GenerationRequest,
        projected: &GenerationRequest,
        evidence: &mut EvidenceDelta,
    ) -> Result<(), ConversionFailure> {
        if let Some(policy) = self.policy {
            evidence.push(policy.receipt(source, projected)?);
        }
        if let Some(rule) = self.effort_rule {
            use crate::conversion::generation::{Effect, ReasoningControls, RuleId};
            evidence.push(Effect::Reasoning {
                rule: RuleId::new(rule),
                input: ReasoningControls::request(source),
                output: ReasoningControls::request(projected),
                preset_tokens: None,
                owner: Owner::Root,
            });
        }
        Ok(())
    }
}

fn project_cache(
    settings: &mut GenerationSettings,
    items: &mut [(ItemId, Item)],
    table: &Dispositions,
    evidence: &mut EvidenceDelta,
) {
    let ttl = matches!(table.get(Feature::ExplicitCache), Disposition::Omit(_));
    let lifetime = matches!(
        table.get(Feature::RequestLifetimeCache),
        Disposition::Omit(_)
    );
    let mut omitted_ttl = false;
    let mut omitted_lifetime = false;
    let mut boundary = |value: &mut Option<PrefixCacheBoundary>| match *value {
        Some(PrefixCacheBoundary::Ttl(_)) if ttl => {
            *value = None;
            omitted_ttl = true;
        }
        Some(PrefixCacheBoundary::RequestLifetime) if lifetime => {
            *value = None;
            omitted_lifetime = true;
        }
        _ => {}
    };
    if let Some(root) = &mut settings.instructions {
        for part in &mut root.parts {
            boundary(&mut part.cache);
        }
    }
    for (_, item) in items.iter_mut() {
        match item {
            Item::Instruction(instruction) => {
                for part in &mut instruction.parts {
                    boundary(&mut part.cache);
                }
            }
            Item::Message(message) => {
                for part in &mut message.parts {
                    boundary(&mut part.cache);
                }
            }
            Item::ToolResult(result) | Item::CustomResult(result) => {
                if let ToolOutput::Parts(parts) = &mut result.output {
                    for part in parts {
                        boundary(&mut part.cache);
                    }
                }
            }
            _ => {}
        }
    }
    if ttl {
        for_each_function_tool_mut(settings, items, |_, tool| {
            omitted_ttl |= tool.cache.take().is_some();
        });
        for (_, item) in items {
            match item {
                Item::ToolCall(call) => omitted_ttl |= call.cache.take().is_some(),
                Item::ToolResult(result) | Item::CustomResult(result) => {
                    omitted_ttl |= result.cache.take().is_some()
                }
                Item::ToolChange(change) => {
                    omitted_ttl |= change.cache.take().is_some();
                }
                _ => {}
            }
        }
    }
    for (feature, changed) in [
        (Feature::ExplicitCache, omitted_ttl),
        (Feature::RequestLifetimeCache, omitted_lifetime),
    ] {
        if changed && let Some(rule) = table.get(feature).rule() {
            evidence.push(rule.effect(ConversionBoundary::RequestProject));
        }
    }
}

fn with_tool_error_status(
    request: Cow<'_, GenerationRequest>,
) -> Result<(Cow<'_, GenerationRequest>, bool), CodecError> {
    use morphiecore_semantic::value::Text;
    if !request
            .items()
            .iter()
            .any(|(_, item)| matches!(item, Item::ToolResult(result) | Item::CustomResult(result) if result.is_error.is_some()))
    {
        return Ok((request, false));
    }
    let mut next = request
        .items()
        .iter()
        .flat_map(|(_, item)| item.part_ids())
        .filter(|id| id.scope() == LocalScope::DERIVED)
        .map(PartId::get)
        .max()
        .unwrap_or(0);
    let mut items = request.items().to_vec();
    for (_, item) in &mut items {
        if let Item::ToolResult(result) | Item::CustomResult(result) = item
            && let Some(is_error) = result.is_error.take()
        {
            // This reports the supplied flag, not an inferred execution conclusion.
            let prefix = format!("[tool result is_error={is_error}]\n");
            match &mut result.output {
                ToolOutput::Text(text) => text.insert_str(0, &prefix),
                ToolOutput::Structured(value) => {
                    result.output = ToolOutput::Text(format!("{prefix}{}", value.value()));
                }
                ToolOutput::Parts(parts) => {
                    next = next.checked_add(1).ok_or(CodecError::Limit)?;
                    parts.insert(
                        0,
                        Part::new(
                            PartId::scoped(LocalScope::DERIVED, next),
                            InputPart::Text(
                                Text::new(prefix, "tool error status", 64)
                                    .map_err(|_| CodecError::Limit)?,
                            ),
                        ),
                    );
                }
            }
        }
    }
    let request = request.into_owned().with_items(items)?;
    Ok((Cow::Owned(request), true))
}
