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
    - [Google strict function arguments](#google-strict-function-arguments)
    - [Protocol tree ownership](#protocol-tree-ownership)
      - [Shared Aliyun billing dialect](#shared-aliyun-billing-dialect)
      - [Subscription Responses](#subscription-responses)
    - [Field handling](#field-handling)
    - [Client Responses history shorthand](#client-responses-history-shorthand)
    - [Candidate-local projection](#candidate-local-projection)
    - [Model-effective values](#model-effective-values)
  - [Request and reported meaning](#request-and-reported-meaning)
    - [Conversion ladder](#conversion-ladder)
    - [Output-ceiling adaptation](#output-ceiling-adaptation)
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
      - [Ordered effort repair direction](#ordered-effort-repair-direction)
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

Generation supports JSON and SSE delivery. Responses, native Messages and Google upstreams use streaming intake even for downstream JSON delivery. Chat retains the [qualified JSON exception](refs/2026-10-27-client-ir-boundaries.md#production-corpus-and-intake-experiment-2026-10-30) for metadata and URL annotations without chunk carriers. Interactions uses stateless, full-history requests.

All three Generation client protocols dispatch through the public model's configured candidate route. A Messages client request does not require a native Anthropic upstream. Standalone tasks (Embeddings, Images, Speech, Transcription) operate on distinct contracts outside Generation.

- **Official Protocol Baselines:** Public Chat, Responses, and Messages adhere to current official versioned specifications. Provider-specific syntax and deviations belong strictly to trusted upstream adapters.
- **Diagnostic Extensions:** Optional namespaced response fields (`morphiecore`) provide safe-to-ignore telemetry and loss reporting. Standard SDK agent loops operate without parsing or returning these extensions.
- **Messages Version Policy:** Downstream Messages requires exactly one `anthropic-version` header, either `2023-06-01` or `2026-10-21` (including `?beta=true`), over the same current body grammar. Missing, duplicate, combined and unsupported versions reject before body collection. Legacy sampling fields (`temperature`, `top_p`, `top_k`) are validated then omitted under `messages.omit-removed-sampling.v1`; null defaults need no loss receipt. `anthropic-beta` is ignored as a header and never selects body semantics or trusted upstream headers.

Provider metadata resides in [providers.json](../src/provider/providers.json). Public models and bindings are defined in [topology/catalog.rs](../src/topology/catalog.rs) and assembled at [gateway startup](../src/gateway/startup/bootstrap.rs).

### Shared Provider metadata

`src/provider/providers.json` owns default origins, operation paths, override suffixes and authentication. It is embedded at compile time via `include_bytes!`, validated once, and immutable at runtime. Private `accounts.json` declarations bind named credentials and API-key target overrides through `src/account.rs`, deriving credential kind/profile from the catalog under the [credential ownership contract](provider-account-redesign-plan.md#credential-kind-and-header-placement); secret material and verified OAuth identities stay in the credential store.

| Field | Contract |
|---|---|
| `base_url` | Trusted origin only; no API prefix, query, fragment, or credentials |
| `auth` | `{ "kind": "bearer" }`, `x_api_key`, `x_goog_api_key`, or `{ "kind": "oauth", "profile": "openai" \| "grok" }` |
| `endpoints` | Closed map: `models`, `chat_completions`, `responses`, `messages`, `generate_content`, `interactions`, `embeddings`, `images`, `speech`, `transcription` |
| `path` | Concrete path relative to `base_url`; omitted operations are undeclared |
| `suffix` | Operation-relative path appended to an account's complete API-root override; never infer version-prefix changes |
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
| `account` | Named account declarations and effective target/credential binding | No secret duplication or caller-controlled target selection |
| `credential` | Credential store, API keys and managed OAuth registrations | Credentials never enter semantic IR |
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
- **Meaningful presence:** IR fields use `Option<T>` for optional/default values; intake normalizes documented null defaults (L2) and encoders emit the target grammar (required-nullable fields as `null`, optional fields omitted). Empty strings, zero counts, explicit false and observed empty containers remain values. A tri-state remains only for meaningful reported absence or an explicit clear. Reject-only IR placeholders are removed; malformed known wire values reject at intake. The [field review](refs/2026-10-27-client-ir-boundaries.md#generation-null-contract-review-2026-11-02) records the scoped source evidence.
- **Reasoning separation:** Human-readable thought text, summaries, and opaque replay signatures are distinct typed owners.
- **Honest accounting & terminals:** Missing usage counters are left unknown rather than zeroed. Truncation, failure, and cancellation are never mapped to success.
- **Runtime isolation:** Credentials, retry counters, and target URLs remain strictly outside semantic IR.

#### Canonical ownership

- Generation retains tri-state metadata and reported reasoning object/context/mode observations: explicit null must not authorize request-metadata fallback or missing-field lifecycle resolution. Nullable request reasoning/context use defaults; request mode remains non-nullable. No missing timestamp, cache diagnostic or comparison count is synthesized, and explicit diagnostic `unavailable` is a value.
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

#### Ordered tool availability

**Selected direction, expanded 2026-11-04; shared/native batch slice implemented.** Assess the complete Chat/Responses/Messages tool loop in all nine client-to-upstream directions. Preserve ordinary declarations, including Responses `additional_tools`, at their conversation positions in source IR. Prefer native/equivalent carriage, then useful bounded semantic approximation rather than rejection solely because exact carriage is unavailable. The user accepts lossy conversion as a design direction; concrete content/guarantee changes still need named dispositions and selection before activation.

This supersedes the earlier restriction of the assessment to native mappings and proven initial-prefix lifting. General active-set projection, historical declaration retention with current selection restrictions, reversible namespace aliases and custom-text wrappers are now evaluated explicitly as alternatives, not presumed equivalent or already implemented. Tool-removal prose remains instruction content, not a structural withdrawal. Explicit strictness, allowed calls, call cardinality and seriality must be interpreted by their source meaning; accepting history loss does not silently waive them.

**First-package content selection, 2026-11-04; implemented.** After native/equivalent carriage, the user selected this bounded tool-set projection combination:

| Named L3 disposition | Contract and acknowledged loss |
|---|---|
| Effective active-set projection | Preserve the current callable definitions and historical call/result content; lose old declaration visibility, insertion timing and rendered-prefix/cache stability |
| Retained-history/current-set projection | Prefer target-native historical declarations plus exact current-call restriction when enforceable; acknowledge lost withdrawal intervals or replacement semantics. Otherwise use the selected active-set projection only when the source set is determinate |
| Allowed-set inventory filtering | Where no allowed-subset carrier exists, filter root and inline declarations together, preserving allowed identities and optional/required invocation; lose filtered-definition visibility and potentially prefix-cache reuse |

Project candidate copies only. Never expose never-activated/withdrawn tools, infer structural withdrawal from prose, guess an unknown collision winner or fabricate an old schema. Strictness, selection, call count, seriality and dispatch remain independent guarantees; no downgrade is selected. Namespace aliases, custom-text wrappers, examples, result-media relocation, prompt annotations and non-developer returned-addition replay are not selected by this combination.

**Google extension selected 2026-11-07.** Existing generateContent and Interactions targets use the same effective active-set projection when ordered declarations have no native carrier. Historical call/result content and identities survive; old declaration visibility, insertion timing and prefix/cache stability take the existing `generation.collapse-tool-set.v1` loss. This adds no Google API, hosted execution, strict guarantee or serial carrier.

Repair the shared tool contract rather than adding a decoder-only exception. Initial definitions, namespace leaves and ordered additions must receive consistent validation, selection resolution, feature accounting, bounds and target checks. Reuse existing semantic tools, request/delivery projection, fidelity and codecs; no gateway tool registry, executor or nine pair-specific adapters. [Design and conversion matrix](api-expansion-design.md#three-protocol-tool-conversion-design) owns proposed mappings; [dated evidence](refs/2026-10-27-client-ir-boundaries.md#nine-direction-tool-assessment-2026-11-04) owns findings; [Roadmap](roadmap.md#conditional-stage-3-follow-ups) owns execution gates. Client-history annotations use the separate [L2 contract](#semantic-equivalence-compatibility); discovery and hosted execution remain outside this work.

Current shared validation separates intrinsic choice shape/count/uniqueness in settings from request-level membership in ordered availability. Deferred definitions stay withheld until surfaced; removal retains the identity for later re-addition. Current available identities have their own finite bound, independent of retained historical versions. Historical calls keep their original identity/arguments and are not validated against today's schema. Messages serial intent is emitted even for inline-only definitions, and candidate collapse updates history and settings atomically before validation.

Forced-choice cardinality is directional, not label-only. An OpenAI forced specific/custom choice means exactly one call and decodes onto the serial carrier (`parallel_tool_calls:false`); a Messages named or `any` choice without `disable_parallel_tool_use` means one-or-more and encodes to OpenAI targets as the exact required subset rather than a strengthened forced spelling; `none` makes the serial flag vacuous and it is omitted next to that selection. Targets without any serial carrier (Google paths) reject a forced choice instead of silently weakening it.

Named one-or-more choices become required subsets in candidate-local lowering (`openai.named-tool-subset.v1`, L2), before target admission. Writers only render the selected carrier. A dialect without that carrier rejects before dispatch; it cannot bypass admission by introducing `allowed_tools` during serialization.

The DeepSeek beta strict binding is endpoint-qualified: the model declares strict-tools capability, the standard binding's endpoint contract explicitly declines it, and a `deepseek-beta` provider chat path plus an explicit models-file candidate and account activate validation. Nothing redirects the standard endpoint, and catalog presence alone activates nothing.

Its default path is `/beta/chat/completions`; an account's complete API root receives only the operation-relative `/chat/completions` suffix. No beta-segment inference, duplicate removal or alternate-path retry applies.

One shared owner projects ordered tool declarations onto each target's selected carrier: Chat targets take the effective active set, Responses targets keep developer batches and convert pure by-value additions, and Messages targets with native changes lift a leading batch prefix, convert legal-position additions to ordered blocks, or fall back to the active set when no legal system placement exists. Allowed-set inventory filtering applies the flat restriction and required invocation on Messages targets. Bailian Responses also uses this selected loss when forced selection names one function but the inventory contains several: collapse root/inline definitions to the effective set, filter to the selected original function, and preserve the required-subset carrier and independent serial control. Bare multi-function `required`, explicit strictness and unsupported thinking profiles keep their own admission boundaries. Batch redeclarations without a documented winner block every projected fallback while native forwarding keeps them; converted items take fresh identities so surviving item ids keep their kind; historical call/result content and raw arguments are never rewritten. Responses source envelopes are validated and projected away before tool-item identity replacement, so converted batches cannot leave stale container members. Source-contiguity, participant attribution and replay checks remain enforced.

Retained projections must fit root declaration capacity (namespace containers and leaves included) and the final item count. A known carrier-capacity failure selects the existing active-set collapse only when the source set is determinate; malformed source values, unknown references and ambiguous winners remain errors. Final semantic and target validation still apply. Native Responses batch-only forwarding needs no effective inventory; other walks index identities while preserving declaration order, rather than scanning all historical definitions per member.

`Item::ToolBatch` owns one ordered definition list and a typed role observation; `FidelityRecords` retains its one wire ID. Empty batches and function/custom/namespace members keep their positions. This is distinct from Messages' singleton replacement/reference/removal operation and selects no same-name schema winner. One borrowed declaration view and one mutable function visitor cover roots, namespace leaves, singleton additions and batches for validation, feature/strictness checks and candidate-only omissions.

Responses input accepts only developer batches. Native output preserves every declared role without creating an instruction; manual and managed replay reject non-developer batches. Added snapshots may be empty; the bounded `ToolBatchSnapshot` event installs the final definitions before observed item completion. Batches have no partial wire status, so an incomplete overall response cannot finalize an unclosed batch. JSON, SSE and retained snapshots use this same owner and custom-tool capability checks apply to both opening and final definitions. [Source qualification](refs/2026-10-27-client-ir-boundaries.md#tool-contract-qualification-2026-11-04) and [SDK procedure](development.md#sdk-loopback-gate) describe the independent reference and connected check.

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

Bailian Responses reasoning event aliases are binding-qualified. Qwen's `reasoning_text` events describe summary text; the selected `deepseek-v4.1-flash-1017` binding uses the native reasoning-content domain instead. Event accumulation and item snapshots must agree on that domain. Item-done mismatches retain exact rejection; existing typed snapshot reasons classify the difference without granting terminal replay-refresh permission.

#### Standard request boundary

Four decoupled responsibilities:
1. **Client intake:** Enforces strict official protocol syntax; ignores SDK-specific helper properties.
2. **Semantic IR:** Holds normalized cross-protocol meaning.
3. **Target encoding:** Validates target-specific constraints and formats native requests.
4. **Provider egress dialect:** Translates standard target requests to vendor wire formats where required.

Strict-tool admission follows the effective binding/API disposition in shared request projection, including direct library lowering; writers render that decision. Native OpenAI/SIWC Luna uses the strict carrier. OpenRouter Luna delegates its unassessed endpoint guarantee and discovery reports `not_assessed`. Qwen Bailian Chat carries explicit strictness and resolves conditional defaults through the same owner; Bailian Responses has no equivalent strict-function carrier and rejects explicit true there. GLM's strict restriction remains model-qualified. These carrier decisions do not attest arbitrary-schema enforcement.

#### Google strict function arguments

**Selected 2026-11-09:** existing generateContent and Interactions targets admit a qualified closed-object function-argument subset through their native request-wide validation modes. The [local SDK evidence](refs/2026-10-27-official-sdk-sources.md#google-function-argument-carriers-2026-11-09) is separate from text-response schema support.

- Preserve supplied schemas exactly. The selected vocabulary is `type`, `properties`, `required`, `additionalProperties:false`, array `items`, string/numeric `enum`, `title` and `description`; nested objects must also be closed. Boolean types are supported, boolean enums are not. Integer enum literals stay within the safe binary64 integer range; fractional enum literals must survive binary64 serialization without precision change. Length, pattern, references, composition and other unqualified constraints retain explicit-strict rejection. Non-strict generateContent schemas may use references/composition without a literal root type; preserve their original document and delegate validation upstream. Explicit scalar roots still reject.
- Strict automatic selection uses `VALIDATED` / `validated`; required selection uses `ANY` / `any`; none stays none. Allowed names remain exact. generateContent optional allowed subsets use `VALIDATED`, including non-strict requests, without claiming a new strict guarantee.
- The mode covers the whole effective inventory. With any strict tool, every declaration's supplied schema must fit the subset. Mixed strict/non-strict declarations are accepted only within that inventory; non-strictness does not promise invalid arguments. Conditional source defaults resolve on candidate copies through the same schema and inventory check, otherwise taking the existing named conditional-default fallback. Explicit true never takes that fallback.
- Parameter omission keeps the native parameterless declaration. No schema deletion, argument repair, inferred prompting or local retry supplies a missing guarantee. Serial intent still rejects independently; one declared function can produce multiple calls.

Ordinary Google text/call signatures use the existing managed Responses continuation contract, not a stateless extension. Clients use `store:true` and `previous_response_id`; Chat/Messages and manually reconstructed stateless Responses retain their unsupported attachment boundary. This selects no signature deletion, synthetic reasoning owner or new Google API.

#### Protocol tree ownership

Module layout mirrors `vendor / api / responsibility`:
- `protocol::openai::{chat, responses}`
- `protocol::anthropic::messages`
- `protocol::google::{generate_content, interactions}`

##### Shared Aliyun billing dialect

DashScope and Aliyun TokenPlan share the `Bailian` wire dialect for Chat and Responses:
- Readable reasoning mapped to `reasoning_content` with `preserve_thinking`.
- Chat total output ceilings mapped to `max_completion_tokens`; Responses uses `max_output_tokens` with the model-specific scope in [output-ceiling adaptation](#output-ceiling-adaptation).
- Responses output constraints use the existing standard `text.format` carrier, preserving schema, name, description and explicit strictness. Do not reject the whole text configuration because verbosity lacks a native carrier. The [source refresh](refs/2026-10-19-bailian-compatible-requests.md#responses-output-format-refresh-2026-11-08) distinguishes documented layouts, observed SDK-flat carriage and model-specific qualification.
- Responses carries explicit `parallel_tool_calls`; Kimi's unavailable serial guarantee remains rejected. Default/enabled-thinking `qwen3.8-flash` and `glm-5.3` reject required, required-subset and specific selection because both required spellings fail the [qualified native controls](ar5-review.md#tool-policy-and-grok-request-qualification-2026-11-08). Serial admission does not supply that independent forcing guarantee. Model-bound direct lowering retains the existing mapping identity for the same checks; unbound task encoding does not qualify a model.

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
- Output items in history require standard fields (`type`, `id`, `role`, `content`, `status`). Assistant `output_text` may omit its empty annotations collection under the selected [L2 compatibility rule](#semantic-equivalence-compatibility); no other required fields are inferred.

#### Candidate-local projection

Transformations are candidate-private:
- Source IR remains immutable across multiple failover attempts.
- Native/equivalent history projections preserve earlier turns when new turns are appended. Named lossy projections such as selected tool-history collapse can alter the rendered prefix and lose cache reuse; they must record that consequence rather than promise prefix stability. They never mutate source history or alter routing affinity.

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

**Low-loss priority, reaffirmed 2026-11-07.** Field spelling or shape differences, incomplete capability assessment and missing optional controls are not independent rejection reasons. Prefer semantic matching, then a useful approximation, then omission with existing loss evidence. Apply these choices at the responsible projection/dialect boundary; ordinary L2–L4 rules need no new per-rule approval. Content or explicit-guarantee downgrades still need their concrete disposition selected. Unresolved cases may remain limited while independently useful workflows ship under the [partial-delivery exit](roadmap.md#partial-delivery-exit).

**Timing.** Request-side levels are resolved per candidate during planning, on candidate-local copies, so request-determined impossibility fails before dispatch (P4). After dispatch the upstream has already generated output; report and metadata facts bottom out at L4, and only content or outcome that would be misrepresented may fail delivery.

**Invariants outside the ladder:** credentials, auth, origin and trusted target selection; replay signature, origin and format trust; resource and time bounds; malformed known values and strict JSON; terminal honesty and validated EOF; call identity; no executable claim for incomplete tool input; explicit history-read intent; already-published accounting; candidate-local projection over immutable source IR; the routing freeze. No level invents content, forges signatures or fabricates success.

**Ownership.** The request side has one owner: one feature vocabulary, with each target declaring a disposition per feature (native, equivalent, approximate, omit, delegate or reject), and one pure projection applying it per candidate during planning. This vocabulary replaces the separate requirement, feature-set, feature-enum, target-contract and published-rejection representations; encoders consume only projected copies, and source-level admission keeps only grammar checks and L5. The delivery side has one owner in [`ClientAdapter`](../src/adapter/downstream.rs): a (fact, client protocol) table drives one projection over the IR event stream; JSON and retained views reduce the projected events to a snapshot and render it, so JSON/SSE agreement holds by construction and encoders only render. Discovery and `/capabilities` read the same tables.

Safety-assessment omission and its evidence belong to that projection, not a second encoder wrapper. Rendering and replay sealing do not introduce separate adaptation-result channels.

The request implementation lives in [`lowering/request.rs`](../src/lowering/request.rs) and [`lowering/disposition.rs`](../src/lowering/disposition.rs). Source trust validation and visible-history transfer precede candidate feature admission; the final check sees projected semantic fields and envelope controls. Presentation, summary granularity and encrypted-include decisions use candidate rows, refreshed after allocation conversion where native thinking availability matters; hidden-display guarantees without a carrier reject through those rows. Parallel permission preserves the binding's disposition, independently of the explicit serial guarantee. Native grammar and replay-identity assertions remain codec responsibilities. Ordered Messages effort/tool rewrites consume the same table rather than a second carrier-policy object. Request-envelope session affinity remains outside semantic IR, with its target disposition resolved by this owner. The [single delivery projection](#delivery-projection-ownership) and [shared discovery compilation](#generation-text-model-discovery) consume these owners.

**Evidence.** Receipts record only L3 approximations, L4 omissions and schema-completion estimates; L2 normalizations, derivations, defaults and request inferences produce none. Target encoders carry whole evidence deltas so coverage and truncation counts survive merging; Messages retains its directional preference for detailed receipts over matching field summaries. Rule IDs live in the disposition tables rather than a separate projection-rule enum and scattered string literals. Remaining field-specific follow-ups are listed in the [roadmap](roadmap.md#remaining-issue-dispositions).

Applications selected on 2026-10-30 are implemented, including request strictness, effort, ceilings, Messages intake/omission, delivery projection, history transfer and shared discovery.

| Area | Level and rule |
|---|---|
| Explicit `strict:true` without an equivalent carrier | L5. Conditional (omitted) strictness is always resolvable without a compatibility classifier: an already-closed schema on a target with an equivalent strict carrier stays strict, every other case resolves non-strict under the conditional-default loss (L3); a request-wide strict grammar overflow resolves all conditional definitions non-strict, and rejects only when explicit strict tools alone overflow |
| Effort | Nearest supported level, ties upward, across Reasoning/Response scopes at the same level (L3); manual budgets on adaptive-only bindings convert to adaptive plus effort. When an explicit effort already exists, preserve it rather than replacing it with a budget-derived level; omit the uncarriable numeric budget under a named loss |
| Output ceilings | Preserve equivalent carriers; clamp to a known model maximum or retain a narrower accounting scope with loss (L3); omit when no useful carrier exists (L4). See [output-ceiling adaptation](#output-ceiling-adaptation) |
| `required_with_tools` history with a Standard Chat client | Deployers select `readable_reasoning` on such public models; otherwise a candidate with a thinking-off carrier disables thinking under a named loss (L3), and one without rejects at planning. Not delegated, because the next turn is known to fail upstream |
| Uncarried reasoning in delivery or history | Omit the owner, keep visible content and tool links (L3); applies to opaque reasoning on Standard Chat and unsigned reasoning on Messages; never forged |
| Messages reasoning publication | An undecided summary waits for its actual signature in the bounded schedule. An irreversibly uncarriable shape discards its payload immediately and retains only bounded lifecycle tracking; invisible owners do not block visible successors. Summary interruption stays an interruption and cannot supply item or response completion |
| Unsealed or foreign client reasoning in history | Omit under visible-history transfer (L3); never forwarded |
| `stop` without a target carrier | L5 for now; gateway-side truncation may later provide L2 |
| Messages `stop_sequences` on a non-Messages target with an ambiguous natural/stop-sequence cause | `end_turn` under a named approximation (L3) |
| Unknown finish or incomplete reasons | Evidence-backed per-dialect maps; Responses carries an unspecified reason as `incomplete_details:{}`; otherwise L5 on Chat and Messages, so no outcome is misreported as a token limit |
| Responses client-history annotations | L2 `responses.client-history-empty-annotations.v1`; see [semantic-equivalence compatibility](#semantic-equivalence-compatibility) |
| Messages `thinking.block_binding` | Public adaptive-thinking intake normalizes `null`/`{}` under L2; `drop_block` takes `messages.omit-block-binding.v1` (L3) without relaxing gateway replay checks; `"error"` is L5. Trusted native target carriers remain independent |
| Legacy Messages sampling | Exact `temperature`/`top_p` in `[0,1]` and mathematically integral `top_k` in `[0,18446744073709551615]` take `messages.omit-removed-sampling.v1` (L4) at public intake; equivalent decimal/exponent spellings are accepted without rounding; malformed known values reject |
| Messages version and beta headers | See the [version policy](#protocols) |
| Unscheduled capabilities | Moderation, background, context management and compaction requests, hosted tools and items, `between_tools`, message-local null/reset and unselected format/task-budget controls, `clear_at`, nonempty `logit_bias`, legacy function carriers, `inference_geo` and upstream-reported storage keep their current rejection until scheduled |

Existing rule families remain valid ladder applications:

| Rule Family | Policy |
|---|---|
| Prediction / advisory controls | Preserve in IR; apply exact native carrier, or omit via named receipt |
| Reasoning allocation / effort | Preserve budget and effort; apply directional quantization or scope approximation |
| Tool-result errors | Preserve native error flag; wrap in `[tool result is_error=...]` if target lacks boolean carrier |
| Chat JSON grouping | Concatenate contiguous plain assistant text before function/custom calls, preserving call identity, order and raw arguments/input. Do not flatten rich/replay-bearing parts or reorder text across calls; custom SSE remains unsupported |
| Instruction roles | Canonical Developer mapping; preserve root vs. ordered lifetimes |
| Output ceilings | Use the [output-ceiling disposition](#output-ceiling-adaptation); insert a target default only when the original client ceiling is absent |
| Terminal stops | Coarsen natural/stop-sequence causes to `stop` or `end_turn` under named rules |
| SIWC failed-response codes | `responses.coarsen-provider-error-code.v1` maps confirmed quota to `rate_limit_exceeded`, unsupported capability to `invalid_prompt`, and service/auth/permission codes to `server_error`; failed outcome and original source evidence remain intact |

- `chat.omit-text-item-status.v1`: Plain-text incomplete message items omit individual item status when overall finish reason is `length`.
(See [`Rule`](../src/lowering/disposition.rs) for request rules and the [delivery projection owner](#delivery-projection-ownership) for client dispositions.)

#### Output-ceiling adaptation

**Selected 2026-11-07.** A client output ceiling is an adaptable generation control. Prefer retaining useful control over rejecting a request because the target cannot express exactly the same ceiling:

| Target carrier | Disposition |
|---|---|
| Same accounting scope under another field name | L2: preserve the value and use the target spelling |
| Equivalent ceiling above a known model maximum | L3: clamp to that maximum under the existing rule |
| Only a reply-token ceiling for a total-output request | L3: retain the same numeric value as a reply-token ceiling and record the accounting-scope loss; do not report that thinking tokens were bounded by it |
| No useful ceiling carrier or no expressible value | L4: omit the ceiling and record the loss; do not invent a tokenizer-based limit or silently raise the requested value |
| Carrier exists but model acceptance is unassessed | Delegate upstream validation rather than pre-reject solely for missing qualification |

Resolve spelling, scope and supported values for the actual binding/API, not a shared model label. Record approximations and omissions through the existing conversion evidence; no client echo is required. Preserve original intent on the source request, including absence versus a ceiling omitted by projection, so default insertion cannot overwrite it.

`generation.reply-only-output-ceiling.v1` records total-to-reply scope adaptation on thinking-enabled non-Qwen Bailian Responses targets. Values below that API's minimum take the existing ceiling-omission rule. Native source decoding retains its separate scope restriction: reply-only source values are not silently reinterpreted as total-output ceilings. DeepSeek Chat uses native `max_tokens`; Standard Chat keeps its own spelling.

Gateway request/response byte limits, deadlines, model bounds and honest terminal reporting remain independent. The adaptation introduces neither gateway-side token counting nor fabricated length termination or a general formula for increasing thinking budgets.

#### Semantic-equivalence compatibility

Accepted on 2026-10-28; this is ladder level L2 of the [conversion ladder](#conversion-ladder). The selected null-metadata, empty-logprobs and [reviewed equivalent forms](refs/2026-10-27-client-ir-boundaries.md#semantic-equivalence-review-2026-10-28) are implemented. Further L2 applications are found through the [scoped follow-ups](roadmap.md#remaining-issue-dispositions) rather than a separate field-by-field review. Protocol-defined equivalent empty values, empty collections, defaults and structural placeholders belong to lossless mapping. Accept, normalize, omit or supply them as the target grammar requires, without a separate named-omission path or loss receipt.

Equivalence depends on the field, direction and lifecycle contract, not emptiness alone. It must preserve content, control intent, explicit constraints, identity, report facts and subsequent behavior. Preserve source observations; apply target representation changes in the existing codec/projection owners. Do not introduce recursive empty-field stripping or globally equate absent, null, empty, false and zero. Unknown fields and malformed known values retain their existing admission rules.

Selected applications are reported `metadata:null` on Chat SSE and Messages delivery, and `logprobs:[]` on Messages delivery and history admission. These carry no metadata entries or token-probability observations to lose. Same-protocol carriers still preserve source presence where representable, and target-required fields remain required. Null metadata does not echo request metadata or become Messages `metadata.user_id`; empty/nonempty metadata maps and other unsupported execution hints retain their current dispositions. Nonempty logprobs still reject where no carrier exists, and an explicit request for probabilities is checked independently of an empty output report.

An empty update that clears state, an empty restriction set, an identity-bearing block or a terminal report is not equivalent to omission merely because its payload is empty. Stream accumulation must not erase earlier reports or suppress completion. Tests belong at the responsible boundaries: equivalent forms agree across JSON/SSE/history; meaningful nonempty values, source observations and state transitions remain protected.

This decision removes `chat.omit-null-stream-metadata.v1` and `messages.omit-null-response-metadata.v1` and their receipts. Actual removal of meaningful information continues to require a selected named loss or rejection.

Further selected applications: null-only system fingerprints on Responses/Messages and terminal explanations on Messages carry no report value; explicit `logprobs:false` needs no Messages probability carrier. Document citations default off: an omitted/false `citations.enabled` normalizes to an uncited document without a loss receipt; enabled citations remain unselected and malformed known flags reject. Empty cache options normalize without a dedicated rule. On trusted always-parallel targets, explicit parallel permission is exact; explicit serial intent still rejects. The obsolete `cache.omit-inactive-options.v1` and `deepseek.omit-parallel-tool-permission.v1` rules are removed. Meaningful fingerprints/explanations, active cache controls and requests for probabilities retain their loss/rejection contracts.

Chat request `logit_bias:null` is its documented default, equivalent to omission with no active token bias or loss receipt. Non-null values retain rejection; tokenizer-bound active bias remains unselected. This field-specific rule does not equate arbitrary empty objects with null.

Selected on 2026-11-06: `responses.client-history-empty-annotations.v1` interprets an omitted `output_text.annotations` collection as empty at Responses Client-history intake only. Explicit arrays retain their values and validation; null and malformed known values reject. Target encoding emits the official required array. Upstream response/event snapshots and closed request codecs keep their required-array validation. This L2 compatibility produces no loss/insertion receipt and does not infer identity, lifecycle or other missing fields. It accepts the empty-collection scenario; it cannot recover nonempty annotations discarded by a client.

#### Client/IR projection principles

Accepted on 2026-10-28 for Client/IR stabilization. Apply [semantic-equivalence compatibility](#semantic-equivalence-compatibility) first; classify remaining projection rules under the principles below. They are applications of the [conversion ladder](#conversion-ladder). Sections marked *implementation pending* state the target design; current behavior is unchanged until the corresponding [selected follow-up](roadmap.md#remaining-issue-dispositions) lands.

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

Responses retrieval retains the argument string selected by the shared event projection. SSE, JSON and retained snapshots consume those same selected bytes, including incomplete prefixes without JSON parsing. Native structured observations and protected replay dependencies remain unchanged; representation conversion alone needs no loss receipt.

#### Delivery projection ownership

Implemented on 2026-11-01. `ClientAdapter` owns one stateful `ClientProjection` and a shared fact/disposition table. It selects native argument carriers, reasoning/refusal substitutions, report omissions, usage views, terminal mappings and grouping before rendering. SSE encodes its selected events; JSON and retained history reduce those events and render the resulting snapshot. The old independent snapshot representability owner and encoder-side terminal lowering are removed. Encoders retain protocol grammar, lifecycle and resource checks rather than selecting another conversion policy.

Projection operates on private delivery copies. Execution-assigned response/item aliases are merged before materializing JSON or retained views; protected call dependencies keep their execution ownership until the separately scheduled identity migration. Wire-only Messages block/owner closure does not finish an incomplete semantic item.

New Responses message aliases use `msg_` for both delivery and cross-protocol request history. Native returned aliases and call/result identities remain unchanged. Generated-ID collision checks follow the target carrier: Responses validates generated message aliases; Chat cannot reject a collision involving a message alias it never emits. JSON, SSE and retained delivery share the same execution-assigned alias.

Replay dependencies refresh for the affected owner after projected item start/finish and for all owners at terminal, before final validation. Metadata and alias refreshes do not rebuild replay dependencies. Intermediate readable deltas may leave an open owner's dependency stale; no consumer relies on that association before its item boundary or terminal.

Null-equivalent delivery reports and hints use `Option`: service class, fingerprints, refusal details, probability reports and execution echoes. Request/response `metadata` remains three-state because explicit null suppresses request-metadata echo. Equivalent accounting conversions and empty/default carriers do not acquire loss receipts; L3/L4 removals and explicit estimates retain their named evidence.

For a Chat client, native lifecycle opening service reports are omitted under `chat.omit-opening-service-report.v1`; a terminal non-null served class uses its exact carrier. Source opening/terminal reports stay distinct. Messages refusal becomes visible text under `messages.refusal-as-text.v1`, selected before both JSON and SSE renderers.

The [closed dialect terminal maps](refs/2026-10-17-generation-contracts.md#chat-terminal-source-recheck-2026-11-01) share JSON/chunk classification. Resource, network and server failures use the failure path; interrupted generation stays incomplete. Unknown labels reject. Unrepresented Responses incomplete-reason labels take `responses.omit-unrepresented-incomplete-reason.v1` without inventing natural completion. Terminal details that would change a visible completion guarantee retain their L5 boundary.

#### Generated tool arguments and execution input

Selected on 2026-10-29; initial cross-view implementation landed on 2026-10-31. An unfinished generated argument prefix projects exactly to an OpenAI argument string without parsing, repairing or completing its bytes. Preserve incomplete call lifecycle where carried, the incomplete overall outcome and token-limit termination. Format conversion alone needs no loss receipt; unrelated lifecycle, grouping and protected-replay restrictions remain authoritative. A generated string is not a complete validated invocation. Native Messages complete JSON/history still requires an input object; its partial SSE carrier does not supply that object.

All qualified non-Chat target requests select SSE, including the body flag, transport Accept header and response-media admission. JSON clients aggregate that intake after validated EOF, so native Messages can report incomplete arguments without inventing a native JSON object. Replaced non-Chat JSON dispatch is removed; complete native JSON codecs still provide independent directional protocol coverage.

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

Standard Responses permits opening `service_tier:auto` to resolve to terminal `default`, independently of Provider-specific reasoning-report rules. Preserve both observations; a changed same-phase report still rejects. The [official service-tier contract](https://developers.openai.com/api/reference/resources/responses/methods/create) distinguishes configured processing from the tier actually used.

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

##### Ordered effort repair direction

**Direction selected 2026-11-05; representation selected 2026-11-06.** Root settings are initial controls. `ConfigurationUpdate.boundary` independently carries `FollowingResponse` or `NewInputTurn`; effort scope describes the affected work, never activation timing. Responses updates select subsequent responses. Messages updates immediately after a new-input user turn select its reply; elsewhere they remain pending until new user input. Tool-result-only turns do not activate them; mixed results and new input do. Active values persist until replaced. `GenerationRequest::resolved_configuration` derives active/pending controls in one bounded, pure scan without storing a second mutable setting. Active and pending values are derived `ConfigurationState` snapshots, not merged update patches. Both use one ordered transition rule, so off clears prior reasoning effort and later enable cannot resurrect it; immediate updates also apply to the pending state.

Equivalent native carriers retain historical and repeated markers. Non-equivalent or carrier-less candidates resolve source timing before target scope/level/budget conversion and refresh current-control admission. `generation.collapse-configuration.v1` records the loss of historical/pending controls; `generation.configuration-prefix-change.v1` records their removed message boundaries and cache-prefix change. Source history stays unchanged across candidates and failures. The same bounded projection extends to the existing Google targets by user selection on 2026-11-07; their scope/level mapping follows source-effective activation, never the last marker. A reasoning-scoped named effort replaces an earlier off selection; explicit off clears inapplicable reasoning allocation, while response-scoped effort remains independent. History transfer compares source and candidate states at every configuration marker, not merely the final effective value. If deleting an assistant owner changes an activation interval, even a native-capable target takes the selected collapse using original source controls. No synthetic assistant boundary or untrusted replay is retained as a placeholder.

The existing message-envelope owner now includes `System`. Public and native Messages intake share known-field/role validation and empty-string normalization, retaining original system membership rather than raw payloads. Native grouping validates order/contiguity before its fast path and preserves system-container boundaries under append. Public unknown fields are still filtered. Removing or changing an earlier effort marker blocks a candidate that retains later signed/redacted thinking; changes strictly after protected blocks can proceed. Changing root effort alone does not invalidate thinking. `drop_block` does not excuse a gateway prefix rewrite. Direct Messages encoding retains strict issuer admission and shares transfer-before-projection with routed encoding; rendering performs no later history deletion.

Named local effort is admitted only on system messages; omitted system content is empty. Content-bearing empty configuration has no fields to apply; empty content without a control rejects. Message-local null configuration/effort and Responses null update effort remain unresolved and reject without inventing reset semantics. Directly adjacent Responses updates reject; Messages repeated markers remain lawful. No new format, task budget, `clear_at`, thinking mode or weaker content/explicit guarantee is selected. [Source qualification](refs/2026-10-27-client-ir-boundaries.md#ordered-effort-contract-qualification-2026-11-05) owns evidence; [Roadmap](roadmap.md#remaining-agent-qualification) owns remaining acceptance.

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

This direction does not select a general adjustment formula or change thinking-budget validation. The [output-ceiling adaptation](#output-ceiling-adaptation) separately selects equivalent spelling, model-maximum clamp, narrower-scope approximation and carrier-less omission with loss evidence. Other allocation changes must name the changed intent, target, finite bound, useful answer reserve and evidence; numeric thinking intent is not silently overridden. Source protocol validity, model maxima, memory/wire limits, timeouts and terminal honesty remain enforced. Unknown capacity is not unlimited capacity. The [budget proposal](api-expansion-design.md#independent-output-ceilings-unselected) owns unresolved alternatives.

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

The gateway publishes each model's `preferred_interface` and each interface's `agent_defaults` and `delivery_adaptations`. One startup compilation derives the Generation contracts shared by models and `/capabilities` from the pinned public Agent-default corpus and the [conversion-ladder](#conversion-ladder) disposition tables; `/capabilities` retains other tasks. Default-request qualification checks the serialized ingress body against the configured request ceiling before semantic admission and candidate projection.

Preference follows only the primary candidate's native Chat, Responses or Messages correspondence, and only when that activated interface passes every assessed default text/tool/reasoning request, including SSE. Otherwise it is `not_assessed`, with null protocol/uniformity. The `basis` is explicit; `uniform` is true only if every candidate qualifies the same native correspondence. No fallback interface is ranked by conversion loss, and no candidate count, order or identity is disclosed. A body-hash `ETag` is allowed; no maintained revision is introduced.

Simple feature support, default-request disposition and full delivery/history support are separate claims. Explicit-strict synthetic tools do not stand in for omitted-strict Agent declarations. Keep unassessed/nonuniform facts and partial/composition coverage explicit. Publishing a preference does not establish that a client consumes it: integration must state whether application is automatic, explicit configuration or advisory. This neither permits capability-based candidate skipping nor changes routing priority.

Tool selection/history discovery uses the same candidate projector as execution for finite seriality, source-directional named cardinality, required allowed-subset and ordered-addition witnesses. The [usage domains](usage.md#generation-text-model-discovery) define their limits; a witness does not qualify arbitrary tool histories or live model behavior. The pinned Agent-default corpus is a reproducible artifact baseline, not an assertion that a same-version installed serializer is byte-identical; installed-client acceptance must recharacterize drift independently.

Effective control support and request admission are distinct projections of the same probe. An omitted maximum-output control is `unavailable` as effective support but an admitted named adaptation in `/capabilities`, not rejected input. Private per-candidate default qualification includes JSON/SSE independently of route-common delivery publication; matching another interface also requires the same primary endpoint and credential scope.

## History and replay

### Reasoning replay

Upstream opaque tokens and signatures are delivered in tagged [`mc1.`](file:///home/sakura/development/MorphieCore/src/protocol/replay_envelope.rs#L33) envelopes via [`protocol::replay_envelope`](file:///home/sakura/development/MorphieCore/src/protocol/replay_envelope.rs):
- **Isolation:** Payloads are bound to route fingerprints generated by [`issuer`](file:///home/sakura/development/MorphieCore/src/protocol/replay_envelope.rs#L40) and [`deployment_issuer`](file:///home/sakura/development/MorphieCore/src/protocol/replay_envelope.rs#L46) (binding provider ID, upstream model, and origin/path).
- **Candidate history transfer:** Incompatible reasoning is omitted on candidate copies under `transfer-visible-history.v1` by the [request projection](../src/protocol/request.rs), preserving visible messages and semantic tool links. Issuer isolation and assessed carrier availability are separate selectors.
- **Supported replay carriers:** Anthropic thinking signatures and `redacted_thinking`, Responses `encrypted_content`, Google `thoughtSignature`, Interactions `function_call.signature`, and the explicitly selected [Chat reasoning transport](#chat-reasoning-extensions).
- **Responses stream authority (selected 2026-11-06):** `response.output_item.done` binds completed reasoning replay once, following the official [streaming contract](https://developers.openai.com/api/reference/resources/responses/streaming-events). The terminal response snapshot is independently decoded and verifies output order, owner/type, lifecycle, phase, visible content, tool arguments and other known fields; it never replaces completed stream items. Two nonempty encrypted encodings on the same completed reasoning item need not be byte-equal: the item-done value remains authoritative for JSON, SSE and next-request replay. Replay presence changes, malformed values and differing incomplete-item tokens still reject. Required item closure, terminal publication, origin isolation and EOF checks remain strict.

- **Incomplete reasoning:** Same-issuer `incomplete` reasoning with a finalized replay token forwards exactly to Responses. Client `in_progress` partial tokens remain invalid at replay opening.
- **Client trust:** Malformed recognized `mc1.` envelopes reject before projection without mutation. Unsealed native values confer no issuer authority and are omitted during target history projection; they are never upgraded to trusted replay.
- **Visible-history transfer (L3):** When a target lacks a carrier for historical reasoning (including unsealed or foreign reasoning, full-readable text on summary-only targets, foreign citations, and obsolete markers), the uncarriable reasoning owner is omitted under a named loss receipt (`chat.transfer-visible-history.v1`, `responses.transfer-visible-history.v1`, `messages.transfer-visible-history.v1`). Chat history selection shares final validation's part and following-carrier shape checks; a readable or structured carrier flag alone is insufficient. Visible content and semantic tool links are strictly preserved; reasoning is never forged or moved into text. Explicit client `read_history: true` cannot be satisfied by omission and rejects.

#### Binding reasoning-history contract

A Generation binding declares one reasoning-history contract: `Unsupported`, `Ignored`, `Accepted`, or `RequiredWithTools`. `None` indicates an unassessed binding.

1. **Unassessed bindings (`None`)**: Planning does not reject on unassessed facts. The candidate sends carriable history and delegates validation to upstream (delegation under the conversion ladder); discovery publishes `not_assessed`. Delegation never applies to credentials, auth, or replay origin trust.
2. **`RequiredWithTools` handling**: When a binding declares `RequiredWithTools` and a Standard Chat client sends tools with thinking active:
   - If the public model profile selects `readable_reasoning`, the request is admitted.
   - Otherwise, if the candidate supports disabling thinking (`enable_thinking: false` or effort `none`), planning falls back to disabling thinking under named loss `chat.fallback-thinking-off.v1` (L3).
   - The fallback changes allocation and its active presentation only; explicit context, history-read, hidden-display and replay-mismatch guarantees still reach ordinary target admission.
   - If thinking cannot be turned off, or explicit `read_history: true` prevents the fallback, planning rejects (`admission.client_representation`).
   - Tool-free requests or requests with thinking already off add no history requirement.
3. **Discovery**: Discovery reads candidate declarations, publishing a common fact, `non_uniform` for differing assessed facts, or `not_assessed` if any candidate is unassessed.

Request projection applies the same carrier and trust checks for planning, dispatch re-projection and prefix-cache copies. The executor selects a target; lowering owns its history projection. Neither a declaration nor successful encoding establishes full-loop carriage. Standard Chat's selected delivery omission below is independent of guessing a Provider history fact. DeepSeek's [thinking contract](https://api-docs.deepseek.com/guides/thinking_mode/) (read 2026-10-28) requires **all previous turns' reasoning** when a thinking request carries tools; Zhipu's documented requirement concerns the current tool cycle. The shared fact does not collapse those scopes or authorize dropping explicit read-history intent.

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
- Typed `reasoning_details` for summaries and protected replay. Each owner keeps a separate `reasoning.encrypted` entry containing its existing `mc1.` envelope; opening restores the original native format, payload and issuer. Signatures are not flattened into readable text or attached to adjacent text details that clients may merge.

**Selected 2026-11-08:** Extend the existing profile rather than adding another configuration value or enabling the whole MorphieCore dialect. Pure unsigned text keeps its scalar JSON/SSE shape. Readable deltas never wait for a signature: summary metadata streams with its typed part; a late signature backfills only already-retained structured metadata at actual owner closure, without repeating visible text. This is not whole-response buffering or permission to publish a partial replay value. Following-carrier, owner/dependency, origin, lifecycle and terminal checks remain required.

Clients must return the complete details sequence and envelope bytes unchanged. The profile has no `reasoning` alias or private thinking controls; unknown Client fields still follow the [semantic boundary](#field-handling). This transport does not activate ordinary Google text/call attachment carriers. Dated client retention evidence and the adjacent-signature counterexample are indexed in [References](references.md#research-notes).

Standard Chat omits unsigned readable reasoning under `chat.omit-readable-reasoning.v1`, matching official Chat, which exposes reasoning only through usage. Source parsing/reduction, accounting, visible content, tool links, and terminal authority remain intact; reasoning is never moved into visible text. Standard Chat has no carrier for replay-bearing reasoning, so delivery omits the owner under `chat.omit-uncarried-reasoning.v1`, preserving visible content and tool links without forging signatures. Declared `required_with_tools` Chat bindings apply the [fallback contract](#binding-reasoning-history-contract).

### Compacted history

Compacted conversation summaries are preserved as distinct `Compaction` items with opaque payloads wrapped in `mc1.` envelopes. Compaction items are required context and reject if the target lacks a compaction carrier.

### Stateful history

Gateway-managed continuation via `previous_response_id` on OpenAI Responses is managed by [`protocol::openai::responses::managed`](file:///home/sakura/development/MorphieCore/src/protocol/openai/responses/managed.rs):
- Stores delivered response projections, typed hidden replay attachments and request contexts in process-local TTL memory (default 1 hour).
- Synchronous retrieval, deletion, and cursor-paginated input listing.
- Upstream requests remain fully stateless (`store: false`).

Google part/call thought signatures use this retained owner when the public protocol has no equivalent carrier. An explicit `store:true` and continuation through `previous_response_id` preserve them without exposing fabricated wire fields. Reconstructing stateless input from that signature-less wire view cannot restore those attachments; stateless delivery rejects rather than silently discarding replay authority.

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

Selected and implemented on 2026-11-07: trusted OpenCode Go/Haiku intake may close an open tool-input block at an explicit `max_tokens` message delta when the received bytes form a valid unfinished object prefix. Aggregate and incremental decoding share that closure, preserve exact bytes and identity, and mark the call incomplete without executable completion. The rule requires a started object, subsequent `message_stop` and strict EOF; public Standard Messages decoding retains its baseline ordering. It does not infer closure for complete JSON, other block kinds or unrelated terminals.

## Runtime and trust

### Failover

Shared named-account binding, scoped health, material-aware auth probes, success-owned affinity and bounded same-target recovery implement the accepted [execution policy](provider-account-redesign-plan.md#execution-policy). [Roadmap](roadmap.md#remaining-agent-qualification) tracks remaining profile qualification and prerequisites. Production deployment requires separate approval.

Failover operates on precompiled candidate lists under a cache-affine routing policy.

#### Legal candidates

Routes compile at startup (up to 64 candidates per model). Ingress requests cannot specify arbitrary candidate targets.

#### Request conversion reuse

Gateway preflight retains credential-free final bytes, protocol-header projections, bounded prefix digests and adaptation evidence together with the actual bound candidate. Executor ordering moves the whole record; preparation consumes it without reconstructing JSON or repeating semantic conversion. Public planning results remain metadata-only, and discovery uses the checked projection directly.

The checked projection owns an immutable wire value, its bounded serialized-size measurement and evidence. Retention admission, exact body allocation and eligible prefix hashing reuse that measurement instead of serializing again to count. Gateway materialization shares lookup eligibility with affinity selection and derives prefixes only for eligible initial lookups; direct preparation, disabled/explicit/unmapped lookup paths and already-hinted requests do not derive them.

The request-local retention budget is 1 MiB, charging body/header/digest allocation capacity and evidence including spare vector slots. Overflow leaves candidate admission and ordering unchanged: an attempted cache miss recomputes the existing pure conversion. Remaining records belong to the exchange future and are released on completion or cancellation, not retained in shared gateway state.

Preparation still validates the trusted binding, streaming and final body bounds before attaching credentials and account headers. Codecs validate header carriers during request encoding; materialization stores the projected headers as a plain vector rather than retaining a deferred error state. An affinity hint changes a typed candidate-local request and carries its new checked conversion; a hint that cannot fit leaves the original accepted body intact. The test-only forced-stream override likewise invalidates its body. No path patches encoded JSON or reparses it through generic `Value` deserialization, which could reinterpret ordinary reserved-key schema objects.

#### Advancement

The production model loader fixes one account target before dispatch. Configuration exposes no fallback switch, credential-member budget or route send ceiling; the old chain/transition internals are removed. One lease and one prepared request own the original send and at most one serial retry. The retry reuses the exact effective endpoint, credential material, headers and body, with a network-free lifecycle/material veto before sending.

Publication remains irreversible: no later attempt may replay committed output. Provider-owned finite classification separates health from the narrow resend allowlist. Typed transport causes distinguish positive interruption/connection timeout from TLS, local faults, unknown causes and ambiguous post-dispatch timeout; no private URL or third-party message enters policy. Unknown structured codes retain actual-status fallback and unknown in-band errors gain no synthetic-500 resend permission.

The short recovery clock covers cleanup, hints, preparation and the retry until projected deliverable content or a validated successful no-content terminal. Headers, usage and lifecycle frames cannot remove it. Buffered JSON remains private and terminal publication still requires validated EOF; readiness removes neither the original request deadline nor the attempt deadline. The [execution contract](provider-account-redesign-plan.md#bounded-same-target-retry) owns budgets and eligibility.

#### Health and recovery

One exchange-local failure ledger retains the original HTTP observation, later interruption, material revision and previous-attempt evidence. Terminal cleanup resolves health and releases its lease before asynchronous downstream normalization.

- **Scoping:** Auth/quota constrain an account; transient, rate-limit and permission failures constrain its exact model/operation/origin/path target. Independent constraints retain their own deadlines.
- **Probe ownership:** Acquisition is atomic across account and target scopes. Account probes exclude sibling-model probes in either acquisition order; target probes alone do not exclude healthy sibling traffic. Cancellation releases ownership without declaring recovery.
- **Recovery:** Only validated successful work can clear health, subject to generation fencing. Reported failures and independently identified transport interruptions retain observation times and material revisions through cleanup or cancellation. Clean HTTP EOF missing a protocol terminal is malformed, not positive transport-interruption evidence. A completed retry discards its first failure's pending penalty; otherwise merged account/target constraints advance each affected scope at most once. Attempt-local output resets while usage and diagnostics remain separately attributable. Provider-owned timing and narrowly scoped material replacement follow the [selected contract](provider-account-redesign-plan.md#cooldown-and-recovery).
- **Downstream hints:** Upstream reset evidence remains available to health/recovery. Returned `Retry-After` describes only a credible short route-wide block and is suppressed when another usable target is ready; `x-should-retry` is never emitted.

#### Routing unit

An explicit conversation/session carrier is the routing unit, scoped by authenticated Agent and public model. Only a valid binding earns one bounded cooldown wait; after waiting, lifecycle and probe ownership are rechecked before dispatch. Successful final delivery conditionally establishes affinity to the serving target.

### Conversation and cache affinity

Affinity keys and advisory cache policies are defined in [`protocol::cache`](file:///home/sakura/development/MorphieCore/src/protocol/cache.rs):
- `X-MorphieCore-Conversation-Id` establishes route affinity across multi-turn exchanges via hashed SHA-256 grouping keys.
- Prefix-cache indexing supplies advisory request hints, never an implicit conversation or account-routing key.
- Target cache capabilities are enforced via [`CacheTargetPolicy`](file:///home/sakura/development/MorphieCore/src/protocol/cache.rs#L48), omitting unsupported advisory hints under declared loss rules.

Bound replay issuers include configuration namespace, credential provenance and the exact public-model/target/operation identity. API-key replacement invalidates old private replay; normal OAuth material renewal preserves provenance. Codecs use the bound issuer for OpenAI, Messages and Google replay, not only prefix-cache keys.

### Streaming and delivery

Delivery lifecycle and event streaming are coordinated by [`ResponseDelivery`](file:///home/sakura/development/MorphieCore/src/execution/delivery.rs#L100) under [`SseLimits`](file:///home/sakura/development/MorphieCore/src/protocol/stream.rs#L8):
- **Pre-publication holding:** Bounded lifecycle-only frames wait for content while the one same-target retry remains possible.
- **Publication freeze:** Flushing frames irreversibly prohibits another send; subsequent upstream failures abort the stream with terminal error events.
- **Honest completion:** Streams require validated upstream EOF before downstream release.

#### Semantic stream normalization

Codecs validate and reduce upstream events in source order, emitting legal target streams:
- Preserves delta ordering, call identities, and signature attachments.
- Emits block openers before deltas, and closes blocks before terminal events.
- Interleaving tool calls and text parts are serialized to conform to target grammar.

Responses opens with exactly one `response.created`, including when its actual status is `queued`. Queued does not establish semantic Started; only an observed `response.in_progress` starts a queued response. Encoding then emits progress without a second created event. The pinned [OpenAI SDK created-event contract](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_created_event.py) admits a Response with queued status, and its [accumulator](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/lib/streaming/responses/_responses.py#L515-L524) requires created first. Queued EOF, cancellation and errors cannot fabricate progress or successful completion.

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

- **Store:** Private files and redacted, zeroizing in-memory secret types; loaders reject unsafe paths.
- **Driver ownership:** Managers share an immutable driver registry. Each driver has one registration-aware login entrance and one renewal entrance; pending renewal still requires its separate identity verification and durable publication steps.
- **Automatic OAuth renewal:** Startup owns the deduplicated maintenance registry, including configured disabled accounts. Listener-owned renewal consumes that registry and scans tokens expiring within 120s, rotating credentials serially under account locks.
- **API-key access:** Startup binds a reference; borrowing enforces enabled state, quarantine, record/epoch/generation identity and material digest. No separate strict-binding policy is maintained.
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
- **Partial streams:** One sticky gap marker records rejected JSON/argument fragments while later independently sanitizable SSE units remain capturable. Repeated fragments do not consume a record each. Framing failure stops capture permanently; skipped units, aborted sources and truncated EOF retain `incomplete`, even when a later business event reports completion.

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
| Readable Chat reasoning | 2026-10-24; revised 2026-11-08 | Existing opt-in profile carries `reasoning_content` plus typed summary/protected `reasoning_details`; separate `mc1.` entries preserve native replay, while pure unsigned text retains its wire shape |
| Latest Messages baseline | 2026-10-24 | Current `2026-10-21` body grammar; revised 2026-10-30: `2023-06-01` admitted over it, removed sampling fields omitted, beta headers ignored |
| Canonical accounting | 2026-10-23 | Scoped modality breakdowns; single reasoning presentation owner |
| Five-stage order | 2026-10-20 | Stage 1 (Lossless Codecs), 2 (Agent Tools), 3 (Stability/Counting), 4 (Modalities/Decision/Files), 5 (Backlog) |
| Shared Aliyun dialect | 2026-10-19 | Shared `Bailian` dialect for DashScope and TokenPlan |
| Bailian Responses output formats | 2026-11-08 | Preserve the existing `text.format` carrier through preparation instead of rejecting its whole owner. Keep strict-function admission and origin-bound report fidelity independent; Provider content qualification remains scoped in [AR5](ar5-review.md#pending-observations). |
| Bailian Responses tool policy | 2026-11-08 | Carry the documented parallel control without inferring forced-call guarantees. Reject observed unsupported Qwen Flash/GLM thinking-mode forcing at the shared admission and native boundaries; retain strict rejection and Kimi's serial restriction. |
| Bailian GLM/DeepSeek bindings | 2026-11-07 | Register GLM Responses and DeepSeek Chat/Responses under the existing DashScope Provider. Reuse canonical `deepseek-flash` and public `deepseek-v4.1-flash-1017`, with the exact dated upstream name on Bailian only. Preserve native candidate order, isolate replay issuers, use Bailian low/high/max effort assessment and ceiling rules, and do not inherit native-beta strictness. Catalog selection does not activate a deployed route; current acceptance and unresolved SDK/decode boundaries belong to [AR5](ar5-review.md#binding-and-independent-rechecks-2026-11-07). |
| Dual-SDK Decision direction | 2026-10-23 | Shared Decision IR supporting TypeSafe and OpenAI Decision SDKs |
| Stage-4 equal priority | 2026-10-25 | Decision, Embedding, Audio, Images, Files/Uploads, and Video advance at equal priority |
| AR5 partial delivery | 2026-11-07 | Deliver usable principal workflows, recorded low-loss conversions and verified affected paths; explicitly defer unresolved boundaries and missing prerequisites to next-stage analysis instead of requiring every review observation to close. [Exit criteria](roadmap.md#partial-delivery-exit) |
| Independent Embedding | 2026-10-23 | Decoupled Embedding on `POST /v1/embeddings`; float/Base64 compatibility |
| Grok Imagine static scope | 2026-10-22 | Static image codec (`grok-imagine-image-2.0`); video deferred to async API |

### Semantic ownership decisions

| Decision | Date | Accepted Architectural Consequence |
|---|---|---|
| Standard request boundary | 2026-10-26 | Client intake enforces official protocol grammar; Provider dialects isolated to egress |
| Conversion ladder | 2026-10-30 | Same-name → same-semantic → partial lossy → omit → reject only when omission would misrepresent the response; unknown target facts delegate to upstream validation; one request-side and one delivery-side disposition owner |
| Candidate request dispositions | 2026-10-31 | One `Feature` vocabulary and target disposition owner replace separate admission/target feature gates, including presentation and parallel permission; conditional strict inference requires a known closed vocabulary, explicit strict keeps upstream delegation, and native assertions keep grammar and replay identity |
| Binding/API strict-tool ownership | 2026-11-07 | Remove blanket Bailian writer vetoes; shared lowering owns explicit/conditional admission across codec and direct entry points. Qwen Chat and native Luna carry strict, Bailian Responses declines it, and unassessed router Luna delegates with honest discovery |
| Delivery dispositions | 2026-11-01 | One adapter-owned event projection selects client facts; SSE renders events and JSON/retained views reduce them. Replaced snapshot checks and encoder terminal lowering are removed; execution aliases and wire-only closure preserve identity/lifecycle |
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
| Low-loss output ceilings | 2026-11-07 | Prefer equivalent controls, then useful narrower-scope limits, then omission with loss; unsupported ceilings alone do not require request rejection. Keep runtime bounds and reported terminals independent. [Contract](#output-ceiling-adaptation) |
| Go truncated tool-stream closure | 2026-11-07 | Carry trusted Messages dialect into intake; explicit max-token termination closes the observed unfinished object prefix as incomplete with exact bytes, identity and strict final EOF. Standard decoding and partial-tool non-execution remain protected. [Response rules](#response-rules) |
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
| Managed OAuth login admission | 2026-11-02 | Active sessions and pending logins reject another login before authority I/O. Publication reserves a verified subject across issued clients under the profile transaction, including signed-out accounts. Configuration validation includes disabled accounts; only explicit account removal releases the subject |
| OAuth registration removal | 2026-11-02 | Logout and recovery loading preserve removal tombstones. Re-registration creates a fresh immutable identity while retaining monotonic revision/generation counters; old bindings and stale revision-checked removal cannot revive or delete the replacement |
| Stable failover routing | 2026-11-02 | Ordered precompiled account targets, generation-fenced scoped health and cache-affine request-boundary selection; no balancing or implicit content-derived conversations |
| Provider accounts and service identity | 2026-11-02 | Versioned ordered named accounts replace stored pools across startup, auth/upstream CLIs and standalone tasks. API-key `openai` and SIWC `chatgpt` have separate catalog contracts. Stable names identify accounts; credential provenance owns replay/security identity. [Account model](provider-account-redesign-plan.md#account-and-configuration-model); remaining qualification is listed in [Roadmap](roadmap.md#remaining-agent-qualification) |
| Provider-owned credential kind | 2026-11-04 | Each Provider fixes exactly one credential kind and, for OAuth, one profile; separate access contracts use separate Provider IDs. Accounts reference managed instances by alias without repeating kind/profile. Shared binding, provisioning and diagnostics derive authentication from the catalog; obsolete fields reject before credential access. Endpoint header placement, credential lifecycles and store namespaces/formats remain distinct and unchanged. [Contract](provider-account-redesign-plan.md#credential-kind-and-header-placement); [remaining qualification](roadmap.md#remaining-agent-qualification) |
| DeepSeek acceptance publication | 2026-11-06 | Select the beta Chat binding explicitly for fresh-account acceptance. Publish optional label `deepseek-v4.1-flash-1017` over canonical/upstream `deepseek-flash`, with independent public routing/replay identity. No standard-endpoint redirection or upstream revision pinning is implied. [Configuration](usage.md#model-configuration) |
| Cache-first bounded recovery | 2026-11-02 | Target fixed after first dispatch; scoped health, success-only versioned affinity, atomic probes and a short bound-target wait are implemented. One serial pre-publication retry uses positive Provider/transport evidence within a 30 s recovery-to-content budget and unchanged request deadline. Merge health separately from delivery and emit only short credible `Retry-After`, without `x-should-retry`. Full contract/defaults: [execution policy](provider-account-redesign-plan.md#execution-policy); remaining gates: [Roadmap](roadmap.md#remaining-agent-qualification) |
| Account lifecycle and target trust | 2026-11-02 | Configured OAuth maintenance is independent of routing enablement; subject reservations survive logout until removal. Trusted API-key root overrides compose explicit operation suffixes; OAuth overrides reject. Durable verified replacement releases only old-material auth for an exclusive probe; stale 401s cannot penalize the replacement. [Targets](provider-account-redesign-plan.md#targets-and-trust); [maintenance](provider-account-redesign-plan.md#oauth-maintenance) |
| Scoped health and replay ownership | 2026-11-02 | Account/target constraints retain independent deadlines and atomic probe exclusion; validated failures survive pre-EOF cancellation without certifying completion. Bound codec replay uses namespace, credential provenance and exact target/operation identity, invalidating replaced keys while preserving ordinary OAuth rotation. A valid conversation binding alone earns one deadline-clipped cooldown wait |
| Breaking account-system cutover | 2026-11-02 | No old configuration/store-format/CLI compatibility, migration or stored-record reuse requirement. The operator provisions fresh keys and managed OAuth sessions; existing private stores are not automatically changed or deleted. Reuse secure mechanisms and official protocol contracts, not legacy pool execution. [Transition contract](provider-account-redesign-plan.md#configuration-transition-and-cli-output); [remaining qualification](roadmap.md#remaining-agent-qualification) |
| Application configuration ownership | 2026-11-07 | Maintain one current schema at version 1; the operator owns using it, with no compatibility or dedicated old-schema defenses/tests. Ordinary strict parsing and current safety checks remain. Preserve four private files; service policy is grouped into routing, OAuth, delivery and resources. Global implicit affinity belongs to service routing. Upload and post-admission execution keep separate deadlines. Expose only request/response budgets, upload deadline and bounded Responses history retention alongside existing policy; keep protocol, replay and internal safety ceilings code-owned. No history-disable feature. Validate disabled candidates, preserve value-free diagnostic context, and inspect compiled/effective settings through the existing loader. [Current fields](usage.md#configure-the-gateway) |
| Request conversion reuse | 2026-10-30 | [Bounded request-local retention](#request-conversion-reuse) preserves each body's bound identity and evidence; cache misses do not alter admission, and credential attachment stays attempt-local |
| Ingress Agent authentication | 2026-10-25 | Authenticates Agent keys at ingress, binding stable `agent_id` to context |
| Reverse-proxy delivery | 2026-10-26 | Proxy terminates TLS and compression; gateway enforces auth and unbuffered SSE |

### Delivery and evidence decisions

| Decision | Date | Accepted Architectural Consequence |
|---|---|---|
| Upstream protocol tests | 2026-10-23 | Pinned official SDK and upstream fixtures serve as regression baseline |
| Product-hypothesis test selection | 2026-11-04 | Aggressively prune implementation restatements and dependency-only experiments; retain independent product/protocol/safety evidence. Coverage and test counts are not progress or acceptance criteria. [Selection procedure](development.md#test-selection-and-pruning) |
| Runtime ownership convergence | 2026-11-04 | One projection owns delivery adaptations; one exchange ledger retains independent failure observations and terminal cleanup releases health before awaiting normalization. [Credential ownership](#credentials-and-security) uses one login/renewal driver contract and startup-owned maintenance registration |
| Delta coalescing | 2026-10-26 | Default 50ms / 16 KiB buffer merges adjacent text deltas |
| Messages ordered block delivery | 2026-10-28 | Codec-local bounded serialization preserves owner/part order, incremental head output and signed history; incomplete text-owner wire closure preserves source lifecycle, native intake and EOF authority |
| Semantic-equivalence compatibility | 2026-10-28 | Protocol-equivalent empty/default forms use ordinary lossless mapping without named omissions or loss receipts; covers null-only reports, empty logprobs/cache options and already-enabled permissions; source observations and meaningful constraints remain protected |
| Responses empty-annotations history compatibility | 2026-11-06 | Select missing-as-empty only for Client-history `output_text.annotations`; preserve explicit values, required identity/lifecycle fields and strict upstream snapshots. Encoding emits the official array without a loss/insertion receipt. [L2 contract](#semantic-equivalence-compatibility) |
| Responses replay snapshot authority | 2026-11-06 | Bind completed reasoning replay once at item-done; terminal snapshots verify field contracts without overwriting completed items or asserting ciphertext equivalence. Preserve replay presence, visible content, identities, lifecycle and EOF checks. [Reasoning replay](#reasoning-replay) |
| Responses queued opening and generated message identities | 2026-11-07 | Emit created/queued before actual progress without conflating Queued and Started; mint `msg_` only for new message aliases, preserving native IDs and target-local collision checks. [Stream normalization](#semantic-stream-normalization); [delivery identity](#delivery-projection-ownership) |
| Binding-qualified Bailian reasoning domain | 2026-11-07 | DeepSeek Responses retains reasoning content rather than Qwen's summary-event alias; exact item snapshot validation remains intact. [Dialect boundary](#protocol-baselines-and-provider-dialects) |
| Google ordered controls | 2026-11-07 | Extend source-timed configuration collapse and effective tool-set projection to existing generateContent/Interactions targets with the existing control/history/prefix losses; explicit strictness, seriality, selection and replay guards remain independent. [Tools](#ordered-tool-availability); [effort](#ordered-effort-repair-direction) |
| Google function-argument validation | 2026-11-09 | Select a schema-checked closed-object subset and request-wide native validation modes on existing APIs; conditional defaults use the same inventory check, unsupported explicit constraints and serial guarantees still reject. Non-strict reference schemas remain unchanged for upstream validation. Ordinary signatures use managed Responses, without stateless carrier fabrication. [Contract](#google-strict-function-arguments) |
| Unified tool semantics | 2026-11-04 | Expand ordered-tool root repair to all nine Chat/Responses/Messages directions, including request, return and replay. Accept bounded semantic approximation as the design direction; evaluate concrete collapse/alias/wrapper policies with explicit losses before activation. Source history and safety remain intact; strictness/selection/cardinality cannot silently weaken. New Google APIs, discovery, hosted execution and Client-history annotations remain outside scope. [Contract](#ordered-tool-availability); [design](api-expansion-design.md#three-protocol-tool-conversion-design); [execution](roadmap.md#conditional-stage-3-follow-ups) |
| Ordered definition batch ownership | 2026-11-04 | One typed batch owns definitions, role and position; codec fidelity owns the wire ID. Shared borrowed/mutable views include every declaration location without cloning or inventing schema precedence. Native Responses request/output/SSE/retained history share this owner; only developer batches can enter request history, and statusless batches require observed complete closure before stream finalization. [Contract](#ordered-tool-availability) |
| Bounded tool-set projection | 2026-11-05 | Delivered through one shared owner: effective active-set collapse, target-native retained additions (with fresh converted identities and legal-position/capacity checks) and allowed-set inventory filtering. Accept their explicit historical-definition/timing/cache losses, not weaker tool guarantees or guessed collision precedence. [Contract](#ordered-tool-availability); Native batches bypass effective-set construction; mixed histories use indexed lookup |
| DeepSeek strict binding qualification | 2026-11-05 | Delivered as an endpoint-scoped catalog binding: the model declares strict-tools, the standard binding's endpoint contract explicitly declines it, and a `deepseek-beta` provider chat path with an explicit models-file candidate/account activates it. pi 1.5.0's strict Chat read loop passes on it. Preserve the standard binding without implicit redirection; fresh-credential Provider acceptance remains AR5 and production activation remains separately approved |
| Forced-choice cardinality normalization | 2026-11-05 | Directional call-count meaning is carried on existing semantic carriers: OpenAI forced choices decode onto the serial carrier, Messages one-or-more choices lower to the exact required subset before target admission, and writers only render that carrier, and vacuous seriality under `none` is omitted. Targets without a serial carrier reject instead of weakening; no new semantic dimension was added |
| Ordered effort root repair | 2026-11-05 / 2026-11-06 | Explicit `FollowingResponse` / `NewInputTurn` boundaries and existing System envelopes selected after the temporal counterexample. One pure source resolver precedes target conversion; equivalent histories retain markers, other candidates record control/prefix losses and reject invalidated native thinking. Named Messages effort is admitted; unqualified null/reset and new controls remain excluded. The separately selected Google extension uses the same owner. [Contract](#ordered-effort-repair-direction); [remaining qualification](roadmap.md#remaining-agent-qualification) |
| Client/IR projection principles | 2026-10-28 | After semantic-equivalence handling, P1–P5 classify remaining projections: request-context deduction, carrier-less non-content omission, delivery closure, planning-time impossibility and conditional source defaults |
| Responses strictness resolution | 2026-10-28 | Omitted `strict` resolves per tool and target by the documented compatibility rule; revised 2026-10-30: unknown compatibility and grammar overflow resolve non-strict, explicit strict stays guarded |
| Ladder applications | 2026-10-30 | Nearest effort (ties upward), model-maximum clamp, thinking-off fallback for required Chat history, uncarried reasoning omission, Messages `end_turn`/`tool_use` derivations, `block_binding` levels; `stop` without carrier and unknown Chat/Messages stop causes remain rejections |
| IR/codec simplification | 2026-10-30 | One request feature vocabulary; delivery projection over the event stream with JSON as reduce-and-render; `Option` presence except contract-defined null meaning; receipts only for L3/L4/estimates; no strictness compatibility classifier. Upstream streaming-only intake and folding `FidelityRecords` identity into IR are selected subject to their roadmap experiments |
| Qualified upstream intake | 2026-11-01 | Responses, native Messages and Google use SSE-only execution intake, including JSON-client requests; complete JSON codecs remain directional tests. Chat retains R0's metadata/URL-annotation exception. [Original qualification evidence](refs/2026-10-27-client-ir-boundaries.md#production-corpus-and-intake-experiment-2026-10-30) |
| Representation identity probe | 2026-11-02 | Retain local owner/resource/part/group identities and codec-owned `FidelityRecords`: stable external IDs still need local remapping across independently decoded turns, and citations/replay dependencies must follow edits. Protocol aliases and source dependencies stay outside semantic IR. Remove the redundant input-text-form table; equivalent text forms need no receipt |
| Generation null contracts | 2026-11-02 | Optional reasoning intent/presentation, instructions, text/audio controls and cache/timestamp diagnostics normalize null defaults to `Option`; known non-nullable wire fields still reject null. Preserve empty observations, explicit zero/false and report-authoritative metadata/reasoning nulls. Messages nullable thinking display uses the model default without inventing hidden display |
| Shared Generation discovery | 2026-11-02 | One compiled contract feeds models and capabilities; publish default-request preflight and separate delivery adaptations. Prefer only the primary candidate's qualified native interface, otherwise `not_assessed`; expose uniformity without ranking or routing changes |
| Reasoning presentation and continuity | 2026-10-28 | `summary:"auto"` and encrypted `include` resolve against target availability; explicit summarizers and missing opaque carriers take named omissions |
| Carrier-less report omission | 2026-10-28 | Served service classes and refusal categories without carriers are omitted on delivery copies; no approximate service tier; empty logprobs use semantic-equivalence compatibility instead |
| Messages terminal derivation | 2026-10-28 | Completed call-free outcomes without admitted stop sequences derive `end_turn` from any source |
| Generalized visible-history transfer | 2026-10-28 | Revises scoped opaque replay: uncarried reasoning of any issuer or lifecycle is omitted; same-issuer incomplete reasoning forwards exactly to Responses |
| Binding reasoning-history contract | 2026-10-28 | One assessed binding fact (`unsupported`/`ignored`/`accepted`/`required_with_tools`) is the shared owner for projection, delivery, planning and discovery |
| Phased binding-history qualification | 2026-10-29 | Explicit unassessed declarations (revised 2026-10-30: delegate instead of pre-rejecting); established facts migrate without inferring Provider history semantics from replay switches or output |
| Standard Chat reasoning omission | 2026-10-28 | Standard omits unsigned readable reasoning on delivery copies; revised 2026-10-30: uncarried opaque reasoning is omitted too, never forged. Required-history tools use the readable profile or the selected thinking-off fallback; explicit history-read intent and inability to disable thinking retain rejection |
| Owner-local cache omission | 2026-10-28 | Revises owner-local cache admission: all routes admit breakpoints; non-native candidates omit them by named rule |
| Incomplete generated tool arguments | 2026-10-29 | Select byte-preserving OpenAI prefix-string projection with incomplete lifecycle/termination; no JSON repair or executable-input claim; native Messages object restrictions remain |
| Grouped tool-result association | 2026-10-29 | Associate results by call ID without reordering; remove unsupported adjacency assumptions on qualified consumers, retaining identities, causal order and replay guards |
| Three-protocol default-loop closure | 2026-10-29 | Stage 4 requires current-client default reasoning/tool/history success on all three client protocols; retained rejections and nondefault profiles do not substitute; preserve actual built-in model metadata |
| Messages baseline retained | 2026-10-28 | Superseded 2026-10-30 by the revised Messages version policy; discovery still publishes unsupported behavior |
| Agent-facing discovery | 2026-10-28 | Gateway-published preferred interface and adaptation rules from one compiled owner over the conformance corpus |
| Scoped source accounting | 2026-10-20 | Retains raw usage scopes independently without premature summation |
| Honest delivery | 2026-10-20 | Publication freezes failover; completion requires validated upstream EOF |
| Configurable capture | 2026-10-20 | Four opt-in capture boundaries with mandatory credential redaction |
