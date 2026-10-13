# Architecture

MorphieCore translates client and Provider protocols through one typed semantic IR. It is a gateway, not an agent loop: planning, memory and tool execution belong to the caller.

This page records design principles and settled decisions. Code and tests define behavior; [Roadmap](roadmap.md) sets scope and direction.

## Contents

- [Protocols](#protocols)
- [Layers and data flow](#layers-and-data-flow)
- [Semantic IR](#semantic-ir)
- [Adaptation](#adaptation)
- [Reasoning replay](#reasoning-replay)
- [Chat reasoning extensions](#chat-reasoning-extensions)
- [Failover](#failover)
- [Streaming and delivery](#streaming-and-delivery)
- [Native Messages projection](#native-messages-projection)
- [Conversation and cache affinity](#conversation-and-cache-affinity)
- [Credentials and security](#credentials-and-security)
- [Settled decisions](#settled-decisions)

## Protocols

| Direction | Protocols |
|---|---|
| Downstream | OpenAI Chat Completions, OpenAI Responses, Anthropic Messages |
| Upstream | OpenAI Responses/Chat and compatible dialects, Anthropic Messages, Google generateContent, Google Interactions |

Generation supports JSON and SSE. Google upstreams use streaming intake even for downstream JSON delivery. Interactions uses stateless, full-history requests.

Downstream Messages uses a public model's Chat candidates; it does not imply a native Anthropic upstream. Standalone Embeddings, Images, Speech and Transcription have separate contracts and are frozen.

Provider definitions, public models and bindings are maintained in [provider/catalog.rs](../src/provider/catalog.rs), [topology/catalog.rs](../src/topology/catalog.rs) and [gateway bootstrap](../src/gateway/startup/bootstrap.rs). Registration does not prove activation or live account access.

## Layers and data flow

```text
downstream HTTP: authenticate and collect within bounds
  → client codec: wire → IR
  → validate requirements and resolve the trusted route
  → prepare each candidate from the unchanged source IR
  → target projection and upstream codec: IR → wire
  → transport: trusted origin, auth and deadline
  → upstream codec: JSON/SSE → IR response or events
  → client projection and codec: IR → JSON/SSE
  → acknowledged body delivery
```

| Module | Responsibility | Boundary |
|---|---|---|
| `semantic` | Task types, validation, requirements and stream reduction | No protocol, Provider or execution dependency |
| `protocol` | Codecs, dialects, capabilities and named projections | No network, credentials or registry |
| `lowering` | Cross-protocol capability dispatch and representation errors | Protocols own their representation checks |
| `adapter` | Generic requests, replay isolation and upstream dispatch | Trusted bindings select rules |
| `topology` | Compile public model → route → endpoints | Endpoints fix origin, path, model and credential scope |
| `execution` | Candidate planning, preparation, intake and delivery state | No agent planning or tool execution |
| `transport` | HTTP I/O and SSE framing | Prepared trusted requests only |
| `credential` | Private store, API keys, OAuth and ordered pools | No credentials in the IR |
| `gateway` | Ingress, startup, budgets, routing and cancellation | CLI listener is loopback only |

Directories follow the Rust module tree. Keep architectural root boundaries; group internals by domain and expose deliberate entry points. Do not add compatibility-only aliases or a generic plugin framework.

Request transforms stay with their request owner. Stream grammar, state, reduction and materialization share a stream boundary. Chat and Responses own their envelopes and SSE codecs; only genuinely shared content and state logic is shared.

Protocol differences stay in protocol modules. Native Anthropic, Google and Interactions adapters do not borrow OpenAI switches.

## Semantic IR

The IR is neither a protocol DTO nor a union of vendor fields.

- **Ordered items:** instructions, messages, tool calls/results, reasoning and Provider tool observations share one ordered history. A message is one container, not the whole conversation.
- **Explicit links:** calls and results link by call ID, never position or name. Item IDs, part IDs, wire IDs and stream indexes remain distinct.
- **Exact JSON:** arguments and schemas retain numeric precision and key order. Parsing rejects duplicate keys and bounds depth, nodes and bytes.
- **Meaningful presence:** distinguish absent, null, empty and default where semantics differ, not mechanically for every field.
- **Separate reasoning:** readable text and summaries are distinct from opaque continuation values. Effort, budget and display are controls.
- **Shared resources:** request/response resources have one declaration table and scoped references. A URL match does not merge identities or authorize I/O.
- **Owner-local replay:** typed replay belongs to its semantic owner. Resource identity, replay permission and runtime cache scope are different concepts.
- **Honest usage:** missing counts are not zero; cumulative snapshots are not summed; derived counts are not reported counts.
- **Honest terminals:** response closure, artifact completeness and turn progress are separate. Failure, truncation and cancellation never become success.
- **No runtime state:** targets, credentials, retries and publication state stay outside the IR.

Clients currently resend full history each turn. Future gateway-managed `previous_response_id` history will materialize the same full IR and use the same pipeline. Cache affinity is not history storage.

When a concept cannot be expressed, show a minimal counterexample and structural options before changing the IR. Do not hide it in private adapter fields.

## Adaptation

### Field handling

| Class | Handling |
|---|---|
| Must preserve | Preserve instructions, order, tools, arguments, call/result links, required continuations, stop reasons and core usage; reject an unrepresentable target |
| Degradable | Apply a named rule to the candidate copy and record it; never silently patch the source |
| Unknown | Ignore unregistered protocol fields; keep known fields strict and never forward unknown input |

Upstream responses are strict on known types, values and relations. Unknown fields are ignored. Unknown Responses event types are skipped and counted; registered content and terminal events remain strict.

Downstream generation requests ignore unregistered fields and record bounded field names, never values. Unknown content or role discriminators are rejected because ignoring them would lose content. Critical known semantics that a target cannot express return an error. Trusted configuration and frozen-task requests stay closed.

Parsing direction is explicit: client input, upstream reports and encoder checks have different policies. Nested parsing inherits that direction; no ambient or thread-local policy is used.

### Media

Image, audio, video and file resources are inert: inline data or a URL, never fetched, decoded or uploaded by the gateway. Each kind is explicit, because a URL source carries no media type. An inline file keeps its declared MIME, and targets that dispatch by MIME may carry it.

- Chat video uses the registered extension `{"type":"video_url","video_url":{"url":…}}`, shared by OpenRouter, DashScope, Zhipu, vLLM and ModelBest. Only targets whose dialect declares it receive video.
- Per-use options without a target slot (image detail, file detail, filename) are dropped by a named rule. Inbound options with no IR slot that change model-visible content, such as a Messages document `context`, are rejected.
- Tool results may carry media of any kind. Admission needs tool-result media and the kind's input modality. Responses carries tool media; standard Chat does not, so Chat targets reject it.
- Provider file IDs and file URIs are bound to the gateway credential's workspace or project. Client-supplied references are rejected.
- Google protocol boundaries reject media URLs on the Files API host, even when a client labels them as ordinary image or file URLs. Public URLs stay inert.
- Image and file formats, encoded sizes and counts are target constraints, including tool-result media. Pixel dimensions and actual image validity remain upstream checks; the gateway does not decode or transcode images.
- Some standard options are protocol records, not IR fields. A Responses `prompt_cache_breakpoint` on an image or file reaches Responses targets; Chat targets reject it. A Messages image with `transformations.oversized_image: "error"` keeps it on Messages targets and is rejected elsewhere, because other targets may downsize; `downsize` is the default and needs no record.
- A failover route may include candidates narrower than the public contract. Requests they cannot represent skip them, which reduces failover rather than admission.

Carrier tables and sources: [Multimodal carriers](refs/multimodal.md).

### Candidate-local projection

Each candidate starts from the unchanged source IR. Static and streaming paths use the same named rules.

Semantic support, wire representation, execution properties and client promises are separate capability layers. Public capability is not the union of all candidates.

For a fixed target, earlier history projects byte-identically as later turns are added; cache markers may move. Rules must not reinterpret earlier content based on the current turn or request position. This protects cache prefixes and preserved-thinking replay.

Native container boundaries may be omitted by a named client projection. Ordered items, exact arguments and call/result identity remain intact; a later Chat projection does not recover the original container ownership.

## Reasoning replay

Opaque upstream values reach clients in a tagged envelope:

```text
mc1. + base64url JSON: {o: issuer fingerprint, f: native format, d: payload}
```

The fingerprint binds Provider and upstream model. It does not include public aliases, credentials or ingress keys. Clients must treat the whole value as opaque.

1. Restore the native payload only for the matching Provider/model and carrier.
2. Drop foreign payloads and reasoning items left empty by that drop.
3. Preserve readable foreign reasoning as assistant text through a named rule.
4. Never send bare, malformed or unsupported-version envelopes upstream.
5. Reject a same-target value if its required carrier is unavailable.

Native Messages sends unsigned or foreign thinking as text. OpenAI targets do so when an opaque replay value was dropped; reasoning that never carried a value keeps its kind when representable. A changed item kind gets a derived identity in the candidate copy, not the issuer's identity.

The envelope is not encryption, authentication or local proof. It cannot choose a route, origin or credential. The issuing upstream validates its payload.

Some native thinking is account-bound. Exact-credential affinity helps preserve it, but Provider/model compatibility alone cannot guarantee replay after credential fallback.

Carriers include Responses `encrypted_content`, Chat encrypted reasoning details, Anthropic thinking signatures, Gemini `thoughtSignature` and Interactions signatures.

Google signatures travel as reasoning items. The IR has no part- or call-level replay slots; Provider-tool replay stays separate.

- A generateContent value names its anchor: call ID, thought or tail. The encoder restores it by anchor, not position.
- An Interactions value is a thought signature or a call-ID-anchored signature.
- Signature-only items trail the response, so a message stays contiguous with its calls. Chat carries them ahead of the message.
- Preserve every signature, even when an upstream sample accepts an omitted one.
- generateContent and Interactions are separate formats. A value of the other format is dropped as foreign, not translated, so failover between the two bindings loses reasoning instead of failing.

## Chat reasoning extensions

Chat uses OpenRouter shapes for semantics its standard fields cannot carry:

| Carrier | Meaning |
|---|---|
| `reasoning`, `reasoning_content` | Readable reasoning view |
| `reasoning_details` | Ordered reasoning items for replay |
| `reasoning.text` | `text`, optional `signature` |
| `reasoning.summary` | `summary` |
| `reasoning.encrypted` | Opaque `data` |

Details may also carry `id`, `format` and `index`. Signatures and encrypted data use the replay envelope above. Clients return details unchanged.

One assistant message carries the reasoning items ahead of it. A replay value closes its item; adjacent items without values merge. The readable view concatenates readable details.

Streamed readable deltas carry both readable views and no empty opener. Client input does not rely on returned indexes or format labels because target clients merge fragments; duplicate readable views must agree.

Register new extension shapes here one at a time. The old `_openbridge` attachment is rejected and never emitted.

## Failover

### Legal candidates

Startup compiles a public model to a route with legal endpoints and credential candidates. Business input cannot create candidates or name an executor.

Compiled order is the default. Runtime policy may prefer the conversation's last successful executor and move cooling executors to the end. Cooling candidates remain a last resort. This is failover, not load balancing.

An executor is the exact credential × Provider × upstream model. API-key rotation changes its identity; OAuth token refresh preserves the verified principal. A different credential is a different cache domain.

### Advancement

Advancement requires enabled policy, an unpublished body, a remaining candidate and remaining attempt/time budget.

| Failure | Advancement |
|---|---|
| Transport/connect failure, timeout, server failure, unavailable credential | Advance where the route or credential policy permits |
| Exhausted quota | Next key or another Provider, where enabled |
| Rate limit | Another Provider; do not rotate keys for unknown quota scope |
| Authentication/permission failure, context overflow, request projection failure | Do not advance |
| Safety refusal | Deliver it; never retry on another model |

A non-API 2xx reply, such as HTML or an empty body, is not success. Invalid or truncated upstream intake can advance while unpublished. Downstream projection failures do not acquire retry permission.

Rate limits, exhausted quota and rejected credentials cool an executor. Server errors and timeouts do not. Health and conversation affinity are bounded in-memory state.

### Routing unit

The declared conversation is the routing unit. Turns, messages, branches and inferred subtasks are not routing keys.

After honest completion, later requests prefer that executor. A successful failover updates affinity. There are no weights, rotations, concurrency lanes or load-spreading rules.

## Streaming and delivery

When another candidate could answer, lifecycle-only frames are held before publication. Content releases the buffer; byte and elapsed-time bounds also release it. This lets a pre-publication failure advance without exposing the attempt.

Publishing any body frame freezes advancement. Body handoff acknowledges commit. Completion requires validated upstream EOF and the final handoff.

After publication:

- Never splice another candidate into the body.
- A classified upstream failure may produce a protocol failure frame.
- Other failures abort delivery.
- Never invent a successful terminal or replay the request.

A Responses stream with context overflow or exhausted quota can return HTTP 200 with `response.failed` even before normal output, because Codex reads those classes from stream events.

Upstream streaming and client delivery are independent. JSON clients receive a bounded aggregate; SSE clients receive incremental output. A native stop event is not transport EOF.

Bodies, frames, event counts and deadlines are bounded. Client disconnect, timeout or shutdown cancels upstream work. Held-stream keep-alives remain [deferred](roadmap.md#only-if-needed).

## Native Messages projection

Native requests preserve thinking, tool-use/result order and call IDs. Messages owns its profile, cache controls, limits and session headers.

### Request rules

- Effort maps to adaptive thinking. `none`/`minimal` map to `low`; budgets use a named effort projection.
- Summary intent maps to summarized or omitted display. Unsupported reasoning context and sampling controls are dropped by named rules.
- A leading developer instruction maps to the system prompt by name.
- Call IDs outside the native grammar map to stable digests; collisions fail rather than merge calls.
- Each native request uses top-level ephemeral cache control. Cache hits still depend on upstream thresholds.
- A source without an output cap gets `max_tokens: 32000`, the default of opencode's Messages client, because Messages requires the field.
- OpenAI stateless controls are not forwarded as native fields. Requesting an encrypted reasoning carrier selects delivery, not native thinking.

Exact rules live in [semantic_request.rs](../src/protocol/anthropic/semantic_request.rs).

### Response rules

Native observations stay unchanged. OpenAI delivery projects a copy:

- Chat requires leading thinking, then text, then calls; it rejects unrepresentable interleaving. Responses retains independent item order.
- Structured tool arguments become exact JSON strings; call/result links remain intact.
- Max-token/context limits are truncation. Refusal after content is incomplete. Unsupported server-tool continuation is rejected.
- Cache-inclusive usage is computed only from complete reported operands; missing usage is not invented.
- Delivery generates required IDs/timestamps only on its own copy, stable within the response. They are not upstream facts or stored-history keys.
- Text, thinking and tool arguments stream incrementally. Full signatures are enveloped after block validation.

## Conversation and cache affinity

The explicit `X-MorphieCore-Conversation-Id` header takes precedence over agent session headers. See [Usage](usage.md#conversation-and-cache-affinity) for accepted headers and constraints.

The gateway derives Provider-specific cache/session carriers; it does not read Provider session headers as conversation authority. Standard `prompt_cache_key` is a separate cache-grouping intent.

Without a declared conversation, a short-lived prefix-digest index offers best-effort cache-group reuse. It stores no bodies, selects no credential and restores no history. Opaque-reasoning histories are excluded.

Implicit cache scope is isolated by ingress key, credential source and public model. It is independent of the replay fingerprint. Losing the index loses cache hits, not correctness.

## Credentials and security

Credentials come only from the application's private store. A manager owns API keys, OAuth identities and ordered pools. Inference borrows access for a fixed identity; it never logs in or refreshes.

Ingress accepts one configured token via Bearer or `x-api-key` and authenticates before body collection. Business data cannot select an origin, credential or auth/proxy header. Inbound headers are not passed through.

Transport uses trusted prepared requests, no redirects and no implicit HTTP retries. Environment proxies require explicit startup opt-in. Credentials, locators, private config and upstream bodies never reach clients.

## Settled decisions

| Decision | Consequence |
|---|---|
| One semantic core | Gateway and future agent library share the IR; callers own agent behavior. Beyond context edits and response continuation, agent-only machinery joins the IR when the library needs it |
| Separate task contracts | Generation, Embeddings, Images, Speech and Transcription are not one universal message type |
| Pure protocol boundaries | Encode from final typed input; no network in codecs or post-encoding JSON patches |
| Functional ownership | Keep root layers; organize internals by domain, without compatibility aliases or plugin frameworks |
| Layered capabilities | Public promises are not the union of candidate features |
| Declared adaptation | Named candidate-local losses; known fields strict, unknown protocol fields ignored |
| Append-only history projection | Earlier content does not change when a later turn is added |
| Scoped opaque replay | Restore only compatible native values; foreign readable reasoning becomes text |
| Failover, not balancing | Legal candidates, bounded advancement and exact-credential affinity |
| Conversation routing | No inferred turn, branch or message routing |
| Honest delivery | Publication forbids advancement; success requires validated EOF and handoff |
| Agent-shaped failures | Sanitized error classes match pinned client behavior; refusals are not retries |
| Delivery-local identity | Generated client IDs/timestamps are not upstream facts or history keys |
| Stateless Google Interactions | `store: false` and full history; no `previous_interaction_id` or revision header. Signatures keep their native formats |
| Separate credential lifecycle | Private file store and ordered pools; no inference-triggered login or refresh |
| Agent-sized bounds | Bodies, IR budgets and deadlines fit whole-history agent sessions and long turns: tens of MiB and one hour per exchange. Upstreams enforce their own maxima |
| Inert, credential-free media | Inline data or URLs only; no gateway fetch or file service, and no client-supplied Provider file references |
| Standard carriers only | A target without a standard carrier for a semantic rejects it. The gateway does not move content to another role or add model-visible text to make it fit; for example, Chat targets reject tool-result media |
| Target-local image restrictions | Formats, encoded sizes, counts and unavailable per-use options are checked or projected at the protocol boundary, not encoded as Provider-specific IR fields |
| Client-owned output caps | The gateway neither caps nor invents `max_tokens`; the upstream enforces its model limit. Only a protocol that requires the field fills a fixed default |

Pinned clients and protocol sources are in [References](references.md). Observed client behavior is an oracle, not permission to weaken security or fabricate semantics.
