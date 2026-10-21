//! One adjacent prepared text run. Time is supplied by intake, never read here.
use super::{EventContext, PreparedEvents};
use crate::semantic::task::generation::{ItemId, PartId, PartKind, StreamEvent};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeltaCoalescing {
    pub max_delay_ms: u64,
    pub max_pending_bytes: usize,
}
impl DeltaCoalescing {
    pub(crate) fn valid(self) -> bool {
        (1..=100).contains(&self.max_delay_ms) && (1024..=1 << 20).contains(&self.max_pending_bytes)
    }
}

pub(super) struct Coalescer {
    policy: DeltaCoalescing,
    text: Option<(ItemId, PartId)>,
    seen_content: bool,
    closed: bool,
    pending: Option<PendingText>,
    pub(super) peak_bytes: usize,
}
struct PendingText {
    item: ItemId,
    part: PartId,
    fragment: String,
    context: Arc<EventContext>,
    deadline: Instant,
    bytes: usize,
    encoded_text: usize,
}
pub(super) fn escaped_bytes(text: &str) -> usize {
    text.bytes()
        .map(|byte| match byte {
            b'"' | b'\\' => 2,
            0..=31 => 6,
            _ => 1,
        })
        .sum()
}
impl EventContext {
    pub(super) fn retained_bytes(&self) -> Option<usize> {
        *self.accounted_bytes.get_or_init(|| self.account_context())
    }
    // Large reported settings must not be revalidated for every bypassed delta.
    fn account_context(&self) -> Option<usize> {
        let metadata = &self.metadata;
        let context = &metadata.context;
        // Large instruction trees bypass this small delivery buffer rather than
        // receive a second budget.
        if context
            .instruction_messages
            .as_ref()
            .is_some_and(|items| !items.is_empty())
        {
            return None;
        }
        let report_storage = context.reports.iter().count()
            * std::mem::size_of::<crate::semantic::context::ResponseReport>();
        // Only variable text/JSON needs the escaped-payload allowance. Applying
        // it to the fixed report structure would bypass small buffers even for
        // empty metadata.
        let settings = context
            .reports
            .validate()
            .ok()?
            .saturating_sub(report_storage);
        let owners: usize = context
            .reports
            .iter()
            .filter_map(|r| r.settings.as_ref())
            .map(|settings| {
                std::mem::size_of_val(settings)
                    + settings
                        .tools
                        .as_deref()
                        .unwrap_or_default()
                        .iter()
                        .map(|tool| {
                            std::mem::size_of_val(tool) + match tool {
                                crate::semantic::task::generation::ToolDefinition::Namespace(
                                    group,
                                ) => {
                                    group.tools.len()
                                        * std::mem::size_of::<
                                            crate::semantic::task::generation::ToolDefinition,
                                        >()
                                }
                                _ => 0,
                            }
                        })
                        .sum::<usize>()
            })
            .sum();
        let execution = serde_json::to_vec(&context.execution).ok()?.len();
        let diagnostics = serde_json::to_vec(&context.cache_diagnostics).ok()?.len();
        Some(
            std::mem::size_of::<Self>()
                + metadata.id.as_ref().map_or(0, String::len)
                + metadata.model.len()
                + metadata.created.as_ref().map_or(0, |n| n.to_string().len())
                + context.system_fingerprint.value().map_or(0, String::len)
                + metadata
                    .storage
                    .as_ref()
                    .and_then(|s| s.previous_response_id.value())
                    .map_or(0, String::len)
                + settings.saturating_mul(64)
                + report_storage
                + owners
                + execution.saturating_mul(64)
                + diagnostics.saturating_mul(64)
                + self.fidelity.retained_bytes()
                + metadata.instruction_fidelity.retained_bytes(),
        )
    }
    fn frame_overhead(&self) -> usize {
        *self.frame_bytes.get_or_init(|| self.account_frame())
    }
    fn account_frame(&self) -> usize {
        // Covers all three text envelopes, 256-byte escaped item aliases,
        // sequence/index fields, SSE framing and at most 1023 padding bytes.
        4096 + self.metadata.id.as_deref().map_or(0, escaped_bytes)
            + escaped_bytes(&self.metadata.model)
            + self
                .metadata
                .created
                .as_ref()
                .map_or(0, |n| n.to_string().len())
            + self
                .metadata
                .context
                .system_fingerprint
                .value()
                .map_or(0, |s| escaped_bytes(s))
    }
}
impl Coalescer {
    pub(super) fn new(policy: DeltaCoalescing) -> Self {
        Self {
            policy,
            text: None,
            seen_content: false,
            closed: false,
            pending: None,
            peak_bytes: 0,
        }
    }
    pub(super) fn deadline(&self) -> Option<Instant> {
        self.pending.as_ref().map(|p| p.deadline)
    }
    pub(super) fn flush(&mut self) -> Option<PreparedEvents> {
        self.pending.take().map(|p| PreparedEvents {
            events: vec![StreamEvent::Delta {
                item: p.item,
                part: p.part,
                fragment: p.fragment,
                logprobs: vec![],
            }],
            context: p.context,
            barrier: false,
        })
    }
    pub(super) fn close(&mut self) {
        self.pending = None;
        self.text = None;
        self.closed = true;
    }
    pub(super) fn push(
        &mut self,
        mut batch: PreparedEvents,
        now: Instant,
        frame_limit: usize,
    ) -> Vec<PreparedEvents> {
        debug_assert!(!self.closed);
        let seen = self.seen_content;
        for event in &batch.events {
            match event {
                StreamEvent::PartStarted { item, part, kind } => {
                    self.text = (*kind == PartKind::Text).then_some((*item, *part));
                }
                StreamEvent::PartFinished { item, part } if self.text == Some((*item, *part)) => {
                    self.text = None
                }
                StreamEvent::ItemFinished { item, .. }
                    if self.text.is_some_and(|(owner, _)| owner == *item) =>
                {
                    self.text = None
                }
                StreamEvent::Delta { fragment, .. } if !fragment.is_empty() => {
                    self.seen_content = true
                }
                _ => {}
            }
        }
        let delta = match batch.events.as_slice() {
            [
                StreamEvent::Delta {
                    item,
                    part,
                    fragment,
                    logprobs,
                },
            ] if seen
                && !batch.barrier
                && self.text == Some((*item, *part))
                && !fragment.is_empty()
                && logprobs.is_empty() =>
            {
                Some((*item, *part, fragment))
            }
            _ => None,
        };
        let mut ready = vec![];
        let Some((item, part, fragment)) = delta else {
            ready.extend(self.flush());
            ready.push(batch);
            return ready;
        };
        let encoded = escaped_bytes(fragment);
        let added = fragment.len().saturating_mul(2).saturating_add(encoded);
        let same_run = self.pending.as_ref().is_some_and(|pending| {
            Arc::ptr_eq(&pending.context, &batch.context)
                && pending.item == item
                && pending.part == part
                && pending.deadline > now
                && pending.bytes.saturating_add(added) <= self.policy.max_pending_bytes
                && pending.encoded_text.saturating_add(encoded)
                    <= frame_limit.saturating_sub(batch.context.frame_overhead())
        });
        if same_run {
            let pending = self.pending.as_mut().expect("pending run");
            pending.fragment.reserve_exact(fragment.len());
            pending.fragment.push_str(fragment);
            pending.bytes += added;
            pending.encoded_text += encoded;
            self.peak_bytes = self.peak_bytes.max(pending.bytes);
            return ready;
        }
        ready.extend(self.flush());
        let bytes = batch
            .context
            .retained_bytes()
            .and_then(|bytes| bytes.checked_add(added));
        if let Some(bytes) = bytes.filter(|bytes| *bytes <= self.policy.max_pending_bytes)
            && encoded <= frame_limit.saturating_sub(batch.context.frame_overhead())
        {
            self.peak_bytes = self.peak_bytes.max(bytes);
            let Some(StreamEvent::Delta { fragment, .. }) = batch.events.pop() else {
                unreachable!("eligible text delta");
            };
            self.pending = Some(PendingText {
                item,
                part,
                fragment,
                context: batch.context,
                bytes,
                encoded_text: encoded,
                deadline: now + Duration::from_millis(self.policy.max_delay_ms),
            });
        } else {
            ready.push(batch);
        }
        ready
    }
}
