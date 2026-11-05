//! One owner for the selected bounded tool-set projections: the effective
//! active-set collapse, target-native retained additions and allowed-set
//! inventory filtering. Projections act on candidate copies only; source IR,
//! historical call/result content and their identities stay immutable.
//! https://developers.openai.com/api/docs/guides/tools-tool-search
//! https://platform.claude.com/docs/en/build-with-claude/mid-conversation-system-messages
use crate::semantic::task::generation::*;

/// Which ordered-declaration carrier the target can express.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrderedCarrier {
    /// Chat has no ordered declaration carrier; only the effective set fits.
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
        let available = !matches!(tool, ToolDefinition::Function(t) if t.dispatch.defer_loading)
            && !matches!(tool, ToolDefinition::Custom(t) if t.dispatch.defer_loading);
        Self {
            tool: tool.clone(),
            available,
        }
    }
    /// Declaration identity: namespaces key by container name, leaves by kind.
    fn key(&self) -> (Option<ToolKind>, &str) {
        (self.tool.kind(), self.tool.name().as_str())
    }
}

/// Project one candidate's ordered tool declarations onto the target carrier.
/// `initial` are the root declarations; `items` is the ordered item list.
pub(crate) fn project(
    carrier: OrderedCarrier,
    initial: &[ToolDefinition],
    items: &[(ItemId, Item)],
) -> Result<Projection, Rejection> {
    if !items
        .iter()
        .any(|(_, item)| matches!(item, Item::ToolBatch(_) | Item::ToolChange(_)))
    {
        return Ok(Projection::Native);
    }
    // One availability walk feeds every mode: entries track the effective set
    // while `retained` tracks the per-position native representation.
    let mut entries: Vec<Entry> = initial.iter().map(Entry::offered).collect();
    let mut kept: Vec<(ItemId, Item)> = Vec::new();
    let mut retained: Vec<(ItemId, Item)> = Vec::new();
    let mut root_additions: Vec<ToolDefinition> = Vec::new();
    let mut all_native = carrier != OrderedCarrier::None;
    let mut converted = false;
    let mut converted_ids: Vec<ItemId> = Vec::new();
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
            kept.push((*id, item.clone()));
            retained.push((*id, item.clone()));
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
                        if collides(&entries, tool) {
                            return Err(Rejection::Collision);
                        }
                        entries.push(Entry::offered(tool));
                        root_additions.push(tool.clone());
                    }
                    converted = true;
                    continue;
                }
                let batch_collides = batch.tools.iter().any(|tool| collides(&entries, tool));
                collision |= batch_collides;
                for tool in &batch.tools {
                    push_entry(&mut entries, tool);
                }
                match carrier {
                    OrderedCarrier::Batches => retained.push((
                        *id,
                        Item::ToolBatch(ToolBatch {
                            role: batch.role,
                            tools: batch.tools.clone(),
                        }),
                    )),
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
                                converted_ids.push(fresh);
                                retained.push((
                                    fresh,
                                    Item::ToolChange(ToolChange {
                                        action: ToolChangeAction::AddDefinition(Box::new(
                                            function.clone(),
                                        )),
                                        cache: None,
                                    }),
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
                    let existed = entries
                        .iter()
                        .any(|entry| entry.key() == (Some(ToolKind::Function), name));
                    let next = Entry::offered(&ToolDefinition::Function((**tool).clone()));
                    match entries
                        .iter_mut()
                        .find(|entry| entry.key() == (Some(ToolKind::Function), name))
                    {
                        // By-value replacement is the documented Messages meaning.
                        Some(entry) => *entry = next,
                        None => entries.push(next),
                    }
                    match carrier {
                        OrderedCarrier::Changes => retained.push((*id, item.clone())),
                        // A replacement has no documented batch precedence.
                        OrderedCarrier::Batches => {
                            if existed {
                                all_native = false;
                            } else {
                                // A surviving id must keep its item kind; the
                                // converted batch takes a fresh local identity.
                                let fresh = ItemId::new(next_id);
                                next_id += 1;
                                converted_ids.push(fresh);
                                retained.push((
                                    fresh,
                                    Item::ToolBatch(ToolBatch {
                                        role: ToolBatchRole::Developer,
                                        tools: vec![ToolDefinition::Function((**tool).clone())],
                                    }),
                                ));
                                converted = true;
                            }
                        }
                        OrderedCarrier::None => {}
                    }
                }
                ToolChangeAction::AddReference(_) | ToolChangeAction::Remove(_) => {
                    let name = change.name();
                    let Some(entry) = entries
                        .iter_mut()
                        .find(|entry| entry.key() == (Some(ToolKind::Function), name.as_str()))
                    else {
                        return Err(Rejection::UnknownChange);
                    };
                    entry.available = matches!(change.action, ToolChangeAction::AddReference(_));
                    if matches!(carrier, OrderedCarrier::Changes) {
                        retained.push((*id, item.clone()));
                    } else {
                        // References and removals have no other native carrier.
                        all_native = false;
                    }
                }
            },
            Item::ToolCall(_) | Item::CustomCall(_) => {
                pending_calls += 1;
                kept.push((*id, item.clone()));
                retained.push((*id, item.clone()));
            }
            Item::ToolResult(_) => {
                pending_calls = pending_calls.saturating_sub(1);
                kept.push((*id, item.clone()));
                retained.push((*id, item.clone()));
            }
            Item::CustomResult(_) => {
                pending_calls = pending_calls.saturating_sub(1);
                kept.push((*id, item.clone()));
                retained.push((*id, item.clone()));
            }
            _ => {
                kept.push((*id, item.clone()));
                retained.push((*id, item.clone()));
            }
        }
    }
    if all_native && converted_run_out_of_place(&retained, &converted_ids) {
        // A converted change must keep a legal system placement: after a user
        // turn, before an assistant turn or the end of the history.
        all_native = false;
    }
    if all_native {
        if !converted {
            return Ok(Projection::Native);
        }
        return Ok(Projection::Retained {
            items: retained,
            root_additions,
        });
    }
    if collision {
        return Err(Rejection::Collision);
    }
    Ok(Projection::ActiveSet {
        items: kept,
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

fn collides(entries: &[Entry], tool: &ToolDefinition) -> bool {
    entries
        .iter()
        .any(|entry| entry.key() == (tool.kind(), tool.name().as_str()))
}

fn push_entry(entries: &mut Vec<Entry>, tool: &ToolDefinition) {
    match entries
        .iter_mut()
        .find(|entry| entry.key() == (tool.kind(), tool.name().as_str()))
    {
        // A batch does not select a same-name schema winner; availability keeps
        // the name offered while `collision` blocks projected fallbacks.
        Some(entry) => entry.available |= Entry::offered(tool).available,
        None => entries.push(Entry::offered(tool)),
    }
}

fn converted_run_out_of_place(retained: &[(ItemId, Item)], converted: &[ItemId]) -> bool {
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
        if !system_side(&retained[index].1) {
            index += 1;
            continue;
        }
        let start = index;
        while index < retained.len() && system_side(&retained[index].1) {
            index += 1;
        }
        if !retained[start..index]
            .iter()
            .any(|(id, _)| converted.contains(id))
        {
            continue;
        }
        let before_ok = start == 0 || user_side(&retained[start - 1].1);
        let after_ok = index == retained.len() || !user_side(&retained[index].1);
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
