# Architecture

MorphieCore translates client and Provider protocols through a typed semantic IR. It functions as a gateway, not an agent runtime: planning, agent memory, and tool execution belong to the caller.

This page defines accepted principles, domain contracts, and architectural decisions. [Roadmap](roadmap.md) defines scope, priorities, and acceptance gates; [API expansion design](api-expansion-design.md) tracks unselected proposals. Configuration and operational commands belong to [Usage](usage.md) and [Development](development.md).

## Contents

- [System boundaries](#system-boundaries)
  - [Protocols](#protocols)
  - [Shared Provider metadata](#shared-provider-metadata)
  - [Layers and data flow](#layers-and-data-flow)
    - [Domain vocabulary and failure contracts](#domain-vocabulary-and-failure-contracts)
- [Semantic contracts](#semantic-contracts)
  - [Semantic IR](#semantic-ir)
    - [Canonical ownership](#canonical-ownership)
    - [Instruction authority and canonical roles](#instruction-authority-and-canonical-roles)
    - [Participant ownership](#participant-ownership)
    - [Tool examples and document context](#tool-examples-and-document-context)
    - [Citation point anchors](#citation-point-anchors)
    - [Responses item and event observations](#responses-item-and-event-observations)
    - [Google foreground intake](#google-foreground-intake)
    - [Scoped accounting](#scoped-accounting)
    - [Cached context references](#cached-context-references)
    - [Safety policy and reports](#safety-policy-and-reports)
  - [Independent task boundaries](#independent-task-boundaries)
    - [Decision API direction](#decision-api-direction)
    - [Token counting policy](#token-counting-policy)
    - [Independent Embedding path](#independent-embedding-path)
      - [Embedding encoding compatibility](#embedding-encoding-compatibility)
      - [Google Embedding compatibility](#google-embedding-compatibility)
- [Adaptation](#adaptation)
  - [Source and target boundaries](#source-and-target-boundaries)
    - [Protocol baselines and Provider dialects](#protocol-baselines-and-provider-dialects)
    - [Standard request boundary](#standard-request-boundary)
    - [Protocol tree ownership](#protocol-tree-ownership)
      - [Shared Aliyun billing dialect](#shared-aliyun-billing-dialect)
      - [Subscription Responses](#subscription-responses)
    - [Field handling](#field-handling)
    - [Client Responses history shorthand](#client-responses-history-shorthand)
    - [Candidate-local projection](#candidate-local-projection)
    - [Model-effective values](#model-effective-values)
  - [Request and reported meaning](#request-and-reported-meaning)
    - [Conversion ladder](#conversion-ladder)
    - [Semantic-equivalence compatibility](#semantic-equivalence-compatibility)
    - [Client/IR projection principles](#clientir-projection-principles)
    - [Downstream schema completion](#downstream-schema-completion)
    - [Reported text configuration](#reported-text-configuration)
    - [Message phase transfer](#message-phase-transfer)
    - [Explanatory terminal reports](#explanatory-terminal-reports)
    - [Service policies and reports](#service-policies-and-reports)
    - [Lifecycle settings and service reports](#lifecycle-settings-and-service-reports)
    - [End-user attribution](#end-user-attribution)
    - [Selected request approximations](#selected-request-approximations)
    - [Messages terminal projection](#messages-terminal-projection)
  - [Reasoning controls](#reasoning-controls)
    - [Reasoning activation and effort scope](#reasoning-activation-and-effort-scope)
      - [Google response-effort projection](#google-response-effort-projection)
    - [Reasoning level calibration](#reasoning-level-calibration)
      - [Thinking-budget quantization](#thinking-budget-quantization)
      - [Bounded token-budget adaptation](#bounded-token-budget-adaptation)
      - [Initial mapping plan](#initial-mapping-plan)
      - [Implementation owners](#implementation-owners)
  - [Media](#media)
  - [Adaptation evidence](#adaptation-evidence)
    - [Domain evidence interface](#domain-evidence-interface)
    - [Atomic evidence and output batches](#atomic-evidence-and-output-batches)
    - [Typed conversion failures](#typed-conversion-failures)
    - [Public capability and adaptation notice](#public-capability-and-adaptation-notice)
    - [Generation Text model discovery](#generation-text-model-discovery)
- [History and replay](#history-and-replay)
  - [Reasoning replay](#reasoning-replay)
    - [Binding reasoning-history contract](#binding-reasoning-history-contract)
  - [Chat reasoning extensions](#chat-reasoning-extensions)
  - [Compacted history](#compacted-history)
  - [Stateful history](#stateful-history)
  - [Agent session files](#agent-session-files)
- [Native Messages projection](#native-messages-projection)
  - [Request rules](#request-rules)
  - [Missing output ceilings](#missing-output-ceilings)
  - [Owner-local cache admission](#owner-local-cache-admission)
  - [Caller-selected automatic cache admission](#caller-selected-automatic-cache-admission)
  - [Response rules](#response-rules)
- [Runtime and trust](#runtime-and-trust)
  - [Failover](#failover)
    - [Legal candidates](#legal-candidates)
    - [Request conversion reuse](#request-conversion-reuse)
    - [Advancement](#advancement)
    - [Health and recovery](#health-and-recovery)
    - [Routing unit](#routing-unit)
  - [Conversation and cache affinity](#conversation-and-cache-affinity)
  - [Streaming and delivery](#streaming-and-delivery)
    - [Semantic stream normalization](#semantic-stream-normalization)
    - [Reverse-proxy delivery](#reverse-proxy-delivery)
    - [Optional delta coalescing](#optional-delta-coalescing)
  - [Credentials and security](#credentials-and-security)
  - [Agent identity and cluster diagnostics](#agent-identity-and-cluster-diagnostics)
    - [Identity and authentication](#identity-and-authentication)
    - [Shared history and usage attribution](#shared-history-and-usage-attribution)
  - [Observability](#observability)
  - [Configurable exchange capture](#configurable-exchange-capture)
- [Settled decisions](#settled-decisions)
  - [Scope and activation decisions](#scope-and-activation-decisions)
  - [Semantic ownership decisions](#semantic-ownership-decisions)
  - [Projection and reasoning decisions](#projection-and-reasoning-decisions)
  - [History and cache decisions](#history-and-cache-decisions)
  - [Routing and security decisions](#routing-and-security-decisions)
  - [Delivery and evidence decisions](#delivery-and-evidence-decisions)

## System boundaries

### Protocols

| Direction | Supported Protocols |
|---|---|
| Downstream | OpenAI Chat Completions, OpenAI Responses, Anthropic Messages |
| Upstream | OpenAI Responses/Chat and compatible dialects, Anthropic Messages, Google generateContent, Google Interactions |

Generation supports JSON and SSE transport. Google upstreams use streaming intake even for downstream JSON delivery. Interactions uses stateless, full-history requests.

All three Generation client protocols dispatch through the public model's configured candidate route. A Messages client request does not require a native Anthropic upstream. Standalone tasks (Embeddings, Images, Speech, Transcription) operate on distinct contracts outside Generation.

- **Official Protocol Baselines:** Public Chat, Responses, and Messages adhere to current official versioned specifications. Provider-specific syntax and deviations belong strictly to trusted upstream adapters.
- **Diagnostic Extensions:** Optional namespaced response fields (`morphiecore`) provide safe-to-ignore telemetry and loss reporting. Standard SDK agent loops operate without parsing or returning these extensions.
- **Messages Version Policy:** Downstream Messages decodes the current `2026-10-21` body grammar and requires exactly one `anthropic-version` header (including `?beta=true`). Accepted revision (2026-10-30, implementation pending): the dated `2023-06-01` version is also admitted over the same current grammar, its removed sampling fields (`temperature`, `top_p`, `top_k`) take a named omission, and `anthropic-beta` is ignored as a header while beta body fields stay governed by body rules. Until implemented, other versions and nonempty beta lists reject before body collection.

Provider metadata resides in [providers.json](../src/provider/providers.json). Public models and bindings are defined in [topology/catalog.rs](../src/topology/catalog.rs) and assembled at [gateway startup](../src/gateway/startup/bootstrap.rs).

### Shared Provider metadata

`src/provider/providers.json` is the authoritative source for Provider origins, paths, authentication schemes, and default credential pools. It is embedded at compile time via `include_bytes!`, validated once, and immutable at runtime.

| Field | Contract |
|---|---|
| `base_url` | Trusted origin only; no API prefix, query, fragment, or credentials |
| `auth` | `{ "kind": "bearer" }`, `x_api_key`, `x_goog_api_key`, or `{ "kind": "oauth", "profile": "openai" \| "grok" }` |
| `credential_pool` | Default local pool name; never a secret or business selector |
| `endpoints` | Closed map: `models`, `chat_completions`, `responses`, `messages`, `generate_content`, `interactions`, `embeddings`, `images`, `speech`, `transcription` |
| `path` | Concrete path relative to `base_url`; omitted operations are undeclared |
| `auth` (endpoint) | Optional header override; cannot alter credential kind or OAuth profile |
| `sources` | Reference URLs documenting the upstream interface |

### Layers and data flow

```text
Downstream HTTP intake (authenticate and bound body collection)
  → Client Codec: Wire → IR
  → Validate requirements and resolve trusted route
  → Prepare candidate from immutable source IR
  → Target Projection & Upstream Codec: IR → Wire
  → Transport: Trusted origin, auth header, deadlines
  → Upstream Codec: JSON/SSE → IR response or stream events
  → Client Projection & Codec: IR → JSON/SSE
  → Acknowledged body delivery
```

| Module | Core Responsibility | Invariant Boundary |
|---|---|---|
| `semantic` | Task definitions, validation, invariants, and stream reduction | No protocol, Provider, or network dependencies |
| `protocol` | Wire codecs, dialects, envelopes, and target projections | No I/O, credential, or runtime execution dependencies |
| `lowering` | Cross-protocol target dispatch and representation checks | Protocols own their specific task projections |
| `adapter` | Protocol codec dispatch based on target binding rules | Pure transformations; no I/O or publication state |
| `topology` | Model routing table: public model → route → endpoints | Endpoints define fixed origins, paths, and credential scopes |
| `execution` | Candidate attempt planning, lifecycle, and delivery state | No agent loop, tool execution, or prompt planning |
| `transport` | HTTP client execution and SSE framing | Executes prepared trusted requests only |
| `credential` | Credential store, API keys, OAuth tokens, and pools | Credentials never enter semantic IR |
| `gateway` | Ingress listener, startup, budgets, routing, and cancellation | Ingress binds loopback only |

#### Domain vocabulary and failure contracts

Internal abstractions maintain unambiguous terminology:
- **API:** Official wire specification.
- **Dialect:** Vendor-specific wire deviations from a baseline API.
- **Profile:** Explicit admission or presentation configuration.
- **Candidate:** Potential upstream route target; an **Attempt** executes a candidate.
- **Projection:** Candidate-local target transformation derived from source IR.
- **Publication:** Irreversible commitment point where body frames are flushed to the client, freezing failover advancement.

Domain errors are strongly typed with stable machine-readable codes. Source failure classifications (`SourceFailureKind`), recovery decisions (`RecoveryDecision`), and client error presentations remain distinct coordinates. Public error bodies preserve official protocol envelopes.

## Semantic contracts

### Semantic IR

Semantic IR represents the union of official protocol concepts (OpenAI, Anthropic, Google). Concepts supported by at least one official standard have an authoritative owner in the IR.

- **Ordered items:** Instructions, messages, tool calls/results, reasoning, compaction items, and tool observations share a single ordered timeline.
- **Text-only instructions:** Application instructions consist exclusively of plain text. Non-text instruction payloads are rejected at intake.
- **Role-scoped content:** Media belongs to user inputs and tool results; assistant media requires dedicated output owners. Citations and token probabilities belong to assistant output; refusals belong to assistant messages.
- **Explicit links:** Tool calls and results link strictly via call IDs, never by positional inference or function name.
- **Exact JSON:** Tool arguments and JSON schemas preserve numeric precision and key order. Duplicate keys and unbounded nesting reject.
- **Meaningful presence:** Explicit distinction between absent, null, empty, and default states where semantically meaningful. Accepted simplification (2026-10-30, migrating per field): IR fields use `Option<T>`; intake normalizes documented null defaults (L2) and encoders emit the target grammar (required-nullable fields as `null`, optional fields omitted). A tri-state type remains only where an official contract gives null its own meaning, such as an explicit clear. IR placeholders that exist only to reject unsupported operations are removed; intake rejects them directly.
- **Reasoning separation:** Human-readable thought text, summaries, and opaque replay signatures are distinct typed owners.
- **Honest accounting & terminals:** Missing usage counters are left unknown rather than zeroed. Truncation, failure, and cancellation are never mapped to success.
- **Runtime isolation:** Credentials, retry counters, and target URLs remain strictly outside semantic IR.

#### Canonical ownership

- `UsageReport.counts` owns aggregate counts; modality details reside in `details.modalities`. Flat views are derived.
- `ReasoningRequest.presentation` owns visibility and summary granularity. Activation, effort scope, budgets, readable text, and opaque tokens remain independent.
- `Part.cache` owns prefix breakpoints as `RequestLifetime` or `Ttl(PrefixCacheTtl)`.
- `ResourceDescription::Image.size_policy` controls input image sizing (`AllowDownsize` vs `RejectOversized`).

#### Instruction authority and canonical roles

The IR establishes canonical `Developer` meaning for application instructions, unifying legacy OpenAI `system`, modern OpenAI `developer`, and Anthropic `system`.

- **Source normalization:** Standard OpenAI `system` and `developer` messages normalize to canonical `developer` in IR. Anthropic root `system` strings and blocks normalize to request-wide root application instructions.
- **Mixed roles:** Consecutive or interleaved system/developer instructions normalize in place without reordering or merging.
- **Root vs. History lifetime:** Root instructions are request-level settings. Continuation requests that omit root instructions do not erase retained ordered instructions.

Target encoding maps canonical instructions to native carriers:

| Target Contract | Root Instructions Carrier | Ordered Instructions Carrier |
|---|---|---|
| OpenAI Chat | Leading `developer` message | `developer` messages at current positions |
| OpenAI Responses | Top-level `instructions` field | `developer` input items at current positions |
| Anthropic Messages | Top-level `system` string/blocks | Mid-conversation system messages where supported |
| DeepSeek Chat / Bailian Chat | Leading `system` message | `system` messages (dialect mapping) |
| DeepSeek Responses | Top-level `instructions` field | `system` input items |
| Zhipu Chat | String-only `system` message | String-only `system` messages |

#### Participant ownership

`Instruction.participant` and `Message.participant` carry model-visible participant identifiers (such as OpenAI Chat `name`).
- Exact 1:1 mapping for OpenAI Chat `system`, `developer`, `user`, and `assistant` names.
- Targets without participant carriers reject requests containing participant names rather than silently omitting them.

#### Tool examples and document context

- `FunctionTool.input_examples` holds bounded, ordered tool input examples. Mapped natively to Anthropic Messages input examples.
- `FileDescription.context` holds per-use model-visible document context for file resources.

#### Citation point anchors

`ClaimAnchor::Point(TextPoint)` represents a character insertion boundary in a text part with explicit byte or Unicode units, distinct from range anchors or file list indices.

#### Responses item and event observations

- `created_by` attributes function results and compaction items in Responses.
- Event-only observations (`PartInterrupted`, `CompactionProgress`) are emitted during streaming without creating synthetic static history fields.
- `ResponseError.original_type` retains upstream error classifications on failed response snapshots.

#### Google foreground intake

- Intake for `generateContent` and `Interactions` accepts optional stream lifecycle IDs and started statuses.
- Final envelope authority governs stream completion: terminal outcomes require strict upstream EOF before publication.
- Tool results with `is_error` are mapped without inferring execution failure from result text.
- Structured function responses in `generateContent` preserve JSON objects directly under `functionResponse.response.output`.

#### Scoped accounting

`UsageReports` maintains the latest snapshot per operation or step scope without summing across distinct execution boundaries:
- Missing counts remain unknown; zero is recorded only when explicitly reported by the upstream.
- Derived operations (such as total tokens = input + output) occur only where established by upstream protocol contracts.
- Messages stream intake processes incremental usage checkpoints, emitting the final usage report at message closure.

#### Cached context references

`GenerationRequest` supports explicit cached input base references (`cached_content`), retaining opaque identifiers and reported expiration timestamps. The gateway does not perform client-side cache creation, fetching, or renewal.

#### Safety policy and reports

`GenerationSettings` carries probability-based category filtering. `GenerationResponse` retains ordered `SafetyReport` observations independently of generated content:
- Assessment-only reports with no blocking signals are omitted from downstream delivery via `generation.omit-safety-assessment.v1`.
- Blocking safety reports reject delivery and yield classified terminal errors.

### Independent task boundaries

Non-Generation tasks maintain independent request, execution, and result lifecycles.

#### Decision API direction

Dual-SDK client access (TypeSafe Decision SDK and OpenAI Decision SDK) backed by a unified Decision IR.
- Interoperability across TypeSafe, OpenAI, Perplexity, and Cloudflare.
- Capability-union design: unambiguous typed owners for question rubrics, confidence scores, and structured justifications.
- Independent synchronous execution without dependencies on Generation routing.

#### Token counting policy

Token counting (`count_tokens`) adheres to an upstream-first policy:
- Prefer upstream-reported token counts from target models.
- When unavailable or inaccurate, use a documented character-based heuristic (`ceil(bytes / 4)`).
- The gateway does not run local BPE tokenizers or reconstruct proprietary model templates.

#### Independent Embedding path

Operational on `POST /v1/embeddings`, decoupled from Generation routing:
- Supports text input samples, dense vector output, and request-level dimensionality constraints.
- Bound targets: DashScope text models, OpenAI `text-embedding-3-small` (via OpenRouter), and Google `gemini-embedding-2`.
- Direct single-attempt execution; no failover across distinct embedding spaces.

##### Embedding encoding compatibility

| Upstream Vector Format | Downstream Format | Conversion Policy |
|---|---|---|
| Float JSON | Float JSON | Direct numeric pass-through |
| Base64 | Float JSON | Decode IEEE 754 binary32, widen to binary64, format JSON |
| Base64 | Base64 | Exact standard Base64 transfer |
| Float JSON | Base64 | Direct rounding to binary32 (`embedding.float-json-to-binary32.v1`) |

##### Google Embedding compatibility

Google `gemini-embedding-2` uses native synchronous `batchEmbedContents` (up to 100 inputs, 60,000 total values):
- `embedding.bound-model-label.v1`: Delivers public bound model name when upstream omits model ID.
- `embedding.input-only-total-from-prompt.v1`: Derives total tokens from input count when total is omitted.
- `embedding.estimate-missing-usage.v1`: Supplies request-local usage estimates when upstream omits usage counters.

## Adaptation

### Source and target boundaries

#### Protocol baselines and Provider dialects

```text
Decode: Wire → Dialect Normalization → Protocol Baseline + Declared Extensions → IR + Fidelity
Encode: IR + Eligible Fidelity → Target Baseline + Declared Extensions → Dialect Encoding → Wire
```

- **Baseline:** Standard official specification at a pinned release.
- **Dialect:** Target-specific normalization (e.g. Aliyun Bailian, Grok Responses, SIWC).
- Codecs remain pure: dialect transformations operate on in-memory representations without network I/O.

#### Standard request boundary

Four decoupled responsibilities:
1. **Client intake:** Enforces strict official protocol syntax; ignores SDK-specific helper properties.
2. **Semantic IR:** Holds normalized cross-protocol meaning.
3. **Target encoding:** Validates target-specific constraints and formats native requests.
4. **Provider egress dialect:** Translates standard target requests to vendor wire formats where required.

#### Protocol tree ownership

Module layout mirrors `vendor / api / responsibility`:
- `protocol::openai::{chat, responses}`
- `protocol::anthropic::messages`
- `protocol::google::{generate_content, interactions}`

##### Shared Aliyun billing dialect

DashScope and Aliyun TokenPlan share the `Bailian` wire dialect for Chat and Responses:
- Readable reasoning mapped to `reasoning_content` with `preserve_thinking`.
- Total output ceilings mapped to `max_completion_tokens`.

##### Subscription Responses

SIWC (Sign-in with ChatGPT) and Grok use Responses endpoints with stateless upstream execution (`store: false`):
- Prompt cache keys mapped via `prompt_cache_key`.
- Streaming intake required for all client requests; terminal output reconstructed from event state.

#### Field handling

Known fields follow the [conversion ladder](#conversion-ladder); this table states how intake classes enter it.

| Classification | Gateway Action |
|---|---|
| **Content and explicit guarantees** | Instructions, messages, media, tools, arguments, call links, output constraints and explicit strictness: same-name or same-semantic carriers, or a user-selected approximation such as tool-error wrapping or visible-history transfer; otherwise reject |
| **Controls and hints** | Sampling, effort, presentation, cache and service hints: walk the ladder through bounded approximation and named omission on candidate copies |
| **Reports and metadata** | Delivered report facts without a client carrier take named omission; they never fail delivery |
| **Unscheduled capabilities** | Stateful, hosted-execution and other unscheduled capabilities keep their [retained rejections](roadmap.md#retained-rejections) until scheduled |
| **Legacy standard carrier** | Convert known historical fields when semantics are unambiguous |
| **Unknown extension field** | Accept and discard at valid object positions; do not forward or store |
| **Malformed known value** | Grammar error; outside the ladder |

#### Client Responses history shorthand

- Assistant text shorthand strings are accepted at intake.
- Output items in history require standard fields (`type`, `id`, `role`, `content`, `status`). Missing annotations arrays in user/tool items reject.

#### Candidate-local projection

Transformations are candidate-private:
- Source IR remains immutable across multiple failover attempts.
- Projections maintain history-prefix stability: appending new turns does not alter the projection of earlier conversation turns.

#### Model-effective values

Target capabilities enforce bounded ceilings via `ModelLimits` (maximum output tokens, allowed reasoning efforts). Candidate projection clamps an output ceiling above a known model maximum under a named rule, keeping an honest length terminal when reached; an unsupported effort level resolves to the nearest [supported level](#reasoning-level-calibration). Final validation checks the projected request. Combinations without such a bounded resolution still reject.

### Request and reported meaning

#### Conversion ladder

Accepted on 2026-10-30; it replaces the earlier exact-first fallback and the fail-closed "known unsupported" class. Every known field, control, report and history owner crossing a protocol or target boundary takes the first applicable level:

| Level | Name | Rule | Examples |
|---|---|---|---|
| L1 | Same-name | The target carries the same field with the same meaning | Chat `stop` to Chat; Responses `reasoning.effort` to Responses |
| L2 | Same-semantic | A different carrier or shape with identical meaning; no loss receipt ([semantic equivalence](#semantic-equivalence-compatibility)) | Documented null/empty defaults; disabled flags; a parameterless function as an empty object schema; `stop` beside calls as `tool_use` |
| L3 | Partial lossy | A bounded approximation preserving the main intent, under a named rule with evidence | Nearest supported effort; conditional strictness as non-strict; uncarried reasoning omitted while visible history stays |
| L4 | Omit | No expressible meaning on the target, and dropping it cannot make the response appear to satisfy something it did not; named omission | Sampling and verbosity controls, cache hints, metadata and service hints, carrier-less reports |
| L5 | Reject | Inexpressible, and omission would let the client mistake the response for satisfying its request | Media the model must see, `n > 1`, tools or explicit output constraints with no carrier, explicit strict guarantees |

**Burden of proof.** Rejection requires the L5 reason; admission requires no prior equivalence evidence. Field-specific evidence selects among L1–L4 and names the rule.

**Unknown facts.** When a target fact is unknown, such as an unassessed binding history contract or unknown schema compatibility, the gateway sends the carriable form and lets upstream validation decide (delegation). Delegation never applies to trust, replay origin or credentials.

**Timing.** Request-side levels are resolved per candidate during planning, on candidate-local copies, so request-determined impossibility fails before dispatch (P4). After dispatch the upstream has already generated output; report and metadata facts bottom out at L4, and only content or outcome that would be misrepresented may fail delivery.

**Invariants outside the ladder:** credentials, auth, origin and trusted target selection; replay signature, origin and format trust; resource and time bounds; malformed known values and strict JSON; terminal honesty and validated EOF; call identity; no executable claim for incomplete tool input; explicit history-read intent; already-published accounting; candidate-local projection over immutable source IR; the routing freeze. No level invents content, forges signatures or fabricates success.

**Ownership.** The request side has one owner: one feature vocabulary, with each target declaring a disposition per feature (native, equivalent, approximate, omit, delegate or reject), and one pure projection applying it per candidate during planning. This vocabulary replaces the separate requirement, feature-set, feature-enum, target-contract and published-rejection representations; encoders consume only projected copies, and source-level admission keeps only grammar checks and L5. The delivery side has one owner in [`ClientAdapter`](../src/adapter/downstream.rs): a (fact, client protocol) table drives one projection over the IR event stream; JSON and retained views reduce the projected events to a snapshot and render it, so JSON/SSE agreement holds by construction and encoders only render. Discovery and `/capabilities` read the same tables.

The request implementation lives in [`lowering/request.rs`](../src/lowering/request.rs) and [`lowering/disposition.rs`](../src/lowering/disposition.rs). Source trust validation and visible-history transfer precede candidate feature admission; the final check sees projected semantic fields and envelope controls. Presentation, summary granularity and encrypted-include decisions use candidate rows, refreshed after allocation conversion where native thinking availability matters; hidden-display guarantees without a carrier reject through those rows. Parallel permission preserves the binding's disposition, independently of the explicit serial guarantee. Native grammar and replay-identity assertions remain codec responsibilities. Ordered Messages effort/tool rewrites consume the same table rather than a second carrier-policy object. Request-envelope session affinity remains outside semantic IR, with its target disposition resolved by this owner. The single delivery projection and complete discovery derivation remain scheduled work.

**Evidence.** Receipts record only L3 approximations, L4 omissions and schema-completion estimates; L2 normalizations, derivations, defaults and request inferences produce none. Target encoders carry whole evidence deltas so coverage and truncation counts survive merging; Messages retains its directional preference for detailed receipts over matching field summaries. Rule IDs live in the disposition tables rather than a separate projection-rule enum and scattered string literals. Migration order is owned by the [roadmap](roadmap.md#conversion-ladder-migration).

Applications selected on 2026-10-30: request strictness, effort, ceilings and Messages omission are implemented; the remaining delivery, intake and history migrations follow the [roadmap](roadmap.md#conversion-ladder-migration).

| Area | Level and rule |
|---|---|
| Explicit `strict:true` without an equivalent carrier | L5. Conditional (omitted) strictness is always resolvable without a compatibility classifier: an already-closed schema on a target with an equivalent strict carrier stays strict, every other case resolves non-strict under the conditional-default loss (L3); a request-wide strict grammar overflow resolves all conditional definitions non-strict, and rejects only when explicit strict tools alone overflow |
| Effort | Nearest supported level, ties upward, across Reasoning/Response scopes at the same level (L3); manual budgets on adaptive-only bindings convert to adaptive plus effort. When an explicit effort already exists, preserve it rather than replacing it with a budget-derived level; omit the uncarriable numeric budget under a named loss |
| Output ceilings | Clamp to a known model maximum (L3); targets without a ceiling carrier omit it (L4) |
| `required_with_tools` history with a Standard Chat client | Deployers select `readable_reasoning` on such public models; otherwise a candidate with a thinking-off carrier disables thinking under a named loss (L3), and one without rejects at planning. Not delegated, because the next turn is known to fail upstream |
| Uncarried reasoning in delivery or history | Omit the owner, keep visible content and tool links (L3); applies to opaque reasoning on Standard Chat and unsigned reasoning on Messages; never forged |
| Messages reasoning publication | An undecided summary waits for its actual signature in the bounded schedule. An irreversibly uncarriable shape discards its payload immediately and retains only bounded lifecycle tracking; invisible owners do not block visible successors. Summary interruption stays an interruption and cannot supply item or response completion |
| Unsealed or foreign client reasoning in history | Omit under visible-history transfer (L3); never forwarded |
| `stop` without a target carrier | L5 for now; gateway-side truncation may later provide L2 |
| Messages `stop_sequences` on a non-Messages target with an ambiguous natural/stop-sequence cause | `end_turn` under a named approximation (L3) |
| Unknown finish or incomplete reasons | Evidence-backed per-dialect maps; Responses carries an unspecified reason as `incomplete_details:{}`; otherwise L5 on Chat and Messages, so no outcome is misreported as a token limit |
| Messages `thinking.block_binding` | `null`/`{}` L2; `drop_block` L3 under existing replay checks; `"error"` L5 |
| Messages version and beta headers | See the [version policy](#protocols) |
| Unscheduled capabilities | Moderation, background, context management and compaction requests, hosted tools and items, `between_tools`, per-message configuration, `clear_at`, nonempty `logit_bias`, legacy function carriers, `inference_geo` and upstream-reported storage keep their current rejection until scheduled |

Existing rule families remain valid ladder applications:

| Rule Family | Policy |
|---|---|
| Prediction / advisory controls | Preserve in IR; apply exact native carrier, or omit via named receipt |
| Reasoning allocation / effort | Preserve budget and effort; apply directional quantization or scope approximation |
| Tool-result errors | Preserve native error flag; wrap in `[tool result is_error=...]` if target lacks boolean carrier |
| Chat JSON grouping | Concatenate contiguous plain assistant text before function/custom calls, preserving call identity, order and raw arguments/input. Do not flatten rich/replay-bearing parts or reorder text across calls; custom SSE remains unsupported |
| Instruction roles | Canonical Developer mapping; preserve root vs. ordered lifetimes |
| Output ceilings | Preserve explicit ceilings up to the model maximum; insert target default only when client ceiling is absent |
| Terminal stops | Coarsen natural/stop-sequence causes to `stop` or `end_turn` under named rules |

- `chat.omit-text-item-status.v1`: Plain-text incomplete message items omit individual item status when overall finish reason is `length`.
(See [`Rule`](../src/lowering/disposition.rs) for shared rule definitions; the remaining delivery consumers live in [`openai/lowering/projection.rs`](../src/protocol/openai/lowering/projection.rs)).

#### Semantic-equivalence compatibility

Accepted on 2026-10-28; this is ladder level L2 of the [conversion ladder](#conversion-ladder). The selected null-metadata, empty-logprobs and [reviewed equivalent forms](refs/2026-10-27-client-ir-boundaries.md#semantic-equivalence-review-2026-10-28) are implemented. Further L2 applications are found through the [ladder migration](roadmap.md#conversion-ladder-migration) rather than a separate field-by-field review. Protocol-defined equivalent empty values, empty collections, defaults and structural placeholders belong to lossless mapping. Accept, normalize, omit or supply them as the target grammar requires, without a separate named-omission path or loss receipt.

Equivalence depends on the field, direction and lifecycle contract, not emptiness alone. It must preserve content, control intent, explicit constraints, identity, report facts and subsequent behavior. Preserve source observations; apply target representation changes in the existing codec/projection owners. Do not introduce recursive empty-field stripping or globally equate absent, null, empty, false and zero. Unknown fields and malformed known values retain their existing admission rules.

Selected applications are reported `metadata:null` on Chat SSE and Messages delivery, and `logprobs:[]` on Messages delivery and history admission. These carry no metadata entries or token-probability observations to lose. Same-protocol carriers still preserve source presence where representable, and target-required fields remain required. Null metadata does not echo request metadata or become Messages `metadata.user_id`; empty/nonempty metadata maps and other unsupported execution hints retain their current dispositions. Nonempty logprobs still reject where no carrier exists, and an explicit request for probabilities is checked independently of an empty output report.

An empty update that clears state, an empty restriction set, an identity-bearing block or a terminal report is not equivalent to omission merely because its payload is empty. Stream accumulation must not erase earlier reports or suppress completion. Tests belong at the responsible boundaries: equivalent forms agree across JSON/SSE/history; meaningful nonempty values, source observations and state transitions remain protected.

This decision removes `chat.omit-null-stream-metadata.v1` and `messages.omit-null-response-metadata.v1` and their receipts. Actual removal of meaningful information continues to require a selected named loss or rejection.

Further selected applications: null-only system fingerprints on Responses/Messages and terminal explanations on Messages carry no report value; explicit `logprobs:false` needs no Messages probability carrier. Document citations default off: an omitted/false `citations.enabled` normalizes to an uncited document without a loss receipt; enabled citations remain unselected and malformed known flags reject. Empty cache options normalize without a dedicated rule. On trusted always-parallel targets, explicit parallel permission is exact; explicit serial intent still rejects. The obsolete `cache.omit-inactive-options.v1` and `deepseek.omit-parallel-tool-permission.v1` rules are removed. Meaningful fingerprints/explanations, active cache controls and requests for probabilities retain their loss/rejection contracts.

Chat request `logit_bias:null` is its documented default, equivalent to omission with no active token bias or loss receipt. Non-null values retain rejection; tokenizer-bound active bias remains unselected. This field-specific rule does not equate arbitrary empty objects with null.

#### Client/IR projection principles

Accepted on 2026-10-28 for Client/IR stabilization. Apply [semantic-equivalence compatibility](#semantic-equivalence-compatibility) first; classify remaining projection rules under the principles below. They are applications of the [conversion ladder](#conversion-ladder). Sections marked *implementation pending* state the target design; current behavior is unchanged until the corresponding [migration](roadmap.md#conversion-ladder-migration) slice lands.

| Principle | Rule |
|---|---|
| P1 Request-context deduction | A fact implied by the admitted request and the source grammar is an exact derivation recorded as `infer_from_request`; the source observation stays unreported |
| P2 Carrier-less non-content facts | Meaningful advisory controls, presentation requests and report facts without a target carrier take selected named omission when no exact mapping exists. Semantically equivalent forms need no loss receipt. Visible content and explicit constraints never take this omission |
| P3 Delivery closure | Anything delivered on a route is admissible as history on that route, exactly or through a named projection |
| P4 Planning-time impossibility | A failure determined by the request and the binding contract occurs before dispatch, never after publication |
| P5 Conditional source defaults | An omitted field with a documented conditional default is resolved per target by that documented rule, not by a fixed value |

Accepted applications:

- **Omitted Responses strictness (P5):** the [Responses reference](https://developers.openai.com/api/reference/resources/responses/methods/create) and [function-calling guide](https://developers.openai.com/api/docs/guides/function-calling#strict-mode) define attempted strict normalization with non-strict fallback; Chat omission is non-strict. The revised rule is implemented on the OpenAI Chat/Bailian and Messages paths as of 2026-10-30: an already-closed schema with an equivalent strict carrier stays strict; every other omitted-strict definition resolves non-strict under `openai.omit-conditional-tool-strictness.v1` (L3). No compatibility or normalization-feasibility classifier remains. An omitted parameter list is parameterless; required schema carriers use a closed empty object (L2), without changing source absence. Standard Responses retains its native omission. Explicit true never takes the omitted-default loss. The IR default names this conditional rule, not a promise to normalize supplied schemas.
- **Reasoning presentation and continuity requests (P2):** `summary:"auto"` selects the most detailed available summarizer; resolving to the fixed native summarizer or no available summary is exact. Explicit `concise`/`detailed` take a named omission without a selectable carrier. Encrypted-reasoning `include` is exact when the target emits no reasoning items and takes a named continuity omission when reasoning has no opaque carrier.
- **Carrier-less reports (P2):** served service classes and refusal categories without an OpenAI carrier are omitted on delivery copies, extending [explanatory terminal reports](#explanatory-terminal-reports). Anthropic standard capacity and OpenAI default processing remain distinct classes; no approximate tier is reported. Empty probability reports instead follow [semantic-equivalence compatibility](#semantic-equivalence-compatibility). Messages delivery applies the same omission to served OpenAI processing classes and echoed execution hints (`metadata`, `background`, `prompt_cache_key`, `store`).
- **Messages terminal derivation (P1):** see [Messages terminal projection](#messages-terminal-projection).
- **Messages baseline:** revised on 2026-10-30 by the [version policy](#protocols): `2023-06-01` is admitted over the current body grammar and beta headers are ignored, while beta body semantics follow body rules. Agents still learn unsupported features through [discovery](#generation-text-model-discovery).

The first presentation/continuity consumer (2026-10-29) projects Bailian Responses request copies before target validation. Auto resolves to the native default without a loss receipt; explicit granularity uses `responses.omit-reasoning-summary.v1`. Encrypted include uses `responses.omit-encrypted-reasoning-include.v1`, except that an admitted explicit thinking-off request has no new reasoning to encrypt and needs no loss receipt. These rules do not change activation, effort, budgets, context, read-history intent, history owners or source observations. Null/omitted visibility guards remain. This request-control policy is independent of S11 history qualification; other target families still need their selected consumers.

The conditional-default consumers share one resolver for root definitions, namespace leaves where carried, and inline-added definitions. Projection mutates private settings/items only; supplied schemas and source observations are unchanged. Unknown schema keywords, including inside closed objects and referenced definitions, take the same named non-strict fallback rather than local normalization-feasibility classification. The existing subset walker checks closure and the known vocabulary together; it does not rewrite schemas. Property names and annotation values are not schema keywords. Explicit strict requests retain upstream delegation for unknown keywords, independently of conditional-default inference. Native Responses keeps omitted parameters and strictness; Chat's existing omitted non-strict default also keeps optional spelling. Bailian final writers omit the unsupported strict flag under their native non-strict default. The request disposition owner selects these rules before final encoding.

The Messages consumer resolves conditional defaults once, after selected tool-change carrier projection, using the binding's strict opt-in and native exact subset. String enum/const casing and unsupported reference carriers cannot provide an equivalent strict guarantee. Root and inline strict tools share one native grammar budget with JSON output. If tentative conditional strictness exceeds that budget, every conditional definition becomes non-strict; explicit strict definitions and the output schema remain unchanged, and final validation rejects if those alone overflow. Projection and final wire validation reuse the same subset/budget checker. Parameterless definitions receive the required empty-object input schema without a loss receipt; equivalent empty schemas may still use an available strict carrier.

Responses retrieval of native structured tool output retains the argument string actually published by its SSE encoder. The existing bounded stream state owns those bytes; a private retained copy checks call identity, lifecycle and parsed-value agreement before replacing its reserialized string. Native structured observations remain unchanged, and the representation change needs no loss receipt.

#### Generated tool arguments and execution input

Selected on 2026-10-29; cross-view implementation pending. An unfinished generated argument prefix may project exactly to an OpenAI argument string without parsing, repairing or completing its bytes. Preserve incomplete call lifecycle where carried, the incomplete overall outcome and token-limit termination. Format conversion alone needs no loss receipt; unrelated lifecycle, grouping and protected-replay restrictions remain authoritative. A generated string is not a complete validated invocation. Native Messages complete JSON/history still requires an input object; its partial SSE carrier does not supply that object.

Client acceptance must distinguish error-result events from actual execution. Current pi fails all calls on a length-stopped message even when salvaged arguments parse, then supplies error feedback. That observed consumer behavior supports this narrow contract, not every consumer or a signature/dependency rewrite.

#### Tool-result association

Selected on 2026-10-29. Call IDs own result association, not vector adjacency. Preserve grouped calls followed by their corresponding results without reordering, flattening or dropping fields. Existing identity, causal-order, duplicate, bounds and replay checks remain. Exact association needs no new loss.

The Bailian Responses consumer (2026-10-30) removes its predecessor-position check at the shared native intake/target/writer boundary. Association stays with IR validation; no reordering pass or duplicate association owner is introduced. Independent grouped/adjacent input and output expectations preserve IDs, arguments and results, while invalid links still fail semantic validation. A phase-free production-construction control compares actual managed `previous_response_id` continuation with stateless history in JSON/SSE, including the complete ordered upstream request. String-only result implementation, phase, history-carrier and replay restrictions remain separate.

[Native qualification](refs/2026-10-27-client-ir-boundaries.md#pre-stage-4-contract-qualification-2026-10-29) covers direct Qwen Flash/Max, TokenPlan Qwen Flash, native MiMo and OpenRouter GPT grouped histories. This is not a Qwen-only semantic rule. It does not attest every model, result ordering, arbitrary interleaving, media-array carrier or billing endpoint; stricter target restrictions require endpoint evidence rather than literal-position inference from one guide. Phase remains a separate semantic/carrier question.

#### Downstream schema completion

Generation Text returns complete downstream schemas: **Upstream facts → Justified inference → Simple estimates**.
- `responses.infer-required-settings.v1`: Fills missing `tools`, `tool_choice`, and `parallel_tool_calls` from admitted request settings.
- `responses.echo-request-metadata.v1`: Echoes original admitted request metadata only when upstream omits metadata entirely ([`Rule::ECHO_REQUEST_METADATA`](../src/lowering/disposition.rs)); this exact inference produces no loss receipt.
- `generation.complete-delivery-usage.v1`: Completes missing usage fields using text payload byte estimates (`ceil(bytes / 4)`).

#### Reported text configuration

Preserves absent, null, and object states in `TextOptions`. Null reported formats are retained without defaulting to plain text. Request-side null text configurations reject at intake.

#### Message phase transfer

Responses preserves `commentary` and `final_answer` labels. Targets without phase carriers omit phase under named rules (`generation.omit-delivery-message-phase.v1`) while preserving text and tool linkages.

#### Explanatory terminal reports

- Anthropic `stop_details` maps to `TerminalDetails.refusal`.
- Google `finishMessage` maps to `TerminalExplanation`.
- OpenAI delivery omits the carrier-less refusal report, including its stable category, under `openai.omit-explanatory-refusal-details.v1`. Refusal remains an incomplete/content-filter outcome; report text never becomes generated content or successful completion.
- Other carrier-less explanations are omitted on delivery copies without modifying the underlying stop cause. Native representations retain their original reports.
- Null-only refusal/explanation reports carry no report content and need no omission receipt; they follow semantic-equivalence compatibility, without stripping non-null report objects or changing source observations.

#### Service policies and reports

- Requested service policies and served classes have separate owners. Anthropic `standard`/`priority` capacity and OpenAI `default`/`flex`/`priority` processing classes remain distinct; explicit policy constraints require a supported target meaning.
- `openai.omit-unrepresentable-service-class.v1` omits served classes without an OpenAI carrier on delivery copies. It never substitutes an approximate tier. The test-only strict-complete reporting policy is scheduled for removal; production bindings use faithful reporting.
- Stream encoders validate original service observations before projecting rendering copies. Conflicting single-report or lifecycle updates still reject even when both classes would be omitted.

#### Lifecycle settings and service reports

Responses and Interactions maintain separate opening and terminal reports validated by [`protocol::reports`](file:///home/sakura/development/MorphieCore/src/protocol/reports.rs). Cross-phase updates permit only evidenced runtime resolutions (e.g. `summary: auto → detailed` or `service: configured → standard`); conflicting drifts reject fail-closed.

#### End-user attribution

Opaque user attribution projects across protocols via [`protocol::attribution`](file:///home/sakura/development/MorphieCore/src/protocol/attribution.rs):
- Projects between OpenAI `safety_identifier`, Anthropic `metadata.user_id`, and Google `labels.safety_identifier`.
- Explicit safety identifiers take precedence over legacy `user` fields.
- Non-authoritative metadata: never utilized for authentication or cache identity.
- Source identity bounds remain strict; unsupported Google label values take named omission without fallback, truncation or hashing. Native OpenAI/Messages identity bounds remain validated before encoding.

#### Selected request approximations

- `openai.wrap-tool-result-errors.v1`: Prefixes tool results with boolean status text when target lacks native error flags.
- Terminal approximations: Coarsens stop causes to OpenAI `stop`, Anthropic `end_turn`, or `tool_use`.

#### Messages terminal projection

A completed, call-free outcome maps to `end_turn` for any source when both the admitted and effective request have no stop sequence. Responses has no stop-sequence parameter, and Chat defines `stop` as a natural stop or a provided sequence, so this is exact request-grounded inference (`InferFromRequest`). `messages.infer-end-turn-from-request.v1` supplies target-only natural completion and finished-turn progress; source response and terminal reports remain unchanged. JSON, SSE and retained views use the same premise.

Explicit stop causes are preserved. Unbound requests, declared stop sequences, continuation, incomplete/failed/cancelled outcomes and calls do not gain a generic end-turn derivation. SSE still requires validated upstream EOF and final metadata before publishing its terminal.

Selected on 2026-10-30 (implementation pending): a completed outcome whose only ambiguity is a declared stop sequence on a non-Messages target maps to `end_turn` under a named approximation (L3), because the source cannot say which cause applied. A Chat `stop` finish accompanied by function calls maps to `tool_use` (L2), as forced tool choice reports `stop` with calls. Incomplete, failed and cancelled outcomes remain unchanged.

Existing Gemini 3.8 Flash bindings retain their narrower rules:
- `messages.google-stop-without-sequences.v1` and `messages.interactions-completed-as-end-turn.v1` use the same request-grounded inference classification for their call-free completions.
- `messages.google-tool-stop.v1` remains a separately selected tool-stop projection. Disabling that Google-specific policy does not disable generic call-free inference and does not permit the ambiguous tool-stop case.

### Reasoning controls

#### Reasoning activation and effort scope

The IR decouples reasoning activation from effort level and scope:
- **Activation:** `ReasoningRequest` (enabled vs disabled).
- **Effort Control:** `EffortControl { scope, level }` with [`EffortScope::Reasoning`](file:///home/sakura/development/MorphieCore/src/semantic/task/generation/content/reasoning.rs#L27) (per-thought) or [`EffortScope::Response`](file:///home/sakura/development/MorphieCore/src/semantic/task/generation/content/reasoning.rs#L27) (turn-wide).
- Explicit `disabled` reasoning takes precedence over response-wide effort settings.

##### Google response-effort projection

`google.gemini-3.8-flash.response-effort-to-thinking.v1` maps response-scoped low/medium/high to Gemini thinking effort levels via [`ReasoningConversion::Scope`](file:///home/sakura/development/MorphieCore/src/protocol/reasoning.rs#L131).

#### Reasoning level calibration

Explicit hand-authored mappings define canonical model bindings. Under the [conversion ladder](#conversion-ladder), unsupported levels resolve to the nearest supported level (ties upward) and Reasoning/Response effort scopes convert at the same level. Fixed or ignored sampling controls (temperature, top-p, top-k) are omitted for models that do not support them through their feature disposition rows.

##### Thinking-budget quantization

When targets require discrete qualitative levels, numeric token budgets are quantized to discrete presets defined in [`ThinkingBudgets`](file:///home/sakura/development/MorphieCore/src/protocol/reasoning.rs#L35):
- Canonical presets are maintained in [`SHARED_BUDGETS`](file:///home/sakura/development/MorphieCore/src/protocol/reasoning.rs#L49) (6 discrete tiers spanning minimal to max).
- Selection matches minimal absolute token distance via [`ThinkingBudgets::nearest`](file:///home/sakura/development/MorphieCore/src/protocol/reasoning.rs#L108); exact ties resolve upward to the higher effort level.
- Current validation rejects requests whose thinking budget exceeds the client-specified maximum output token ceiling.

##### Bounded token-budget adaptation

Selected on **2026-10-28**: functionality may take precedence over conservative gateway-derived token defaults or presets through bounded, target-qualified adaptation. Prefer exact native controls; distinguish original client intent, derived allocation and actual model limits.

This direction does not select an adjustment formula or change current validation. A concrete rule must name the changed guarantee, target, finite bound, useful answer reserve and evidence. Explicit client ceilings or numeric thinking intent are not silently overridden; any relaxation of them needs its own selected rule. The 2026-10-30 [model-maximum clamp](#model-effective-values) is such a rule: it only lowers a ceiling the model cannot exceed. Source protocol validity, model maxima, memory/wire limits, timeouts and terminal honesty remain enforced. Unknown capacity is not unlimited capacity. The [budget proposal](api-expansion-design.md#independent-output-ceilings-unselected) owns unresolved alternatives.

##### Initial mapping plan

Bindings declare model-specific qualitative level translation via [`ReasoningEffortMapping`](file:///home/sakura/development/MorphieCore/src/protocol/reasoning.rs#L358) and [`ReasoningRequestPolicy`](file:///home/sakura/development/MorphieCore/src/protocol/reasoning.rs#L155):
- Conversions are strictly directional. A level missing from a binding's table resolves to the nearest supported level, ties upward, under the binding's named effort rule. Bindings that expose no effort control omit it under a named rule (L4); explicit history-read intent keeps its own guard.
- Upstream thinking budgets translate to qualitative effort via [`ReasoningConversion::BudgetToEffort`](file:///home/sakura/development/MorphieCore/src/protocol/reasoning.rs#L137), and qualitative effort expands to discrete token budgets via [`ReasoningConversion::EffortToBudget`](file:///home/sakura/development/MorphieCore/src/protocol/reasoning.rs#L142).

##### Implementation owners

- [`semantic::task::generation`](file:///home/sakura/development/MorphieCore/src/semantic/task/generation/mod.rs): Activation and scoped effort types.
- [`protocol::reasoning`](file:///home/sakura/development/MorphieCore/src/protocol/reasoning.rs): Quantization presets, nearest-level resolution, and policy projection.
- [`topology::catalog::bindings`](file:///home/sakura/development/MorphieCore/src/topology/catalog/bindings.rs): Binding-level policy selection and limits.

### Media

Media resources (images, audio, video, files) are handled as inert payloads (base64 data or public URLs):
- The gateway does not fetch URLs, transcode formats, or parse image pixels.
- Chat video uses registered extension `video_url`; Responses video uses `input_video` in user messages.
- Instructions are strictly text-only; media in system/developer roles is rejected at intake.
- File references: Client-provided Provider file IDs are rejected. Only direct inline data or HTTPS URLs are admitted.
- Image transformations: `oversized_image: "error"` sets `RejectOversized`; default allows upstream downsizing.

### Adaptation evidence

Every lossy conversion or field omission generates bounded typed evidence.

#### Domain evidence interface

The conversion pipeline tracks adaptation evidence via [`Conversion<T>`](file:///home/sakura/development/MorphieCore/src/protocol/conversion/generation/evidence.rs#L837), pairing transformed payloads with a typed [`EvidenceDelta`](file:///home/sakura/development/MorphieCore/src/protocol/conversion/generation/evidence.rs#L739):
- **Body-free isolation:** Evidence records only structured [`Effect`](file:///home/sakura/development/MorphieCore/src/protocol/conversion/generation/evidence.rs#L478) items and rule IDs; user payloads, prompts, and tokens are strictly excluded.
- **Change categories:** Field modifications classify into canonical [`Change`](file:///home/sakura/development/MorphieCore/src/protocol/conversion/generation/evidence.rs#L361) families (`Normalize`, `Derive`, `Default`, `InferFromRequest`, `Omit`, `Approximate`, `Estimate`).
- **Bounded allowances:** Deltas enforce hard upper bounds of [`MAX_EFFECTS`](file:///home/sakura/development/MorphieCore/src/protocol/conversion/generation/evidence.rs#L726) (64 entries) and 16 KiB per delta. When saturated, an explicit truncation count is recorded without modifying business payloads.

#### Atomic evidence and output batches

Transformations return `Result<Conversion<T>, ConversionFailure>`. Evidence is committed only upon outer boundary acceptance:
- `Ready(EncodedBatch)`: Output frames serialized and validated.
- `NoCarrier(EvidenceDelta)`: Intentional transformation produced no output frames.
- `Deferred`: State retained pending future events or markers.

#### Typed conversion failures

`ConversionFailure` identifies the responsible boundary, affected locus, and root cause (`CodecError` or `RepresentationError`).
`SourceFailureEvidence` captures upstream HTTP status hints, envelope error codes, and body completeness without retaining unbounded error strings.

#### Public capability and adaptation notice

- `AdaptationNotice`: Optional, namespaced `morphiecore` field exposed in downstream JSON or terminal SSE frames.
- Maximum 16 entries / 2048 bytes.
- Standard protocol fields maintain official semantics; clients may safely ignore the notice.

#### Generation Text model discovery

`GET /v1/morphiecore/models` and `GET /v1/morphiecore/models/{model}` publish verified model capability directories:
- **Scope:** Active Generation Text interfaces and assessed image inputs.
- **Descriptors:** Supported protocols, delivery modes (JSON/SSE), token limits (context window, max output), and evaluated feature states (`faithful`, `approved_adaptation`, `unavailable`, `not_assessed`).
- **Pi Integration:** Context windows and image capabilities project directly to Pi client configurations.

Accepted extension (2026-10-28; implementation pending): the gateway publishes, in client-agnostic vocabulary, each model's preferred client interface and each interface's adaptation rules. Preference follows the primary candidate, with a `basis` field and a uniformity flag; no candidate count, order or identity is disclosed. One compiled owner derives these facts from the agent-default conformance corpus and the [conversion-ladder](#conversion-ladder) disposition tables; the models endpoint owns Generation while `/capabilities` keeps other tasks. A body-hash `ETag` is allowed; no maintained revision is introduced.

Simple feature support, default-request disposition and full delivery/history support are separate claims. Explicit-strict synthetic tools do not stand in for omitted-strict Agent declarations. Keep unassessed/nonuniform facts and partial/composition coverage explicit. Publishing a preference does not establish that a client consumes it: integration must state whether application is automatic, explicit configuration or advisory. This neither permits capability-based candidate skipping nor changes routing priority.

## History and replay

### Reasoning replay

Upstream opaque tokens and signatures are delivered in tagged [`mc1.`](file:///home/sakura/development/MorphieCore/src/protocol/replay_envelope.rs#L33) envelopes via [`protocol::replay_envelope`](file:///home/sakura/development/MorphieCore/src/protocol/replay_envelope.rs):
- **Isolation:** Payloads are bound to route fingerprints generated by [`issuer`](file:///home/sakura/development/MorphieCore/src/protocol/replay_envelope.rs#L40) and [`deployment_issuer`](file:///home/sakura/development/MorphieCore/src/protocol/replay_envelope.rs#L46) (binding provider ID, upstream model, and origin/path).
- **Candidate history transfer:** Incompatible reasoning is omitted on candidate copies under `transfer-visible-history.v1` by the [request projection](../src/protocol/request.rs), preserving visible messages and semantic tool links. Issuer isolation and assessed carrier availability are separate selectors.
- **Supported replay carriers:** Anthropic thinking signatures and `redacted_thinking`, Responses `encrypted_content`, Google `thoughtSignature`, and Interactions `function_call.signature`.

Accepted revision (2026-10-28, P3): same-issuer `incomplete` reasoning with a finalized replay token is forwarded exactly to Responses, whose input union admits that status; implemented with preserved item identity/status and issuer checks. Finalized foreign reasoning and finalized native reasoning with an incompatible carrier format are omitted independently of item lifecycle, preserving visible history and tool links. Client `in_progress` partial tokens remain invalid at replay opening; partial native values and required compaction/attachments retain their guards.

The first assessed request-history migration (2026-10-29) applies to Bailian Responses bindings declaring `accepted`: summary-only owners retain their exact carrier; owners containing full-readable text or opaque replay have no carrier and take `responses.transfer-visible-history.v1`, including same-issuer and unattributed owners. The whole uncarried owner is omitted rather than moving reasoning into visible text or stripping only its signature. Explicit `read_history:true` cannot be satisfied by omission. The wire predicate is shared with ordinary Bailian validation; dependency/finality checks, affected call-alias pruning and fidelity cleanup share the existing transfer implementation. Retained summaries, visible item IDs and semantic call/result links remain intact. Selected on 2026-10-30 (implementation pending): every request consumer applies the same transfer to reasoning its target cannot carry, including unsealed or foreign client reasoning, foreign citations and obsolete markers; unassessed bindings follow the delegation rule below.

#### Binding reasoning-history contract

Accepted on 2026-10-28 (P3, P4), with phased migration and explicit unassessed declarations accepted on 2026-10-29. An assessed Generation binding declares one reasoning-history fact: `unsupported`, `ignored`, `accepted` or `required_with_tools`. `None` means unassessed: no acceptance, ignoring or rejection is inferred. Revised 2026-10-30 (implementation pending): planning does not reject on an unassessed fact; the candidate sends carriable history and upstream validation decides, while discovery still publishes `not_assessed`. The selected protocol target owns the declaration; a common accessor supplies consumers without dialect inference. Wire carriers, replay isolation and thinking controls remain separate.

The first migration declares endpoint-qualified facts for native Xiaomi, direct DashScope Qwen, DeepSeek, native Zhipu and the SIWC/Grok subscriptions. Other bindings remain unassessed. Planning reads `required_with_tools` for Standard Chat clients on Chat candidates: nonempty projected tools with thinking not off reject before dispatch unless the client selects readable reasoning. Selected on 2026-10-30 (implementation pending): deployers select `readable_reasoning` on such public models; otherwise a candidate with a thinking-off carrier disables thinking under a named loss, and only a candidate without one rejects. Tool-free requests and admitted off controls add no history requirement. The old Qwen output guard is retired for assessed bindings whose unsigned output now projects to Standard Chat; unassessed TokenPlan Qwen admission stays unchanged until the delegation revision lands. Discovery reads the same declarations and publishes a common fact, `non_uniform` for differing assessed facts, or `not_assessed` if any candidate is unassessed.

Request/history migration starts with the assessed Bailian Responses rule above; remaining history/delivery consumers still need migration. Neither a declaration nor successful encoding establishes full-loop carriage. Standard Chat's selected delivery omission below is independent of guessing a Provider history fact. DeepSeek's [thinking contract](https://api-docs.deepseek.com/guides/thinking_mode/) (read 2026-10-28) requires **all previous turns' reasoning** when a thinking request carries tools; Zhipu's documented requirement concerns the current tool cycle. The shared fact does not collapse those scopes or authorize dropping explicit read-history intent.

Consumer responsibilities remain bounded:

| Consumer | Contract |
|---|---|
| Binding/target declaration | Own endpoint-qualified history semantics; do not inherit native, billing-plan or model defaults from a shared dialect |
| Planning | Check encoded request reachability and whether the client can carry required reasoning; do not claim a stateless caller supplied complete history |
| Request/history projection | Preserve available required history and explicit read intent; apply only selected carrier/scope projections, never reinterpret all-prior as current-cycle-only |
| Delivery | Keep output visibility/carriage separate from history requirements and source observations; protect replay independently |
| Discovery | Publish assessed coarse/common facts without claiming uniform retention scope or full-loop certification |

Every candidate starts with immutable admitted input; an omission for one candidate cannot remove an owner needed by a later candidate. Managed Responses continuation preserves admitted input and delivered history, while equivalent stateless history is checked through client carriage and replay opening. Neither path adds a new completeness ledger or upstream conversation persistence.

### Chat reasoning extensions

Public Chat defaults to the official Standard dialect. The trusted model profile `chat_profile: readable_reasoning` enables:
- `reasoning_content` in assistant JSON, SSE deltas, and returned history.
- `preserve_thinking` history reading intent.
- Unsigned, readable reasoning only; opaque signatures and structured reasoning details reject under this profile.

Accepted revision (2026-10-28, P2, P4; implemented 2026-10-29): Standard Chat omits unsigned readable reasoning under `chat.omit-readable-reasoning.v1`, matching official Chat, which exposes reasoning only through usage. The codec-local policy removes reasoning owners from static and retained copies and suppresses their complete event lifecycle before grouping/rendering. Source parsing/reduction, accounting, visible content, tool links and terminal authority remain intact. Omission evidence identifies the original owner; reasoning is never moved into visible assistant text.

Replay-bearing reasoning currently remains protected even when its readable parts are empty or the native carrier contains no cryptographic signature. Selected on 2026-10-30 (implementation pending): Standard Chat has no carrier for it, so delivery omits the whole owner under a named loss instead of failing after dispatch; continuity is lost, visible content and tool links stay, and no signature is forged or moved into text. Explicit readable/structured profiles keep their existing carriers. When a declared Chat binding's [history contract](#binding-reasoning-history-contract) is `required_with_tools`, a Standard tool request with thinking not off rejects at planning unless the public model selects `readable_reasoning`; the 2026-10-30 thinking-off fallback in that section applies once implemented.

### Compacted history

Compacted conversation summaries are preserved as distinct `Compaction` items with opaque payloads wrapped in `mc1.` envelopes. Compaction items are required context and reject if the target lacks a compaction carrier.

### Stateful history

Gateway-managed continuation via `previous_response_id` on OpenAI Responses is managed by [`protocol::openai::responses::managed`](file:///home/sakura/development/MorphieCore/src/protocol/openai/responses/managed.rs):
- Stores delivered response projections and request contexts in process-local TTL memory (default 1 hour).
- Synchronous retrieval, deletion, and cursor-paginated input listing.
- Upstream requests remain fully stateless (`store: false`).

### Agent session files

Selected architectural direction for downstream session persistence:
- Per-Agent JSONL directories containing session interaction logs.
- Cross-agent collaboration requires explicit read or copy operations, preserving author attribution and replay boundaries.

## Native Messages projection

### Request rules

- **System blocks:** Request-wide instructions compose into top-level `system` text blocks.
- **Mid-conversation system:** Subsequent instructions and configuration updates become mid-conversation system messages.
- **Output configuration:** JSON schema output maps to `output_config.format`.
- **Cache markers:** Preserves owner-local 5m/1h breakpoints.

Selected on 2026-10-31: the historical thinking-prefix requirement for manual thinking belongs to an actual native Messages candidate after projection, not public Client admission. A cross-protocol response may legitimately omit an uncarriable reasoning owner; its tool-result continuation must remain decodable. Common field, turn-order, tool-reference, signature and origin validation stays strict. Candidates that still use native manual thinking retain the prefix check before dispatch.

### Missing output ceilings

[`TargetContract.default_output_tokens`](file:///home/sakura/development/MorphieCore/src/protocol/anthropic/messages/target.rs#L70) defines optional ceiling insertion:
- Declared by Go/Haiku Messages (`32000`) to satisfy Anthropic mandatory `max_tokens`.
- Injected on candidate copies only when client request omits `max_tokens`. Explicit ceilings are preserved.

### Owner-local cache admission

Owner-local breakpoints are admitted on every route. Candidates with native carriers preserve the supported controls; other candidates omit them under named rules at each owner, so no candidate is skipped and route membership is unchanged. Request-wide, tool-definition and content-part cache controls share the request disposition owner.

### Caller-selected automatic cache admission

Request-wide automatic caching is enabled on native Messages targets with verified support. Breakpoints are placed at eligible turn boundaries without exceeding the 4-marker ceiling.

### Response rules

- Messages delivery requires valid signatures on carried thinking blocks. Unsigned or multi-summary reasoning without a signed carrier is omitted under `messages.omit-uncarried-reasoning.v1`; malformed signatures still reject. Signatures are never synthesized and signed reasoning keeps its exact carrier.
- A zero-argument call whose arguments are an empty string delivers `{}` input (L2); other non-object tool input remains rejected.
- Block start order preserves source item ordering.
- Streaming argument fragments are buffered during whitespace prefixes to maintain valid JSON grammar for accumulators.

## Runtime and trust

### Failover

Failover operates on precompiled candidate lists under a cache-affine routing policy.

#### Legal candidates

Routes compile at startup (up to 64 candidates per model). Ingress requests cannot specify arbitrary candidate targets.

#### Request conversion reuse

Gateway preflight retains credential-free final bytes, protocol-header projections, bounded prefix digests and adaptation evidence together with the actual bound candidate. Executor ordering moves the whole record; preparation consumes it without reconstructing JSON or repeating semantic conversion. Public planning results remain metadata-only, and discovery uses the checked projection directly.

The checked projection owns an immutable wire value, its bounded serialized-size measurement and evidence. Retention admission, exact body allocation and eligible prefix hashing reuse that measurement instead of serializing again to count. Gateway materialization shares lookup eligibility with affinity selection and derives prefixes only for eligible initial lookups; direct preparation, disabled/explicit/unmapped lookup paths and already-hinted requests do not derive them.

The request-local retention budget is 1 MiB, charging body/header/digest allocation capacity and evidence including spare vector slots. Overflow leaves candidate admission and ordering unchanged: an attempted cache miss recomputes the existing pure conversion. Remaining records belong to the exchange future and are released on completion or cancellation, not retained in shared gateway state.

Preparation still validates the trusted binding, streaming and final body bounds before attaching credentials and account headers. Codecs validate header carriers during request encoding; materialization stores the projected headers as a plain vector rather than retaining a deferred error state. An affinity hint changes a typed candidate-local request and carries its new checked conversion; a hint that cannot fit leaves the original accepted body intact. The test-only forced-stream override likewise invalidates its body. No path patches encoded JSON or reparses it through generic `Value` deserialization, which could reinterpret ordinary reserved-key schema objects.

#### Advancement

Advancement to alternative candidates is governed by [`Transition::may_advance`](file:///home/sakura/development/MorphieCore/src/execution/fallback.rs#L40):
- **Publication freeze:** Advancement is strictly prohibited once downstream delivery has committed frames.
- **Fail-closed modes:** Auth failures, context window overflows, and semantic projection failures terminate without retry.
- **Transitory failover:** Connect failures, timeouts, and quota exhaustion advance across [`Transition`](file:///home/sakura/development/MorphieCore/src/execution/fallback.rs#L19) variants (`Credential`, `SameProvider`, `Provider`) within attempt budgets.

#### Health and recovery

- **Scoping:** Health cooldowns are tracked by credential, provider, deployment origin, and model.
- **Probe ownership:** When cooldown expires, exactly one foreground request acquires probe ownership. Background probing is not used.
- **Backoff:** Transient failures back off from 1s to 60s; rate/quota failures back off from 60s to 1800s.

#### Routing unit

The declared conversation (`X-MorphieCore-Conversation-Id`) is the routing unit. Successful completions establish affinity to the serving executor.

### Conversation and cache affinity

Affinity keys and advisory cache policies are defined in [`protocol::cache`](file:///home/sakura/development/MorphieCore/src/protocol/cache.rs):
- `X-MorphieCore-Conversation-Id` establishes route affinity across multi-turn exchanges via hashed SHA-256 grouping keys.
- Fallback prefix cache indexing provides best-effort routing affinity when conversation IDs are omitted.
- Target cache capabilities are enforced via [`CacheTargetPolicy`](file:///home/sakura/development/MorphieCore/src/protocol/cache.rs#L48), omitting unsupported advisory hints under declared loss rules.

### Streaming and delivery

Delivery lifecycle and event streaming are coordinated by [`ResponseDelivery`](file:///home/sakura/development/MorphieCore/src/execution/delivery.rs#L100) under [`SseLimits`](file:///home/sakura/development/MorphieCore/src/protocol/stream.rs#L8):
- **Pre-publication holding:** Lifecycle-only frames are buffered until the first content delta arrives, allowing unpublished failures to fail over cleanly.
- **Publication freeze:** Flushing content frames irreversibly freezes candidate advancement; subsequent upstream failures abort the stream with terminal error events.
- **Honest completion:** Streams require validated upstream EOF before downstream release.

#### Semantic stream normalization

Codecs validate and reduce upstream events in source order, emitting legal target streams:
- Preserves delta ordering, call identities, and signature attachments.
- Emits block openers before deltas, and closes blocks before terminal events.
- Interleaving tool calls and text parts are serialized to conform to target grammar.

Messages downstream delivery uses one active wire block in semantic owner/part declaration order, independently of numeric IDs. The head streams incrementally; successors wait for its actual owner finish, including a delayed thinking signature. Thinking emits one signature immediately before block stop; redacted blocks keep their native shape. Reports retain their existing independent timing and authority.

The codec-local [scheduler](../src/protocol/anthropic/messages/client/sse/schedule.rs) bounds retained payloads and container overhead, releases drained capacity, and rejects known unsupported content before queueing. [Frame construction](../src/protocol/anthropic/messages/client/sse.rs) checks serialized size and cumulative wire/event budgets during drainage. Cancellation or rejection discards pending content. EOF-authorized incomplete termination may close unfinished text/tool wire blocks without inventing semantic completion; source reduction, argument validation and publication rules remain outside the scheduler. Native Messages intake is unchanged.

An observed incomplete text-item finish also authorizes closure of that owner's remaining wire blocks after their queued content drains. It preserves the source item status and unfinished part facts, matching existing static rendering and terminal-only closure. A completed item still requires actual part completion; block closure does not authorize a successful operation terminal.

#### Reverse-proxy delivery

MorphieCore operates behind a loopback-bound reverse proxy:
- SSE responses set `X-Accel-Buffering: no` and `Cache-Control: no-store`.
- The proxy terminates TLS and provides client compression; MorphieCore handles authentication and streaming logic.

#### Optional delta coalescing

Configured via [`DeltaCoalescing`](file:///home/sakura/development/MorphieCore/src/execution/delivery/coalescing.rs#L17) (default 50 ms / 16 KiB bounds):
- Merges consecutive adjacent `PartKind::Text` fragments into larger downstream frames.
- Flushes immediately upon non-text events, usage reports, or timer expiration.

### Credentials and security

- **Store:** Credentials reside exclusively in private memory-locked storage.
- **Automatic OAuth renewal:** Proactive background renewal scans tokens expiring within 120s, rotating credentials serially under account locks.
- **Ingress Auth:** Binds exactly one configured Agent key via Bearer token or `x-api-key`.

### Agent identity and cluster diagnostics

#### Identity and authentication

- `agent_id`: Stable configured agent identity across key rotations.
- Authentication table maps keys to `agent_id` with constant-time verification.

#### Shared history and usage attribution

- Every request is tagged with an immutable Agent identity and runtime nonce.
- Usage is attributed to the executing Agent rather than the conversation owner.

### Observability

- **Tracing stack:** `tracing` / `tracing-subscriber` for spans, `tracing-opentelemetry` for traces, and `opentelemetry-appender-tracing` for structured logs.
- **Local writer:** Asynchronous, non-blocking worker writing structured JSONL log records to rotating 1 MiB files.
- **Privacy:** Operational logs are strictly body-free; prompts, completions, and secrets are excluded.

### Configurable exchange capture

Opt-in diagnostic capture configured via [`gateway::capture::CaptureConfig`](file:///home/sakura/development/MorphieCore/src/gateway/capture.rs#L34) records sanitized HTTP headers and bodies at four pipeline boundaries:
1. Inbound client request (`client_request`)
2. Outbound upstream request (`upstream_request`)
3. Inbound upstream response (`upstream_response`)
4. Outbound client response (`client_response`)

- **Sanitization:** Authorization headers, cookies, API keys, and query tokens are strictly redacted before enqueueing.
- **Bounded quotas:** Configured per-unit (256 KiB), per-exchange (4 MiB), and total file quota (64 MiB).
- **Isolation:** Best-effort asynchronous worker; buffer saturation or write failure never blocks inference.

## Settled decisions

Authoritative log of accepted architectural decisions and their systemic consequences:

### Scope and activation decisions

| Decision | Date | Accepted Architectural Consequence |
|---|---|---|
| Model capability discovery | 2026-10-26 | Unified discovery contracts for Generation Text and Pi; other modalities schema-only |
| DeepSeek parallel-tool admission | 2026-10-25 | Rejects serial tool intent; omits parallel permission with evidence on DeepSeek targets |
| Messages signed delivery | 2026-10-27 | Enforces native thinking signatures; revised 2026-10-30: unsigned thinking is omitted, never signed |
| Local telemetry wiring | 2026-10-25 | Native structured logs via tracing/OTel without global runtime locks |
| Minimal diagnostic attribution | 2026-10-25 | Admission-owned Agent and request nonces tag traces and capture logs |
| Unified Generation evidence | 2026-10-24 | Replaces separate audit logs with `EvidenceDelta` and `Conversion<T>` |
| Standards reset & redesign | 2026-10-23 | Pinned official standards and upstream test suites; its exact-first fallback is superseded by the conversion ladder |
| Standard stream errors | 2026-10-24 | Standard Responses `error` events for classified stream failures |
| Native-only continuation | 2026-10-24 | Replaces synthetic manifest reasoning with genuine native replay carriers |
| Readable Chat reasoning | 2026-10-24 | Explicit `readable_reasoning` profile for `reasoning_content` |
| Latest Messages baseline | 2026-10-24 | Current `2026-10-21` body grammar; revised 2026-10-30: `2023-06-01` admitted over it, removed sampling fields omitted, beta headers ignored |
| Canonical accounting | 2026-10-23 | Scoped modality breakdowns; single reasoning presentation owner |
| Five-stage order | 2026-10-20 | Stage 1 (Lossless Codecs), 2 (Agent Tools), 3 (Stability/Counting), 4 (Modalities/Decision/Files), 5 (Backlog) |
| Shared Aliyun dialect | 2026-10-19 | Shared `Bailian` dialect for DashScope and TokenPlan |
| Dual-SDK Decision direction | 2026-10-23 | Shared Decision IR supporting TypeSafe and OpenAI Decision SDKs |
| Stage-4 equal priority | 2026-10-25 | Decision, Embedding, Audio, Images, Files/Uploads, and Video advance at equal priority |
| Independent Embedding | 2026-10-23 | Decoupled Embedding on `POST /v1/embeddings`; float/Base64 compatibility |
| Grok Imagine static scope | 2026-10-22 | Static image codec (`grok-imagine-image-2.0`); video deferred to async API |

### Semantic ownership decisions

| Decision | Date | Accepted Architectural Consequence |
|---|---|---|
| Standard request boundary | 2026-10-26 | Client intake enforces official protocol grammar; Provider dialects isolated to egress |
| Conversion ladder | 2026-10-30 | Same-name → same-semantic → partial lossy → omit → reject only when omission would misrepresent the response; unknown target facts delegate to upstream validation; one request-side and one delivery-side disposition owner |
| Candidate request dispositions | 2026-10-31 | One `Feature` vocabulary and target disposition owner replace separate admission/target feature gates, including presentation and parallel permission; conditional strict inference requires a known closed vocabulary, explicit strict keeps upstream delegation, and native assertions keep grammar and replay identity |
| Client semantic boundary | 2026-10-27 | Unknown extension fields discarded; revised 2026-10-30: known fields enter the conversion ladder instead of fail-closed rejection |
| Three-protocol fidelity | 2026-10-27 | Semantic fidelity across Chat, Responses, and Messages with stream normalization |
| Text item status omission | 2026-10-27 | Plain text incomplete items omit item status when overall finish is length |
| Responses metadata echo | 2026-10-27 | Echoes admitted request metadata when upstream omits metadata entirely |
| Image size policy | 2026-10-24 | `AllowDownsize` vs `RejectOversized` owned per image use |
| Citation point anchors | 2026-10-21 | TextPoint insertion boundary anchors in text parts |
| Explanatory terminal reports | 2026-10-21 | Bounded refusal details and terminal explanations with named delivery omission |
| Text configuration presence | 2026-10-20 | Preserves absent, null, and object states in reported text options |
| Canonical instruction roles | 2026-10-25 | Canonical Developer role unifies application instructions across protocols |
| Participant ownership | 2026-10-24 | Preserves instruction and message participant names for OpenAI Chat |

### Projection and reasoning decisions

| Decision | Date | Accepted Architectural Consequence |
|---|---|---|
| Independent activation & effort | 2026-10-20 | Decouples boolean reasoning activation from scoped effort controls |
| Strict downstream schemas | 2026-10-23 | Downstream schema completion fills missing fields via derivation or text estimates |
| User attribution mapping | 2026-10-19 | Unifies `safety_identifier`, `metadata.user_id`, and `labels.safety_identifier` |
| Phase transfer | 2026-10-18 | Responses phase labels omitted on Chat/Messages delivery copies |
| Preset thinking quantization | 2026-10-20 | 6-level discrete token budget quantization with upward tie-breaking |
| Explicit effort precedence | 2026-10-31 | Preserve explicit effort over a conflicting numeric budget; omit an uncarriable budget with its named receipt, and quantize only when effort is absent |
| Thinking budget ceiling guard | 2026-10-20 | P3b rejects requests where thinking budget exceeds client output ceiling |
| Bounded token-budget adaptation | 2026-10-28 | Allows target-qualified review of gateway-derived defaults/presets to preserve functionality; concrete adjustment rules remain unselected |
| Messages missing ceiling default | 2026-10-20 | Go/Haiku injects `32000` default output tokens when client ceiling is omitted |
| Tool-result error wrapping | 2026-10-20 | Prefixes tool results with boolean status text for targets lacking native flags |
| Google terminal projection | 2026-10-21 | Gemini 3.8 Flash completions map to Messages `tool_use` or `end_turn` |

### History and cache decisions

| Decision | Date | Accepted Architectural Consequence |
|---|---|---|
| Append-only history projection | 2026-10-20 | Earlier conversation turns project immutably as new turns are appended |
| Scoped opaque replay | 2026-10-20 | Envelopes opaque tokens (`mc1.`); strips incompatible foreign reasoning on transfer |
| Manual-thinking history boundary | 2026-10-31 | Public Messages intake retains common trust/order checks; actual native manual-thinking candidates enforce the historical thinking prefix after projection |
| Prefix-cache ownership | 2026-10-24 | `Part.cache` owns request-lifetime vs explicit TTL breakpoints |
| Compacted context | 2026-10-20 | Opaque compaction items preserved in history; reject if target lacks carrier |
| In-memory stateful history | 2026-10-20 | Responses managed continuation via process-local TTL history |
| Agent session files | 2026-10-25 | Per-Agent JSONL directories for retained session persistence |

### Routing and security decisions

| Decision | Date | Accepted Architectural Consequence |
|---|---|---|
| Entry path safety | 2026-10-20 | Concrete unescaped ASCII paths; rejects dot segments and encoded separators |
| Automatic OAuth renewal | 2026-10-26 | Background renewal under account locks for tokens expiring within 120s |
| Stable failover routing | 2026-10-20 | Bounded candidate pools, generation-fenced cooling, and single-attempt defaults |
| Request conversion reuse | 2026-10-30 | [Bounded request-local retention](#request-conversion-reuse) preserves each body's bound identity and evidence; cache misses do not alter admission, and credential attachment stays attempt-local |
| Ingress Agent authentication | 2026-10-25 | Authenticates Agent keys at ingress, binding stable `agent_id` to context |
| Reverse-proxy delivery | 2026-10-26 | Proxy terminates TLS and compression; gateway enforces auth and unbuffered SSE |

### Delivery and evidence decisions

| Decision | Date | Accepted Architectural Consequence |
|---|---|---|
| Upstream protocol tests | 2026-10-23 | Pinned official SDK and upstream fixtures serve as regression baseline |
| Delta coalescing | 2026-10-26 | Default 50ms / 16 KiB buffer merges adjacent text deltas |
| Messages ordered block delivery | 2026-10-28 | Codec-local bounded serialization preserves owner/part order, incremental head output and signed history; incomplete text-owner wire closure preserves source lifecycle, native intake and EOF authority |
| Semantic-equivalence compatibility | 2026-10-28 | Protocol-equivalent empty/default forms use ordinary lossless mapping without named omissions or loss receipts; covers null-only reports, empty logprobs/cache options and already-enabled permissions; source observations and meaningful constraints remain protected |
| Client/IR projection principles | 2026-10-28 | After semantic-equivalence handling, P1–P5 classify remaining projections: request-context deduction, carrier-less non-content omission, delivery closure, planning-time impossibility and conditional source defaults |
| Responses strictness resolution | 2026-10-28 | Omitted `strict` resolves per tool and target by the documented compatibility rule; revised 2026-10-30: unknown compatibility and grammar overflow resolve non-strict, explicit strict stays guarded |
| Ladder applications | 2026-10-30 | Nearest effort (ties upward), model-maximum clamp, thinking-off fallback for required Chat history, uncarried reasoning omission, Messages `end_turn`/`tool_use` derivations, `block_binding` levels; `stop` without carrier and unknown Chat/Messages stop causes remain rejections |
| IR/codec simplification | 2026-10-30 | One request feature vocabulary; delivery projection over the event stream with JSON as reduce-and-render; `Option` presence except contract-defined null meaning; receipts only for L3/L4/estimates; no strictness compatibility classifier. Upstream streaming-only intake and folding `FidelityRecords` identity into IR are selected subject to their roadmap experiments |
| Upstream intake experiment | 2026-10-30 | R0 preserves a named Chat JSON exception: full-response metadata and URL annotations lack standard chunk carriers. Reachable Responses/Messages corpus comparisons found no analogous blocker; production transport/decoder changes remain R2 work. [Evidence and unqualified paths](refs/2026-10-27-client-ir-boundaries.md#production-corpus-and-intake-experiment-2026-10-30) |
| Reasoning presentation and continuity | 2026-10-28 | `summary:"auto"` and encrypted `include` resolve against target availability; explicit summarizers and missing opaque carriers take named omissions |
| Carrier-less report omission | 2026-10-28 | Served service classes and refusal categories without carriers are omitted on delivery copies; no approximate service tier; empty logprobs use semantic-equivalence compatibility instead |
| Messages terminal derivation | 2026-10-28 | Completed call-free outcomes without admitted stop sequences derive `end_turn` from any source |
| Generalized visible-history transfer | 2026-10-28 | Revises scoped opaque replay: uncarried reasoning of any issuer or lifecycle is omitted; same-issuer incomplete reasoning forwards exactly to Responses |
| Binding reasoning-history contract | 2026-10-28 | One assessed binding fact (`unsupported`/`ignored`/`accepted`/`required_with_tools`) is the shared owner for projection, delivery, planning and discovery |
| Phased binding-history qualification | 2026-10-29 | Explicit unassessed declarations (revised 2026-10-30: delegate instead of pre-rejecting); established facts migrate without inferring Provider history semantics from replay switches or output |
| Standard Chat reasoning omission | 2026-10-28 | Standard omits unsigned readable reasoning on delivery copies; revised 2026-10-30: uncarried opaque reasoning is omitted too, never forged; declared required-history Chat tools reject at planning without the readable profile |
| Owner-local cache omission | 2026-10-28 | Revises owner-local cache admission: all routes admit breakpoints; non-native candidates omit them by named rule |
| Incomplete generated tool arguments | 2026-10-29 | Select byte-preserving OpenAI prefix-string projection with incomplete lifecycle/termination; no JSON repair or executable-input claim; native Messages object restrictions remain |
| Grouped tool-result association | 2026-10-29 | Associate results by call ID without reordering; remove unsupported adjacency assumptions on qualified consumers, retaining identities, causal order and replay guards |
| Three-protocol default-loop closure | 2026-10-29 | Stage 4 requires current-client default reasoning/tool/history success on all three client protocols; retained rejections and nondefault profiles do not substitute; preserve actual built-in model metadata |
| Messages baseline retained | 2026-10-28 | Superseded 2026-10-30 by the revised Messages version policy; discovery still publishes unsupported behavior |
| Agent-facing discovery | 2026-10-28 | Gateway-published preferred interface and adaptation rules from one compiled owner over the conformance corpus |
| Scoped source accounting | 2026-10-20 | Retains raw usage scopes independently without premature summation |
| Honest delivery | 2026-10-20 | Publication freezes failover; completion requires validated upstream EOF |
| Configurable capture | 2026-10-20 | Four opt-in capture boundaries with mandatory credential redaction |
