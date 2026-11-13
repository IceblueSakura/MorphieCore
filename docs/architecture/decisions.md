<a id="settled-decisions"></a>
# Settled Architectural Decisions

Authoritative log of accepted architectural decisions, trade-offs, and systemic consequences across MorphieCore subsystems.

See [Architecture Overview](index.md) for system boundaries and core IR semantics.

## Contents

  - [Scope and activation decisions](#scope-and-activation-decisions)
  - [Semantic ownership decisions](#semantic-ownership-decisions)
  - [Projection and reasoning decisions](#projection-and-reasoning-decisions)
  - [History and cache decisions](#history-and-cache-decisions)
  - [Routing and security decisions](#routing-and-security-decisions)
  - [Delivery and evidence decisions](#delivery-and-evidence-decisions)


Authoritative log of accepted architectural decisions and their systemic consequences:

### Scope and activation decisions

| Decision | Scope | Accepted Architectural Consequence |
|---|---|---|
| Model capability discovery | Discovery | Unified discovery contracts for Generation Text and Pi; other modalities schema-only. |
| SIWC GPT context planning | Discovery | Operator-selected 1,050,000-token total windows for Sol, Astra and Luna match the public OpenAI API. Observed SIWC directory defaults remain separate evidence; independent input/output declarations retain their service scope. [Capacity policy and sources](../refs/2026-10-26-model-discovery.md#capacity-sources) |
| Hosted MiniCPM thinking | Reasoning | All four ModelBest MiniCPM bindings carry explicit activation through the nested template Boolean, retaining standard effort and per-model readable-history spelling. Absent activation keeps the hosted default; no new budget, history-consumption or strict-tool guarantee is inferred. [Hosted Chat contract](adaptation-and-dialects.md#hosted-minicpm-chat) |
| Capability snapshot publication budget | Discovery | Enforces a 4 MiB capability-snapshot ceiling for complete multi-model publication intersected with `response_bytes`; overflows fail closed without payload truncation. Generation Text discovery retains an independent budget. [Publication contract](../usage/api-reference.md#capability-discovery-and-adaptation-notice) |
| Native model choices and pi capacity planning | Discovery | Models publishes native effort names in `values` and additional accepted names in `compatibility_values`, derived from actual candidate projection. Pi hides aliases, consumes declared context/output, and uses input capacity only when total context is absent while preserving explicit caps and native preference clamping. Active metadata reconciles during idle states. [Discovery contract](adaptation-and-dialects.md#generation-text-model-discovery) |
| DeepSeek parallel-tool admission | Tools | Rejects serial tool intent; omits parallel permission with evidence on DeepSeek targets. |
| Native DeepSeek Chat request subset | Tools | Rejects schema text output and thinking-mode forcing at target projection. Whole-inventory allowed sets use equivalent scalar selection; proper subsets reject without declaration loss. Native Responses remains independent. [Contract](adaptation-and-dialects.md#native-deepseek-chat) |
| Go model-scoped effort controls | Reasoning | HY4 and Muse preserve standard controls with service-native level domains and existing nearest-level adaptation; no provider-wide MiMo or output-guarantee expansion. [Contract](adaptation-and-dialects.md#opencode-go-controls) |
| Provider-local session projection | Cache affinity | SIWC carries a stable header derived from the effective cache key; Go retains independent conversation grouping and its existing hash domain; OpenRouter derives body sessions only from declared conversations. Shared typed namespaces and source-tagged hashing remain stateless, with closed Provider-specific carriers and no new downstream extension. [Contract](runtime-and-routing.md#upstream-session-projection) |
| Messages signed delivery | Delivery | Enforces native thinking signatures; unsigned thinking is omitted on delivery copies and never synthesized. |
| Failure diagnostics convergence | Observability | Native OpenTelemetry Logs, trusted attribution, and bounded opt-in body context replace local JSONL, spans, and diagnostic delivery counters. Coarse configuration errors suffice; external retention and production activation remain separate. [Observability](runtime-and-routing.md#observability) |
| Unified Generation evidence | Evidence | Standardizes on `EvidenceDelta` and `Conversion<T>` across generation pipelines, replacing separate audit logs. |
| Standards reset & redesign | Architecture | Pinned official standards and upstream test suites govern baseline behavior, with request and delivery handling directed by the conversion ladder. |
| Standard stream errors | Delivery | Standard Responses `error` events handle classified stream failures. |
| Native-only continuation | History | Genuine native replay carriers replace synthetic manifest reasoning. |
| Standard downstream APIs | Protocols | Chat, Responses and Messages use official semantic carriers only. Private Chat reasoning and Responses owner-replay profiles are retired; unrepresentable ordinary attachments have field-level reasons and explicit rejection. [Stateless boundary](history-and-replay.md#stateless-api-boundaries) |
| Latest Messages baseline | Codec | Standardizes on current official body grammar (2026-10-21 baseline, admitting 2023-06-01 over it); removed sampling fields are omitted and beta headers are ignored. |
| Canonical accounting | Accounting | Scoped modality breakdowns with a single reasoning presentation owner. |
| Five-stage order | Roadmap | Stage 1 (Lossless Codecs), Stage 2 (Agent Tools), Stage 3 (Stability/Counting), Stage 4 (Modalities/Decision/Files), and Stage 5 (Backlog). Stage 3 scope is closed with token counting deferred; conditional tool follow-ups do not gate Stage 4. |
| Stage-4 entry | Roadmap | Direct Stage-4 entry without prerequisite gates. Real-account failover and opt-in Agent session storage are scheduled independently without delaying Stage-4 progress. [Roadmap](../roadmap/unscheduled-issues.md#routing-qualification) |
| Shared Aliyun dialect | Dialect | Shared `Bailian` dialect for DashScope and TokenPlan. |
| Bailian verified-only function strictness | Tools | Both Aliyun plans default to `bailian.relax-function-strictness.v1`; only independently verified exact model/API pairs retain strictness (currently empty). Supplied schemas and source intent remain preserved while other tool guarantees and structured-text strictness remain separate. [Policy](adaptation-and-dialects.md#shared-aliyun-billing-dialect) |
| Bailian Responses output formats | Codec | Preserves the existing `text.format` carrier through preparation rather than rejecting the whole owner, keeping strict-function admission and origin-bound report fidelity independent. |
| Bailian Responses tool policy | Tools | Carries the documented parallel control without inferring forced-call guarantees. Rejects unsupported Qwen Flash/GLM thinking-mode forcing at shared admission and native boundaries; function strictness follows the verified-only service policy, and Kimi serial restrictions remain enforced. |
| Bailian GLM/DeepSeek bindings | Catalog | Registers GLM Responses and DeepSeek Chat/Responses under DashScope. Reuses canonical `deepseek-flash` and public `deepseek-v4.1-flash-1017` with exact upstream names on Bailian only, preserving native candidate order, isolated replay issuers, and Bailian effort assessment without inheriting native-beta strictness. |
| Dual-SDK Decision direction | Decision | Shared Decision IR supporting TypeSafe and OpenAI Decision SDKs. |
| Stage-4 equal priority | Roadmap | Decision, Embedding, Audio, Images, Files/Uploads, and Video advance at equal priority. |
| Partial delivery | Roadmap | Delivers usable principal workflows, recorded low-loss conversions, and verified affected paths, deferring unresolved boundaries to subsequent analysis rather than blocking progress on closed observations. [Exit criteria](../roadmap/index.md#partial-delivery-exit) |
| Independent Embedding | Embedding | Decoupled Embedding on `POST /v1/embeddings` supporting float/Base64 compatibility. |
| Grok Imagine static scope | Media | Static image codec (`grok-imagine-image-2.0`); video deferred to async API. |

### Semantic ownership decisions

| Decision | Scope | Accepted Architectural Consequence |
|---|---|---|
| Standard request boundary | Ingress | Client intake enforces official protocol grammar; Provider dialects remain isolated to egress. |
| Conversion ladder | Semantics | Ordered precedence: same-name carrier → same-semantic substitution → partial lossy substitution → omission → reject only when omission misrepresents the response. Unknown target capabilities delegate to upstream validation with dedicated request- and delivery-side disposition owners. |
| Runtime-owned execution strategy | Gateway | Reject known target limitations requiring strategy replacement and return an actionable standard error; do not automatically disable thinking or repair/retry history. This supersedes the earlier thinking-off fallback while retaining field/carrier adaptations. [Boundary](index.md#system-boundaries) |
| Candidate request dispositions | Request | A single `Feature` vocabulary and target disposition owner replace separate admission/target feature gates. Conditional strict inference requires a known closed vocabulary; explicit strict delegates upstream, and native assertions preserve grammar and replay identity. |
| Binding/API strict-tool ownership | Tools | Shared lowering owns explicit and conditional admission across codec and direct entry points. Native GPT bindings carry strict, unassessed router Luna delegates with honest discovery, and Bailian follows the verified-only service policy. |
| SIWC flat tool carriage | Tools | Candidate-local lowering moves flat function/custom definitions into a leading developer batch without renaming calls or changing schemas, strictness, selection or historical order. [Contract](adaptation-and-dialects.md#subscription-responses) |
| Delivery dispositions | Delivery | A single adapter-owned event projection selects client facts; SSE renders events, and JSON/retained views reduce them. Replaced snapshot checks and encoder terminal lowering are removed; execution aliases and wire-only closure preserve identity and lifecycle. |
| Client semantic boundary | Semantics | Unknown extension fields are discarded; known fields enter the conversion ladder rather than failing closed. |
| Three-protocol fidelity | Protocols | Semantic fidelity across Chat, Responses, and Messages with stream normalization. |
| Text item status omission | Delivery | Plain text incomplete items omit item status when overall finish reason is length. |
| Responses metadata echo | Responses | Echoes admitted request metadata only when upstream omits metadata entirely. |
| Image size policy | Media | `AllowDownsize` versus `RejectOversized` owned per image use. |
| Citation point anchors | Citations | TextPoint insertion boundary anchors in text parts. |
| Explanatory terminal reports | Delivery | Bounded refusal details and terminal explanations with named delivery omission. |
| Text configuration presence | Text | Preserves absent, null, and object states in reported text options. |
| Canonical instruction roles | Roles | Canonical Developer role unifies application instructions across protocols. |
| Participant ownership | Chat | Preserves instruction and message participant names for OpenAI Chat. |

### Projection and reasoning decisions

| Decision | Scope | Accepted Architectural Consequence |
|---|---|---|
| Independent activation & effort | Reasoning | Decouples boolean reasoning activation from scoped effort controls. |
| Model-effective reasoning controls | Reasoning | One target constraint owns always-thinking activation and native effort levels across API-key and subscription bindings; discovery reuses dispatch projection. [Contract](adaptation-and-dialects.md#reasoning-activation-and-effort-scope) |
| Strict downstream schemas | Schemas | Downstream schema completion fills missing fields via derivation or byte-based text estimates. |
| User attribution mapping | Attribution | Unifies `safety_identifier`, `metadata.user_id`, and `labels.safety_identifier`. |
| Phase transfer | Responses | Responses phase labels (`commentary`, `final_answer`) are omitted on Chat/Messages delivery copies. |
| Preset thinking quantization | Reasoning | 6-level discrete token budget quantization with upward tie-breaking. |
| Explicit effort precedence | Reasoning | Preserves explicit effort over conflicting numeric budgets, omits uncarriable budgets with named receipts, and quantizes only when qualitative effort is absent. |
| Thinking budget ceiling guard | Guard | Rejects requests where the thinking budget exceeds the client-specified maximum output token ceiling. |
| Bounded token-budget adaptation | Budget | Allows target-qualified review of gateway-derived defaults and presets to preserve functionality while keeping numeric thinking intent uncompromised. |
| Low-loss output ceilings | Ceilings | Prefers equivalent controls, then narrower-scope limits, then omission with loss receipts; unsupported ceilings alone do not force request rejection. Runtime bounds and reported terminals remain independent. [Contract](adaptation-and-dialects.md#output-ceiling-adaptation) |
| Go truncated tool-stream closure | Intake | Trusted Messages intake closes an open tool-input block at explicit max-token termination when received bytes form a valid unfinished object prefix, marking the call incomplete without executable completion. Standard decoding and partial-tool non-execution remain protected. [Response rules](adaptation-and-dialects.md#response-rules) |
| Messages missing ceiling default | Messages | Go/Haiku injects `32000` default output tokens when the client request omits `max_tokens`. |
| Tool-result error wrapping | Tools | Prefixes tool results with boolean status text for targets lacking native error flags. |
| Google terminal projection | Google | Gemini 3.8 Flash completions map to Messages `tool_use` or `end_turn` under request-grounded inference. |

### History and cache decisions

| Decision | Scope | Accepted Architectural Consequence |
|---|---|---|
| Append-only history projection | History | Earlier conversation turns project immutably as new turns append. |
| Scoped opaque replay | Replay | Envelopes opaque tokens (`mc1.`); strips incompatible foreign reasoning on transfer while preserving visible messages and tool links. |
| Cross-source reasoning omission | History | Cross-protocol or cross-Provider requests may discard reasoning text, summaries and incompatible protected history even for the same model name. Preserve visible history and tool links; no reasoning-to-answer conversion or universal provenance ledger is required. [Contract](history-and-replay.md#cross-protocol-and-cross-provider-history) |
| Same-source OpenAI reasoning | Replay | Bind native Chat/Responses reasoning to service provenance, not model labels; delegate model compatibility to upstream validation. Keep model-specific cache/route, compaction and other signature identities. No model-family table, previous-model state or automatic replay repair. [Contract](history-and-replay.md#cross-protocol-and-cross-provider-history) |
| Manual-thinking history boundary | Messages | Public Messages intake retains common trust and turn-order checks; native manual-thinking candidates enforce thinking prefixes after projection. |
| Prefix-cache ownership | Cache | `Part.cache` owns request-lifetime versus explicit TTL breakpoints. |
| Compacted context | History | Opaque compaction items are preserved in history; requests reject if the target lacks a compaction carrier. |
| In-memory stateful history | Responses | Responses managed continuation via process-local TTL history. |
| Agent session files | Sessions | Per-Agent JSONL directories for retained session persistence. |

### Routing and security decisions

| Decision | Scope | Accepted Architectural Consequence |
|---|---|---|
| Entry path safety | Security | Concrete unescaped ASCII paths; rejects dot segments and encoded separators. |
| Automatic OAuth renewal | OAuth | Background renewal under account locks for tokens expiring within 120s. Pending identity verification resumes automatically after network or timeout failures; other verification failures await explicit refresh. |
| Managed OAuth login admission | OAuth | Active sessions and pending logins reject another login before authority I/O. Publication reserves a verified subject across issued clients under the profile transaction, including signed-out accounts; only explicit account removal releases the subject. |
| Browser callback input | OAuth | Browser login accepts both HTTP callback and a full pasted redirect URL on stdin, with the first valid result winning. Plain echoed line input replaces hidden terminal mechanisms without input/callback overlap coordination. [Contract](runtime-and-routing.md#credentials-and-security) |
| OAuth registration removal | OAuth | Logout and recovery loading preserve removal tombstones. Re-registration creates a fresh immutable identity while retaining monotonic revision and generation counters; old bindings cannot delete replacements. |
| Stable failover routing | Routing | Prefer configured order, with advisory scoped cooldowns and bounded success-updated conversation preferences. No probe exclusivity, routing generations or concurrent affinity arbitration; authenticated identity and replay isolation remain strict. |
| Provider accounts and service identity | Accounts | Versioned ordered named accounts replace stored pools across startup, auth/upstream CLIs, and standalone tasks. Stable names identify accounts; credential provenance owns replay and security identity. [Account model](../provider-account-redesign-plan.md#account-and-configuration-model) |
| Provider-owned credential kind | Credentials | Each Provider fixes exactly one credential kind and, for OAuth, one profile. Accounts reference managed instances by alias without repeating kind/profile. Catalog drives shared binding, provisioning, and diagnostics; obsolete fields reject before credential access. [Contract](../provider-account-redesign-plan.md#credential-kind-and-header-placement) |
| DeepSeek public identity | Catalog | Publishes only `deepseek-v4.1-flash-1017`, retaining canonical/native upstream `deepseek-flash` and the existing binding order, credentials and replay issuers. The beta Chat binding remains an explicit strict-tool selection. [Configuration](../usage/gateway-configuration.md#model-configuration) |
| Cache-first bounded recovery | Recovery | Target remains fixed after first dispatch. A short preferred-target wait and at most one same-target pre-publication retry remain; cooldown expiry admits ordinary concurrent requests. [Execution policy](../provider-account-redesign-plan.md#execution-policy) |
| Account lifecycle and target trust | Trust | Configured OAuth maintenance operates independently of routing enablement; subject reservations survive logout until explicit removal. Trusted API-key root overrides compose explicit operation suffixes; OAuth overrides reject. [Targets](../provider-account-redesign-plan.md#targets-and-trust) |
| Scoped health and replay ownership | Health | Account and target cooldowns retain independent deadlines. Live constraints survive concurrent successes; verified OAuth replacement releases only old-material auth constraints. Replay retains namespace, credential provenance and exact target identity. |
| Breaking account-system cutover | Accounts | Clean cutover without backward-compatibility shims, migrations, or stored-record reuse. The operator provisions fresh keys and managed OAuth sessions using secure mechanisms and official protocol contracts. [Transition contract](../provider-account-redesign-plan.md#configuration-transition-and-cli-output) |
| Application configuration ownership | Config | Maintains a single current schema version with strict parsing and safety checks. Service policy is grouped into routing, OAuth, delivery, and resources. Budget, deadline, and bounded Responses history retention are exposed alongside existing policy, while protocol and internal safety limits remain code-owned. [Current fields](../usage/gateway-configuration.md#configure-the-gateway) |
| Request conversion reuse | Performance | Bounded request-local retention preserves each body's bound identity and evidence; cache misses do not alter admission, and credential attachment stays attempt-local. [Request conversion reuse](runtime-and-routing.md#request-conversion-reuse) |
| Ingress Agent authentication | Security | Authenticates Agent keys at ingress, binding stable `agent_id` to context. |
| Reverse-proxy delivery | Network | Loopback reverse-proxy terminates TLS and compression; gateway enforces authentication and unbuffered SSE. |

### Delivery and evidence decisions

| Decision | Scope | Accepted Architectural Consequence |
|---|---|---|
| Upstream protocol tests | Verification | Pinned official SDK and upstream fixtures serve as regression baseline. |
| Product-hypothesis test selection | Verification | Aggressively prunes implementation restatements and dependency experiments; retains independent product, protocol, and safety evidence. [Selection procedure](../development.md#test-selection-and-pruning) |
| Runtime ownership convergence | Runtime | One projection owns delivery adaptations; one exchange ledger retains independent failure observations, and terminal cleanup releases health before awaiting normalization. [Credential ownership](runtime-and-routing.md#credentials-and-security) uses a unified driver contract and startup-owned maintenance registration. |
| Final-delivery settlement | Delivery | Final acknowledgement wins over later consumer drop, shutdown, and deadlines only for the synchronous completion suffix. Retains eligible-success checks, live cooldowns, and pre-final cancellation. [Delivery contract](runtime-and-routing.md#streaming-and-delivery) |
| Delta coalescing | Streaming | Default 50ms / 16 KiB buffer merges adjacent text deltas. |
| Messages ordered block delivery | Messages | Codec-local bounded serialization preserves owner/part order, incremental head output, and signed history; incomplete text-owner wire closure preserves source lifecycle, native intake, and EOF authority. |
| Semantic-equivalence compatibility | Semantics | Protocol-equivalent empty/default forms use ordinary lossless mapping without named omissions or loss receipts, covering null-only reports, empty logprobs/cache options, and already-enabled permissions. |
| Responses empty-annotations history compatibility | Responses | Selects missing-as-empty only for Client-history `output_text.annotations`; preserves explicit values, required identity/lifecycle fields, and strict upstream snapshots. [L2 contract](adaptation-and-dialects.md#semantic-equivalence-compatibility) |
| Responses replay snapshot authority | Responses | Binds completed reasoning replay once at item-done; terminal snapshots verify field contracts without overwriting completed items or asserting ciphertext equivalence. [Reasoning replay](history-and-replay.md#reasoning-replay) |
| Responses queued opening and generated message identities | Responses | Emits created/queued before actual progress without conflating Queued and Started; mints `msg_` only for new message aliases, preserving native IDs and target-local collision checks. [Stream normalization](runtime-and-routing.md#semantic-stream-normalization); [delivery identity](adaptation-and-dialects.md#delivery-projection-ownership) |
| Binding-qualified Bailian reasoning domain | Bailian | DeepSeek Responses retains reasoning content rather than Qwen's summary-event alias; exact item snapshot validation remains intact. [Dialect boundary](adaptation-and-dialects.md#protocol-baselines-and-provider-dialects) |
| Google ordered controls | Google | Extends source-timed configuration collapse and effective tool-set projection to existing generateContent/Interactions targets with existing control/history/prefix losses; explicit strictness, seriality, selection, and replay guards remain independent. [Tools](index.md#ordered-tool-availability); [effort](adaptation-and-dialects.md#ordered-effort-repair-direction) |
| Google function-argument validation | Google | Selects closed objects, bounded nullable unions, and read-only root-definition references through native request-wide validation. Native none satisfies strict/serial intent without dropping declarations or applying conditional fallback; enabled calls retain unsupported-constraint and serial rejection. [Contract](adaptation-and-dialects.md#google-strict-function-arguments) |
| Google ordinary-signature boundary | Google | Reject standard stateless delivery of ordinary text/call signatures without a carrier. Explicit managed Responses may retain them; no private replay profile is offered. [Fields and reasons](history-and-replay.md#stateless-api-boundaries) |
| Unified tool semantics | Tools | Expands ordered-tool root repair to all nine Chat/Responses/Messages directions (request, return, replay) with bounded semantic approximation, preserving source history and safety invariants. [Contract](index.md#ordered-tool-availability); [design](../proposals/index.md#three-protocol-tool-conversion-design) |
| Ordered definition batch ownership | Tools | One typed batch owns definitions, role, and position; codec fidelity owns the wire ID. Shared borrowed/mutable views cover every declaration location without cloning or inventing schema precedence. [Contract](index.md#ordered-tool-availability) |
| Bounded tool-set projection | Tools | Delivered through one shared owner: effective active-set collapse, target-native retained additions with fresh converted identities, and allowed-set inventory filtering, accepting explicit historical losses without weakening tool guarantees. [Contract](index.md#ordered-tool-availability) |
| DeepSeek strict binding qualification | DeepSeek | Endpoint-scoped catalog binding: model declares strict-tools, standard binding explicitly declines, and `deepseek-beta` provider path activates it. Accepted for the selected strict Chat read loop. [Acceptance status](../development.md#overall-acceptance-status) |
| Forced-choice cardinality normalization | Tools | Directional call-count meaning is carried on existing semantic carriers: OpenAI forced choices decode onto the serial carrier, Messages one-or-more choices lower to exact required subsets, and targets without serial carriers reject. |
| Ordered effort root repair | Reasoning | Explicit `FollowingResponse` / `NewInputTurn` boundaries and System envelopes resolve source timing before target conversion; equivalent histories retain markers while non-equivalent candidates record control/prefix losses and reject invalidated thinking. [Contract](adaptation-and-dialects.md#ordered-effort-repair-direction) |
| Client/IR projection principles | Projection | P1–P5 classify remaining projections after semantic-equivalence handling: request-context deduction, carrier-less non-content omission, delivery closure, planning-time impossibility, and conditional source defaults. |
| Responses strictness resolution | Responses | Omitted `strict` resolves per tool and target by documented compatibility rules; unknown compatibility and grammar overflow resolve non-strict while explicit strict remains guarded. |
| Ladder applications | Semantics | Nearest effort (ties upward), model-maximum clamp, uncarried reasoning omission, Messages `end_turn`/`tool_use` derivations, and `block_binding` levels; carrier-less stop causes retain rejection. Strategy replacement follows the separate gateway/runtime boundary. |
| IR/codec simplification | Architecture | Single request feature vocabulary; delivery projection over the event stream with JSON as reduce-and-render; `Option` presence except contract-defined null meaning; receipts only for L3/L4/estimates. |
| Qualified upstream intake | Intake | Responses, native Messages, and Google use SSE-only execution intake, including JSON-client requests; complete JSON codecs remain directional tests. Chat retains R0's metadata/URL-annotation exception. [Intake evidence](../refs/2026-10-27-client-ir-boundaries.md#production-corpus-and-intake-experiment-2026-10-30) |
| Representation identity probe | Semantics | Local owner/resource/part/group identities and codec-owned `FidelityRecords` remap external IDs across independently decoded turns; citations and replay dependencies follow edits while protocol aliases remain outside semantic IR. |
| Generation null contracts | Semantics | Optional reasoning intent/presentation, instructions, text/audio controls, and cache/timestamp diagnostics normalize null defaults to `Option`; known non-nullable wire fields reject null. |
| Shared Generation discovery | Discovery | Single compiled contract feeds models and capabilities, publishing default-request preflight and separate delivery adaptations. Prefers only the primary candidate's qualified native interface, otherwise `not_assessed`. |
| Reasoning presentation and continuity | Reasoning | `summary:"auto"` and encrypted `include` resolve against target availability; explicit summarizers and missing opaque carriers take named omissions. |
| Carrier-less report omission | Delivery | Served service classes and refusal categories without carriers are omitted on delivery copies; empty logprobs use semantic-equivalence compatibility instead. |
| Messages terminal derivation | Messages | Completed call-free outcomes without admitted stop sequences derive `end_turn` from any source under request-grounded inference. |
| Generalized visible-history transfer | History | Uncarried reasoning of any issuer or lifecycle is omitted on delivery copies under named loss rules; same-issuer incomplete reasoning forwards exactly to Responses. |
| Binding reasoning-history contract | Reasoning | One assessed binding fact (`unsupported`/`ignored`/`accepted`/`required_with_tools`) serves as the shared owner for projection, delivery, planning, and discovery. |
| Phased binding-history qualification | History | Explicit unassessed declarations delegate to upstream validation rather than pre-rejecting; established facts migrate without inferring Provider history semantics from replay switches or output. |
| Standard Chat reasoning omission | Chat | Standard Chat omits unsigned readable reasoning and uncarried opaque reasoning on delivery copies without forging signatures. Thinking-enabled/default tools with required history reject before dispatch; runtime-selected thinking-off requests retain ordinary admission. |
| Owner-local cache omission | Cache | All routes admit owner-local breakpoints; non-native candidates omit them under named rules. |
| Incomplete generated tool arguments | Tools | Byte-preserving OpenAI prefix-string projection with incomplete lifecycle/termination; no JSON repair or executable-input claim; native Messages object restrictions remain enforced. |
| Grouped tool-result association | Tools | Associates results by call ID without reordering, removing unsupported adjacency assumptions on qualified consumers while preserving identities, causal order, and replay guards. |
| Three-protocol default-loop closure | Protocols | Preserve standard-client text/tool/history workflows within assessed target contracts. Required continuation state without a standard carrier has an explicit refusal boundary, not a private client adaptation. |
| Messages baseline policy | Messages | Pinned official 2026-10-21 baseline with 2023-06-01 compatibility; discovery publishes unsupported behavior without maintaining obsolete baseline branches. |
| Agent-facing discovery | Discovery | Gateway-published preferred interface and adaptation rules derived from one compiled owner over the conformance corpus. |
| Scoped source accounting | Accounting | Retains raw usage scopes independently without premature summation. |
| Honest delivery | Delivery | Publication freezes failover; completion requires validated upstream EOF. |
| Configurable capture | Observability | Four opt-in capture boundaries with mandatory credential redaction. |
