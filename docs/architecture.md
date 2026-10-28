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
    - [Accurate IR and declared loss](#accurate-ir-and-declared-loss)
    - [Downstream schema completion](#downstream-schema-completion)
    - [Reported text configuration](#reported-text-configuration)
    - [Message phase transfer](#message-phase-transfer)
    - [Explanatory terminal reports](#explanatory-terminal-reports)
    - [Service policies and reports](#service-policies-and-reports)
    - [Lifecycle settings and service reports](#lifecycle-settings-and-service-reports)
    - [End-user attribution](#end-user-attribution)
    - [Selected request approximations](#selected-request-approximations)
    - [Google Messages terminal projection](#google-messages-terminal-projection)
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
- **Messages Version Policy:** Downstream Messages requires the pinned HTTP baseline `2026-10-21`. Requests must include exactly one matching `anthropic-version` header (including `?beta=true`). Removed or obsolete fields are rejected at intake.

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
- **Meaningful presence:** Explicit distinction between absent, null, empty, and default states where semantically meaningful.
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

| Classification | Gateway Action |
|---|---|
| **Must preserve** | Exact semantic preservation (instructions, tools, arguments, links, stops). Reject if unrepresentable |
| **Degradable** | Apply approved advisory omission or behavioral approximation on target copy; emit typed receipt |
| **Known unsupported** | Reject recognized standard semantics that have no target carrier (fail-closed) |
| **Legacy standard carrier** | Convert known historical fields when semantics are unambiguous |
| **Unknown extension field** | Accept and discard at valid object positions; do not forward or store |

#### Client Responses history shorthand

- Assistant text shorthand strings are accepted at intake.
- Output items in history require standard fields (`type`, `id`, `role`, `content`, `status`). Missing annotations arrays in user/tool items reject.

#### Candidate-local projection

Transformations are candidate-private:
- Source IR remains immutable across multiple failover attempts.
- Projections maintain history-prefix stability: appending new turns does not alter the projection of earlier conversation turns.

#### Model-effective values

Target capabilities enforce bounded ceilings via `ModelLimits` (maximum output tokens, allowed reasoning efforts). Preflight validation checks projected requests against model limits, rejecting invalid combinations without silent clipping.

### Request and reported meaning

#### Accurate IR and declared loss

Order of precedence: **Lossless mapping → Bounded approximation → Meaningful simple estimate → Rejection**.

| Rule Family | Policy |
|---|---|
| Prediction / advisory controls | Preserve in IR; apply exact native carrier, or omit via named receipt |
| Reasoning allocation / effort | Preserve budget and effort; apply directional quantization or scope approximation |
| Tool-result errors | Preserve native error flag; wrap in `[tool result is_error=...]` if target lacks boolean carrier |
| Instruction roles | Canonical Developer mapping; preserve root vs. ordered lifetimes |
| Output ceilings | Preserve explicit ceilings; insert target default only when client ceiling is absent |
| Terminal stops | Coarsen natural/stop-sequence causes to `stop` or `end_turn` under named rules |

- `chat.omit-null-stream-metadata.v1`: Chat SSE omits reported `metadata: null`.
- `messages.omit-null-response-metadata.v1`: Messages JSON and SSE omit only reported `metadata: null`, losing explicit-null presence on the delivered/retained view while preserving the upstream observation. Absent metadata needs no receipt; empty/nonempty maps and other unsupported execution hints still reject. The rule does not echo request metadata or reinterpret it as `metadata.user_id`.
- `chat.omit-text-item-status.v1`: Plain-text incomplete message items omit individual item status when overall finish reason is `length`.
(See [`ProjectionRule`](file:///home/sakura/development/MorphieCore/src/protocol/openai/lowering/projection.rs#L10) for the complete lowering inventory).

#### Downstream schema completion

Generation Text returns complete downstream schemas: **Upstream facts → Justified inference → Simple estimates**.
- `responses.infer-required-settings.v1`: Fills missing `tools`, `tool_choice`, and `parallel_tool_calls` from admitted request settings.
- `responses.echo-request-metadata.v1`: Echoes original admitted request metadata only when upstream omits metadata entirely ([`ProjectionRule::EchoRequestMetadata`](file:///home/sakura/development/MorphieCore/src/protocol/openai/lowering/projection.rs#L18)).
- `generation.complete-delivery-usage.v1`: Completes missing usage fields using text payload byte estimates (`ceil(bytes / 4)`).

#### Reported text configuration

Preserves absent, null, and object states in `TextOptions`. Null reported formats are retained without defaulting to plain text. Request-side null text configurations reject at intake.

#### Message phase transfer

Responses preserves `commentary` and `final_answer` labels. Targets without phase carriers omit phase under named rules (`generation.omit-delivery-message-phase.v1`) while preserving text and tool linkages.

#### Explanatory terminal reports

- Anthropic `stop_details` maps to `TerminalDetails.refusal`.
- Google `finishMessage` maps to `TerminalExplanation`.
- Explanations are omitted on cross-protocol delivery copies when no carrier exists, without modifying the underlying stop cause.

#### Service policies and reports

- Maps Anthropic service tiers and OpenAI priority flags to `StandardProcessing`, `FlexibleProcessing`, and `PriorityProcessing`.
- Service tier reports in stream deltas are rejected if altered after the stream opening snapshot.

#### Lifecycle settings and service reports

Responses and Interactions maintain separate opening and terminal reports validated by [`protocol::reports`](file:///home/sakura/development/MorphieCore/src/protocol/reports.rs). Cross-phase updates permit only evidenced runtime resolutions (e.g. `summary: auto → detailed` or `service: configured → standard`); conflicting drifts reject fail-closed.

#### End-user attribution

Opaque user attribution projects across protocols via [`protocol::attribution`](file:///home/sakura/development/MorphieCore/src/protocol/attribution.rs):
- Projects between OpenAI `safety_identifier`, Anthropic `metadata.user_id`, and Google `labels.safety_identifier`.
- Explicit safety identifiers take precedence over legacy `user` fields.
- Non-authoritative metadata: never utilized for authentication or cache identity.
- Strict fail-closed validation: values violating target length or character constraints reject without truncation or hashing.

#### Selected request approximations

- `openai.wrap-tool-result-errors.v1`: Prefixes tool results with boolean status text when target lacks native error flags.
- `deepseek.omit-parallel-tool-permission.v1`: Removes explicit `parallel_tool_calls: true` for DeepSeek targets; explicit `false` rejects.
- Terminal approximations: Coarsens stop causes to OpenAI `stop`, Anthropic `end_turn`, or `tool_use`.

#### Google Messages terminal projection

Gemini 3.8 Flash bindings map native completions to Messages terminals:
- `messages.google-tool-stop.v1`: Tool stops without explicit causes map to `tool_use`.
- `messages.google-stop-without-sequences.v1`: Natural stops on requests without stop sequences map to `end_turn`.

### Reasoning controls

#### Reasoning activation and effort scope

The IR decouples reasoning activation from effort level and scope:
- **Activation:** `ReasoningRequest` (enabled vs disabled).
- **Effort Control:** `EffortControl { scope, level }` with [`EffortScope::Reasoning`](file:///home/sakura/development/MorphieCore/src/semantic/task/generation.rs) (per-thought) or [`EffortScope::Response`](file:///home/sakura/development/MorphieCore/src/semantic/task/generation.rs) (turn-wide).
- Explicit `disabled` reasoning takes precedence over response-wide effort settings.

##### Google response-effort projection

`google.gemini-3.8-flash.response-effort-to-thinking.v1` maps response-scoped low/medium/high to Gemini thinking effort levels via [`ReasoningConversion::Scope`](file:///home/sakura/development/MorphieCore/src/protocol/reasoning.rs#L131).

#### Reasoning level calibration

Explicit hand-authored mappings define canonical model bindings. Fixed or ignored sampling controls (temperature, top-p, top-k) are omitted for models that do not support them via target-specific rules (`gemini.omit-sampling.v1`).

##### Thinking-budget quantization

When targets require discrete qualitative levels, numeric token budgets are quantized to discrete presets defined in [`ThinkingBudgets`](file:///home/sakura/development/MorphieCore/src/protocol/reasoning.rs#L35):
- Canonical presets are maintained in [`SHARED_BUDGETS`](file:///home/sakura/development/MorphieCore/src/protocol/reasoning.rs#L49) (6 discrete tiers spanning minimal to max).
- Selection matches minimal absolute token distance via [`ThinkingBudgets::nearest`](file:///home/sakura/development/MorphieCore/src/protocol/reasoning.rs#L108); exact ties resolve upward to the higher effort level.
- Current validation rejects requests whose thinking budget exceeds the client-specified maximum output token ceiling.

##### Bounded token-budget adaptation

Selected on **2026-10-28**: functionality may take precedence over conservative gateway-derived token defaults or presets through bounded, target-qualified adaptation. Prefer exact native controls; distinguish original client intent, derived allocation and actual model limits.

This direction does not select an adjustment formula or change current validation. A concrete rule must name the changed guarantee, target, finite bound, useful answer reserve and evidence. Explicit client ceilings or numeric thinking intent are not silently overridden; any relaxation of them needs its own selected rule. Source protocol validity, model maxima, memory/wire limits, timeouts and terminal honesty remain enforced. Unknown capacity is not unlimited capacity. The [budget proposal](api-expansion-design.md#independent-output-ceilings-unselected) owns unresolved alternatives.

##### Initial mapping plan

Bindings declare model-specific qualitative level translation via [`ReasoningEffortMapping`](file:///home/sakura/development/MorphieCore/src/protocol/reasoning.rs#L358) and [`ReasoningRequestPolicy`](file:///home/sakura/development/MorphieCore/src/protocol/reasoning.rs#L155):
- Conversions are strictly directional; missing or unsupported levels reject at intake and validation rather than defaulting.
- Upstream thinking budgets translate to qualitative effort via [`ReasoningConversion::BudgetToEffort`](file:///home/sakura/development/MorphieCore/src/protocol/reasoning.rs#L137), and qualitative effort expands to discrete token budgets via [`ReasoningConversion::EffortToBudget`](file:///home/sakura/development/MorphieCore/src/protocol/reasoning.rs#L142).

##### Implementation owners

- [`semantic::task::generation`](file:///home/sakura/development/MorphieCore/src/semantic/task/generation.rs): Activation and scoped effort types.
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

## History and replay

### Reasoning replay

Upstream opaque tokens and signatures are delivered in tagged [`mc1.`](file:///home/sakura/development/MorphieCore/src/protocol/replay_envelope.rs#L33) envelopes via [`protocol::replay_envelope`](file:///home/sakura/development/MorphieCore/src/protocol/replay_envelope.rs):
- **Isolation:** Payloads are bound to route fingerprints generated by [`issuer`](file:///home/sakura/development/MorphieCore/src/protocol/replay_envelope.rs#L40) and [`deployment_issuer`](file:///home/sakura/development/MorphieCore/src/protocol/replay_envelope.rs#L46) (binding provider ID, upstream model, and origin/path).
- **Foreign history transfer:** Routing to an incompatible model strips foreign reasoning on candidate copies (`transfer-visible-history.v1`) via [`protocol::replay_projection`](file:///home/sakura/development/MorphieCore/src/protocol/replay_projection.rs), preserving visible messages and tool calls.
- **Supported replay carriers:** Anthropic thinking signatures and `redacted_thinking`, Responses `encrypted_content`, Google `thoughtSignature`, and Interactions `function_call.signature`.

### Chat reasoning extensions

Public Chat defaults to the official Standard dialect. The trusted model profile `chat_profile: readable_reasoning` enables:
- `reasoning_content` in assistant JSON, SSE deltas, and returned history.
- `preserve_thinking` history reading intent.
- Unsigned, readable reasoning only; opaque signatures and structured reasoning details reject under this profile.

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

### Missing output ceilings

[`TargetContract.default_output_tokens`](file:///home/sakura/development/MorphieCore/src/protocol/anthropic/messages/target.rs#L70) defines optional ceiling insertion:
- Declared by Go/Haiku Messages (`32000`) to satisfy Anthropic mandatory `max_tokens`.
- Injected on candidate copies only when client request omits `max_tokens`. Explicit ceilings are preserved.

### Owner-local cache admission

Explicit cache breakpoints are admitted when all candidates on a route support native Messages caching (Go/Haiku binding). Mixed routes remain cache-closed.

### Caller-selected automatic cache admission

Request-wide automatic caching is enabled on native Messages targets with verified support. Breakpoints are placed at eligible turn boundaries without exceeding the 4-marker ceiling.

### Response rules

- Messages delivery requires valid signatures on thinking blocks; unsigned reasoning rejects.
- Block start order preserves source item ordering.
- Streaming argument fragments are buffered during whitespace prefixes to maintain valid JSON grammar for accumulators.

## Runtime and trust

### Failover

Failover operates on precompiled candidate lists under a cache-affine routing policy.

#### Legal candidates

Routes compile at startup (up to 64 candidates per model). Ingress requests cannot specify arbitrary candidate targets.

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
| Messages signed delivery | 2026-10-27 | Enforces native thinking signatures; unsigned thinking rejects |
| Local telemetry wiring | 2026-10-25 | Native structured logs via tracing/OTel without global runtime locks |
| Minimal diagnostic attribution | 2026-10-25 | Admission-owned Agent and request nonces tag traces and capture logs |
| Unified Generation evidence | 2026-10-24 | Replaces separate audit logs with `EvidenceDelta` and `Conversion<T>` |
| Standards reset & redesign | 2026-10-23 | Pinned official standards, exact-first fallback, and upstream test suites |
| Standard stream errors | 2026-10-24 | Standard Responses `error` events for classified stream failures |
| Native-only continuation | 2026-10-24 | Replaces synthetic manifest reasoning with genuine native replay carriers |
| Readable Chat reasoning | 2026-10-24 | Explicit `readable_reasoning` profile for `reasoning_content` |
| Latest Messages baseline | 2026-10-24 | Enforces HTTP baseline `2026-10-21`; removes obsolete sampling fields |
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
| Client semantic boundary | 2026-10-27 | Unknown extension fields discarded; known unsupported semantics reject |
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
| Ingress Agent authentication | 2026-10-25 | Authenticates Agent keys at ingress, binding stable `agent_id` to context |
| Reverse-proxy delivery | 2026-10-26 | Proxy terminates TLS and compression; gateway enforces auth and unbuffered SSE |

### Delivery and evidence decisions

| Decision | Date | Accepted Architectural Consequence |
|---|---|---|
| Upstream protocol tests | 2026-10-23 | Pinned official SDK and upstream fixtures serve as regression baseline |
| Delta coalescing | 2026-10-26 | Default 50ms / 16 KiB buffer merges adjacent text deltas |
| Messages ordered block delivery | 2026-10-28 | Codec-local bounded serialization preserves owner/part order, incremental head output and signed history; incomplete text-owner wire closure preserves source lifecycle, native intake and EOF authority |
| Messages null metadata omission | 2026-10-28 | Selected JSON/SSE null-only omission with per-view evidence; source observations remain unchanged and maps/other unsupported execution hints reject |
| Scoped source accounting | 2026-10-20 | Retains raw usage scopes independently without premature summation |
| Honest delivery | 2026-10-20 | Publication freezes failover; completion requires validated upstream EOF |
| Configurable capture | 2026-10-20 | Four opt-in capture boundaries with mandatory credential redaction |
