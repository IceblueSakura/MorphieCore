# Architecture Overview and Semantic Contracts

MorphieCore translates client and Provider protocols through a typed semantic IR. It functions as a gateway, not an agent runtime: planning, agent memory, and tool execution belong to the caller.

This page defines accepted principles, domain contracts, system boundaries, and semantic IR ownership. The architecture is modularized across specialized domain documents:
- [Protocol adaptation and Provider dialects](adaptation-and-dialects.md)
- [Runtime, routing, and trust](runtime-and-routing.md)
- [History and state replay](history-and-replay.md)
- [Settled architectural decisions](decisions.md)

[Roadmap](../roadmap/index.md) defines scope, priorities, and acceptance gates; [API expansion design](../proposals/index.md) tracks unselected proposals. Configuration and operational commands belong to [Usage](../usage/index.md); development conventions and overall acceptance status belong to [Development](../development.md).

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
    - [Ordered tool availability](#ordered-tool-availability)
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

## System boundaries

### Protocols

| Direction | Supported Protocols |
|---|---|
| Downstream | OpenAI Chat Completions, OpenAI Responses, Anthropic Messages |
| Upstream | OpenAI Responses/Chat and compatible dialects, Anthropic Messages, Google generateContent, Google Interactions |

Generation supports JSON and SSE delivery. Responses, native Messages and Google upstreams use streaming intake even for downstream JSON delivery. Chat retains the [qualified JSON exception](../refs/2026-10-27-client-ir-boundaries.md#production-corpus-and-intake-experiment-2026-10-30) for metadata and URL annotations without chunk carriers. Interactions uses stateless, full-history requests.

All three Generation client protocols dispatch through the public model's configured candidate route. A Messages client request does not require a native Anthropic upstream. Standalone tasks (Embeddings, Images, Speech, Transcription) operate on distinct contracts outside Generation.

- **Official Protocol Baselines:** Public Chat, Responses, and Messages adhere to current official versioned specifications. Provider-specific syntax and deviations belong strictly to trusted upstream adapters.
- **Diagnostic Extensions:** Optional namespaced response fields (`morphiecore`) provide safe-to-ignore telemetry and loss reporting. Standard SDK agent loops operate without parsing or returning these extensions.
- **Messages Version Policy:** Downstream Messages requires exactly one `anthropic-version` header, either `2023-06-01` or `2026-10-21` (including `?beta=true`), over the same current body grammar. Missing, duplicate, combined and unsupported versions reject before body collection. Legacy sampling fields (`temperature`, `top_p`, `top_k`) are validated then omitted under `messages.omit-removed-sampling.v1`; null defaults need no loss receipt. `anthropic-beta` is ignored as a header and never selects body semantics or trusted upstream headers.

Provider metadata resides in [providers.json](../../src/provider/providers.json). Public models and bindings are defined in [topology/catalog.rs](../../src/topology/catalog.rs) and assembled at [gateway startup](../../src/gateway/startup/bootstrap.rs).

### Shared Provider metadata

`src/provider/providers.json` owns default origins, operation paths, override suffixes and authentication. It is embedded at compile time via `include_bytes!`, validated once, and immutable at runtime. Private `accounts.json` declarations bind named credentials and API-key target overrides through `src/account.rs`, deriving credential kind/profile from the catalog under the [credential ownership contract](../provider-account-redesign-plan.md#credential-kind-and-header-placement); secret material and verified OAuth identities stay in the credential store.

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
- **Meaningful presence:** IR fields use `Option<T>` for optional/default values; intake normalizes documented null defaults (L2) and encoders emit the target grammar (required-nullable fields as `null`, optional fields omitted). Empty strings, zero counts, explicit false and observed empty containers remain values. A tri-state remains only for meaningful reported absence or an explicit clear. Reject-only IR placeholders are removed; malformed known wire values reject at intake. The [field review](../refs/2026-10-27-client-ir-boundaries.md#generation-null-contract-review-2026-11-02) records the scoped source evidence.
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

Assess the complete Chat/Responses/Messages tool loop in all nine client-to-upstream directions. Preserve ordinary declarations, including Responses `additional_tools`, at their conversation positions in source IR. Prefer native/equivalent carriage, then useful bounded semantic approximation rather than rejection solely because exact carriage is unavailable. Lossy conversion is applied under explicit named dispositions and selection before activation.

This supersedes earlier restrictions to native mappings and proven initial-prefix lifting. General active-set projection, historical declaration retention with current selection restrictions, reversible namespace aliases and custom-text wrappers are evaluated explicitly as alternatives, not presumed equivalent or already implemented. Tool-removal prose remains instruction content, not a structural withdrawal. Explicit strictness, allowed calls, call cardinality and seriality must be interpreted by their source meaning; accepting history loss does not silently waive them.

After native/equivalent carriage, the bounded tool-set projection combination is applied:

| Named L3 disposition | Contract and acknowledged loss |
|---|---|
| Effective active-set projection | Preserve the current callable definitions and historical call/result content; lose old declaration visibility, insertion timing and rendered-prefix/cache stability |
| Retained-history/current-set projection | Prefer target-native historical declarations plus exact current-call restriction when enforceable; acknowledge lost withdrawal intervals or replacement semantics. Otherwise use the selected active-set projection only when the source set is determinate |
| Allowed-set inventory filtering | Where no allowed-subset carrier exists, filter root and inline declarations together, preserving allowed identities and optional/required invocation; lose filtered-definition visibility and potentially prefix-cache reuse |

Project candidate copies only. Never expose never-activated/withdrawn tools, infer structural withdrawal from prose, guess an unknown collision winner or fabricate an old schema. Strictness, selection, call count, seriality and dispatch remain independent guarantees; this tool-set combination selects no downgrade. Bailian function strictness has its separately selected [service policy](adaptation-and-dialects.md#shared-aliyun-billing-dialect). Namespace aliases, custom-text wrappers, examples, result-media relocation, prompt annotations and non-developer returned-addition replay are not selected by this combination.

Existing generateContent and Interactions targets use the same effective active-set projection when ordered declarations have no native carrier. Historical call/result content and identities survive; old declaration visibility, insertion timing and prefix/cache stability take the existing `generation.collapse-tool-set.v1` loss. This adds no Google API, hosted execution, strict guarantee or serial carrier.

Repair the shared tool contract rather than adding a decoder-only exception. Initial definitions, namespace leaves and ordered additions must receive consistent validation, selection resolution, feature accounting, bounds and target checks. Reuse existing semantic tools, request/delivery projection, fidelity and codecs; no gateway tool registry, executor or nine pair-specific adapters. [Unselected conversion designs](../proposals/index.md#three-protocol-tool-conversion-design) owns proposed mappings; [dated evidence](../refs/2026-10-27-client-ir-boundaries.md#nine-direction-tool-assessment-2026-11-04) owns findings; [Roadmap](../roadmap/unscheduled-issues.md#conditional-stage-3-follow-ups) owns execution gates. Client-history annotations use the separate [L2 contract](adaptation-and-dialects.md#semantic-equivalence-compatibility); discovery and hosted execution remain outside this work.

Current shared validation separates intrinsic choice shape/count/uniqueness in settings from request-level membership in ordered availability. Deferred definitions stay withheld until surfaced; removal retains the identity for later re-addition. Current available identities have their own finite bound, independent of retained historical versions. Historical calls keep their original identity/arguments and are not validated against today's schema. Messages serial intent is emitted even for inline-only definitions, and candidate collapse updates history and settings atomically before validation.

Forced-choice cardinality is directional, not label-only. An OpenAI forced specific/custom choice means exactly one call and decodes onto the serial carrier (`parallel_tool_calls:false`); a Messages named or `any` choice without `disable_parallel_tool_use` means one-or-more and encodes to OpenAI targets as the exact required subset rather than a strengthened forced spelling; `none` makes the serial flag vacuous and it is omitted next to that selection. Targets without any serial carrier (Google paths) reject a forced choice instead of silently weakening it.

Named one-or-more choices become required subsets in candidate-local lowering (`openai.named-tool-subset.v1`, L2), before target admission. Writers only render the selected carrier. A dialect without that carrier rejects before dispatch; it cannot bypass admission by introducing `allowed_tools` during serialization.

The DeepSeek beta strict binding is endpoint-qualified: the model declares strict-tools capability, the standard binding's endpoint contract explicitly declines it, and a `deepseek-beta` provider chat path plus an explicit models-file candidate and account activate validation. Nothing redirects the standard endpoint, and catalog presence alone activates nothing.

Its default path is `/beta/chat/completions`; an account's complete API root receives only the operation-relative `/chat/completions` suffix. No beta-segment inference, duplicate removal or alternate-path retry applies.

One shared owner projects ordered tool declarations onto each target's selected carrier: Chat targets take the effective active set, Responses targets keep developer batches and convert pure by-value additions, and Messages targets with native changes lift a leading batch prefix, convert legal-position additions to ordered blocks, or fall back to the active set when no legal system placement exists. Allowed-set inventory filtering applies the flat restriction and required invocation on Messages targets. Bailian Responses also uses this selected loss when forced selection names one function but the inventory contains several: collapse root/inline definitions to the effective set, filter to the selected original function, and preserve the required-subset carrier and independent serial control. Bare multi-function `required`, explicit strictness and unsupported thinking profiles keep their own admission boundaries. Batch redeclarations without a documented winner block every projected fallback while native forwarding keeps them; converted items take fresh identities so surviving item ids keep their kind; historical call/result content and raw arguments are never rewritten. Responses source envelopes are validated and projected away before tool-item identity replacement, so converted batches cannot leave stale container members. Source-contiguity, participant attribution and replay checks remain enforced.

Retained projections must fit root declaration capacity (namespace containers and leaves included) and the final item count. A known carrier-capacity failure selects the existing active-set collapse only when the source set is determinate; malformed source values, unknown references and ambiguous winners remain errors. Final semantic and target validation still apply. Native Responses batch-only forwarding needs no effective inventory; other walks index identities while preserving declaration order, rather than scanning all historical definitions per member.

`Item::ToolBatch` owns one ordered definition list and a typed role observation; `FidelityRecords` retains its one wire ID. Empty batches and function/custom/namespace members keep their positions. This is distinct from Messages' singleton replacement/reference/removal operation and selects no same-name schema winner. One borrowed declaration view and one mutable function visitor cover roots, namespace leaves, singleton additions and batches for validation, feature/strictness checks and candidate-only omissions.

Responses input accepts only developer batches. Native output preserves every declared role without creating an instruction; manual and managed replay reject non-developer batches. Added snapshots may be empty; the bounded `ToolBatchSnapshot` event installs the final definitions before observed item completion. Batches have no partial wire status, so an incomplete overall response cannot finalize an unclosed batch. JSON, SSE and retained snapshots use this same owner and custom-tool capability checks apply to both opening and final definitions. [Source qualification](../refs/2026-10-27-client-ir-boundaries.md#tool-contract-qualification-2026-11-04) and [SDK procedure](../development.md#sdk-loopback-gate) describe the independent reference and connected check.

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

