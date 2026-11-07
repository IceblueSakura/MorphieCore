//! One owner for the selected bounded tool-set projections: the effective
//! active-set collapse, target-native retained additions and allowed-set
//! inventory filtering. Projections act on candidate copies only; source IR,
//! historical call/result content and their identities stay immutable.
//! https://developers.openai.com/api/docs/guides/tools-tool-search
//! https://platform.claude.com/docs/en/build-with-claude/mid-conversation-system-messages
use crate::semantic::task::generation::*;
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

/// Which ordered-declaration carrier the target can express.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrderedCarrier {
    /// Targets without ordered declarations carry only the effective set.
    None,
    /// Responses carries developer batches; by-value replacement changes are
    /// native only as pure additions.
    Batches,
    /// Messages carries by-value replacement changes; developer batches are
    /// native as pure additions, or as a leading lifted prefix.
    Changes,
}

/// The projection selected for one candidate copy.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Projection {
    /// No ordered declarations need projection; nothing changes.
    Native,
    /// Ordered declarations keep target-native positions. `items` replaces the
    /// whole item list; `root_additions` extend the root declarations.
    Retained {
        items: Vec<(ItemId, Item)>,
        root_additions: Vec<ToolDefinition>,
    },
    /// The effective final set becomes the root declarations; ordered items drop.
    ActiveSet {
        items: Vec<(ItemId, Item)>,
        tools: Vec<ToolDefinition>,
    },
}

/// Why no bounded projection can be selected.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Rejection {
    /// A same-name redeclaration needs an unknown source winner.
    Collision,
    /// A reference or removal names no known definition.
    UnknownChange,
}

struct Entry {
    tool: ToolDefinition,
    available: bool,
}

impl Entry {
    fn offered(tool: &ToolDefinition) -> Self {
        Self {
            tool: tool.clone(),
            available: offered(tool),
        }
    }
}
fn offered(tool: &ToolDefinition) -> bool {
    !matches!(tool, ToolDefinition::Function(t) if t.dispatch.defer_loading)
        && !matches!(tool, ToolDefinition::Custom(t) if t.dispatch.defer_loading)
}

/// Project one candidate's ordered tool declarations onto the target carrier.
/// `initial` are the root declarations; `items` is the ordered item list.
pub(crate) fn project(
    carrier: OrderedCarrier,
    initial: &[ToolDefinition],
    items: &[(ItemId, Item)],
) -> Result<Projection, Rejection> {
    // Native batch forwarding needs no schema winner or effective inventory.
    // Source validation and target admission remain with the caller.
    if carrier == OrderedCarrier::Batches
        && !items
            .iter()
            .any(|(_, item)| matches!(item, Item::ToolChange(_)))
    {
        return Ok(Projection::Native);
    }
    if !items
        .iter()
        .any(|(_, item)| matches!(item, Item::ToolBatch(_) | Item::ToolChange(_)))
    {
        return Ok(Projection::Native);
    }
    // One availability walk feeds every mode: entries track the effective set
    // while `retained` tracks the per-position native representation.
    let mut entries: Vec<Entry> = initial.iter().map(Entry::offered).collect();
    // Preserve first-declaration order separately from indexed identity lookup.
    let mut positions: BTreeMap<_, _> = initial
        .iter()
        .enumerate()
        .map(|(i, tool)| ((tool.kind(), tool.name().as_str()), i))
        .collect();
    let mut retained = Vec::new();
    let mut root_additions: Vec<ToolDefinition> = Vec::new();
    let mut all_native = carrier != OrderedCarrier::None;
    let mut converted = false;
    let mut converted_ids = BTreeSet::new();
    // Batch redeclarations have no documented precedence; native forwarding
    // needs no winner, but every projected fallback does.
    let mut collision = false;
    // A leading declaration prefix is equivalent to root declarations; lifting
    // crosses no history, identity or cache boundary there.
    let mut leading = true;
    // Messages cannot separate a pending call from its result with a change.
    let mut pending_calls = 0usize;
    let mut next_id = items
        .iter()
        .map(|(id, _)| id.get())
        .max()
        .unwrap_or_default()
        + 1;
    for (id, item) in items {
        if let Item::Instruction(_) = item {
            retained.push((*id, Cow::Borrowed(item)));
            continue;
        }
        let liftable = leading
            && matches!(carrier, OrderedCarrier::Changes)
            && matches!(item, Item::ToolBatch(batch) if batch.role == ToolBatchRole::Developer);
        if !liftable {
            leading = false;
        }
        match item {
            Item::ToolBatch(batch) => {
                if liftable {
                    for tool in &batch.tools {
                        // A leading batch crossing a root name needs an
                        // unknown winner; lifting cannot resolve it.
                        let key = (tool.kind(), tool.name().as_str());
                        if positions.contains_key(&key) {
                            return Err(Rejection::Collision);
                        }
                        positions.insert(key, entries.len());
                        entries.push(Entry::offered(tool));
                        root_additions.push(tool.clone());
                    }
                    converted = true;
                    continue;
                }
                let batch_collides = batch
                    .tools
                    .iter()
                    .any(|tool| positions.contains_key(&(tool.kind(), tool.name().as_str())));
                collision |= batch_collides;
                for tool in &batch.tools {
                    let key = (tool.kind(), tool.name().as_str());
                    match positions.get(&key) {
                        Some(&index) => entries[index].available |= offered(tool),
                        None => {
                            positions.insert(key, entries.len());
                            entries.push(Entry::offered(tool));
                        }
                    }
                }
                match carrier {
                    OrderedCarrier::Batches => retained.push((*id, Cow::Borrowed(item))),
                    OrderedCarrier::Changes => {
                        let native = pending_calls == 0
                            && batch
                                .tools
                                .iter()
                                .all(|tool| matches!(tool, ToolDefinition::Function(_)));
                        if native && !batch_collides {
                            // One batch maps to its ordered member changes; the
                            // members render as consecutive same-position blocks.
                            for tool in &batch.tools {
                                let ToolDefinition::Function(function) = tool else {
                                    unreachable!("checked function members")
                                };
                                let fresh = ItemId::new(next_id);
                                next_id += 1;
                                converted_ids.insert(fresh);
                                retained.push((
                                    fresh,
                                    Cow::Owned(Item::ToolChange(ToolChange {
                                        action: ToolChangeAction::AddDefinition(Box::new(
                                            function.clone(),
                                        )),
                                        cache: None,
                                    })),
                                ));
                            }
                            converted = true;
                        } else {
                            // A converted addition would assert replacement
                            // precedence the source never documented.
                            all_native = false;
                        }
                    }
                    OrderedCarrier::None => {}
                }
            }
            Item::ToolChange(change) => match &change.action {
                ToolChangeAction::AddDefinition(tool) => {
                    let name = tool.name.as_str();
                    let key = (Some(ToolKind::Function), name);
                    let existing = positions.get(&key).copied();
                    let next = Entry {
                        available: !tool.dispatch.defer_loading,
                        tool: ToolDefinition::Function((**tool).clone()),
                    };
                    match existing {
                        // By-value replacement is the documented Messages meaning.
                        Some(index) => entries[index] = next,
                        None => {
                            positions.insert(key, entries.len());
                            entries.push(next);
                        }
                    }
                    match carrier {
                        OrderedCarrier::Changes => retained.push((*id, Cow::Borrowed(item))),
                        // A replacement has no documented batch precedence.
                        OrderedCarrier::Batches => {
                            if existing.is_some() {
                                all_native = false;
                            } else {
                                // A surviving id must keep its item kind; the
                                // converted batch takes a fresh local identity.
                                let fresh = ItemId::new(next_id);
                                next_id += 1;
                                converted_ids.insert(fresh);
                                retained.push((
                                    fresh,
                                    Cow::Owned(Item::ToolBatch(ToolBatch {
                                        role: ToolBatchRole::Developer,
                                        tools: vec![ToolDefinition::Function((**tool).clone())],
                                    })),
                                ));
                                converted = true;
                            }
                        }
                        OrderedCarrier::None => {}
                    }
                }
                ToolChangeAction::AddReference(_) | ToolChangeAction::Remove(_) => {
                    let name = change.name();
                    let Some(&index) = positions.get(&(Some(ToolKind::Function), name.as_str()))
                    else {
                        return Err(Rejection::UnknownChange);
                    };
                    entries[index].available =
                        matches!(change.action, ToolChangeAction::AddReference(_));
                    if matches!(carrier, OrderedCarrier::Changes) {
                        retained.push((*id, Cow::Borrowed(item)));
                    } else {
                        // References and removals have no other native carrier.
                        all_native = false;
                    }
                }
            },
            Item::ToolCall(_) | Item::CustomCall(_) => {
                pending_calls += 1;
            }
            Item::ToolResult(_) | Item::CustomResult(_) => {
                pending_calls = pending_calls.saturating_sub(1);
            }
            _ => {}
        }
        if !matches!(item, Item::ToolBatch(_) | Item::ToolChange(_)) {
            retained.push((*id, Cow::Borrowed(item)));
        }
    }
    if all_native
        && (retained.len() > MAX_ITEMS
            || declaration_count(
                initial
                    .iter()
                    .chain(&root_additions)
                    .map(ToolDeclaration::from),
            )
            .is_err()
            || converted_run_out_of_place(&retained, &converted_ids))
    {
        // Retained carriage must fit both container bounds and legal system
        // positions. Only these known representation failures select collapse;
        // unknown schema winners still reject below.
        all_native = false;
    }
    if all_native {
        if !converted {
            return Ok(Projection::Native);
        }
        return Ok(Projection::Retained {
            items: retained
                .into_iter()
                .map(|(id, item)| (id, item.into_owned()))
                .collect(),
            root_additions,
        });
    }
    if collision {
        return Err(Rejection::Collision);
    }
    Ok(Projection::ActiveSet {
        items: items
            .iter()
            .filter(|(_, item)| !matches!(item, Item::ToolBatch(_) | Item::ToolChange(_)))
            .cloned()
            .collect(),
        tools: effective(entries),
    })
}

/// Filter an effective inventory to the allowed identities. Namespace-qualified
/// references have no flat inventory meaning and are rejected by the caller.
pub(crate) fn filter_allowed(
    tools: Vec<ToolDefinition>,
    allowed: &[ToolReference],
) -> Vec<ToolDefinition> {
    tools
        .into_iter()
        .filter(|tool| {
            allowed.iter().any(|reference| {
                reference.namespace.is_none()
                    && tool.kind() == Some(reference.kind)
                    && reference.name.as_str() == tool.name().as_str()
            })
        })
        .collect()
}

fn converted_run_out_of_place(
    retained: &[(ItemId, Cow<'_, Item>)],
    converted: &BTreeSet<ItemId>,
) -> bool {
    let system_side = |item: &Item| {
        matches!(
            item,
            Item::Instruction(_) | Item::ToolChange(_) | Item::ConfigurationUpdate(_)
        )
    };
    let user_side = |item: &Item| {
        matches!(item, Item::ToolResult(_) | Item::CustomResult(_))
            || matches!(&item, Item::Message(m) if m.role == MessageRole::User)
    };
    let mut index = 0;
    while index < retained.len() {
        if !system_side(retained[index].1.as_ref()) {
            index += 1;
            continue;
        }
        let start = index;
        while index < retained.len() && system_side(retained[index].1.as_ref()) {
            index += 1;
        }
        if !retained[start..index]
            .iter()
            .any(|(id, _)| converted.contains(id))
        {
            continue;
        }
        let before_ok = start == 0 || user_side(retained[start - 1].1.as_ref());
        let after_ok = index == retained.len() || !user_side(retained[index].1.as_ref());
        if !before_ok || !after_ok {
            return true;
        }
    }
    false
}

fn effective(entries: Vec<Entry>) -> Vec<ToolDefinition> {
    entries
        .into_iter()
        .filter(|entry| entry.available)
        .map(|mut entry| {
            // Surfaced deferred tools become ordinary definitions.
            if let ToolDefinition::Function(tool) = &mut entry.tool {
                tool.dispatch.defer_loading = false;
            }
            if let ToolDefinition::Custom(tool) = &mut entry.tool {
                tool.dispatch.defer_loading = false;
            }
            entry.tool
        })
        .collect()
}
