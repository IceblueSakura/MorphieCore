# History and Replay Semantics

This document defines multi-turn conversation contracts: standard-protocol reasoning replay, stateless carrier limits, compacted history, and agent session storage.

See [Architecture Overview](index.md) for system boundaries and core IR semantics; [Protocol adaptation](adaptation-and-dialects.md) for wire mappings; and [Settled decisions](decisions.md) for accepted trade-offs.

## Contents

- [History and replay](#history-and-replay)
  - [Reasoning replay](#reasoning-replay)
    - [Binding reasoning-history contract](#binding-reasoning-history-contract)
  - [Cross-protocol and cross-Provider history](#cross-protocol-and-cross-provider-history)
  - [Standard Chat reasoning](#standard-chat-reasoning)
  - [Stateless API boundaries](#stateless-api-boundaries)
  - [Compacted history](#compacted-history)
  - [Stateful history](#stateful-history)
  - [Agent session files](#agent-session-files)

## History and replay

### Reasoning replay

Upstream opaque tokens and signatures are delivered in tagged [`mc1.`](../../src/protocol/replay_envelope.rs) envelopes via [`protocol::replay_envelope`](../../src/protocol/replay_envelope.rs):
- **Isolation:** [Startup binding](../../src/gateway/startup/bind.rs) gives native OpenAI reasoning a service-source fingerprint: deployment namespace, credential provenance, Provider, upstream protocol and trusted origin/path. Public and upstream model labels are excluded. Compaction, other native signatures and route/cache identity retain the strict model-specific target fingerprint. Unbound codec callers retain their explicitly supplied origins.
- **Origin ownership:** Request projection, final JSON checks and event checks share one effective strict target origin. An explicit library origin takes precedence over the bound codec scope; native reasoning then uses its separately bound service-source scope when present. A reasoning-only scope cannot authorize an unbound target. Native carrier checks and assessed history requirements stay separate from identity; catalog replay switches do not gate same-source carriage.
- **Candidate history transfer:** Incompatible reasoning is omitted on candidate copies under target-specific `chat`, `responses`, `messages`, `gemini` and `interactions` `.transfer-visible-history.v1` rules by the [request projection](../../src/protocol/request.rs), preserving visible messages and semantic tool links. Issuer isolation and assessed carrier availability are separate selectors.
- **Supported replay carriers:** Anthropic thinking signatures and `redacted_thinking`, Responses `encrypted_content`, Google `thoughtSignature`, Interactions `function_call.signature`. Standard Chat has no reasoning replay carrier.
- **Responses stream authority:** `response.output_item.done` binds completed reasoning replay once, following the official [streaming contract](https://developers.openai.com/api/reference/resources/responses/streaming-events).

  The terminal response snapshot is independently decoded and verifies output order, owner/type, lifecycle, phase, visible content, tool arguments and other known fields; it never replaces completed stream items.

  Two nonempty encrypted encodings on the same completed reasoning item need not be byte-equal: the item-done value remains authoritative for JSON, SSE and next-request replay.

  Replay presence changes, malformed values and differing incomplete-item tokens still reject.

  Required item closure, terminal publication, origin isolation and EOF checks remain strict.

- **Incomplete reasoning:** Same-issuer `incomplete` reasoning with a finalized replay token forwards exactly to Responses. Client `in_progress` partial tokens remain invalid at replay opening.
- **Client trust:** Malformed recognized `mc1.` envelopes reject before projection without mutation. Unsealed native values confer no issuer authority and are omitted during target history projection; they are never upgraded to trusted replay.
- **Visible-history transfer (L3):** When a target lacks a carrier for historical reasoning (including unsealed or foreign reasoning, full-readable text on summary-only targets, foreign citations, and obsolete markers), the uncarriable reasoning owner is omitted under a named loss receipt (`chat.transfer-visible-history.v1`, `responses.transfer-visible-history.v1`, `messages.transfer-visible-history.v1`, `gemini.transfer-visible-history.v1`, `interactions.transfer-visible-history.v1`).

  Chat history selection shares final validation's part and following-carrier shape checks; a readable or structured carrier flag alone is insufficient.

  Visible content and semantic tool links are strictly preserved; reasoning is never forged or moved into text.

  Explicit client `read_history: true` cannot be satisfied by omission and rejects.

#### Binding reasoning-history contract

A Generation binding declares one reasoning-history contract: `Unsupported`, `Ignored`, `Accepted`, or `RequiredWithTools`. `None` indicates an unassessed binding.

1. **Unassessed bindings (`None`)**: Planning does not reject on unassessed facts. The candidate sends carriable history and delegates validation to upstream (delegation under the conversion ladder); discovery publishes `not_assessed`. Delegation never applies to credentials, auth, or replay origin trust.
2. **`RequiredWithTools` handling**: When a binding declares `RequiredWithTools` and a Standard Chat client sends tools with thinking active:
   - Active or default thinking rejects before dispatch (`admission.reasoning_history_required`); the gateway does not clear effort/budget or disable thinking.
   - If route planning fails on this requirement, HTTP 400 reports `reasoning_history_required` with guidance for the runtime to explicitly disable thinking or select a compatible model/API. Streaming requests receive the same pre-stream JSON error.
   - A new request with explicit `reasoning_effort:"none"` or no tools adds no such history requirement; ordinary target checks still apply, including targets that cannot disable thinking and explicit context/history-read guarantees.
3. **Discovery**: Discovery reads candidate declarations, publishing a common fact, `non_uniform` for differing assessed facts, or `not_assessed` if any candidate is unassessed.

Request projection applies the same carrier and trust checks for planning, dispatch re-projection and prefix-cache copies.

The executor selects a target; lowering owns its history projection.

Neither a declaration nor successful encoding establishes full-loop carriage.

Standard Chat's selected delivery omission below is independent of guessing a Provider history fact.

DeepSeek's [thinking contract](https://api-docs.deepseek.com/guides/thinking_mode/) requires **all previous turns' reasoning** when a thinking request carries tools; Zhipu's documented requirement concerns the current tool cycle.

The shared fact does not collapse those scopes or authorize dropping explicit read-history intent.

Consumer responsibilities remain bounded:

| Consumer | Contract |
|---|---|
| Binding/target declaration | Own endpoint-qualified history semantics; do not inherit native, billing-plan or model defaults from a shared dialect |
| Planning | Check encoded request reachability and whether the client can carry required reasoning; do not claim a stateless caller supplied complete history |
| Request/history projection | Preserve available required history and explicit read intent; apply only selected carrier/scope projections, never reinterpret all-prior as current-cycle-only |
| Delivery | Keep output visibility/carriage separate from history requirements and source observations; protect replay independently |
| Discovery | Publish assessed coarse/common facts without claiming uniform retention scope or full-loop certification |

Every candidate starts with immutable admitted input; an omission for one candidate cannot remove an owner needed by a later candidate. Stateless history is checked through client carriage and replay opening. Experimental managed continuation remains disabled under the [stateful freeze](#stateful-history); no completeness ledger or upstream conversation persistence is added.

### Cross-protocol and cross-Provider history

Cross-protocol or cross-Provider requests may discard historical reasoning, including readable text, summaries and incompatible signed/encrypted owners, even when the model name is unchanged.

Reasoning portability is not an acceptance requirement for these transfers: a matching field or model label does not establish upstream compatibility.

This selected loss avoids forwarding source-specific reasoning that the new target may reject; it does not require a private replay carrier or compatibility layer.

[Current pi behavior](../refs/2026-11-08-chat-reasoning-carriers.md#cross-source-thinking-transfer-2026-11-12) supplies the reference policy.

Apply omission to the candidate-local copy under the existing `*.transfer-visible-history.v1` rules. Preserve ordinary user/assistant text, instructions, tool names, arguments, call/result associations and results. Remove the reasoning owner rather than moving its text into an assistant answer or treating it as a tool result. Compatible same-origin native replay remains eligible for preservation.

For Chat/Responses native OpenAI reasoning, changing only public or upstream model labels within the same service source preserves the available payload and owner dependencies. The gateway delegates model compatibility to upstream validation; it neither promises reuse nor maintains a model-family matrix, previous-model state or a remove-and-retry fallback. [Official cross-model evidence](../refs/2026-11-08-chat-reasoning-carriers.md#openai-cross-model-reasoning-2026-11-12) distinguishes this policy from pi's stricter model-equality filter.

Standard stateless input does not generally identify the original Provider of unsigned reasoning. Projection removes foreign/untrusted replay and uncarriable reasoning shapes; it does not infer every readable owner's provenance or claim pi-style session metadata for all history. Readable reasoning may also be omitted on cross-source transfers without universal provenance tracking or unconditional deletion.

The loss applies to reasoning history, not to visible content or tool execution. Explicit `read_history:true`, malformed recognized envelopes and compacted context retain their existing checks. Ordinary text/call signature delivery follows the separate [stateless boundary](#stateless-api-boundaries).

Reasoning envelopes carrying the former model-specific fingerprint do not match the service-source fingerprint and follow the existing foreign-history omission rule. There is no origin alias or automatic reissue. Visible/tool history remains available; explicit history-read intent still rejects an omission. Strict compaction and other signature identities are unchanged.

### Standard Chat reasoning

Public Chat uses the official Standard dialect. It omits readable and replay-bearing reasoning under `chat.omit-readable-reasoning.v1`, preserving visible content, tool links, usage and terminal authority. Reasoning is never moved into visible text or fabricated as a signature. Required tool-history bindings follow the [pre-dispatch rejection contract](#binding-reasoning-history-contract); only the runtime may choose to disable thinking.

Provider-specific `reasoning_content`, `reasoning_details` and `preserve_thinking` remain upstream dialect concerns. Unknown downstream extension fields are discarded, not retained or forwarded. There is no public reasoning/replay profile or client-specific retention requirement.

| Historical reasoning | Missing Standard Chat carrier |
|---|---|
| Provider-readable `reasoning_content` / `reasoning`, or structured `reasoning_details` | No official assistant-message reasoning-history field; these are Provider extensions |
| Responses `type:"reasoning"` with summary/content, item identity and `encrypted_content` | No equivalent reasoning item or opaque continuation field |
| Messages thinking signatures/redacted data or Google thought-owned signatures | No native signed or encrypted reasoning block |

`reasoning_effort` controls the current computation; `usage` reasoning-token counters report accounting. Neither transports historical reasoning. Missing reasoning carriage alone does not reject an otherwise valid cross-source conversation: use the selected omission above, subject to explicit guarantees and binding admission.

### Stateless API boundaries

Chat and Messages callers submit complete message history. Responses callers submit complete `input`; omitted, null or false `store` is stateless, and every nonnull `previous_response_id` rejects. Explicit `store:false` is recommended for standard clients. The caller retains standard returned owners, call IDs, arguments, results and native reasoning carriers unchanged. No gateway conversation memory is used for this path.

Responses `reasoning.encrypted_content`, Messages `thinking.signature` and `redacted_thinking.data` retain their native opaque purpose through the tagged envelope. Replay still requires compatible target and credential provenance; a process restart with the same binding preserves that identity. Advisory route affinity is not a replay store and does not promise target retention across restart.

The following ordinary attachments lack a lawful standard stateless carrier:

| Upstream field | Owner and required meaning | Missing downstream carrier / disposition |
|---|---|---|
| generateContent `candidates[].content.parts[].thoughtSignature` beside ordinary `text` (`thought` absent/false) | Exact text part, including its boundary, carries native continuation state | Chat message content, Responses `output_text`, and Messages `text` have no corresponding signature field; reject stateless delivery |
| The same `thoughtSignature` beside `functionCall` | Exact function call carries native continuation state | Chat `tool_calls[]`, Responses `function_call`, and Messages `tool_use` have no opaque call-replay field; reject stateless delivery |
| Interactions `step.start.step.signature` or terminal `interaction.steps[].signature` on `type:"function_call"` | The existing scoped decoder represents call-owned continuation | The same call-carrier gap requires rejection if observed. Official ordinary-call field authority and real issuance remain qualified separately; do not infer them from thought signatures |
| Google thought/reasoning signatures | A genuine reasoning owner, distinct from the ordinary attachments above | Use the existing standard reasoning projection where representable; do not reject the whole Provider family or manufacture a reasoning owner for ordinary attachments |

Removing an ordinary signature would lose continuation state; moving it into reasoning, content, call IDs or diagnostics changes its meaning. No private replay extension supplies a substitute. Experimental managed storage does not provide an active escape hatch while the [stateful freeze](#stateful-history) applies.

Reject before dispatch when the selected request/target contract establishes an impossible guarantee. If an actual response first reveals the attachment, JSON fails before successful delivery; an already-published SSE response ends with the standard error mechanism or abort, never a successful terminal or another attempt. The payload-free internal cause is `codec.unsupported.replay`; the public error uses the protocol's upstream-error envelope (OpenAI `upstream_error`, Messages `api_error`). Do not expose signature bytes in errors.

### Compacted history

Compacted conversation summaries are preserved as distinct `Compaction` items with opaque payloads wrapped in `mc1.` envelopes. Compaction items are required context and reject if the target lacks a compaction carrier.

### Stateful history

Upstream and downstream stateful APIs are experimental test features, disabled until the user explicitly enables them. Model adaptation and issue repairs exclude their functional acceptance. No configuration setting or codec capability activates them.

- **Admission:** Responses `store:true` and every nonnull `previous_response_id` return HTTP 400 with `stateful_api_disabled` before history access or upstream dispatch. Omitted/null/false `store` and a null/omitted predecessor are stateless. Other recognized active stateful controls retain their existing rejection.
- **Resources:** authenticated Responses retrieval, deletion and input listing return the same disabled error without consulting or mutating snapshots; missing authentication still returns 401 first.
- **Upstream execution:** send complete history, with Responses and Interactions storage explicitly false and no Provider-held continuation reference. Advisory cache/session grouping remains independent of conversation storage.
- **Tests:** preserve isolated codec, replay, snapshot-lifetime and cancellation invariants. Stateful acceptance scenarios are explicitly ignored as experimental; current SDK/client and real-Provider gates select stateless workflows only.

The dormant [`managed`](../../src/protocol/openai/responses/managed.rs) codec and process-local TTL store retain typed composition and hidden-signature machinery for experimental work. Their existence grants neither public activation nor current acceptance. Google ordinary text/call signatures without a lawful stateless carrier continue to reject; storage cannot silently rescue them.

### Agent session files

Selected direction only; persistence is unimplemented and unscheduled, not a Stage-4 prerequisite. Isolation and lifetime bounds require selection before implementation:

- Per-Agent JSONL directories containing session interaction logs.
- Cross-agent collaboration requires explicit read or copy operations, preserving author attribution and replay boundaries.

