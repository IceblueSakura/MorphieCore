# Architecture

MorphieCore accepts standard client protocols downstream, calls different Provider protocols upstream, and passes everything through one typed semantic representation in between. This page records principles and settled decisions; code and tests define behavior, and the [roadmap](roadmap.md) sets the order of work.

## 1. Target protocols

| Direction | Protocol | Status |
|---|---|---|
| Downstream | OpenAI Responses (primary) | Live |
| Downstream | OpenAI Chat Completions (compatible; declared losses, extension fields for missing semantics) | Live |
| Downstream | Anthropic Messages | After Responses and Chat stabilize |
| Upstream | OpenAI Responses and Chat, including compatible dialects | Live |
| Upstream | Anthropic Messages | JSON and SSE; directory bootstrap activates a binding only when `models` lists it |
| Upstream | Google generateContent, Google Interactions | Planned |

The standalone Embeddings, Images, Speech and Transcription endpoints exist but are frozen: maintenance only, as refactors require.

## 2. Layers and data flow

```text
downstream HTTP (auth, bounded collection)
  → downstream codec: wire → IR request
  → validation / requirements
  → for each candidate of the fixed Route: lowering (project to target caps) → upstream codec: IR → wire
  → transport (trusted origin, auth, timeouts)
  → upstream codec: JSON / SSE → IR response or StreamEvent
  → downstream codec: IR → JSON / SSE → body delivery
```

| Module | Role | Constraint |
|---|---|---|
| `semantic` | IR types, validation, requirements, event reducer | No dependency on protocol, provider or execution |
| `protocol` | Per-protocol codecs, adapters, capabilities and dialects; protocol-owned representation checks and named losses | Pure; projections touch only the candidate's private copy, never network, credentials or registry |
| `lowering` | Cross-protocol `TargetCaps`, shared representation errors and reporting; legacy projections for frozen tasks | Dispatch never replaces the protocol's own checks |
| `adapter` | Generic request, context and replay isolation, dispatch to the selected upstream; legacy adapters for frozen tasks | Rules are chosen by trusted bindings, never by business input |
| `topology` | Static compilation of Public Model → Route → Endpoint | An Endpoint fixes origin, path, protocol, upstream model and credential scope |
| `execution` | Candidate planning, request preparation, incremental intake, delivery state | Does not interpret business semantics |
| `transport` | HTTP I/O and SSE framing | Accepts only prepared, trusted requests |
| `credential` | File store, API keys, OAuth drivers, ordered pools | Credentials never enter the IR |
| `gateway` | Auth, admission, budgets, body publication and cancellation | Loopback only |

`lowering::target::TargetCaps` separates OpenAI targets from native Messages targets; protocol and capability family must match. `protocol::openai::{Adapter, Dialect, TargetCaps}` is the OpenAI entry point, with dialect data in `adaptation` and representation checks, named losses and stream checks in `protocol::openai::lowering`. The projection of native Messages observations onto OpenAI delivery also lives there. `protocol::anthropic` owns the Messages profile, dialect, limits, cache carriers, request projection and session headers; it never borrows the Chat adapter or OpenAI switches. Its replay target derives from the Endpoint's Provider and upstream model, with no extra configuration.

Protocol modules expose only the codec and stream interfaces they need. Native response intake takes the selected history and replay target explicitly. Downstream protocols are added per the roadmap; there is no prebuilt plugin framework.

## 3. IR principles

Long-lived invariants:

- **Ordered heterogeneous items.** Generation history is an ordered list of items (instructions, messages, tool calls and results, reasoning, Provider tool observations). A Message is one container among them. Calls and results link by call ID, never by position or name.
- **Separate identities.** Local item/part IDs, wire IDs, call IDs and stream indexes are distinct; unreported identities are never presented as upstream-reported.
- **Exact JSON.** Tool arguments and schemas keep numeric precision and key order. Raw parsing rejects duplicate keys and bounds depth, nodes and bytes.
- **Presence where it matters.** Absent, null, empty and default are distinguished where meanings differ (tool arguments, schemas, usage), not mechanically for every field.
- **Layered reasoning.** Readable content (summary, text) is separate from opaque continuation payloads. Effort, budget and display are controls, not output.
- **Honest usage.** Unreported values are not zero-filled, cumulative snapshots are not summed, derived values are not stored as reported.
- **Honest terminals.** Response closure, artifact completeness and turn progress (awaiting tool results or done) are separate. Failure, truncation and cancellation never become success.
- **No runtime state.** Selected targets, credentials, retries and commit state stay out of the IR.
- **Context mode.** Clients currently resend the full history each turn (ClientManaged). Gateway-managed, time-limited `previous_response_id` history is the end goal; it will materialize the same full IR and use the same pipeline.

The IR is neither a protocol DTO nor a union of fields. When it cannot express a concept, first present a minimal counterexample and structural options, then decide; never work around it with private adapter fields.

## 4. Adaptation rules

### Field tiers

| Tier | Scope | Handling |
|---|---|---|
| Must preserve | Instructions, message order, tool definitions, arguments, call IDs and result links, required opaque continuations, stop reasons, core usage | Reject when the target cannot express it |
| Degradable | Unsupported sampling parameters, `metadata`, `service_tier`, `logprobs`, usage detail and similar | Remove from the candidate copy by a named rule and record in diagnostics |
| Unknown | Unregistered fields in upstream responses and downstream requests | Ignore; registered fields stay strict. Ignored request field names (never values) are recorded, and nothing unregistered is forwarded. Unknown discriminators (`type`, `role`) are rejected, since ignoring them drops content |

- **Upstream responses:** strict on known fields (type, value, relations), ignore unknown ones. New upstream fields must not break the gateway. Unregistered Responses stream event types are skipped, as Codex does, and only their count reaches diagnostics; registered content and terminal events stay strict.
- **Downstream requests:** expressible fields follow the table; critical semantics the target cannot express return 400 and are never silently dropped. Trusted configuration (`gateway.json`) stays closed. Generation requests from clients decode in an explicit client direction that ignores unregistered fields at any depth and keeps up to 32 names for diagnostics; gateway-built requests and frozen media requests stay closed.
- **Explicit parse direction:** the shared OpenAI reader distinguishes requests, upstream reports and encoder self-checks. Nested items inherit the direction of their parent; SSE snapshot pre-validation counts as upstream. The policy uses no thread-local or ambient state.
- Losses apply only to each candidate's private projection; the source IR is unchanged. Static and streaming paths share the same rules.
- **Append-only projection.** For a fixed target, the projection of earlier history is byte-identical from one request to the next and never depends on later turns; only cache markers may move. Cache hits need it, and Anthropic's preserved-thinking prefix check rejects (400) or drops every thinking block after any earlier edit. Rules keyed on "the current turn" or on request position are therefore not allowed for history.

### Opaque reasoning replay

Every upstream-issued opaque value (OpenAI `encrypted_content`, Anthropic thinking `signature`, Gemini `thoughtSignature`, Interactions step signatures) reaches clients in one **tagged envelope**:

```text
{version, upstream Provider, upstream model, format, raw payload}
```

- The envelope is `mc1.` plus base64url JSON carrying stable Provider/model fingerprints, the native format and the payload. Public model aliases, credentials and ingress keys are not part of the fingerprint.
- On replay, the native payload is restored only when both Provider and model match the current target. Otherwise the value is dropped, along with any reasoning item left empty. Readable reasoning passes to a different target as plain assistant text in every turn, by a named lossy rule, as pi does. The native Messages target does this for any reasoning without its own signature; OpenAI targets still receive it as a reasoning item (roadmap Phase 1). Bare, corrupt or unknown-version envelopes never go upstream.
- No encryption, HMAC or local prefix proof: the tag is a compatibility hint, not authentication, and cannot select route, origin or credential. The issuing upstream validates signatures and rejects tampering.
- A same-target value without a matching carrier is still rejected, not silently dropped.
- Thinking from Anthropic Sonnet 5.5 and Haiku 5.5 is also bound to the producing account; another account's request silently drops it. The envelope stays Provider and model only; target affinity on the exact credential keeps a conversation on its account, and a credential fallback loses that reasoning rather than failing.
- Carriers today: Responses reasoning `encrypted_content` and Chat `reasoning.encrypted` `data`, static and streaming; native Anthropic thinking reuses the same envelope. Google and part/call-level signatures follow the roadmap.

### Chat extension fields

When Chat Completions cannot carry a semantic such as reasoning, MorphieCore follows OpenRouter's existing shapes instead of inventing its own:

- `reasoning` (readable text) and `reasoning_details` (ordered array) on the assistant `message` and stream `delta`. Elements have `type` `reasoning.text` (`text`, optional `signature`), `reasoning.summary` (`summary`) or `reasoning.encrypted` (`data`), plus shared `id`, `format` and `index`.
- Signatures and encrypted payloads use the envelope above. Clients must return `reasoning_details` unchanged; the gateway restores or drops values by the rules above.
- Further extension fields are added one at a time and registered here. The old `_openbridge` attachment does not return.

## 5. Execution and delivery

- Startup compiles Public Model → Route → the legal Endpoints. Business input cannot create candidates or name one; each candidate projects independently from the same immutable IR. Compiled order is the default; trusted runtime policy (cooldown, target affinity on gateway-observed executors) may reorder within the legal set. Fixed candidate order was an initial simplification, not an invariant. Message, tool and schema order inside a projection stays deterministic for cache hits; that order is an invariant.
- Fallback is opt-in, happens only before content is published downstream, and only for advanceable failures: connect/transport errors, timeouts, 5xx, locally unavailable credentials, and a 2xx whose body is not an API reply (an HTML page, an empty body, non-JSON under a JSON type). 401/403 and protocol or projection errors do not advance. The content boundary and the non-API 2xx rule land in roadmap Phase 2; today any published frame freezes and a non-API 2xx is a protocol error.
- Publishing the first content frame freezes the candidate; handing over the body commits; completion requires strict upstream EOF and the final hand-off. Later errors can only abort the body: no splicing another attempt, no fabricated success terminal. Response headers, lifecycle-only frames and SSE comment keep-alives do not freeze the candidate.
- Upstream SSE and downstream delivery are independent: JSON clients get a bounded aggregate, SSE clients get incremental delivery.
- Bodies, frames, event counts and time are bounded. Client disconnect, timeout or shutdown cancels the upstream request.

### Native Messages delivered to OpenAI clients

- Requests are assembled into native messages by item role, preserving the relative order of thinking, `tool_use` and `tool_result` and their call IDs. Stateless defaults such as `store: false` are not sent upstream; `include: ["reasoning.encrypted_content"]` selects the downstream carrier only and does not change native thinking.
- OpenAI reasoning controls map to adaptive thinking: effort → `output_config.effort`, a requested summary → `display: "summarized"`, a disabled summary → `"omitted"`; with no summary, `thinking` stays absent (the 5.x default). Named rules record what does not map: `none`/`minimal` effort sent as `low` (disabling thinking is model-specific on 5.5), concise/detailed summaries as summarized, dropped `reasoning.context`, and dropped `temperature`/`top_p` (5.x rejects non-default sampling). Call IDs outside `^[a-zA-Z0-9_-]{1,64}$` become a stable digest; a collision fails as a duplicate call. Diagnostics list the applied rule names per candidate.
- Every native request carries top-level `cache_control: {"type": "ephemeral"}`, so the API places and advances the breakpoint, like OpenAI's implicit caching. Below the model's minimum prompt length nothing is cached. Breakpoints are placed by hand only if a relay rejects the field.
- Stop reasons: `stop_sequence` completes the turn; `max_tokens` and `model_context_window_exceeded` are truncation; `refusal` is an incomplete answer with reason `content_filter`, so output before it is never presented as complete. `pause_turn` needs server tools and is rejected.
- Native observations stay unchanged. Only the delivery copy drops container boundaries, serializes structured tool arguments to exact JSON strings, and expresses turn progress with OpenAI terminals and tool calls.
- Chat JSON carries at most one leading thinking block, then text, then tool calls. Text after tools or interleaved thinking is rejected rather than reordered. Responses keeps independent item order.
- Cache-inclusive input and total are computed only when input, cache-read, cache-write and output are all reported; otherwise `usage` is `null`. Invalid or overflowing usage still fails.
- The native protocol has no creation time. The OpenAI delivery copy fixes one when the gateway creates the delivery object, shared by all frames, and does not present it as upstream-reported.
- Native SSE maps frame by frame to `StreamEvent`, reusing static stop/usage semantics and the target projection. Text, thinking and tool argument deltas stream; signature fragments do not, and the full payload is enveloped once the block closes and validates.
- `message_stop` is not transport EOF. The success terminal waits for strict EOF, where Chat closes ordinary item state with the final stop reason. A truncated transport aborts the body with no fabricated `completed` or `[DONE]` and no replay. SSE intake can still aggregate into JSON delivery.

## 6. Cache affinity

- The optional `X-MorphieCore-Conversation-Id` header names a Provider-neutral logical conversation; the gateway derives each upstream's session or cache carriers from it.
- Standard `prompt_cache_key` is a separate cache-grouping intent.
- Without a conversation ID, a short-TTL prefix-digest index reuses cache groups on a best-effort basis. It can be dropped at any time, misses only lose cache hits, and it is neither history storage nor part of replay decisions. Histories with opaque reasoning are excluded.
- Implicit cache scope stays isolated by ingress key, credential source and public model; it does not reuse the replay fingerprint.

## 7. Credentials

- Credentials come only from the application's own file store: no keys from environment variables, no third-party auth caches.
- One manager owns API keys and OAuth (Grok, ChatGPT subscription SIWC). Inference uses explicitly configured ordered pools and borrows short-lived access views per fixed identity.
- Requests never trigger login or refresh. Credentials, locators and upstream origins never reach clients.

## 8. Settled decisions

| Decision | Summary |
|---|---|
| One semantic core | One IR serves the gateway and a future Agent library; planning, memory and tool execution belong to the caller |
| Separate tasks | Generation, Embedding, Images, Speech and Transcription each have their own contract, not a universal message container |
| Pure codecs and lowering | No network, credentials or registry; encode from the final IR, never patch JSON afterwards |
| Layered capabilities | Semantic capability, wire representation, execution properties and downstream promises are declared separately; public capability is not the union of candidates |
| Legal candidates, pre-content fallback | Compiled order by default; trusted runtime ordering within the legal set; see §5 |
| Held streams | Follow the structure of Magpie's user-validated held-stream design: while another candidate could answer, hold until first content, keep clients alive with SSE comments, advance on pre-content errors. Decide on typed stream events and trusted bindings, not model-name or byte heuristics |
| Lenient downstream fields | Unknown request fields are ignored and recorded; unknown discriminators are rejected |
| Cross-target thinking as text | On a Provider or model switch, readable reasoning becomes plain assistant text in every turn; opaque values never cross |
| Append-only projection | Earlier history projects byte-identically for a fixed target; see §4 |
| Refusals are answers | A safety refusal is delivered by default and never retried on the same model or another credential of it. A route may opt in to retrying a pre-content refusal on a different model, which then stays sticky for the conversation, as Anthropic recommends. A refusal after content is an incomplete answer |
| Non-API 2xx is a failure | Classified like a 5xx before content is published |
| Target affinity binds the exact credential | The binding names Endpoint, credential identity and upstream model; a different credential is a different cache domain. API keys are identified by binding plus an in-memory material fingerprint, OAuth by verified principal, so key rotation drops the binding and token refresh keeps it |
| Minimal HTTP gateway | Loopback listener, single Bearer ingress auth, no inbound header pass-through |
| Open upstream decoding | Strict on known fields, ignore unknown ones |
| Opaque envelope | No encryption or local proof; restore on same Provider and model, otherwise drop |
| Chat extensions | OpenRouter shapes for semantics Chat cannot express |
| File credentials and ordered pools | Authorization lifecycle is separate from inference borrowing; no automatic refresh or load balancing |
