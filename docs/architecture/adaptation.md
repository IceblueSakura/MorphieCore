# Protocol Adaptation

This document defines cross-provider wire-to-IR and IR-to-wire adaptation rules: intake classification, the conversion ladder, candidate-local projection, output-ceiling and reasoning calibration, media handling, adaptation evidence, and model capability discovery.

Provider-specific wire dialects belong in [Provider dialects](provider-dialects.md). See [Architecture Overview](index.md) for system boundaries and core IR semantics, [Runtime and routing](runtime-and-routing.md) for execution and failover, [History and replay](history-and-replay.md) for multi-turn state, and [Settled decisions](decisions.md) for accepted trade-offs.

## Contents

- [Source and target boundaries](#source-and-target-boundaries)
  - [Protocol baselines and Provider dialects](#protocol-baselines-and-provider-dialects)
  - [Standard request boundary](#standard-request-boundary)
  - [Field handling](#field-handling)
  - [Client Responses history shorthand](#client-responses-history-shorthand)
  - [Candidate-local projection](#candidate-local-projection)
  - [Model-effective values](#model-effective-values)
- [Request and reported meaning](#request-and-reported-meaning)
  - [Conversion ladder](#conversion-ladder)
  - [Output-ceiling adaptation](#output-ceiling-adaptation)
  - [Semantic-equivalence compatibility](#semantic-equivalence-compatibility)
  - [Client/IR projection principles](#clientir-projection-principles)
  - [Delivery projection ownership](#delivery-projection-ownership)
  - [Generated tool arguments and execution input](#generated-tool-arguments-and-execution-input)
  - [Tool-result association](#tool-result-association)
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
  - [Ordered effort projection](#ordered-effort-projection)
  - [Reasoning level calibration](#reasoning-level-calibration)
  - [Thinking-budget quantization](#thinking-budget-quantization)
  - [Bounded token-budget adaptation](#bounded-token-budget-adaptation)
  - [Binding-level mapping](#binding-level-mapping)
  - [Implementation owners](#implementation-owners)
- [Media](#media)
  - [Native static images](#native-static-images)
- [Adaptation evidence](#adaptation-evidence)
  - [Domain evidence interface](#domain-evidence-interface)
  - [Atomic evidence and output batches](#atomic-evidence-and-output-batches)
  - [Typed conversion failures](#typed-conversion-failures)
  - [Public capability and adaptation notice](#public-capability-and-adaptation-notice)
  - [Generation Text model discovery](#generation-text-model-discovery)

## Source and target boundaries

### Protocol baselines and Provider dialects

```text
Decode: Wire → Dialect Normalization → Protocol Baseline + Declared Extensions → IR + Fidelity
Encode: IR + Eligible Fidelity → Target Baseline + Declared Extensions → Dialect Encoding → Wire
```

- **Baseline:** Standard official specification at a pinned release.
- **Dialect:** Target-specific wire normalization operated purely in memory without network I/O; see [Provider dialects](provider-dialects.md) for per-Provider rules.

### Standard request boundary

Four decoupled responsibilities isolate public protocols from vendor wire quirks:
1. **Client intake:** Enforces strict official protocol syntax; ignores unknown extension fields and SDK helper properties.
2. **Semantic IR:** Holds normalized cross-protocol meaning.
3. **Target projection & encoding:** Validates binding constraints and formats native target requests on candidate-local copies.
4. **Provider egress dialect:** Translates standard target requests to vendor wire shapes where required.

Module layout mirrors `vendor / api / responsibility`: `protocol::openai::{chat, responses}`, `protocol::anthropic::messages`, and `protocol::google::interactions`.

### Field handling

Known fields follow the [conversion ladder](#conversion-ladder); this table states how intake classes enter it.

| Classification | Gateway Action |
|---|---|
| **Content and explicit guarantees** | Instructions, messages, media, tools, arguments, call links, output constraints and explicit strictness: same-name or same-semantic carriers, or a user-selected approximation (such as tool-error wrapping or visible-history transfer); otherwise reject |
| **Controls and hints** | Sampling, effort, presentation, cache and service hints: walk the ladder through bounded approximation and named omission on candidate copies |
| **Reports and metadata** | Delivered report facts without a client carrier take named omission; they never fail delivery |
| **Unscheduled capabilities** | Stateful, hosted-execution and other unscheduled capabilities retain their [rejected boundaries](../roadmap/unscheduled-issues.md#retained-rejections) |
| **Legacy standard carrier** | Convert unambiguous historical fields |
| **Unknown extension field** | Accept and discard at valid object positions; do not forward or store |
| **Malformed known value** | Grammar error; outside the ladder and rejected at intake |

### Client Responses history shorthand

- Assistant text shorthand strings are accepted at intake.
- Output items in history require standard fields (`type`, `id`, `role`, `content`, `status`). Assistant `output_text` may omit its empty `annotations` array under [semantic-equivalence compatibility](#semantic-equivalence-compatibility); no other required fields are inferred.

### Candidate-local projection

Transformations are strictly candidate-private over an immutable source IR:
- A dispatched retry reuses the same prepared target and request.
- Native or equivalent history projections preserve earlier turns when new turns append; named lossy projections (such as tool-history collapse) record rendered-prefix/cache loss rather than promising prefix stability, and never mutate source history or alter routing affinity.

### Model-effective values

Target bindings enforce bounded ceilings via `ModelLimits` (context windows, maximum input/output tokens, allowed reasoning efforts, and mandatory-thinking guards). Candidate projection clamps output ceilings above a known model maximum under a named rule while keeping honest `length` terminals, and resolves unsupported effort levels to the nearest supported tier. Final validation checks the projected request; unresolvable combinations reject before dispatch.

## Request and reported meaning

### Conversion ladder

Every known field, control, report and history owner crossing a protocol or target boundary takes the first applicable level:

| Level | Name | Rule | Examples |
|---|---|---|---|
| L1 | Same-name | The target carries the same field with the same meaning | Chat `stop` to Chat; Responses `reasoning.effort` to Responses |
| L2 | Same-semantic | A different carrier or shape with identical meaning; no loss receipt ([semantic equivalence](#semantic-equivalence-compatibility)) | Documented null/empty defaults; parameterless function as `{}`; `stop` beside calls as `tool_use` |
| L3 | Partial lossy | A bounded approximation preserving main intent under a named rule with evidence | Nearest supported effort; conditional strictness as non-strict; uncarried reasoning omitted while visible history stays |
| L4 | Omit | No expressible target meaning, and dropping it cannot make the response appear to satisfy something it did not; named omission | Sampling and verbosity controls, cache hints, metadata and service hints, carrier-less reports |
| L5 | Reject | Inexpressible, and omission would let the client mistake the response for satisfying its request | Media the model must see, `n > 1`, tools or explicit output constraints without a carrier, explicit strict guarantees |

- **Burden of proof & delegation:** Rejection requires an L5 reason; admission requires no prior equivalence evidence. When a target fact is unassessed (such as schema compatibility or unassessed history behavior), the gateway sends the carriable form and delegates validation upstream—never delegating trust, replay origin, or credentials.
- **Execution strategy:** The ladder adapts fields and carriers, never runtime strategy; a restriction requiring automatic thinking disablement or session repair rejects pre-dispatch with an actionable error.
- **Low-loss priority & timing:** Field spelling differences, incomplete assessment, and missing optional controls are not independent rejection reasons; prefer L2 semantic matching, then L3 approximation, then L4 omission (existing L2–L4 patterns need no per-rule confirmation, while content/explicit-guarantee downgrades require concrete selection). Request levels resolve pre-dispatch on candidate copies; post-dispatch delivery facts bottom out at L4 except where content or outcome would be misrepresented.
- **Invariants outside the ladder:** Credential/origin/target trust, replay signature/origin validation, resource and time bounds, strict JSON and malformed-known-value checks, terminal honesty and EOF, call identity, non-executable incomplete tool input, explicit history-read intent, published accounting, and candidate isolation.
- **Unified ownership:** [`morphiecore-lowering`](../../crates/morphiecore-lowering/src/request.rs) applies target dispositions to candidate copies; [`morphiecore-protocol`](../../crates/morphiecore-protocol/src/lowering/request.rs) validates source trust, history, and final carrier grammar; [delivery projection](#delivery-projection-ownership) and [discovery](#generation-text-model-discovery) consume the same typed `Feature` disposition vocabulary (rule IDs live in [`Rule`](../../crates/morphiecore-lowering/src/disposition.rs), emitting receipts only for L3, L4, and schema estimates).

Key ladder dispositions:
- **Conditional vs. explicit strict tools (P5):** `openai.omit-conditional-tool-strictness.v1` (L3) keeps already-closed schemas strict on strict-capable targets and resolves unclosed or overflowed conditional definitions non-strict; explicit `strict:true` stays L5 unless a target policy selects relaxation (e.g. Bailian).
- **Effort & numeric budgets:** Effort maps across Reasoning/Response scopes and resolves missing levels to the nearest supported tier, ties upward (L3). Explicit effort takes precedence over conflicting numeric budgets, omitting the budget under a named rule; when effort is absent on adaptive-only targets, manual budgets quantize to adaptive effort.
- **Messages intake guards:** Adaptive `thinking.block_binding` normalizes `null`/`{}` (L2), omits `drop_block` under `messages.omit-block-binding.v1` (L3), and rejects `"error"` (L5); legacy Messages sampling values (`temperature`/`top_p` in `[0,1]`, integer `top_k`) validate then omit under `messages.omit-removed-sampling.v1` (L4).
- **Rule families:** Advisory/prediction controls omit with receipts when uncarried; `openai.wrap-tool-result-errors.v1` prefixes tool errors where targets lack native boolean flags; Chat JSON grouping concatenates contiguous plain assistant text before calls without reordering across calls; terminal stops coarsen to `stop`/`end_turn`/`tool_use`; `chat.omit-text-item-status.v1` omits incomplete text item status on `length` stops.

### Output-ceiling adaptation

Client output ceilings prefer retaining useful control over rejection when exact scope matches are unavailable:

| Target carrier | Disposition |
|---|---|
| Same accounting scope under another field name | L2: preserve value with target spelling |
| Equivalent ceiling above a known model maximum | L3: clamp to model maximum |
| Reply-only ceiling for a total-output request | L3: retain numeric value as reply-only ceiling with `generation.reply-only-output-ceiling.v1` (never claiming thinking tokens are bounded); omit under L4 if below API minimum |
| No useful ceiling carrier or expressible value | L4: omit without inventing tokenizer limits or raising the requested cap |
| Unassessed model ceiling behavior | Delegate to upstream validation |

Defaults are injected only when the original client request omitted a ceiling, never overwriting an omitted or clamped ceiling; byte/time bounds and honest `length` terminals remain independent.

### Semantic-equivalence compatibility

Ladder level L2 covers lossless mapping of protocol-defined empty values, defaults, and structural placeholders—preserving content, controls, identity, report facts, and transitions without loss receipts:
- **Delivery/report defaults:** Reported `metadata:null` (Chat SSE / Messages), empty `logprobs:[]` / `logprobs:false`, null system fingerprints, omitted/false document `citations.enabled`, empty cache option objects, and explicit parallel permission on always-parallel targets normalize losslessly. Nonempty values, meaningful reports, and empty state-clearing updates remain distinct.
- **Request/history defaults:** Chat `logit_bias:null` equates to omitted bias (`logit_bias` non-null still rejects); parameterless function declarations emit empty `{}` schemas where required; Client Responses history accepts omitted `output_text.annotations` as `[]` while upstream snapshots and target encoders keep the required array.

### Client/IR projection principles

After L2 semantic equivalence, remaining projections classify under P1–P5:

| Principle | Rule |
|---|---|
| P1 Request-context deduction | Facts implied by the admitted request and source grammar derive as `infer_from_request` without mutating source observations |
| P2 Carrier-less non-content facts | Advisory controls, presentation hints, and report facts without a carrier take named omission; visible content and constraints never do |
| P3 Delivery closure | Any delivered response on a route is admissible as history on that route, verbatim or via named projection |
| P4 Planning-time impossibility | Failures determined by request + binding contract fail pre-dispatch |
| P5 Conditional source defaults | Documented conditional omitted defaults resolve per target rather than via fixed constants |

Selected applications:
- **Pi optional strict sampling:** Pi's MorphieCore extension resolves optional JSON-schema `prefer` before native serialization; explicit `require` and wire `strict:true` retain standard gateway enforcement.
- **Reasoning presentation & continuity:** `summary:"auto"` picks the target's native default summarizer (no receipt); explicit `concise`/`detailed` without exact granularity take `generation.omit-reasoning-summary.v1`. Encrypted `include` omits via `generation.omit-encrypted-reasoning-include.v1` only when reasoning lacks an opaque carrier (explicit thinking-off needs no receipt).
- **Carrier-less reports:** Uncarried service classes and refusal categories omit on delivery copies without substituting approximate tiers; Messages also omits echoed OpenAI execution hints (`metadata`, `background`, `prompt_cache_key`, `store`).

### Delivery projection ownership

[`adapter::downstream`](../../crates/morphiecore-protocol/src/adapter/downstream.rs) applies typed delivery dispositions before rendering: SSE projects incremental events, and JSON/transient snapshots reduce the same stream.
- Execution-assigned OpenAI message aliases (`msg_`) merge prior to JSON/snapshot materialization and validate target-local ID uniqueness.
- Replay dependencies refresh on affected item boundaries and at terminal before validation.
- For Chat clients, opening service reports omit via `chat.omit-opening-service-report.v1`.
- For Messages clients, refusals project as visible text (`messages.refusal-as-text.v1`), and non-conforming upstream call IDs deterministically project to `mc_<sha256_prefix_40>` (`messages.project-call-ids.v1`) identically across request and delivery so tool links stay consistent.
- Unknown stop/incomplete labels reject except where an evidence-backed dialect map exists or Responses omits unrepresented incomplete reasons (`responses.omit-unrepresented-incomplete-reason.v1`).

### Generated tool arguments and execution input

Unfinished tool-call argument prefixes project verbatim as OpenAI argument strings without JSON repair or executable-invocation claims, keeping incomplete lifecycles and `length` terminals intact. Downstream JSON can aggregate always-streaming upstreams after validated EOF. Native Messages complete JSON/history still requires a valid input object.

### Tool-result association

Tool results link to calls strictly by call ID, never vector adjacency; grouped calls followed by corresponding results pass without reordering or flattening. Sibling targets without adjacency constraints (such as Bailian Responses) avoid predecessor-position restrictions while retaining strict identity, causal-order, and duplicate checks.

### Downstream schema completion

Missing downstream schema fields resolve as **Upstream facts → Justified inference → Simple estimates**:
- `responses.infer-required-settings.v1`: fills missing `tools`, `tool_choice`, and `parallel_tool_calls` from admitted request settings.
- `responses.echo-request-metadata.v1`: echoes request `metadata` only when upstream omits `metadata` entirely (explicit upstream `null` wins; maps never merge).
- `generation.complete-delivery-usage.v1`: completes missing required usage counters via byte estimates (`ceil(bytes / 4)`).

### Reported text configuration

Preserves absent, `null`, and object states in `TextOptions` without defaulting reported `null` formats to plain text; request-side `null` text configs still reject at intake.

### Message phase transfer

Responses preserves `commentary` and `final_answer` labels; targets without phase carriers omit phase via `generation.omit-delivery-message-phase.v1` while keeping text and tool links.

### Explanatory terminal reports

Anthropic `stop_details` maps to `TerminalDetails.refusal`; OpenAI delivery omits carrier-less refusal categories/details via `openai.omit-explanatory-refusal-details.v1` and omits other carrier-less terminal explanations without changing the underlying stop cause or turning refusals into content.

### Service policies and reports

Requested service policies and served classes remain distinct (Anthropic `standard`/`priority` vs. OpenAI `default`/`flex`/`priority`). Unrepresentable served classes omit via `openai.omit-unrepresentable-service-class.v1` rather than approximating tiers; conflicting same-phase updates fail stream validation.

### Lifecycle settings and service reports

Responses and Interactions validate opening vs. terminal reports via [`protocol::reports`](../../crates/morphiecore-protocol/src/reports.rs). Cross-phase transitions permit evidenced resolutions (`summary: auto → detailed`, `service_tier: auto → default`, `configured → standard`) while rejecting conflicting drifts.

### End-user attribution

Opaque attribution maps across OpenAI `safety_identifier` (preceding legacy `user`), Anthropic `metadata.user_id`, and Google `labels.safety_identifier` via [`protocol::attribution`](../../crates/morphiecore-protocol/src/attribution.rs). It is non-authoritative metadata, never alters cache or auth identity, and omits unsupported Google label values without truncation or hashing.

### Selected request approximations

- `openai.wrap-tool-result-errors.v1`: prefixes tool results with `[tool result is_error=...]` when the target has no boolean error flag.
- Terminal approximations coarsen natural/stop-sequence causes to `stop`, `end_turn`, or `tool_use`.

### Messages terminal projection

Completed call-free outcomes from requests without `stop_sequences` derive `end_turn` (`messages.infer-end-turn-from-request.v1`) across all sources without altering source observations. Completed call-free outcomes with declared stops on non-Messages targets map to `end_turn` under `messages.ambiguous-stop-as-end-turn.v1` (L3); Chat `stop` accompanied by calls maps to `tool_use` (L2). Continuation, incomplete, failed, cancelled, and unbound responses remain unchanged.

## Reasoning controls

### Reasoning activation and effort scope

Semantic IR decouples boolean activation (`ReasoningRequest`) from scoped effort (`EffortControl { scope, level }`, [`EffortScope::Reasoning` vs. `EffortScope::Response`](../../crates/morphiecore-semantic/src/task/generation/content/reasoning.rs)). Explicit `disabled` reasoning takes precedence over response-wide effort.

Target `ModelLimits` independently declare mandatory-thinking constraints (`reject_disabled_reasoning`) and native positive effort levels; discovery and dispatch share these exact constraints.

### Ordered effort projection

Root settings establish initial controls; `ConfigurationUpdate.boundary` carries `FollowingResponse` or `NewInputTurn`. Responses updates select following responses; Messages system updates immediately after a new user input turn select its reply (tool-result-only turns leave updates pending until new user input).
- `GenerationRequest::resolved_configuration` derives active and pending snapshots in one pure scan: disabling clears earlier effort allocations, while response-scoped effort stays independent of reasoning activation.
- Equivalent native targets preserve historical markers; non-native targets resolve source timing before converting scope/level/budget, recording `generation.collapse-configuration.v1` and `generation.configuration-prefix-change.v1` without mutating source IR or forging placeholders.
- Marker changes earlier than protected thinking/redacted blocks reject invalidated replay (root-effort edits alone preserve valid thinking); null configuration updates and directly adjacent Responses updates reject.

### Reasoning level calibration

Hand-authored binding tables resolve missing positive tiers to the nearest supported level, ties upward, and carry effort across Reasoning/Response scopes at the same level. Ignored sampling controls omit under target disposition rules rather than binding generic codecs.

### Thinking-budget quantization

Numeric token budgets quantize against 6 canonical tiers (`minimal` to `max`) in [`ThinkingBudgets`](../../crates/morphiecore-lowering/src/reasoning.rs) by minimum absolute distance, ties upward. Requests whose thinking budget exceeds their explicit output ceiling reject pre-dispatch.

### Bounded token-budget adaptation

Output ceilings clamp to model maxima or adapt scope per [output-ceiling adaptation](#output-ceiling-adaptation). Beyond that, target-specific budget rules must explicitly specify the changed intent, target, finite bound, answer reserve, and evidence—never silently overriding numeric thinking intent or equating unknown capacity with unlimited capacity.

### Binding-level mapping

[`ReasoningEffortMapping`](../../crates/morphiecore-lowering/src/reasoning.rs) and [`ReasoningRequestPolicy`](../../crates/morphiecore-lowering/src/reasoning.rs) execute strictly directional translations between qualitative tiers and upstream/downstream token budgets.

### Implementation owners

- [`semantic::task::generation`](../../crates/morphiecore-semantic/src/task/generation/mod.rs): Activation and scoped effort types.
- [`protocol::reasoning`](../../crates/morphiecore-lowering/src/reasoning.rs): Quantization presets, nearest-level resolution, and policy projection.
- [`topology::catalog::bindings`](../../crates/morphiecore-catalog/src/topology/catalog/bindings.rs): Binding-level policy selection and limits.

## Media

Media resources (images, audio, video, files) are handled as inert payloads (inline base64 data or public HTTPS URLs):
- The gateway never fetches remote URLs or resizes/transforms media inputs; output normalization is bounded to [static images](#native-static-images) and `delivery.ffmpeg` media format conversion.
- **Public ingress boundary:** Standard public HTTP ingress (`Dialect::Standard`) does **not** activate non-standard Chat `video_url` or Responses `input_video` discriminators and rejects them at request decoding. Upstream Provider dialects and IR/Interactions video support remain independent of that public client boundary.
- **Instructions & references:** Application instructions are strictly text-only; client-provided Provider `file_id` values are rejected.
- **Image transformations & `detail`:** `oversized_image: "error"` maps to `RejectOversized` (default allows upstream downsizing). Explicit `detail: "auto"` maps to the target model's native default under `generation.image-auto-as-native-default.v1` (L2, preserving explicit source presence while non-auto unsupported values take `generation.omit-image-detail.v1`).

### Native static images

Standard OpenAI `POST /v1/images/generations` and direct-input multipart `POST /v1/images/edits` (up to 16 `png`/`jpeg`/`webp` sources within an 8 MiB aggregate budget; no masks, `file_id`, JSON edit refs, SVG, or GIF) normalize formats via bounded pipe/temp-file `delivery.ffmpeg` (`png`/`jpeg`/`webp` + `output_compression` without resizing; matching formats without recompression pass through directly).
- If upstream omits `created`, `images.delivery-created.v1` records gateway artifact-preparation Unix seconds; native billing/usage is omitted rather than relabeled as OpenAI Images usage.
- On static non-OpenAI targets, `stream: true` (`n = 1`) executes statically and emits one `image_generation.completed` SSE frame omitting usage; OpenAI GPT Image retains native preview SSE (`partial_images`), masks, and usage.
- Exact `size` or `n > 1` on single-artifact targets (`gemini-nano-banana-2.1`, `grok-imagine-image-2.0`), `n > 6` on Qwen, `mask`, and `background: "transparent"` on non-transparent/JPEG targets reject L5 pre-dispatch; unsupported advisory controls (`quality`, `opaque`, `moderation`, `input_fidelity`, `partial_images` on static targets, unsupported identity hints) omit L4. Missing `delivery.ffmpeg` when transcoding is needed fails cleanly (`503`).

## Adaptation evidence

### Domain evidence interface

Transformations return `Result<Conversion<T>, ConversionFailure>`, pairing transformed payloads with a body-free [`EvidenceDelta`](../../crates/morphiecore-lowering/src/conversion/generation/evidence.rs) (`MAX_EFFECTS = 64`, 16 KiB per delta, explicit truncation counters). Encoders receive a fixed final `TargetContract` at construction without publication-time policy mutation.

### Atomic evidence and output batches

Execution processes checked `EncodedBatch` frame collections; `Report::batch` records `EvidenceDelta` separately from downstream delivery acknowledgement.

### Typed conversion failures

`ConversionFailure` records boundary, locus, and cause (`CodecError` or `RepresentationError`). `SourceFailureEvidence` records upstream status, error codes, and completeness without retaining unbounded raw error strings.

### Public capability and adaptation notice

`AdaptationNotice` is an optional namespaced `morphiecore` field on downstream JSON or terminal SSE frames (max 16 entries / 2048 bytes); standard clients may ignore it safely.

### Generation Text model discovery

`GET /v1/morphiecore/models` and `GET /v1/morphiecore/models/{model}` publish verified capability directories compiled once at startup from active candidate bindings and the [conversion ladder](#conversion-ladder) disposition tables (also feeding `/v1/morphiecore/capabilities` within a fail-closed 4 MiB snapshot budget):
- **Capacities:** Derived from service-qualified catalog seeds or explicit planning policies; mixed routes publish route-common minima only when every active candidate declares a comparable value (replacing origin/path/upstream model clears inherited capacity evidence). Pi hides aliases, prefers advertised total context (falling back to input maximum only when total context is unknown), and reconciles active metadata only at idle boundaries.
- **Native efforts & preferences:** `values` lists route-common native tiers; `compatibility_values` lists accepted projected tiers. `preferred_interface` selects only a primary candidate's exact native Chat/Responses/Messages correspondence when all assessed default text/tool/reasoning probes pass (otherwise `not_assessed`); preferences never rank by conversion loss or disclose candidate order/identity.
- **Witnesses vs. general proof:** Tool selection/history witnesses run the same candidate projector over bounded cases; default capability passes do not certify arbitrary client inventories or live model behavior.
