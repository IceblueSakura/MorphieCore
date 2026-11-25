# Protocol Adaptation and Provider Dialects

This document defines wire-to-IR and IR-to-wire adaptation rules, conversion ladders, reasoning controls, media handling, adaptation evidence reporting, and native Anthropic Messages projection.

See [Architecture Overview](index.md) for system boundaries and core IR semantics; [Runtime and routing](runtime-and-routing.md) for execution and failover; [History and replay](history-and-replay.md) for multi-turn state; and [Settled decisions](decisions.md) for accepted trade-offs.

## Contents

- [Adaptation](#adaptation)
  - [Source and target boundaries](#source-and-target-boundaries)
    - [Protocol baselines and Provider dialects](#protocol-baselines-and-provider-dialects)
    - [Standard request boundary](#standard-request-boundary)
    - [Google strict function arguments](#google-strict-function-arguments)
    - [Protocol tree ownership](#protocol-tree-ownership)
      - [OpenAI Terra](#openai-terra)
      - [Native DeepSeek Chat](#native-deepseek-chat)
      - [OpenCode Go controls](#opencode-go-controls)
      - [Hosted MiniCPM Chat](#hosted-minicpm-chat)
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
      - [Google response-effort projection](#google-response-effort-projection)
    - [Reasoning level calibration](#reasoning-level-calibration)
      - [Thinking-budget quantization](#thinking-budget-quantization)
      - [Bounded token-budget adaptation](#bounded-token-budget-adaptation)
      - [Binding-level mapping](#binding-level-mapping)
      - [Implementation owners](#implementation-owners)
  - [Media](#media)
  - [Adaptation evidence](#adaptation-evidence)
    - [Domain evidence interface](#domain-evidence-interface)
    - [Atomic evidence and output batches](#atomic-evidence-and-output-batches)
    - [Typed conversion failures](#typed-conversion-failures)
    - [Public capability and adaptation notice](#public-capability-and-adaptation-notice)
    - [Generation Text model discovery](#generation-text-model-discovery)
- [Native Messages projection](#native-messages-projection)
  - [Request rules](#request-rules)
  - [Missing output ceilings](#missing-output-ceilings)
  - [Owner-local cache admission](#owner-local-cache-admission)
  - [Caller-selected automatic cache admission](#caller-selected-automatic-cache-admission)
  - [Response rules](#response-rules)

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

Bailian Responses reasoning event aliases are binding-qualified. Legacy shorthand binds `reasoning_text` to summaries, but hosted `deepseek-v4.1-flash-1017` and TokenPlan `qwen4-max` bind it to reasoning content. Event accumulation and item snapshots must agree on that domain. Item-done mismatches retain exact rejection without granting terminal replay-refresh permission.

- **TokenPlan Qwen4-Max Responses:** retains explicit `summary:"auto"` without manufacturing it from omission. This mode requests summary plus ciphertext without a separate `include`. A concrete response-summary echo takes precedence over its null legacy placeholder, not a different concrete value. Empty ciphertext is a placeholder only on the reasoning item's opening event; completion remains strict. Completed summary/ciphertext replays only to the original bound target. [Source contract](../refs/2026-11-13-model-catalog-expansion.md#tokenplan)
- **Native DeepSeek Responses:** summary and encrypted-output preferences have no effective carrier and use the named omissions; reasoning effort is retained. This does not change the Bailian-hosted DeepSeek dialect. [Native contract](https://api-docs.deepseek.com/guides/responses_api)
- **OpenRouter Responses:** a validated empty first-frame `response.failed` is a classified stream error, not a start event or a successful response. A first-frame success, malformed failure, or frames after the terminal still reject. [Streaming contract](https://openrouter.ai/docs/api_reference/streaming#handling-errors-during-streaming)
- **Google GenerateContent:** a signed wire Part begins its own replay owner even when it is the first Part of a frame and its text is empty. It cannot absorb preceding unsigned text. This does not create a standard stateless downstream carrier for ordinary signatures or qualify a deployment. [Signature contract](https://ai.google.dev/gemini-api/docs/generate-content/thought-signatures)

#### Standard request boundary

Four decoupled responsibilities:
1. **Client intake:** Enforces strict official protocol syntax; ignores SDK-specific helper properties.
2. **Semantic IR:** Holds normalized cross-protocol meaning.
3. **Target encoding:** Validates target-specific constraints and formats native requests.
4. **Provider egress dialect:** Translates standard target requests to vendor wire formats where required.

Strict-tool admission follows the effective binding/API disposition in shared request projection, including direct library lowering; writers render that decision. Native OpenAI/SIWC GPT bindings use the strict carrier. Bailian uses the separately selected [verified-only function-strictness policy](#shared-aliyun-billing-dialect), rather than canonical model flags or a blanket explicit-strict rejection. These carrier decisions do not attest arbitrary-schema enforcement.

#### Google strict function arguments

Existing Interactions targets admit a qualified closed-object function-argument subset through their native request-wide validation modes. The [local SDK evidence](../refs/2026-10-27-official-sdk-sources.md#google-function-argument-carriers-2026-11-09) is separate from text-response schema support.

- Preserve supplied schemas exactly.

  The selected vocabulary is `type`, `properties`, `required`, `additionalProperties:false`, array `items`, string/numeric `enum`, `title` and `description`; nested objects must also be closed.

  Required nullable fields may use exactly two `anyOf` branches: one supported schema and one exact `{"type":"null"}`, in either order, with only optional title/description siblings.

  This admits current Agent strict optional-field serialization without arbitrary composition or schema rewriting.

  Boolean types are supported, boolean enums are not.

  Integer enum literals stay within the safe binary64 integer range; fractional enum literals must survive binary64 serialization without precision change.

  Length, pattern, general composition and other unqualified constraints retain explicit-strict rejection when calls are enabled.

  Non-strict schemas remain unchanged for upstream validation, without a literal root-type injection.

  With calls enabled, a supplied root `type` must be the literal `"object"`;
  this carrier check is independent of strictness. Omitted types and root
  references remain unchanged for upstream validation.
- **Bounded strict references:** a root `$defs` object and `#/$defs/<name>` references may describe this same subset, including root aliases and nested/nullable references.

  Decode one JSON Pointer token with standard `~0`/`~1` escapes using `serde_json::Value::pointer`; never fetch, inline or normalize schemas.

  Check every definition, including unused ones; the resolved root must be a concrete object.

  Reference nodes allow only `$ref`, plus `$defs` at the document root.

  Missing/remote/percent-encoded references, extra pointer segments, nested definition repositories, resource scopes, other reference siblings and cycles remain outside this qualified subset.

  Memoized subtree heights bound every logical schema/reference path to 64 edges without exponential DAG expansion; existing document byte/node/depth bounds remain.
- Strict automatic selection uses `validated`; required selection uses `any`; none stays `none`. Allowed names remain exact.
- When calls are enabled, the mode covers the whole effective inventory. With any strict tool, every declaration's supplied schema must fit the subset. Mixed strict/non-strict declarations are accepted only within that inventory; non-strictness does not promise invalid arguments. Conditional source defaults resolve on candidate copies through the same schema and inventory check, otherwise taking the existing named conditional-default fallback. Explicit true never takes that fallback.
- **Native no-call equivalence:** explicit `tool_choice:none` uses `none`, which prohibits every call and satisfies argument/cardinality guarantees vacuously. Shared admission and direct writers retain complete declarations/schemas, skip conditional-strict fallback and accept serial intent without requesting validated decoding, including scalar-root declarations. This does not relax auto/required selection or bypass known-field, bounds, trust, history or other declaration-carrier checks.
- Parameter omission keeps the native parameterless declaration. No schema deletion, argument repair, inferred prompting or local retry supplies a missing guarantee. Serial intent still rejects when calls are enabled; one declared function can produce multiple calls.

Ordinary Google text/call signatures without a lawful stateless carrier reject on Responses and Chat/Messages. Managed continuation is experimental and disabled under the [stateful freeze](history-and-replay.md#stateful-history); the [field-level boundary](history-and-replay.md#stateless-api-boundaries) records owners and rejection timing. No public replay extension, signature deletion or surrogate reasoning owner is available.

#### Protocol tree ownership

Module layout mirrors `vendor / api / responsibility`:
- `protocol::openai::{chat, responses}`
- `protocol::anthropic::messages`
- `protocol::google::interactions`

##### OpenAI Terra

Terra retains sampling and logprobs only when the effective reasoning configuration is explicitly off, after resolving native history updates. Active/default thinking uses the existing named field omissions without changing the source controls or collapsing otherwise-native configuration history. Official API and SIWC service limits remain distinct; [source contracts](../refs/2026-11-13-model-catalog-expansion.md#openai-terra-and-siwc) own the model-specific evidence.

##### Native DeepSeek Chat

Native DeepSeek Chat rejects JSON Schema text constraints and forced tool selection
under active or default thinking. Explicit off retains ordinary required selection;
the gateway never disables thinking to make a request admissible. An allowed set
covering the entire function inventory uses equivalent `auto/required`; a proper
subset rejects rather than deleting declarations. Native Responses retains its
separate format and selection carriers. [Source contracts](../refs/2026-11-12-generation-binding-contracts.md#parameter-contract-recheck--2026-11-13)
keep these restrictions distinct from beta function strictness and Bailian.

##### OpenCode Go controls

HY4 and MiniMax M3.1 Flash carry standard effort through native
Chat bindings. Their service-native positive level domains constrain
projection and discovery; other positive inputs use the existing
`generation.project-effort.v1` nearest-level adaptation. Explicit off keeps its
standard carrier for HY4. MiniMax's always-thinking contract rejects off
rather than omitting it or inferring a positive level.
Absent controls keep upstream defaults. Go MiMo controls and optional output
guarantees retain their existing scope. MiniMax only
supports automatic tool selection; explicit restrictive choices reject before
dispatch. [Current service contracts](../refs/2026-11-13-model-catalog-expansion.md#opencode-go)
retain the dated directory evidence, including retired bindings.

Go Messages rejects tool parameter schemas containing top-level `anyOf`, `allOf`
or `oneOf`, including non-strict tools and explicit no-call requests. Candidate
encoding checks root and retained inline definitions before dispatch and reports
the declaration index with a closed composition reason. Nested composition stays
unchanged. This is a Go target limitation, not a restriction on client parsing or
the semantic schema union; no tool deletion, constraint erasure or argument
wrapping supplies an equivalent carrier.

OpenRouter Responses access-program reports use the existing bounded,
source-bound report view; they do not activate client access-program controls or
become billed token counts. The [source recheck](../refs/2026-10-27-client-ir-boundaries.md#openrouter-response-reports-2026-11-17)
distinguishes these reports from request intent.

##### Hosted MiniCPM Chat

The four ModelBest MiniCPM bindings use `chat_template_kwargs.enable_thinking` as their native Boolean activation carrier. Explicit on/off survives projection; absence keeps each hosted model's default. A positive reasoning-scoped effort also enables thinking, while its standard `reasoning_effort` spelling remains available for upstream validation. Thinking budgets and top-k do not acquire a carrier through this switch.

MiniCPM5 uses `reasoning_content` for readable history; MiniCPM-V 4.7 uses `reasoning`. Both response spellings normalize to one typed reasoning owner in JSON and SSE. Historical reasoning consumption remains `not_assessed`; neither spelling declares an omission requirement. The [hosted contract sources](../refs/2026-10-27-client-ir-boundaries.md#hosted-minicpm-control-recheck-2026-11-13) distinguish activation, output and history evidence.

##### Shared Aliyun billing dialect

Responses function feedback accepts strings and ordered arrays of text parts without flattening or reordering. Media tool results remain outside this qualified carrier.

DashScope and Aliyun TokenPlan share the `Bailian` wire dialect for Chat and Responses:
- Readable reasoning mapped to `reasoning_content` with `preserve_thinking`.
- Chat total output ceilings mapped to `max_completion_tokens`; Responses uses `max_output_tokens` with the model-specific scope in [output-ceiling adaptation](#output-ceiling-adaptation).
- Responses output constraints use the existing standard `text.format` carrier, preserving schema, name, description and explicit strictness. Do not reject the whole text configuration because verbosity lacks a native carrier. The [source refresh](../refs/2026-10-19-bailian-compatible-requests.md#responses-output-format-refresh-2026-11-08) distinguishes documented layouts, observed SDK-flat carriage and model-specific qualification.
- Responses carries explicit `parallel_tool_calls`. Default/enabled-thinking `qwen3.8-flash` and `glm-5.3` reject required, required-subset and specific selection because both required spellings fail the [qualified native controls](../refs/2026-10-19-bailian-compatible-requests.md#responses-tool-policy-refresh-2026-11-08). Serial admission does not supply an independent forcing guarantee. Model-bound direct lowering retains the mapping identity for the same checks; unbound task encoding does not qualify a model.
- Kimi K3.1 Pro accepts only temperature 1.0, top-p 0.95 and presence penalty 0.0, including with thinking off. Incompatible optional values take the existing named field omissions on candidate copies; native source validation retains rejection of these known invalid values. [Current model contract](../refs/2026-11-13-model-catalog-expansion.md#kimi-k31-pro).

**Function strictness selected 2026-11-09:** these are two plans of the same upstream service, so `aliyun-dashscope-cn` and `aliyun-tokenplan-cn` share one default policy on Chat and Responses. Preserve strictness only for an independently verified exact model/API pair; otherwise relax explicit true and conditional defaults under `bailian.relax-function-strictness.v1` (L3). The current verified set is empty: existing Qwen capability flags and isolated DeepSeek successes do not establish enforced function schemas. New models default to relaxation.

The source IR retains the client's original strictness.

For unverified pairs, candidate-local projection changes root and inline function strictness to non-strict, without materializing or rewriting parameter schemas.

Root Bailian declarations omit `strict`; retained standard Responses batches spell their non-strict default as `strict:false`.

Explicit false remains non-strict without a guarantee-loss receipt.

Discovery reports the relaxed target's approximated capability rather than native strict enforcement.

The same owner applies to endpoint encoding and direct lowering, even under a permissive caller-supplied contract.

This explicitly accepts best-effort arguments and downstream validation/correction costs; it revises the former GLM Chat/Bailian Responses rejection and Qwen Chat blanket capability treatment. It does not relax structured-text output strictness, tool identity, required selection, call count, seriality or thinking intent, and adds no argument repair, prompt rewriting or automatic retry. Other Providers keep their own strict contracts.

##### Subscription Responses

SIWC (Sign-in with ChatGPT) and Grok use Responses endpoints with stateless upstream execution (`store: false`):
- Prompt cache keys mapped via `prompt_cache_key`.
- Streaming intake required for all client requests; terminal output reconstructed from event state.

SIWC carries root namespaces natively. Flat function/custom definitions take the equivalent `siwc.inline-root-tools.v1` projection into one leading developer `additional_tools` batch. Preserve names, schemas, strictness, selection, call/result identities and existing inline order on candidate copies. No namespace alias or argument repair is introduced. Inactive dispatch and supported direct-call shapes remain required. [Source contracts](../refs/2026-11-12-generation-binding-contracts.md) distinguish these carriers from hosted tools and account eligibility.

The qualified Grok recovery profiles retain upstream literal-label omissions and tool selection contrary to prompt-only prohibitions as scoped service limitations ([acceptance status](../development.md#overall-acceptance-status)). Preserve request text/order and explicit tool policy; prose does not synthesize `tool_choice:none`. Retain honest interruption, execution/feedback identities and original content assertions. No strictness relaxation, role/history rewrite, argument repair or retry is selected for these symptoms.

#### Field handling

Known fields follow the [conversion ladder](#conversion-ladder); this table states how intake classes enter it.

| Classification | Gateway Action |
|---|---|
| **Content and explicit guarantees** | Instructions, messages, media, tools, arguments, call links, output constraints and explicit strictness: same-name or same-semantic carriers, or a user-selected approximation such as tool-error wrapping or visible-history transfer; otherwise reject |
| **Controls and hints** | Sampling, effort, presentation, cache and service hints: walk the ladder through bounded approximation and named omission on candidate copies |
| **Reports and metadata** | Delivered report facts without a client carrier take named omission; they never fail delivery |
| **Unscheduled capabilities** | Stateful, hosted-execution and other unscheduled capabilities keep their [retained rejections](../roadmap/unscheduled-issues.md#retained-rejections) until scheduled |
| **Legacy standard carrier** | Convert known historical fields when semantics are unambiguous |
| **Unknown extension field** | Accept and discard at valid object positions; do not forward or store |
| **Malformed known value** | Grammar error; outside the ladder |

#### Client Responses history shorthand

- Assistant text shorthand strings are accepted at intake.
- Output items in history require standard fields (`type`, `id`, `role`, `content`, `status`). Assistant `output_text` may omit its empty annotations collection under the selected [L2 compatibility rule](#semantic-equivalence-compatibility); no other required fields are inferred.

#### Candidate-local projection

Transformations are candidate-private:
- Source IR remains immutable across pre-dispatch candidate checks. A dispatched retry reuses the same prepared target and request.
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

**Execution strategy.** The ladder adapts fields and carriers, not the runtime's strategy. Apply the [gateway/runtime boundary](index.md#system-boundaries): a known restriction requiring automatic thinking disablement or another strategy replacement rejects with an actionable standard error. A runtime-selected replacement request is evaluated independently.

**Low-loss priority, reaffirmed 2026-11-07.** Field spelling or shape differences, incomplete capability assessment and missing optional controls are not independent rejection reasons.

Prefer semantic matching, then a useful approximation, then omission with existing loss evidence.

Apply these choices at the responsible projection/dialect boundary; ordinary L2–L4 rules need no new per-rule approval.

Content or explicit-guarantee downgrades still need their concrete disposition selected.

Unresolved cases may remain limited while independently useful workflows ship under the [partial-delivery exit](../roadmap/index.md#partial-delivery-exit).

**Timing.** Request-side levels are resolved per candidate during planning, on candidate-local copies, so request-determined impossibility fails before dispatch (P4). After dispatch the upstream has already generated output; report and metadata facts bottom out at L4, and only content or outcome that would be misrepresented may fail delivery.

**Invariants outside the ladder:** credentials, auth, origin and trusted target selection; replay signature, origin and format trust; resource and time bounds; malformed known values and strict JSON; terminal honesty and validated EOF; call identity; no executable claim for incomplete tool input; explicit history-read intent; already-published accounting; candidate-local projection over immutable source IR; the routing freeze. No level invents content, forges signatures or fabricates success.

**Ownership.** Request and delivery use one typed disposition vocabulary, not separate requirement, feature-set and rejection models.

| Boundary | Responsibility |
|---|---|
| [Semantic projection and dispositions](../../crates/morphiecore-lowering/src/request.rs) | Apply trusted target capabilities to candidate copies. Resolve ordered effort/tools, presentation, summary, encrypted include and parallel permission without erasing explicit guarantees. Refresh native-thinking facts after allocation conversion. |
| [Protocol preparation](../../crates/morphiecore-protocol/src/lowering/request.rs) | Validate source trust, transfer compatible history, apply semantic policy and check final carrier grammar and replay identity. Envelope session affinity stays outside IR. |
| [Delivery](#delivery-projection-ownership) | Project facts and usage views before rendering; no encoder-only omission policy or separate replay-sealing result channel. |
| [Discovery](#generation-text-model-discovery) | Compile capability claims from the same dispositions; distinguish assessed domains from unqualified combinations. |

**Evidence.** Receipts record only L3 approximations, L4 omissions and schema-completion estimates; L2 normalizations, derivations, defaults and request inferences produce none.

Target encoders carry whole evidence deltas so coverage and truncation counts survive merging; Messages retains its directional preference for detailed receipts over matching field summaries.

Rule IDs live in the disposition tables rather than a separate projection-rule enum and scattered string literals.

Remaining field-specific follow-ups are listed in the [roadmap](../roadmap/unscheduled-issues.md#protocol-boundaries).

Applications selected on 2026-10-30 are implemented, including request strictness, effort, ceilings, Messages intake/omission, delivery projection, history transfer and shared discovery.

| Area | Level and rule |
|---|---|
| Explicit `strict:true` without an equivalent carrier | L5 unless a concrete loss is selected, as in the [Bailian service policy](#shared-aliyun-billing-dialect) (L3). Conditional (omitted) strictness is always resolvable without a compatibility classifier: an already-closed schema on a target with an equivalent strict carrier stays strict, every other case resolves non-strict under the conditional-default loss (L3); a request-wide strict grammar overflow resolves all conditional definitions non-strict, and rejects only when explicit strict tools alone overflow |
| Google strictness/serial intent with explicit `tool_choice:none` | L2 compatibility without a loss receipt: native no-call mode satisfies both guarantees, preserving all declarations and schemas without strict-default fallback; enabled calls retain their independent admission gates |
| Go Messages tool schemas with root `anyOf` / `allOf` / `oneOf` | L5 target-carrier rejection, including no-call mode and retained inline definitions; nested schemas and source declarations remain unchanged |
| Effort | Nearest supported level, ties upward, across Reasoning/Response scopes at the same level (L3); manual budgets on adaptive-only bindings convert to adaptive plus effort. When an explicit effort already exists, preserve it rather than replacing it with a budget-derived level; omit the uncarriable numeric budget under a named loss |
| ModelBest Chat thinking | L2 compatibility without a loss receipt: explicit activation uses the nested Boolean; positive reasoning effort also activates thinking and retains the same-name effort field. Absence keeps the hosted default |
| DeepSeek Chat whole-inventory selection | L2 `deepseek.full-tool-inventory-choice.v1`: a complete function allowed set uses scalar `auto/required`, preserving every declaration and selection intent; proper subsets and thinking-mode forcing retain rejection |
| Output ceilings | Preserve equivalent carriers; clamp to a known model maximum or retain a narrower accounting scope with loss (L3); omit when no useful carrier exists (L4). See [output-ceiling adaptation](#output-ceiling-adaptation) |
| `required_with_tools` history with a Standard Chat client | Thinking-enabled/default tool requests reject at planning; the gateway never disables thinking to satisfy the history requirement. Explicitly disabled thinking and tool-free requests retain ordinary admission |
| Uncarried reasoning in delivery or history | Omit the owner, keep visible content and tool links (L3); applies to opaque reasoning on Standard Chat and unsigned reasoning on Messages; never forged |
| Messages reasoning publication | An undecided summary waits for its actual signature in the bounded schedule. An irreversibly uncarriable shape discards its payload immediately and retains only bounded lifecycle tracking; invisible owners do not block visible successors. Summary interruption stays an interruption and cannot supply item or response completion |
| Unsealed or foreign client reasoning in history | Omit under visible-history transfer (L3); never forwarded |
| `stop` without a target carrier | L5 for now; gateway-side truncation may later provide L2 |
| Messages `stop_sequences` on a non-Messages target with an ambiguous natural/stop-sequence cause | `end_turn` under a named approximation (L3) |
| Unknown finish or incomplete reasons | Evidence-backed per-dialect maps; Responses carries an unspecified reason as `incomplete_details:{}`; otherwise L5 on Chat and Messages, so no outcome is misreported as a token limit |
| Responses client-history annotations | L2 compatibility without a loss receipt; see [semantic-equivalence compatibility](#semantic-equivalence-compatibility) |
| Per-use image detail | Preserve an exact carrier. `generation.image-auto-as-native-default.v1` maps explicit `auto` to the selected model's native default (L2, no loss receipt); selected non-auto omissions retain `generation.omit-image-detail.v1` (L4) |
| Messages `thinking.block_binding` | Public adaptive-thinking intake normalizes `null`/`{}` under L2; `drop_block` takes `messages.omit-block-binding.v1` (L3) without relaxing gateway replay checks; `"error"` is L5. Trusted native target carriers remain independent |
| Legacy Messages sampling | Exact `temperature`/`top_p` in `[0,1]` and mathematically integral `top_k` in `[0,18446744073709551615]` take `messages.omit-removed-sampling.v1` (L4) at public intake; equivalent decimal/exponent spellings are accepted without rounding; malformed known values reject |
| Messages version and beta headers | See the [version policy](index.md#protocols) |
| Unscheduled capabilities | Moderation, background, context management and compaction requests, hosted tools and items, `between_tools`, message-local null/reset and unselected format/task-budget controls, `clear_at`, nonempty `logit_bias`, legacy function carriers, `inference_geo` and upstream-reported storage keep their current rejection until scheduled |

Existing rule families remain valid ladder applications:

| Rule Family | Policy |
|---|---|
| Prediction / advisory controls | Preserve in IR; apply exact native carrier, or omit via named receipt |
| Reasoning allocation / effort | Preserve budget and effort; apply directional quantization or scope approximation |
| Tool-result errors | Preserve native error flag; wrap in `[tool result is_error=...]` if target lacks boolean carrier |
| SIWC flat root tools | L2 `siwc.inline-root-tools.v1` moves function/custom declarations into a leading developer batch, preserving their leaf identities and existing history |
| Chat JSON grouping | Concatenate contiguous plain assistant text before function/custom calls, preserving call identity, order and raw arguments/input. Do not flatten rich/replay-bearing parts or reorder text across calls; custom SSE remains unsupported |
| Instruction roles | Canonical Developer mapping; preserve root vs. ordered lifetimes |
| Output ceilings | Use the [output-ceiling disposition](#output-ceiling-adaptation); insert a target default only when the original client ceiling is absent |
| Terminal stops | Coarsen natural/stop-sequence causes to `stop` or `end_turn` under named rules |
| SIWC failed-response codes | `responses.coarsen-provider-error-code.v1` maps confirmed quota to `rate_limit_exceeded`, unsupported capability to `invalid_prompt`, and service/auth/permission codes to `server_error`; failed outcome and original source evidence remain intact |

- `chat.omit-text-item-status.v1`: Plain-text incomplete message items omit individual item status when overall finish reason is `length`.
(See [`Rule`](../../crates/morphiecore-lowering/src/disposition.rs) for request rules and the [delivery projection owner](#delivery-projection-ownership) for client dispositions.)

#### Output-ceiling adaptation

A client output ceiling is an adaptable generation control. Prefer retaining useful control over rejecting a request because the target cannot express exactly the same ceiling:

| Target carrier | Disposition |
|---|---|
| Same accounting scope under another field name | L2: preserve the value and use the target spelling |
| Equivalent ceiling above a known model maximum | L3: clamp to that maximum under the existing rule |
| Only a reply-token ceiling for a total-output request | L3: retain the same numeric value as a reply-token ceiling and record the accounting-scope loss; do not report that thinking tokens were bounded by it |
| No useful ceiling carrier or no expressible value | L4: omit the ceiling and record the loss; do not invent a tokenizer-based limit or silently raise the requested value |
| Carrier exists but model acceptance is unassessed | Delegate upstream validation rather than pre-reject solely for missing qualification |

Resolve spelling, scope and supported values for the actual binding/API, not a shared model label. Record approximations and omissions through the existing conversion evidence; no client echo is required. Preserve original intent on the source request, including absence versus a ceiling omitted by projection, so default insertion cannot overwrite it.

`generation.reply-only-output-ceiling.v1` records total-to-reply scope adaptation on thinking-enabled non-Qwen Bailian Responses targets. Values below that API's minimum take the existing ceiling-omission rule. Native source decoding retains its separate scope restriction: reply-only source values are not silently reinterpreted as total-output ceilings. DeepSeek Chat uses native `max_tokens`; Standard Chat keeps its own spelling.

Qwen4 Max Preview has total-output semantics for Chat `max_tokens` and `max_completion_tokens`, and Responses `max_output_tokens`. Its native source decoder and target projection share that model-qualified scope. Older Qwen Chat's reply-only `max_tokens` restriction remains separate.

Gateway request/response byte limits, deadlines, model bounds and honest terminal reporting remain independent. The adaptation introduces neither gateway-side token counting nor fabricated length termination or a general formula for increasing thinking budgets.

#### Semantic-equivalence compatibility

This is ladder level L2 of the [conversion ladder](#conversion-ladder).

The selected null-metadata, empty-logprobs and [reviewed equivalent forms](../refs/2026-10-27-client-ir-boundaries.md#semantic-equivalence-review-2026-10-28) are implemented.

Further L2 applications are found through the [scoped follow-ups](../roadmap/unscheduled-issues.md#protocol-boundaries) rather than a separate field-by-field review.

Protocol-defined equivalent empty values, empty collections, defaults and structural placeholders belong to lossless mapping.

Accept, normalize, omit or supply them as the target grammar requires, without a separate named-omission path or loss receipt.

Equivalence depends on the field, direction and lifecycle contract, not emptiness alone. It must preserve content, control intent, explicit constraints, identity, report facts and subsequent behavior. Preserve source observations; apply target representation changes in the existing codec/projection owners. Do not introduce recursive empty-field stripping or globally equate absent, null, empty, false and zero. Unknown fields and malformed known values retain their existing admission rules.

Selected applications are reported `metadata:null` on Chat SSE and Messages delivery, and `logprobs:[]` on Messages delivery and history admission.

These carry no metadata entries or token-probability observations to lose.

Same-protocol carriers still preserve source presence where representable, and target-required fields remain required.

Null metadata does not echo request metadata or become Messages `metadata.user_id`; empty/nonempty metadata maps and other unsupported execution hints retain their current dispositions.

Nonempty logprobs still reject where no carrier exists, and an explicit request for probabilities is checked independently of an empty output report.

An empty update that clears state, an empty restriction set, an identity-bearing block or a terminal report is not equivalent to omission merely because its payload is empty. Stream accumulation must not erase earlier reports or suppress completion. Tests belong at the responsible boundaries: equivalent forms agree across JSON/SSE/history; meaningful nonempty values, source observations and state transitions remain protected.

This decision removes `chat.omit-null-stream-metadata.v1` and `messages.omit-null-response-metadata.v1` and their receipts. Actual removal of meaningful information continues to require a selected named loss or rejection.

Further selected applications: null-only system fingerprints on Responses/Messages and terminal explanations on Messages carry no report value; explicit `logprobs:false` needs no Messages probability carrier.

Document citations default off: an omitted/false `citations.enabled` normalizes to an uncited document without a loss receipt; enabled citations remain unselected and malformed known flags reject.

Empty cache options normalize without a dedicated rule.

On trusted always-parallel targets, explicit parallel permission is exact; explicit serial intent still rejects.

The obsolete `cache.omit-inactive-options.v1` and `deepseek.omit-parallel-tool-permission.v1` rules are removed.

Meaningful fingerprints/explanations, active cache controls and requests for probabilities retain their loss/rejection contracts.

Chat request `logit_bias:null` is its documented default, equivalent to omission with no active token bias or loss receipt. Non-null values retain rejection; tokenizer-bound active bias remains unselected. This field-specific rule does not equate arbitrary empty objects with null.

Responses Client-history intake interprets an omitted `output_text.annotations` collection as empty. Explicit arrays retain their values and validation; null and malformed known values reject. Target encoding emits the official required array. Upstream response/event snapshots and closed request codecs keep their required-array validation. This L2 compatibility produces no loss/insertion receipt and does not infer identity, lifecycle or other missing fields. It accepts the empty-collection scenario; it cannot recover nonempty annotations discarded by a client.

#### Client/IR projection principles

Apply [semantic-equivalence compatibility](#semantic-equivalence-compatibility) first; classify remaining projection rules under the principles below. They are applications of the [conversion ladder](#conversion-ladder).

| Principle | Rule |
|---|---|
| P1 Request-context deduction | A fact implied by the admitted request and the source grammar is an exact derivation recorded as `infer_from_request`; the source observation stays unreported |
| P2 Carrier-less non-content facts | Meaningful advisory controls, presentation requests and report facts without a target carrier take selected named omission when no exact mapping exists. Semantically equivalent forms need no loss receipt. Visible content and explicit constraints never take this omission |
| P3 Delivery closure | Anything delivered on a route is admissible as history on that route, exactly or through a named projection |
| P4 Planning-time impossibility | A failure determined by the request and the binding contract occurs before dispatch, never after publication |
| P5 Conditional source defaults | An omitted field with a documented conditional default is resolved per target by that documented rule, not by a fixed value |

Accepted applications:

- **Client sampling preferences:** an optional client-side JSON-schema `prefer`
  is not an explicit wire guarantee. Partial/non-compositional discovery cannot
  establish strict compatibility for the active inventory. The MorphieCore Pi
  extension therefore resolves optional preferences on request-context copies
  before native serialization, preserving schemas, tool changes and source history.
  Explicit `require` stays with the native serializer and ordinary target admission.
  The gateway never reinterprets an incoming `strict:true` as an optional preference.
  This integration changes neither Pi itself nor the public protocol.
- **Omitted Responses strictness (P5):** the [Responses reference](https://developers.openai.com/api/reference/resources/responses/methods/create) and [function-calling guide](https://developers.openai.com/api/docs/guides/function-calling#strict-mode) define attempted strict normalization with non-strict fallback; Chat omission is non-strict.

  Implemented on OpenAI Chat/Bailian and Messages paths: an already-closed schema with an equivalent strict carrier stays strict; every other omitted-strict definition resolves non-strict under `openai.omit-conditional-tool-strictness.v1` (L3).

  No compatibility or normalization-feasibility classifier remains.

  An omitted parameter list is parameterless; required schema carriers use a closed empty object (L2), without changing source absence.

  Standard Responses retains its native omission.

  Explicit true never takes the omitted-default loss.

  The IR default names this conditional rule, not a promise to normalize supplied schemas.
- **Reasoning presentation and continuity requests (P2):** `summary:"auto"` selects the most detailed available summarizer; resolving to the fixed native summarizer or no available summary is exact. Explicit `concise`/`detailed` take a named omission without a selectable carrier. Encrypted-reasoning `include` is exact when the target emits no reasoning items and takes a named continuity omission when reasoning has no opaque carrier.
- **Carrier-less reports (P2):** served service classes and refusal categories without an OpenAI carrier are omitted on delivery copies, extending [explanatory terminal reports](#explanatory-terminal-reports).

  Anthropic standard capacity and OpenAI default processing remain distinct classes; no approximate tier is reported.

  Empty probability reports instead follow [semantic-equivalence compatibility](#semantic-equivalence-compatibility).

  Messages delivery applies the same omission to served OpenAI processing classes and echoed execution hints (`metadata`, `background`, `prompt_cache_key`, `store`).
- **Messages terminal derivation (P1):** see [Messages terminal projection](#messages-terminal-projection).
- **Messages baseline:** under the [version policy](index.md#protocols), `2023-06-01` is admitted over the current body grammar and beta headers are ignored, while beta body semantics follow body rules. Agents still learn unsupported features through [discovery](#generation-text-model-discovery).

Presentation and continuity requests project on candidate copies before final validation across target families.

Native Responses retains its carriers; Messages and Google can approximate explicit summary granularity through their native summarizers.

Auto resolves to the native default without a loss receipt; unavailable explicit granularity uses `generation.omit-reasoning-summary.v1`.

Encrypted include uses `generation.omit-encrypted-reasoning-include.v1` without an opaque carrier, except that admitted explicit thinking-off has no new reasoning to encrypt and needs no loss receipt.

These rules do not change activation, effort, budgets, context, read-history intent, history owners or source observations.

Null/omitted visibility guards remain.

The conditional-default consumers share one resolver for root definitions, namespace leaves where carried, and inline-added definitions.

Projection mutates private settings/items only; supplied schemas and source observations are unchanged.

Unknown schema keywords, including inside closed objects and referenced definitions, take the same named non-strict fallback rather than local normalization-feasibility classification.

The existing subset walker checks closure and the known vocabulary together; it does not rewrite schemas.

Property names and annotation values are not schema keywords.

Explicit strict requests retain upstream delegation for unknown keywords, independently of conditional-default inference.

Native Responses keeps omitted parameters and strictness; Chat's existing omitted non-strict default also keeps optional spelling.

Bailian final writers omit the unsupported strict flag under their native non-strict default.

The request disposition owner selects these rules before final encoding.

The Messages consumer resolves conditional defaults once, after selected tool-change carrier projection, using the binding's strict opt-in and native exact subset.

String enum/const casing and unsupported reference carriers cannot provide an equivalent strict guarantee.

Root and inline strict tools share one native grammar budget with JSON output.

If tentative conditional strictness exceeds that budget, every conditional definition becomes non-strict; explicit strict definitions and the output schema remain unchanged, and final validation rejects if those alone overflow.

Projection and final wire validation reuse the same subset/budget checker.

Parameterless definitions receive the required empty-object input schema without a loss receipt; equivalent empty schemas may still use an available strict carrier.

SSE, JSON and transient projected snapshots consume the same selected argument bytes, including incomplete prefixes without JSON parsing. Native structured observations and protected replay dependencies remain unchanged; representation conversion alone needs no loss receipt.

#### Delivery projection ownership

The [protocol adapter](../../crates/morphiecore-protocol/src/adapter/downstream.rs) applies typed delivery dispositions before rendering. It selects argument carriers, reasoning/refusal substitutions, report omissions, usage views, terminal mappings and grouping. Runtime delivery retains the incremental projection: SSE renders selected events; JSON and transient snapshots reduce them. Encoders own wire grammar and resource checks, not another conversion policy.

Projection operates on private delivery copies. Execution-assigned response/item aliases are merged before materializing JSON or retained views; protected call dependencies keep their execution ownership until the separately scheduled identity migration. Wire-only Messages block/owner closure does not finish an incomplete semantic item.

New Responses message aliases use `msg_` for both delivery and cross-protocol request history. Native returned aliases and call/result identities remain unchanged. Generated-ID collision checks follow the target carrier: Responses validates generated message aliases; Chat cannot reject a collision involving a message alias it never emits. JSON, SSE and retained delivery share the same execution-assigned alias.

Replay dependencies refresh for the affected owner after projected item start/finish and for all owners at terminal, before final validation. Metadata and alias refreshes do not rebuild replay dependencies. Intermediate readable deltas may leave an open owner's dependency stale; no consumer relies on that association before its item boundary or terminal.

Null-equivalent delivery reports and hints use `Option`: service class, fingerprints, refusal details, probability reports and execution echoes. Request/response `metadata` remains three-state because explicit null suppresses request-metadata echo. Equivalent accounting conversions and empty/default carriers do not acquire loss receipts; L3/L4 removals and explicit estimates retain their named evidence.

For a Chat client, native lifecycle opening service reports are omitted under `chat.omit-opening-service-report.v1`; a terminal non-null served class uses its exact carrier. Source opening/terminal reports stay distinct. Messages refusal becomes visible text under `messages.refusal-as-text.v1`, selected before both JSON and SSE renderers. Upstream call IDs outside the Messages `^[a-zA-Z0-9_-]{1,64}$` grammar are delivered as `mc_` plus the first 40 hex digits of their SHA-256 under `messages.project-call-ids.v1`, the same mapping that request preparation applies toward Messages targets. The returned history therefore carries one stable link; the original upstream spelling is not restored.

The [closed dialect terminal maps](../refs/2026-10-17-generation-contracts.md#chat-terminal-source-recheck-2026-11-01) share JSON/chunk classification. Resource, network and server failures use the failure path; interrupted generation stays incomplete. Unknown labels reject. Unrepresented Responses incomplete-reason labels take `responses.omit-unrepresented-incomplete-reason.v1` without inventing natural completion. Terminal details that would change a visible completion guarantee retain their L5 boundary.

#### Generated tool arguments and execution input

An unfinished generated argument prefix projects exactly to an OpenAI argument string without parsing, repairing or completing its bytes. Preserve incomplete call lifecycle where carried, the incomplete overall outcome and token-limit termination. Format conversion alone needs no loss receipt; unrelated lifecycle, grouping and protected-replay restrictions remain authoritative. A generated string is not a complete validated invocation. Native Messages complete JSON/history still requires an input object; its partial SSE carrier does not supply that object.

Downstream JSON can aggregate [streaming upstream intake](index.md#protocols) after validated EOF. This preserves incomplete Messages arguments without inventing a native JSON object.

Client acceptance must distinguish error-result events from actual execution. Current pi fails all calls on a length-stopped message even when salvaged arguments parse, then supplies error feedback. That observed consumer behavior supports this narrow contract, not every consumer or a signature/dependency rewrite.

#### Tool-result association

Call IDs own result association, not vector adjacency. Preserve grouped calls followed by their corresponding results without reordering, flattening or dropping fields. Existing identity, causal-order, duplicate, bounds and replay checks remain. Exact association needs no new loss.

The Bailian Responses consumer removes its predecessor-position check at the shared native intake/target/writer boundary.

Association stays with IR validation; no reordering pass or duplicate association owner is introduced.

Independent grouped/adjacent input and output expectations preserve IDs, arguments and results, while invalid links still fail semantic validation.

String-only result implementation, phase, history-carrier and replay restrictions remain separate.

[Native qualification](../refs/2026-10-27-client-ir-boundaries.md#pre-stage-4-contract-qualification-2026-10-29) covers direct Qwen Flash/Max, TokenPlan Qwen Flash, native MiMo and OpenRouter GPT grouped histories. This is not a Qwen-only semantic rule. It does not attest every model, result ordering, arbitrary interleaving, media-array carrier or billing endpoint; stricter target restrictions require endpoint evidence rather than literal-position inference from one guide. Phase remains a separate semantic/carrier question.

#### Downstream schema completion

Generation Text returns complete downstream schemas: **Upstream facts → Justified inference → Simple estimates**.
- `responses.infer-required-settings.v1`: Fills missing `tools`, `tool_choice`, and `parallel_tool_calls` from admitted request settings.
- `responses.echo-request-metadata.v1`: Echoes original admitted request metadata only when upstream omits metadata entirely ([`Rule::ECHO_REQUEST_METADATA`](../../crates/morphiecore-lowering/src/disposition.rs)); this exact inference produces no loss receipt.
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
- `openai.omit-unrepresentable-service-class.v1` omits served classes without an OpenAI carrier on delivery copies. It never substitutes an approximate tier.
- Stream encoders validate original service observations before projecting rendering copies. Conflicting single-report or lifecycle updates still reject even when both classes would be omitted.

#### Lifecycle settings and service reports

Responses and Interactions maintain separate opening and terminal reports validated by [`protocol::reports`](../../crates/morphiecore-protocol/src/reports.rs). Cross-phase updates permit only evidenced runtime resolutions (e.g. `summary: auto → detailed` or `service: configured → standard`); conflicting drifts reject fail-closed.

Standard Responses permits opening `service_tier:auto` to resolve to terminal `default`, independently of Provider-specific reasoning-report rules. Preserve both observations; a changed same-phase report still rejects. The [official service-tier contract](https://developers.openai.com/api/reference/resources/responses/methods/create) distinguishes configured processing from the tier actually used.

#### End-user attribution

Opaque user attribution projects across protocols via [`protocol::attribution`](../../crates/morphiecore-protocol/src/attribution.rs):
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

A completed outcome whose only ambiguity is a declared stop sequence on a non-Messages target maps to `end_turn` under a named approximation (L3), because the source cannot say which cause applied. A Chat `stop` finish accompanied by function calls maps to `tool_use` (L2), as forced tool choice reports `stop` with calls. Incomplete, failed and cancelled outcomes remain unchanged.

### Reasoning controls

#### Reasoning activation and effort scope

The IR decouples reasoning activation from effort level and scope:
- **Activation:** `ReasoningRequest` (enabled vs disabled).
- **Effort Control:** `EffortControl { scope, level }` with [`EffortScope::Reasoning`](../../crates/morphiecore-semantic/src/task/generation/content/reasoning.rs) (per-thought) or [`EffortScope::Response`](../../crates/morphiecore-semantic/src/task/generation/content/reasoning.rs) (turn-wide).
- Explicit `disabled` reasoning takes precedence over response-wide effort settings.

Model-effective constraints independently declare whether disabled activation is unavailable and which positive effort levels are native. Grok 4.7 and GPT Astra/Sol reject explicit off; Luna retains off. Positive non-native levels use the existing nearest-level mapping, while omission remains omission. Discovery and dispatch consume the same constraint; dialect spelling does not create model support. See the [official source review](../refs/2026-11-12-generation-binding-contracts.md).

##### Ordered effort projection

Root settings are initial controls.

`ConfigurationUpdate.boundary` independently carries `FollowingResponse` or `NewInputTurn`; effort scope describes the affected work, never activation timing.

Responses updates select subsequent responses.

Messages updates immediately after a new-input user turn select its reply; elsewhere they remain pending until new user input.

Tool-result-only turns do not activate them; mixed results and new input do.

Active values persist until replaced.

`GenerationRequest::resolved_configuration` derives active/pending controls in one bounded, pure scan without storing a second mutable setting.

Active and pending values are derived `ConfigurationState` snapshots, not merged update patches.

Both use one ordered transition rule, so off clears prior reasoning effort and later enable cannot resurrect it; immediate updates also apply to the pending state.

Equivalent native carriers retain historical and repeated markers.

Non-equivalent or carrier-less candidates resolve source timing before target scope/level/budget conversion and refresh current-control admission.

`generation.collapse-configuration.v1` records the loss of historical/pending controls; `generation.configuration-prefix-change.v1` records their removed message boundaries and cache-prefix change.

Source history stays unchanged across candidates and failures.

The same bounded projection extends to the existing Google targets; their scope/level mapping follows source-effective activation, never the last marker.

A reasoning-scoped named effort replaces an earlier off selection; explicit off clears inapplicable reasoning allocation, while response-scoped effort remains independent.

History transfer compares source and candidate states at every configuration marker, not merely the final effective value.

If deleting an assistant owner changes an activation interval, even a native-capable target takes the selected collapse using original source controls.

No synthetic assistant boundary or untrusted replay is retained as a placeholder.

The existing message-envelope owner now includes `System`.

Public and native Messages intake share known-field/role validation and empty-string normalization, retaining original system membership rather than raw payloads.

Native grouping validates order/contiguity before its fast path and preserves system-container boundaries under append.

Public unknown fields are still filtered.

Removing or changing an earlier effort marker blocks a candidate that retains later signed/redacted thinking; changes strictly after protected blocks can proceed.

Changing root effort alone does not invalidate thinking.

`drop_block` does not excuse a gateway prefix rewrite.

Direct Messages encoding retains strict issuer admission and shares transfer-before-projection with routed encoding; rendering performs no later history deletion.

Named local effort is admitted only on system messages; omitted system content is empty.

Content-bearing empty configuration has no fields to apply; empty content without a control rejects.

Message-local null configuration/effort and Responses null update effort remain unresolved and reject without inventing reset semantics.

Directly adjacent Responses updates reject; Messages repeated markers remain lawful.

No new format, task budget, `clear_at`, thinking mode or weaker content/explicit guarantee is selected.

[Source qualification](../refs/2026-10-27-client-ir-boundaries.md#ordered-effort-contract-qualification-2026-11-05) owns evidence; [Roadmap](../roadmap/unscheduled-issues.md#unscheduled-issues) owns remaining acceptance.

##### Google response-effort projection

The model-qualified `google.<model>.response-effort-to-thinking.v1` rules map response-scoped effort to Gemini thinking effort via [`ReasoningConversion::Scope`](../../crates/morphiecore-lowering/src/reasoning.rs).

Gemini 4 Argon and Carbon map low/medium/high; Hydrogen also carries xhigh as native `xhigh` on Interactions.

Gemini 4 planning records its unified context window rather than relabeling it as independent input capacity.

Sampling is ignored by these selected models; the existing named omission rules preserve that distinction.

#### Reasoning level calibration

Explicit hand-authored mappings define canonical model bindings. Under the [conversion ladder](#conversion-ladder), unsupported levels resolve to the nearest supported level (ties upward) and Reasoning/Response effort scopes convert at the same level. Fixed or ignored sampling controls (temperature, top-p, top-k) are omitted for models that do not support them through their feature disposition rows.

##### Thinking-budget quantization

When targets require discrete qualitative levels, numeric token budgets are quantized to discrete presets defined in [`ThinkingBudgets`](../../crates/morphiecore-lowering/src/reasoning.rs):
- Canonical presets are maintained in [`SHARED_BUDGETS`](../../crates/morphiecore-lowering/src/reasoning.rs) (6 discrete tiers spanning minimal to max).
- Selection matches minimal absolute token distance via [`ThinkingBudgets::nearest`](../../crates/morphiecore-lowering/src/reasoning.rs); exact ties resolve upward to the higher effort level.
- Current validation rejects requests whose thinking budget exceeds the client-specified maximum output token ceiling.

##### Bounded token-budget adaptation

Functionality may take precedence over conservative gateway-derived token defaults or presets through bounded, target-qualified adaptation. Prefer exact native controls; distinguish original client intent, derived allocation and actual model limits.

This direction does not select a general adjustment formula or change thinking-budget validation.

The [output-ceiling adaptation](#output-ceiling-adaptation) separately selects equivalent spelling, model-maximum clamp, narrower-scope approximation and carrier-less omission with loss evidence.

Other allocation changes must name the changed intent, target, finite bound, useful answer reserve and evidence; numeric thinking intent is not silently overridden.

Source protocol validity, model maxima, memory/wire limits, timeouts and terminal honesty remain enforced.

Unknown capacity is not unlimited capacity.

The [budget proposal](../proposals/index.md#independent-output-ceilings-unselected) owns unresolved alternatives.

##### Binding-level mapping

Bindings declare model-specific qualitative level translation via [`ReasoningEffortMapping`](../../crates/morphiecore-lowering/src/reasoning.rs) and [`ReasoningRequestPolicy`](../../crates/morphiecore-lowering/src/reasoning.rs):
- Conversions are strictly directional. A level missing from a binding's table resolves to the nearest supported level, ties upward, under the binding's named effort rule. Bindings that expose no effort control omit it under a named rule (L4); explicit history-read intent keeps its own guard.
- Upstream thinking budgets translate to qualitative effort via [`ReasoningConversion::BudgetToEffort`](../../crates/morphiecore-lowering/src/reasoning.rs), and qualitative effort expands to discrete token budgets via [`ReasoningConversion::EffortToBudget`](../../crates/morphiecore-lowering/src/reasoning.rs).

##### Implementation owners

- [`semantic::task::generation`](../../crates/morphiecore-semantic/src/task/generation/mod.rs): Activation and scoped effort types.
- [`protocol::reasoning`](../../crates/morphiecore-lowering/src/reasoning.rs): Quantization presets, nearest-level resolution, and policy projection.
- [`topology::catalog::bindings`](../../crates/morphiecore-catalog/src/topology/catalog/bindings.rs): Binding-level policy selection and limits.

### Media

Media resources (images, audio, video, files) are handled as inert payloads (base64 data or public URLs):
- The gateway does not fetch URLs or transform media inputs. Output encoding normalization is limited to [static image generation](#native-static-images).
- Chat video uses registered extension `video_url`; Responses video uses `input_video` in user messages.
- Instructions are strictly text-only; media in system/developer roles is rejected at intake.
- File references: Client-provided Provider file IDs are rejected. Only direct inline data or HTTPS URLs are admitted.
- Media compatibility and FFmpeg normalization: Clients use standard OpenAI requests across all media targets without private profile headers. OpenAI `gpt-4o-mini-tts`, `gpt-transcribe`, and `gpt-image-2.6-*` execute natively (including incremental SSE and `partial_images` previews); non-native targets normalize format gaps statically without resizing (`delivery.ffmpeg`) and emulate OpenAI SSE without fabricating missing token usage (`Speech`/`Transcription` emit chunked deltas + done; `Images` emits one `completed` frame with `n = 1`). Hard content/guarantee bounds remain L5-rejected (`mask`, excess `n`, exact `size` on Gemini/Grok, `background: "transparent"` on non-transparent/JPEG targets, unknown `voice`, `response_format: "text"` with `stream: true`); unsupported advisory controls (`speed`, non-Xiaomi `instructions`, `prompt`, `temperature`, `keywords[]`, `languages[]`, `quality`, `background: "opaque"`, `moderation`, `input_fidelity`, `partial_images` on static image targets, unsupported identity/language hints) take L4 omission.
- Image transformations: `oversized_image: "error"` sets `RejectOversized`; default allows upstream downsizing.
- Image detail: `auto` selects the target model's default processing, not a fixed resolution or token count. Preserve a native carrier when present; otherwise select the native default under `generation.image-auto-as-native-default.v1` (L2). Source IR retains explicit presence while only candidate copies normalize it. Selected unsupported non-auto values retain `generation.omit-image-detail.v1`. Discovery treats exact or L2 auto projection as faithful while retaining independent image, placement, format and bound checks.

#### Native static images

`/v1/images/generations` and direct-input `/v1/images/edits` use the standard OpenAI Images JSON or `completed`-only SSE shape across Qwen Image 3.0 Pro, Gemini Nano Banana 2.1 and Grok Imagine Image 2.0 (without private profile headers). When requested `output_format` (`png`/`jpeg`/`webp`) matches the upstream artifact and `output_compression` is unset, bytes pass through untouched; otherwise, bounded FFmpeg transcoding converts the format after upstream EOF without altering resolution. If the source did not report `created`, `images.delivery-created.v1` sets it to the Unix seconds when the gateway prepared the artifact for publication. Native accounting is omitted and never projected as OpenAI usage. Default GPT Image admission, preview SSE (`partial_images`), creation reports and usage remain unchanged.

| Target | Native wire and terminal |
|---|---|
| Qwen Image 3.0 Pro | Both Aliyun targets use synchronous native multimodal generation/editing (`content` ordered data-URL images + text) with inline PNG data URLs and `input_image_count` verification. |
| Gemini Nano Banana 2.1 | Google Interactions (`/v1/interactions`) with one `user_input` step wrapping ordered `image` content parts and `text`, `store: false`, and `response_format: {"type":"image"}`. Publish one inline image from `model_output` steps only after `"status": "completed"`, empty `errors`, and EOF. Text-only output, `thought` steps and incomplete interactions cannot satisfy completion. |
| Grok Imagine Image 2.0 | xAI `/v1/images/generations` or JSON `/v1/images/edits` (`image`/`images` data-URL objects) with `b64_json` and the named managed Grok OAuth account. Require one inline result with a supported MIME; do not recognize URL, deferred or stored results as inline success. |

| Field or behavior | Disposition |
|---|---|
| Prompt and ordered sources | L1: preserve prompt, declared input image order/MIME, and received artifact order. Edit accepts `multipart/form-data` direct uploads up to 16 PNG/JPEG/WebP images within an 8 MiB aggregate raw input budget (no URL fetching or `file_id`). Qwen accepts exact `n` (1–6) and exact size (`WxH` → `W*H`); enforce reported size and output count. Gemini and Grok accept one artifact with native size. Grok forwards `user`; Gemini and Qwen omit it (L4). |
| Edit mask and hard guarantees | L5: reject `mask` on Qwen/Gemini/Grok, `n > 1` on Gemini/Grok (`n > 6` on Qwen), exact `size` on Gemini/Grok, `background: "transparent"`, and `stream: true` with `n != 1`. Local validation enforces inert shape/MIME/bytes only, leaving pixel/alpha/dimension checks upstream. |
| Format normalization & `output_compression` | Convert static outputs to requested `png`/`jpeg`/`webp` (with `output_compression` on `jpeg`/`webp`) without resizing; pass through untouched when format matches and compression is omitted. `background: "transparent"` with `jpeg` rejects before dispatch. |
| Advisory controls (`quality`, `opaque`, `moderation`, `input_fidelity`, `safety_identifier`, `partial_images`) | L4 omission on static native image targets (`n = 1` with `stream: true` emits a single `completed` SSE frame omitting unknown `usage`). |
| Gemini's accompanying text and thought parts | `images.gemini.artifact-only.v1`: not part of the requested media result; do not serialize or replay them. They cannot substitute for an image. |
| Accounting | `images.native-static-artifacts.v1`: omit native accounting from the public artifact profile. Qwen validates image count, input count and dimensions; dimensions remain an artifact report. Grok retains validated token reports and exactly converts USD ticks in image IR before this projection. No native reports are relabeled as standard Images usage. |

Image, speech-output, and untrusted ASR-input normalization uses the configured `delivery.ffmpeg` executable with pinned demuxers/encoders, pipe-only I/O (or a bounded temporary file for seekable MP4/M4A inputs), fixed worker semaphores, audio duration caps (`-t 600`), image pixel caps, memory/address-space bounds, and the request's exchange deadline. Missing executables or over-budget outputs fail cleanly before dispatch (`503`) or without publishing invalid bytes.

See [source contracts](../refs/2026-11-23-native-static-image-targets.md) and [client activation](../usage/api-reference.md#static-image-generation).

### Adaptation evidence

Every lossy conversion or field omission generates bounded typed evidence. Internal authors pass `Field`, `ConversionBoundary` and `Change` enums directly; wire spelling remains at their serialization owner, without static-string parsing or panic tables.

#### Domain evidence interface

The conversion pipeline tracks adaptation evidence via [`Conversion<T>`](../../crates/morphiecore-lowering/src/conversion/generation/evidence.rs), pairing transformed payloads with a typed [`EvidenceDelta`](../../crates/morphiecore-lowering/src/conversion/generation/evidence.rs):
- **Body-free isolation:** Evidence records only structured [`Effect`](../../crates/morphiecore-lowering/src/conversion/generation/evidence.rs) items and rule IDs; user payloads, prompts, and tokens are strictly excluded.
- **Change categories:** Field modifications classify into canonical [`Change`](../../crates/morphiecore-lowering/src/conversion/generation/evidence.rs) families (`Normalize`, `Derive`, `Default`, `InferFromRequest`, `Omit`, `Approximate`, `Estimate`).
- **Bounded allowances:** Deltas enforce hard upper bounds of [`MAX_EFFECTS`](../../crates/morphiecore-lowering/src/conversion/generation/evidence.rs) (64 entries) and 16 KiB per delta. When saturated, an explicit truncation count is recorded without modifying business payloads.

OpenAI event encoders receive the final `TargetContract` at construction. There is no target-policy setter or publication-time target lock; identity-bound metadata updates and source lifecycle/terminal checks remain independent.

#### Atomic evidence and output batches

Transformations return `Result<Conversion<T>, ConversionFailure>`. Execution produces checked `EncodedBatch` frame collections, which may be empty. `Report::batch` accepts the associated `EvidenceDelta`; delivery acknowledgment and publication remain separate execution boundaries. Pending projection/coalescing state and deferred evidence remain with their owners. Dropping a batch does not acknowledge delivery.

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

Context/input seeds require exact service-qualified evidence, including official service directories, or an explicitly selected planning policy.

The selected SIWC GPT policy fixes Sol, Astra and Luna at 1,050,000 total tokens, matching the public OpenAI API; independent input/output declarations retain their service scope.

Every active candidate must declare a comparable value before discovery publishes a route-common minimum; otherwise that field stays unknown.

Origin/path/upstream-model replacements clear inherited context/input facts.

[Capacity sources](../refs/2026-10-26-model-discovery.md#capacity-sources) record the policy separately from observed service values and unresolved abbreviations.

**Native effort discovery:** effort `values` contains only route-common qualitative names preserved by the actual request projector on every candidate/delivery mode.

Scope-only substitutions may retain a native name; renamed/quantized levels and effort-to-budget presets do not become extra native choices.

Other effective accepted source names are published separately as `compatibility_values`.

Existing standard-client request conversion remains available, without a client-identification branch or weaker admission.

Discovery reuses the execution projection and its typed evidence rather than maintaining a second per-model level catalog.

The pi provider maps only native choices into its complete `thinkingLevelMap`; explicit null entries hide unsupported Pi levels, including aliases.

Pi owns thinking preferences and clamping.

Advertised total context and output maxima take priority over global client fallbacks; explicit per-model caps remain operator policy.

**Input-only planning:** when total context is unknown, pi may use the independently declared input maximum as its planning window, without publishing it as a total-context fact or adding output capacity.

If total context is known, preserve it even when a separate input limit is smaller; pi has no independent input-token-limit field.

Fresh catalog metadata reconciles the active model, including stale scoped selections, only at idle boundaries through public Pi APIs.

Offline/failed refresh retains the last qualified catalog; a prior catalog requires successful refresh before newly discovered facts can be known.

The gateway publishes each model's `preferred_interface` and each interface's `agent_defaults` and `delivery_adaptations`. One startup compilation derives the Generation contracts shared by models and `/capabilities` from the pinned public Agent-default corpus and the [conversion-ladder](#conversion-ladder) disposition tables; `/capabilities` retains other tasks. Default-request qualification checks the serialized ingress body against the configured request ceiling before semantic admission and candidate projection.

Preference follows only the primary candidate's native Chat, Responses or Messages correspondence, and only when that activated interface passes every assessed default text/tool/reasoning request, including SSE. Otherwise it is `not_assessed`, with null protocol/uniformity. The `basis` is explicit; `uniform` is true only if every candidate qualifies the same native correspondence. No fallback interface is ranked by conversion loss, and no candidate count, order or identity is disclosed. A body-hash `ETag` is allowed; no maintained revision is introduced.

Simple feature support, default-request disposition and full delivery/history support are separate claims. Explicit-strict synthetic tools do not stand in for omitted-strict Agent declarations. Keep unassessed/nonuniform facts and partial/composition coverage explicit. Publishing a preference does not establish that a client consumes it: integration must state whether application is automatic, explicit configuration or advisory. This neither permits capability-based candidate skipping nor changes routing priority.

Tool selection/history discovery uses the same candidate projector as execution for finite seriality, source-directional named cardinality, required allowed-subset and ordered-addition witnesses. The [usage domains](../usage/api-reference.md#generation-text-model-discovery) define their limits; a witness does not qualify arbitrary tool histories or live model behavior. The pinned Agent-default corpus is a reproducible artifact baseline, not an assertion that a same-version installed serializer is byte-identical; installed-client acceptance must recharacterize drift independently.

Effective control support and request admission are distinct projections of the same probe. An omitted maximum-output control is `unavailable` as effective support but an admitted named adaptation in `/capabilities`, not rejected input. Private per-candidate default qualification includes JSON/SSE independently of route-common delivery publication; matching another interface also requires the same primary endpoint and credential scope.

## Native Messages projection

### Request rules

- **System blocks:** Request-wide instructions compose into top-level `system` text blocks.
- **Mid-conversation system:** Subsequent instructions and configuration updates become mid-conversation system messages.
- **Output configuration:** JSON schema output maps to `output_config.format`.
- **Cache markers:** Preserves owner-local 5m/1h breakpoints.

The historical thinking-prefix requirement for manual thinking belongs to an actual native Messages candidate after projection, not public Client admission. A cross-protocol response may legitimately omit an uncarriable reasoning owner; its tool-result continuation must remain decodable. Common field, turn-order, tool-reference, signature and origin validation stays strict. Candidates that still use native manual thinking retain the prefix check before dispatch.

### Missing output ceilings

Go/Haiku Messages declares a binding default of 32,000 `max_tokens`, not a semantic or service maximum. Configuration can override or clear it. Only an omitted ceiling receives the default; explicit ceilings take the normal [output-ceiling path](#output-ceiling-adaptation).

### Owner-local cache admission

Owner-local breakpoints are admitted on every route. Candidates with native carriers preserve the supported controls; other candidates omit them under named rules at each owner, so no candidate is skipped and route membership is unchanged. Request-wide, tool-definition and content-part cache controls share the request disposition owner.

### Caller-selected automatic cache admission

Request-wide automatic caching is enabled on native Messages targets with verified support. Breakpoints are placed at eligible turn boundaries without exceeding the 4-marker ceiling.

### Response rules

- Messages delivery requires valid signatures on carried thinking blocks. Unsigned or multi-summary reasoning without a signed carrier is omitted under `messages.omit-uncarried-reasoning.v1`; malformed signatures still reject. Signatures are never synthesized and signed reasoning keeps its exact carrier.
- A zero-argument call whose arguments are an empty string delivers `{}` input (L2); other non-object tool input remains rejected.
- Block start order preserves source item ordering.
- Streaming argument fragments are buffered during whitespace prefixes to maintain valid JSON grammar for accumulators.

Trusted OpenCode Go/Haiku intake may close an open tool-input block at an explicit `max_tokens` message delta when the received bytes form a valid unfinished object prefix. Aggregate and incremental decoding share that closure, preserve exact bytes and identity, and mark the call incomplete without executable completion. The rule requires a started object, subsequent `message_stop` and strict EOF; public Standard Messages decoding retains its baseline ordering. It does not infer closure for complete JSON, other block kinds or unrelated terminals.

