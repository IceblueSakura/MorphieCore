# Architecture

MorphieCore translates client and Provider protocols through one typed semantic IR. It is a gateway, not an agent loop: planning, agent memory and tool execution belong to the caller.

This page records design principles and settled decisions. Code and tests define behavior; [Roadmap](roadmap.md) sets scope and direction.

## Contents

- [Protocols](#protocols)
- [Layers and data flow](#layers-and-data-flow)
- [Semantic IR](#semantic-ir)
- [Adaptation](#adaptation)
- [Reasoning replay](#reasoning-replay)
- [Compacted history](#compacted-history)
- [Chat reasoning extensions](#chat-reasoning-extensions)
- [Failover](#failover)
- [Streaming and delivery](#streaming-and-delivery)
- [Native Messages projection](#native-messages-projection)
- [Conversation and cache affinity](#conversation-and-cache-affinity)
- [Stateful history](#stateful-history)
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
| `adapter` | Requests with protocol provenance, replay isolation and codec dispatch | Trusted bindings select rules; no I/O or publication state |
| `topology` | Compile public model → route → endpoints | Endpoints fix origin, path, model and credential scope |
| `execution` | Candidate planning, preparation, intake and delivery state | No agent planning or tool execution |
| `transport` | HTTP I/O and SSE framing | Prepared trusted requests only |
| `credential` | Private store, API keys, OAuth and ordered pools | No credentials in the IR |
| `gateway` | Ingress, startup, budgets, routing and cancellation | CLI listener is loopback only |

Directories follow the Rust module tree. Keep architectural root boundaries; group internals by domain and expose deliberate entry points. Do not add compatibility-only aliases or a generic plugin framework.

Request transforms stay with their request owner. Stream grammar, state, reduction and materialization share a stream boundary. Chat and Responses own their envelopes and SSE codecs; only genuinely shared content and state logic is shared.

Protocol differences stay in protocol modules. Native Anthropic, Google and Interactions adapters do not borrow OpenAI switches.

Execution is typed dataflow coordinated by scoped state machines. Semantic reduction, protocol representation, attempt intake and acknowledged delivery each own their state. A stage may transform its output but cannot rewrite another stage's authority or expand the trusted candidate set. Optional delivery stages do not require a generic pipeline registry or detached workers.

## Semantic IR

The IR is neither a protocol DTO nor a union of vendor fields.

Generation semantics come from the official OpenAI, Anthropic and Google APIs. One official API is enough to justify a concept. Preserve its meaning in the IR even when a target needs a declared lossy projection; reject losses of protected semantics or cases without an approved adaptation. Provider dialects and adaptation policies belong to protocol codecs, not semantic types. Reasoning activation, numeric budgets and qualitative effort are not interchangeable. Realtime and standalone tasks do not determine Generation completeness.

- **Ordered items:** instructions, messages, tool calls/results, reasoning, compaction and Provider tool observations share one ordered history. A message is one container, not the whole conversation.
- **Explicit links:** calls and results link by call ID, never position or name. Item IDs, part IDs, wire IDs and stream indexes remain distinct.
- **Exact JSON:** arguments and schemas retain numeric precision and key order. Parsing rejects duplicate keys and bounds depth, nodes and bytes. A schema is any JSON object; keyword vocabularies and strict subsets are target constraints.
- **Meaningful presence:** distinguish absent, null, empty and default where semantics differ, not mechanically for every field.
- **Separate reasoning:** readable text and summaries are distinct from opaque continuation values. Effort, budget and display are controls.
- **Shared resources:** request/response resources have one declaration table and scoped references. A URL match does not merge identities or authorize I/O.
- **Owner-local replay:** typed replay belongs to its semantic owner. Resource identity, replay permission and runtime cache scope are different concepts.
- **Honest usage:** missing counts are not zero; cumulative snapshots are not summed; derived counts are not reported counts.
- **Honest terminals:** response closure, artifact completeness and turn progress are separate. Failure, truncation and cancellation never become success.
- **No runtime state:** targets, credentials, retries and publication state stay outside the IR.

Clients may resend full history or use [gateway-managed Responses history](#stateful-history), which materializes the same full IR and uses the same pipeline. Cache affinity is not history storage.

When a concept cannot be expressed, show a minimal counterexample and structural options before changing the IR. Do not hide it in private adapter fields.

## Adaptation

### Accurate IR and declared loss

The selected policy favors useful conversion over exact wire correspondence. Preserve source intent and observed facts in the IR; prefer an exact target carrier, then an explicitly approved omission or approximation on a target-private copy. Missing fidelity alone need not reject a request. This is the migration contract, not a claim that current codecs implement every rule; see the [implementation plan](roadmap.md#ir-and-codec-migration).

Classify losses by their effect on task constraints, output quality, execution and security, not by whether a field is optional in an SDK. A verbosity preference may be omitted when no target carrier exists. A required output schema, tool selection, call/result link, content item or honest terminal cannot be discarded merely to complete a request. Billing, storage and disclosure controls also need an explicit policy even when they do not change answer quality.

Cache grouping keys are advisory. Explicit cache retention constrains storage, so preserve its exact carrier or reject a target without one; automatic caching does not establish a compatible retention policy. Null request cache-options containers are invalid, independently of nullable upstream echoes.

| Treatment | Contract |
|---|---|
| Exact representation | Preserve meaning; check source conflicts before alias normalization |
| Advisory loss | Omit selected verbosity preferences or predicted-output optimization through named rules; other fields need individual classification |
| Behavioral approximation | Use selected effort/budget, summary-granularity and role mappings on target copies. Preserve visibility and output ceilings; preserve supported sampling and omit fixed/ignored sampling only under a target policy. Record changes and validate target restrictions |
| Protected meaning | Preserve task content, constraints, execution identity, security boundaries and result truth; reject when no faithful carrier exists |

Decode according to the source protocol before target projection. Known types, variants and relations remain strict even when a later target may omit the field. A known but unsupported critical field is not an unknown field. A target profile's restriction must not prevent the client decoder from expressing a valid source concept already supported by the IR.

Keep source validation, acceptance with approved adaptation, and faithful target support distinct. Project before checking the final target representation; do not advertise an omitted feature as native support. Capability assessment belongs to IR/codec contracts, not to a new route-ranking or fallback policy. The [temporary routing freeze](roadmap.md#temporary-routing-freeze) permits integration to remain disconnected while these contracts change.

Request controls, upstream-reported settings and downstream delivery have different authority. Sent controls do not prove effective settings. A response-only echo can be omitted under a selected delivery rule without discarding generated content; validate source facts and repeated snapshots before that omission. JSON, SSE and retained delivered history must agree on the applied loss.

### Selected request approximations

D3 selects named omission of Chat `prediction` until a faithful carrier is implemented. Validate its known source shape before omitting the optimization, and record the omission rather than treating it as an unknown field. Predicted content is not a constraint on generated text. Neither output text nor reported accepted/rejected prediction counts may be synthesized from it; loss receipts do not contain the predicted body.

P8 permits approximation of summary/display granularity, such as concise or detailed summary intent to a target's single summary mode. Preserve the requested show/hide behavior independently: hiding must not become disclosure, and requested display must not be silently disabled. An absent preference remains absent. These control approximations do not enable reasoning, fabricate summaries, discard generated content or remove required replay. Reject a target that cannot preserve the visibility contract.

P8 also permits named, target-specific approximation among developer, system and user roles when an exact carrier is unavailable. Preserve the original role and authority in source IR; project only the target copy and record the source/target roles. Preserve content and the relative order of existing history. This does not permit inventing assistant/tool speakers, repairing orphan results or accepting an invalid source role.

Request-wide `instructions` may use a leading role-bearing message when the target has no dedicated instructions field. For Chat, a trusted target-local selector chooses `developer` or `system`: prefer developer for targets that support it, and declare system for compatibility fallback. Validate the selected carrier before encoding; an unsupported carrier still rejects. This is an explicit representation choice, not model-name guessing, client-controlled policy or an upstream-error-triggered retry.

Chat already supports system/developer/user messages, so this selector applies only to the leading message generated from request-wide instructions. It neither moves existing ordered instruction items to the front nor rewrites their explicit roles; those follow exact carriers or a separate selected role rule. Keep instruction text unchanged and emit it once. Trusted `chat_system_instructions` selects system; the default remains developer. Keep the selector in protocol/binding policy, outside semantic IR and the public request schema.

P8 keeps image detail and file detail separate. Prefer exact carriers. Omit an explicit detail only when the selected target's default is proven equivalent, or apply a named, directional approximation backed by evidence for that target. An absent carrier alone does not justify omission, and explicit `auto` is not automatically equivalent to absence. Undeclared or unsupported conversions reject. These rules do not permit dropping media, transcoding it or moving it to another role.

P8 protects the model-visible filename. Preserve it through an exact carrier, including OpenAI `filename` and Messages document `title`; reject a target without one when a filename is present. Do not omit the name or inject it into surrounding text. An empty name remains distinct from absence.

P8 permits omission of an explicit reasoning-context control only when the selected target's default is proven equivalent. Otherwise preserve its exact carrier or reject, including when equivalence is unknown. Keep absent controls absent. This policy does not permit deleting messages, necessary reasoning replay or compacted history to simulate a context setting, and does not apply to a Messages document's `context` field.

P8 rejects `DefaultStrictAsPlain`. Keep omitted normalization-default strictness distinct from omitted non-strict defaults and explicit `strict: false`. Responses normalization-default tools must not become plain native tools; reject when their source semantics cannot be preserved. This decision does not select a new schema-normalization implementation or permit removal of explicit schema constraints.

These are selected policies, not claims that all bindings support every conversion. Concrete mappings and default-equivalence checks use trusted target policy with source evidence, not client-selected fallback logic or upstream-error discovery. Native targets reject uncalibrated detail/context controls and filenames without carriers; a named rule alone does not establish approval.

### Adaptation evidence

Every selected loss produces bounded typed evidence with a stable rule, direction, semantic field category and action. Safe control summaries show enum values and presence; numeric request summaries allow only thinking budgets, representative preset tokens and output-token ceilings. Keep request intent, sent controls, upstream reports and delivered facts distinguishable. Evidence cannot restore omitted values, change semantic meaning or select an upstream.

Reuse the existing diagnostics and protocol projection records rather than add a logging platform or a generic rule engine. Record losses once per affected control or aggregate repeated event occurrences within existing budgets. These summary records do not contain bodies, prompts, schemas, arguments or opaque values; full business content belongs only to the separately controlled capture below. Arbitrary input strings and unknown field names are not automatically safe summary labels.

Evidence belongs to a successfully checked projection boundary. A rejected batch adds no new receipts; earlier successful prefixes retain theirs when later intake or encoding fails. Late-reported facts follow the same rule. Native grouping and accounting transformations record evidence before their source ownership is removed. Encoding evidence alone proves neither network transmission nor effective Provider behavior.

Required output evidence is established by explicit request controls, the source contract or continuation requirements. Preserve it or reject an incompatible target. Selected incidental settings echoes and auxiliary reports may be omitted after source validation, with evidence. Absence of a request flag alone does not prove that citations or other content are incidental; deferred citation support remains deferred.

Chat omits incidental settings echoes because its response has no settings carrier. Responses omits unsupported independent reasoning-control echoes while retaining representable settings. Strict-complete delivery rejects either loss. These rules never drop generated reasoning, probabilities, citations or continuation values. Messages rejects unsupported probabilities and citations; its retained usage uses the same explicit cache/total view as delivery.

### Configurable exchange capture

P6 selects configurable full business-message capture for this internal gateway, not just field summaries. Keep independent header/body controls for four boundaries: inbound client request, outbound upstream request, inbound upstream response and outbound client response. Body capture includes JSON, streamed application content, tool arguments/results and non-authentication opaque payloads when enabled. Capture is not a source of replay or semantic restoration.

Capture is opt-in and off by default. Use private local storage, restrictive file permissions and finite queue, per-exchange and aggregate retention limits. Record redaction, truncation, dropped records and incomplete streams explicitly; do not label a partial record as complete. Keep the current best-effort availability contract: capture failure does not change the business response. Enabling content capture does not authorize remote export or printing captured business content into tool output.

Credentials are excluded even in full mode. Redact Authorization/Proxy-Authorization, authentication cookies, API keys, token-bearing query values, known authentication body fields and credential material before enqueueing or writing. Configuration cannot disable this step. If a capture unit cannot be sanitized safely, omit it and record the reason. Complete business capture is not byte-for-byte credential-bearing wire capture.

Capture belongs at gateway/transport I/O boundaries; codecs remain pure and only return bounded typed adaptation evidence. Stream incrementally within existing lifecycle bounds, sanitize at safe parsing boundaries and do not buffer the whole response or wait for storage before publication. Associate capture with an exchange-local identity and existing attempt ordinal without changing routing, retry or cancellation.

The startup-owned [capture configuration](usage.md#private-exchange-capture) selects four independent header/body boundaries. Authenticated client traffic and Generation upstream I/O use a private exclusive-create writer. Static upstream JSON borrows the existing intake buffer; SSE uses bounded event framing. Unsafe serialized-argument fragments and URI headers are omitted explicitly, not logged as raw strings. Frozen standalone-task upstream capture remains outside these hooks.

Capture is normalized application evidence, not a wire archive. Source EOF is distinct from semantic completion, body handoff and retained completeness. Queue, exchange and aggregate limits are finite; observed drops and writer failures remain queryable even when storage cannot retain a status record. No capture failure changes request acceptance or permits replay.

### Protocol baselines and Provider dialects

Use two codec responsibilities in each direction:

```text
decode: wire → dialect normalization → protocol baseline + declared extensions → IR + fidelity
encode: IR + eligible fidelity → target baseline + declared extensions → dialect representation → wire
```

This is the selected boundary, not a claim that all existing codecs implement it. The baseline is one official API and its selected source revision, not another cross-protocol IR. Keep Chat Completions, Responses, Messages, generateContent and Interactions distinct. Model restrictions and Provider aliases do not redefine the baseline.

A dialect may normalize an equivalent alias, with conflict checks against the canonical field. A declared semantic extension enters typed IR through an explicit decoder; it cannot hide required content in a restoration-only sidecar. Non-semantic reports and representation records stay bounded, source- and owner-bound outside the semantic task. Unknown fields never become restorable extensions.

Trusted adapters select dialects. Apply the same extension admission to requests, static reports, streamed events and returned history, while retaining their direction-specific unknown-field policies. Registered MorphieCore downstream extensions remain explicit extensions, not additions to an official schema.

Encode from the current typed meaning. A source extension cannot override canonical output, restore a deleted owner or cross an incompatible target. Dialect representation precedes final serialization; there are no post-encoding JSON patches. Projection need not be invertible: named approximations remain losses, not equivalent mappings.

Introduce this seam incrementally around concrete mappings. Do not add a universal protocol DTO, arbitrary extension bag, plugin registry, second stream reducer or whole-stream buffering. Existing framing limits, conflict validation, EOF-gated terminals and replay authority remain unchanged.

### Reasoning activation and effort scope

The IR separates activation from a scope-qualified effort control. [Reasoning types](../src/semantic/task/generation/content/reasoning.rs), [settings](../src/semantic/task/generation/request/settings.rs) and their codecs own the contract; target-specific integration limits are in [Roadmap](roadmap.md#integration-handoff).

Put activation on `ReasoningRequest` and one optional `EffortControl { scope, level }` on `GenerationSettings`. The initial scopes are `Reasoning` and `Response`. One scoped control is sufficient for the selected contracts; do not add two parallel effort fields without a counterexample requiring both at once. Keep protocol-relevant absence and null separate from active intent.

Activation distinguishes disabled from enabled reasoning; enabled permits reasoning and does not promise a nonzero token count. Execution/allocation mode, numeric budget, summary, display and replay stay separate. `EffortLevel` contains only positive levels: native off spellings normalize to activation, not a minimum effort. An omitted activation does not invent enabled behavior.

Disabled activation plus response-wide high effort is valid IR. Disabled activation plus a positive reasoning-only effort, adaptive/budgeted allocation or numeric thinking budget is contradictory. Standard/pro execution mode is not an activation switch; validate its combinations against the target contract rather than banning it solely because reasoning is disabled. Validate official wire variants separately before constructing this contract.

OpenAI non-off effort and Google levels have reasoning scope. Anthropic `output_config.effort` has response scope and must not change reasoning-object presence by itself. Preserve source scope in the IR. P1 selects target-private cross-scope approximation through trusted directional policy with loss evidence; equal level labels are not an equivalence. Concrete target mappings must be declared, including identities, and undeclared entries reject rather than silently inherit a same-label conversion. A binding without a selected policy retains rejection.

P2 gives explicit disabled activation priority when the target can represent off but cannot represent independent response-wide effort. Preserve off and omit only that effort through a named rule. A target with no off carrier still rejects; the approximation never enables reasoning.

Requirements and target contracts must distinguish response-wide effort from reasoning support. Reported settings use the same scoped types without acquiring request intent that was not reported. No Provider identifiers or mapping tables enter semantic types, and no compatibility-only effort field survives the migration.

### Reasoning level calibration

The existing level interface performs explicit, hand-authored calibration for the same model under trusted binding/dialect selection. It is not a universal adapter for all reasoning controls. P1/P2 select scope approximation and disabled-priority handling; P3 selects preset-based numeric-budget conversion inspired by [pi's pinned implementation](refs/2026-10-15-generation-projections.md#pi-thinking-budget-presets). These are design decisions, not claims of implemented support.

Different Providers may expose different labels for one model. A selected rule may map `low` to `medium`, for example. Qualitative level-to-level calibration still requires explicit pairs; equal labels or ordinal proximity do not establish equivalence. P3 separately selects nearest-budget quantization as a named numeric approximation, defined below. Preserve original numeric intent and validate the target's mode, minimum and output ceiling.

P4 preserves supported sampling controls. A named target policy may omit controls known to be fixed or ignored by that model. Unknown effectiveness does not justify guessing or dropping them; keep the rejection until a valid carrier or explicit target policy exists. Protocol grammar and model-effective support remain separate.

Encode and decode mappings are directional. A many-to-one encode mapping has no automatic inverse; normalization of an upstream report needs its own explicit rule. Preserve request intent and reported effective settings separately, never synthesizing a reported setting from the request. Apply each mapping once at its boundary, record its named provenance, and validate the result against the target contract.

#### Thinking-budget quantization

The selected shared preset uses six distinct budgets across the positive OpenAI/IR level vocabulary. It follows pi's preset-based approach but deliberately differs from [pi's default values](refs/2026-10-15-generation-projections.md#pi-thinking-budget-presets). These are MorphieCore approximation settings, not official OpenAI token equivalents or universal target limits:

| Level | Thinking tokens |
|---|---|
| none / disabled | Activation only; not a positive budget entry |
| minimal | 1024 |
| low | 2048 |
| medium | 4096 |
| high | 8192 |
| xhigh | 16384 |
| max | 24576 |

A trusted target policy selects this shared preset or an explicitly declared target-specific table. Duplicate token values are valid. For a positive numeric budget `b`, choose the entry with the smallest absolute token distance `abs(b - tokens(level))`; among every entry with that minimum distance, choose the highest level in `minimal < low < medium < high < xhigh < max`. Table iteration order must not affect the result. Use overflow-safe integer distance, not floating-point midpoints.

For a target-specific table `{low: 500, medium: 500, high: 1000, xhigh: 1000}`, budget `750` selects `xhigh`. An exact `500` selects `medium`, and exact `1000` selects `xhigh`; duplicate budgets can intentionally make a level round trip non-identical.

The shared preset's upward tie boundaries are `1536 -> low`, `3072 -> medium`, `6144 -> high`, `12288 -> xhigh` and `20480 -> max`. Exact preset values retain their level, including `8192 -> high`, `16384 -> xhigh` and `24576 -> max`. Arbitrary numeric budgets remain lossy: the representative tokens do not replace the original source value.

Absent controls stay absent. Off is decoded into activation before matching; protocol-specific zero/automatic sentinels never enter positive-budget quantization. If a target can preserve the requested native numeric budget, prefer that carrier instead of quantizing unnecessarily. Qualitative requests with an exact target carrier likewise need no budget round trip.

Retain the original budget and record the selected table, matched level and representative tokens as adaptation evidence, not reported model effort. Quantization does not bypass scope or target-level admission, invent a missing table entry, activate a binding or change route policy.

P3b rejects a request when its applicable explicit output-token ceiling is smaller than the thinking budget. This applies to both client-supplied numeric budgets and budgets generated by a selected level-to-budget conversion. Preserve the budget and ceiling; do not lower the budget, select a cheaper preset, raise the ceiling or disable thinking to make the request fit.

Passing this comparison is necessary, not sufficient. Equality is accepted only if the target contract permits it; stricter budget/output relations, protocol minima and required answer room still apply. Reject a target-invalid combination without automatic adjustment. No extra gateway-defined answer reservation or budget-reduction policy is selected. Ordinary preset lookup and nearest-budget quantization remain unchanged.

Rules cannot expand accepted content types, bypass model restrictions, select a route or modify the source IR. No public request override is selected. Mapping data remains hand-authored Rust policy, not a new startup mapping schema or generic rule engine. P6's diagnostic switches configure capture, not semantic mapping.

#### Initial mapping plan

Select a policy for one canonical model and trusted endpoint/protocol binding, not for every model of a Provider. It declares a stable rule identity, effort scope, accepted native levels and independent encode/decode tables. Use finite typed pairs or match arms; off, absence, null, budgets, display and execution mode are not table entries.

The following is a hypothetical reasoning-scope encode policy, not an activated binding or an assertion of model equivalence:

| Canonical level | Endpoint A accepts medium/high | Endpoint B accepts medium/high/xhigh |
|---|---|---|
| low | medium, explicit calibration | medium, explicit calibration |
| medium | medium, identity | medium, identity |
| high | high, identity | high, identity |
| xhigh | reject until an explicit mapping is chosen | xhigh, identity |
| minimal/max | reject until explicitly supported or mapped | reject until explicitly supported or mapped |

For this example, decode explicitly declares identity on each native level: a reported medium remains medium, even when the request was low. A separately authored decode rule may normalize it differently; never infer that rule from encode. Every handled entry, including an identity, must be declared. Missing entries reject, with no supported-level fallback. Do not chain table entries or apply the encode table to an already adapted candidate.

The code interface is [ReasoningEffortMapping](../src/protocol/reasoning.rs): hand-authored encode/decode pairs, effort scope and declared native levels selected by `ApiKeyBinding.effort_mappings` for an exact protocol member. Endpoint construction carries the policy into request encoding and static/SSE metadata intake; adapter selection validates its canonical-model identity. Bindings without a selected policy use their protocol's representation rules, not a generated mapping.

Activation is outside the positive-level table, so a table cannot turn off into on. Absence and null never create a level. The interface does not add Provider-only level labels to the standard IR.

Selected policies calibrate settings and ordered configuration items independently. Responses intake checks native configuration snapshots across added, done and terminal boundaries before normalization. Request projection maps a private ordered copy; a configuration budget without an item carrier rejects. Always-reasoning bindings also reject disabled configuration history. A settings policy must not silently bypass historical native-level admission.

Validate duplicate keys, output levels, effort scope and binding identity at the trusted target boundary. Existing topology compilation also checks these declarations; its policy is frozen for this migration. Preflight and preparation project afresh from unchanged input through the same pure codec path when connected. Calibration precedes the final supported-level check, but cannot bypass source validation or final target restrictions.

Validate native types, alias conflicts and source-snapshot consistency before a many-to-one normalization can hide a disagreement. Responses intake compares repeated reported settings before effort calibration, separately from item-snapshot validation. Normalize only fields actually reported. Static and streamed metadata use the same rule, and downstream delivery does not reapply the source Provider's encode map.

#### Implementation owners

| Owner | Planned responsibility |
|---|---|
| [Semantic reasoning types](../src/semantic/task/generation/content/reasoning.rs), [settings](../src/semantic/task/generation/request/settings.rs), [requirements](../src/semantic/task/generation/requirements.rs) and [contract](../src/semantic/task/generation/contract.rs) | Activation and scoped effort only; no model-specific level policy |
| [Protocol reasoning helper](../src/protocol/reasoning.rs) | Explicit qualitative mapping pairs plus separate P3 preset lookup and nearest-budget quantization with highest-level ties; no inferred qualitative inverse |
| [Catalog binding declarations](../src/topology/catalog/bindings.rs), [catalog construction](../src/topology/catalog.rs), native catalog constructors and [endpoint compilation](../src/topology/endpoint.rs) | Select policy by canonical model and binding, validate it and pass it through representation caps to the adapter; do not reuse a Provider-wide switch for model-specific levels |
| [OpenAI adaptation](../src/protocol/openai/dialect/adaptation.rs), [request projection](../src/protocol/openai/adapter/request.rs), [Messages projection](../src/protocol/anthropic/semantic_request.rs), [generateContent projection](../src/protocol/google/request.rs) and [Interactions projection](../src/protocol/interactions/request.rs) | Preserve scope and apply a selected level policy once to candidate-private typed representation before final validation and serialization; no Provider-wide level calibration switch |
| [Upstream adapter](../src/adapter/upstream.rs), [static response intake](../src/protocol/openai/static_response.rs), [event intake](../src/protocol/openai/events/decode.rs) and native decoders | Carry the selected decode policy into actual reported-setting parsing, including SSE; do not put policy in `ReplayTarget` or discover it from a response model string |
| [Fidelity](../src/protocol/fidelity.rs) and existing named request rules | Record bounded normalization provenance and actual encode adaptation, without restoring unsupported controls or synthesizing effective settings |

Native `Endpoint::adapter()` currently carries profile/replay labels rather than all representation caps. Propagate the selected reasoning policy explicitly into native intake when implementing this plan; storing it only in request caps would leave decode uncalibrated. The ordinary downstream client decoder uses its declared client contract, not a guessed Provider policy chosen from business JSON.

Keep [candidate preflight](../src/execution/plan.rs) and transport free of mapping tables. Preflight already invokes endpoint encoding, so preparation must reuse that same codec path rather than add a second execution-layer mapper.

### Field handling

| Class | Handling |
|---|---|
| Must preserve | Preserve instructions, order, tools, arguments, call/result links, required continuations, stop reasons and core usage; reject an unrepresentable target |
| Degradable | Apply an approved advisory omission or behavioral approximation to a target-private copy and record it; never silently patch the source |
| Unknown | Ignore unregistered protocol fields; keep known fields strict and never forward unknown input |

Upstream responses are strict on known types, values and relations. Unknown fields are ignored. Unknown Responses event types are skipped and counted; registered content and terminal events remain strict.

Downstream generation requests ignore unregistered fields; diagnostics record their count, not untrusted names or values. Unknown content or role discriminators are rejected because ignoring them would lose content. Protected known semantics that a target cannot express return an error. Trusted configuration and frozen-task requests stay closed.

Parsing direction is explicit: client input, upstream reports and encoder checks have different policies. Nested parsing inherits that direction; no ambient or thread-local policy is used.

### Media

Image, audio, video and file resources are inert: inline data or a URL, never fetched, decoded or uploaded by the gateway. Each kind is explicit, because a URL source carries no media type. An inline file keeps its declared MIME, and targets that dispatch by MIME may carry it.

- Chat video uses the registered extension `{"type":"video_url","video_url":{"url":…}}`, shared by OpenRouter, DashScope, Zhipu, vLLM and ModelBest. Responses video uses `{"type":"input_video","video_url":…}` from Open Responses and OpenRouter, in user messages only. Only targets whose dialect declares the carrier receive video. Responses clients may send `input_video` in tool results, but no bound target carries tool-result video: OpenAI and Interactions results have no video part, and Gemini function responses admit only PNG, JPEG, WebP, PDF and plain text.
- Native codecs reject explicit image/file detail without a declared carrier or selected target rule, and filenames without exact carriers. No target-default equivalence is inferred from a missing slot. Inbound options with no IR slot that change model-visible content, such as a Messages document `context`, remain rejected.
- A file name is the model-visible document name: OpenAI `filename` and Messages document `title` map to each other. A present name needs an exact carrier; do not drop it or inject it into text.
- Instructions may carry image and file parts. Only Responses system and developer messages have a carrier; other targets reject them rather than move media to another role.
- Tool results may carry media of any kind. Admission needs tool-result media and the kind's input modality. An inline file whose MIME type is image, audio or video also needs that modality, in addition to file input. Responses carries tool media; standard Chat does not, so Chat targets reject it.
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

Carriers include Responses `encrypted_content`, Chat encrypted reasoning details, Anthropic thinking signatures and redacted thinking `data`, Gemini `thoughtSignature` and Interactions signatures.

Redacted thinking is an opaque-only reasoning item with its own replay format, so the native block type survives the envelope. Messages clients receive it as `redacted_thinking` whose `data` is the envelope; a streamed reasoning block opens on its first delta or at its end, once a redacted value can be recognized.

Google signatures travel as reasoning items. The IR has no part- or call-level replay slots; Provider-tool replay stays separate.

- A generateContent value names its anchor: call ID, thought or tail. The encoder restores it by anchor, not position.
- A signed thought closes its owner. Later thought text starts another item, so adjacent signed parts never overwrite one another.
- An Interactions value is a thought signature or a call-ID-anchored signature.
- Signature-only items trail the response, so a message stays contiguous with its calls. Chat carries them ahead of the message.
- Preserve every signature, even when an upstream sample accepts an omitted one.
- generateContent and Interactions are separate formats. A value of the other format is dropped as foreign, not translated, so failover between the two bindings loses reasoning instead of failing.

## Compacted history

A compaction item owns opaque context at one position in the ordered history. It is not a readable summary, reasoning or a message with an invented role. Responses preserves its identity and `encrypted_content` in input, static output and streamed item snapshots. Only a completed opaque value can enter replay history; an unfinished stream cannot become a static compaction item.

Compaction uses the [replay envelope](#reasoning-replay) with a distinct format tag. The gateway restores the native payload only for the matching Provider/model and Responses carrier. Bare, malformed, foreign or unbound values are rejected, never stripped: unlike optional reasoning, removing compacted context would lose history. Chat Completions, Messages and Google targets reject it in the absence of a carrier.

Compaction admission is separate from reasoning support. Codec support does not activate existing model bindings. This contract receives and replays compacted history; it does not schedule compaction, expose `/responses/compact` or store conversations. The [official guide](https://developers.openai.com/api/docs/guides/compaction) and [pinned SDK](references.md#pinned-sources) define the selected wire contract.

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

Streamed readable deltas carry both readable views and no empty opener. Client input does not rely on returned indexes or format labels because target clients merge fragments.

For registered plaintext views, absence, null and an empty string make no text assertion. Nonempty duplicate views must agree with each other and with the readable structured view. Malformed known values still fail. An empty details list has no parts; explicit structured parts, including empty text, retain their content, order and owners. This normalization does not change visibility controls or relax post-terminal lifecycle checks.

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

### Optional delta coalescing (planned)

Status: design selected; not implemented or configurable yet. The goal is fewer downstream events, repeated envelopes and body handoffs without changing semantic content. It does not reduce upstream traffic or token usage, and fewer handoffs do not imply proportionally fewer network packets.

```text
upstream decoding and original-event validation/reduction
  → attempt-local intake with terminal withheld until validated EOF
    → streaming: client projection and prepared event context
      → optional bounded delta coalescer → client encoder → body delivery
    → final-only: existing materialized response → client projection → JSON delivery
```

Final-only delivery already supports a declared forced-stream upstream. It uses the existing authoritative reduction and final response, not an infinite coalescing window, an event log or a second full-response accumulator. Unexpected upstream SSE remains subject to the endpoint's declared delivery contract.

#### Ownership and equivalence

| Owner | Responsibility |
|---|---|
| Upstream decoder and semantic reducer | Validate every original event and retain existing byte, event and semantic limits |
| Client projection | Prepare each event once, in order, with stable identity, metadata, fidelity and replay context |
| Delivery-local coalescer | Keep one bounded adjacent run for one attempt; no network, clock source, credential or complete response state |
| Gateway intake | Drive reads, monotonic-time flushes and body handoff in the existing task; retain deadlines, backpressure and cancellation |
| Client encoder and body | Generate downstream sequence numbers, validate target state and preserve publication/acknowledgement gates |

Successful streams must be equivalent after normalizing delta segmentation: ordered content, identities, probabilities, annotations, replay, usage and terminal outcome remain unchanged. Event count, sequence numbers, padding and timing need not match byte-for-byte. Failure paths preserve published prefixes and honest failure; cancellation is not a promise to deliver buffered data.

Preparation and encoding need a separate seam. Do not buffer raw events and later call the existing combined operation against newer attempt metadata or fidelity. Retain the context needed by the pending run, or flush it under its original context before an update. Budget retained context as well as text; avoid copying a full sidecar per delta. Never rerun stateful native projection during flush. Existing target-encoder validation remains in place.

#### Initial merge contract

The first implementation merges only adjacent, nonempty `Delta` fragments of `PartKind::Text`, with the same projected item/part identity, unchanged encoding context and empty logprobs. Concatenate exact strings; never sample or keep only the latest fragment. Other deltas, including reasoning, tool arguments, audio and opaque signatures, pass through unchanged.

Every noneligible event is a flush barrier. This includes resources, annotations, probability snapshots, final text metadata, usage, progress, part/item closure and terminal events. Empty deltas also remain barriers until their observable equivalence is established. Keep only one pending run, not independently buffered queues per part.

The coalescer has empty, pending and closed states. An eligible event starts or extends a pending run; a barrier or size/time threshold emits that run before subsequent events. Cancellation or abandonment discards its state. No state crosses attempts, clients or requests.

#### Scheduling and failure boundaries

1. Preserve the first content-bearing batch's existing publication path without intentional coalescing delay. Do not release held lifecycle frames while hiding that batch in a new buffer. Existing pre-content fallback holding remains a separate policy.
2. Start a fixed flush deadline when a run becomes pending; later fragments do not reset it. An actual timer must flush during upstream silence, and processing multiple events from one network chunk must not starve it. Existing elapsed-time checks on event arrivals are not such a timer.
3. Flush before exceeding the pending-byte limit. Account conservatively for JSON escaping and envelope overhead so merging cannot make individually valid events exceed target frame limits. Original upstream limits are unchanged.
4. Await the existing bounded body handoff. Do not read ahead in a detached task to bypass a slow client. The flush deadline bounds intentional buffering, not downstream/network blocking.
5. Publication forbids advancement; acknowledgement advances the delivery lifecycle. Do not flush merely to manufacture commitment or create a new fallback permission. A failed unpublished attempt loses its buffer before legal advancement; cancellation discards it. On a published classified failure, handle already validated pending content in order before the failure frame when delivery remains possible; malformed intake still cannot create a success terminal.
6. Normal completion drains pending content before terminal encoding, but the terminal itself remains gated by validated upstream EOF. Final-only delivery, stored response snapshots and replay retain their existing authority.

The policy belongs to trusted startup configuration and defaults to disabled. Enabling it requires finite maximum waiting time and pending-byte bounds; choose and validate conservative bounds during implementation rather than claiming an unmeasured universal tuning value. It is not an IR field or a downstream request extension. The implementation sequence and verification gates are in the [roadmap](roadmap.md#optional-delta-coalescing).

## Native Messages projection

Native requests preserve thinking, tool-use/result order and call IDs. Messages owns its profile, cache controls, limits and session headers.

### Request rules

These describe current native behavior, not completion of the [loss-policy migration](roadmap.md#ir-and-codec-migration). A richer reasoning conversion requires a model-bound trusted request policy; absent policies retain rejection. Default-strict-to-plain conversion remains rejected by decision. Selected P8 behavior follows the [request policy](#selected-request-approximations).

- Disabled and adaptive activation have distinct carriers. This selected profile rejects numeric thinking budgets instead of replacing them with effort.
- Response-wide effort maps to `output_config.effort` independently of thinking. Reasoning-only effort is rejected, not broadened to response-wide effort.
- Summary intent maps to summarized or omitted display only with a representable activation/allocation. Display alone does not enable thinking. Uncalibrated reasoning context rejects; this profile's sampling omission follows its fixed/ignored model contract.
- Client `thinking.display` is preserved independently of effort and budget, using the same display parser as native intake.
- A leading developer instruction maps to the system prompt by name.
- Call IDs outside the native grammar map to stable digests; collisions fail rather than merge calls.
- Each native request uses top-level ephemeral cache control. Cache hits still depend on upstream thresholds.
- A JSON-schema output format maps to `output_config.format`, which always constrains decoding. Its name is dropped by a named rule; a JSON-object mode or a schema description has no carrier and is rejected.
- A source without an output cap gets `max_tokens: 32000`, the default of opencode's Messages client, because Messages requires the field.
- OpenAI stateless controls are not forwarded as native fields. Requesting an encrypted reasoning carrier selects delivery, not native thinking.

Exact rules live in [semantic_request.rs](../src/protocol/anthropic/semantic_request.rs).

### Response rules

Native observations stay unchanged. OpenAI delivery projects a copy:

- Chat requires leading thinking, then text, then calls; it rejects unrepresentable interleaving. Responses retains independent item order.
- Structured tool arguments become exact JSON strings; call/result links remain intact.
- Max-token/context limits are truncation. Refusal after content is incomplete. Unsupported server-tool continuation is rejected.
- Cache-inclusive usage is computed only from complete reported operands; missing usage is not invented.
- Delivery generates required IDs/timestamps only on its own copy, stable within the response. They are not upstream facts; a stored Responses snapshot uses its gateway ID as a local history key.
- Text, thinking and tool arguments stream incrementally. Full signatures are enveloped after block validation.

Messages delivery rejects citations until that carrier is implemented. Static and streamed output use the same refusal to discard annotations.

Messages final usage requires reported input/output counts and a known accounting relation. Cache-exclusive input is preserved. Inclusive input needs a reported cache-read count. An unreported cache-write category stays absent rather than zero, and the remaining input keeps any such writes. With both counts reported, at least one must be zero: the IR does not promise that nonzero cache-read and cache-write subsets are disjoint. Missing or ambiguous counts fail projection instead of becoming zero. SSE opening counters are provisional; they never replace the required final report.

## Conversation and cache affinity

The explicit `X-MorphieCore-Conversation-Id` header takes precedence over agent session headers. See [Usage](usage.md#conversation-and-cache-affinity) for accepted headers and constraints.

The gateway derives Provider-specific cache/session carriers; it does not read Provider session headers as conversation authority. Standard `prompt_cache_key` is a separate cache-grouping intent.

Without a declared conversation, a short-lived prefix-digest index offers best-effort cache-group reuse. It stores no bodies, selects no credential and restores no history. Opaque-reasoning histories are excluded.

Implicit cache scope is isolated by ingress key, credential source and public model. It is independent of the replay fingerprint. Losing the index loses cache hits, not correctness.

## Stateful history

Downstream Responses offers stateless full-history requests and gateway-managed `previous_response_id` continuation. The gateway retains canonical request input and the delivered response projection, with replay envelopes intact. The Responses codec materializes their ordered items with new input before ordinary IR validation and candidate preparation. Storage facts stay in protocol metadata, outside Generation IR and upstream execution hints.

Storage defaults to true, matching OpenAI's Responses default. Omitted or null `store` retains the new response; explicit `store: false` opts out. A request may read a stored predecessor without storing its successor. Top-level instructions and other settings come from the new request, not its predecessor; instruction items in history remain history.

The synchronous API includes response retrieval and deletion and cursor-paginated input items. Only fully delivered terminal responses become visible. Unknown, deleted and expired references have the same not-found result. Stored branches own complete snapshots: deleting or expiring an ancestor does not invalidate an already stored descendant. Retrieval is JSON only; there is no event log or stream resumption.

History lives in bounded process memory with a TTL. Expiration or restart invalidates stored references; there is no disk durability or restart recovery. Other storage backends require a later decision, not a prebuilt backend framework.

Capacity accounts for typed payloads, retrieval bytes and owner records. A final snapshot reserves capacity before its terminal frame is queued. The body publishes it synchronously at the final acknowledged handoff, so an immediately following request can resolve it. Dropped or failed delivery releases the reservation. TTL starts at publication, reads do not extend it, and expired entries are reclaimed on store access. Unexpired entries are not evicted.

Upstream calls remain stateless, using full history and `store: false` where supported. Neither Provider-only nor hybrid Provider-held continuation is planned. Downstream storage intent never enables upstream storage. Cache affinity remains separate from history ownership.

The scope is conversation history management, not execution persistence. No `background: true`, detached generation jobs, scheduler or agent loop is planned. JSON and SSE requests keep the existing cancellation and honest-delivery rules.

## Credentials and security

Credentials come only from the application's private store. A manager owns API keys, OAuth identities and ordered pools. Inference borrows access for a fixed identity; it never logs in or refreshes.

Ingress accepts one configured token via Bearer or `x-api-key` and authenticates before body collection. Business data cannot select an origin, credential or auth/proxy header. Inbound headers are not passed through.

Transport uses trusted prepared requests, no redirects and no implicit HTTP retries. Environment proxies require explicit startup opt-in. Credentials, locators, private config and upstream bodies never reach clients.

## Settled decisions

| Decision | Consequence |
|---|---|
| One semantic core | Gateway and future agent library share the IR; callers own agent behavior. Beyond context edits and response continuation, agent-only machinery joins the IR when the library needs it |
| Official Generation vocabulary | OpenAI, Anthropic and Google define semantic concepts; dialect carriers and approximation policies stay in protocol modules. Semantic expression, codec support and activated bindings are separate claims |
| Two codec responsibilities | Normalize Provider dialects into an official API baseline plus declared extensions before semantic decoding; encode in reverse before serialization. Semantic extensions enter IR; eligible non-semantic fidelity stays scoped outside it |
| Independent activation and scoped effort | Activation belongs to reasoning; a single scoped effort control belongs to generation settings. Response-wide effort is not reasoning-only effort, and off is not a positive level |
| Accurate IR with declared lossy projection | Prefer exact carriers, permit approved advisory omissions and behavioral approximations, and reject loss of protected meaning. Preserve the source and record target-local changes; implementation follows the roadmap |
| Explicit same-model level calibration | P1 permits declared directional scope approximation on target copies; level mappings stay model-local. No inferred nearest level, inverse or off-to-on conversion |
| Disabled activation priority | P2 preserves a supported off carrier and names omission of independent unrepresentable response effort; a target without off support still rejects |
| Preset-based thinking allocation | P3 uses six distinct shared budgets from 1024 to 24576; target-specific tables may contain duplicates. Numeric conversion minimizes absolute token distance and chooses the highest level on every tie. Preserve original intent and hard caps; this is explicit lossy quantization, not an inferred qualitative inverse |
| Thinking budget and output ceiling | P3b rejects explicit or conversion-generated budgets above the applicable client output ceiling. Preserve both values; stricter target relations still apply, without automatic reduction or cap increases |
| Target-effective sampling | P4 preserves supported controls and permits named omission only for declared fixed/ignored behavior; unknown effectiveness is not permission to discard a control |
| Required and incidental output evidence | P5 preserves required output facts and allows selected incidental-report loss after validation; request intent never fabricates upstream reports |
| Configurable internal exchange capture | P6 permits opt-in business headers/bodies at all four I/O boundaries with separate controls, finite retention and best-effort writes. Credentials are always redacted; no switch permits credential-bearing logs |
| Summary granularity and visibility | P8 permits approximate summary granularity while preserving show/hide intent. Do not enable reasoning, fabricate summaries or expose hidden content |
| Role and instruction approximation | P8 permits named developer/system/user mappings on target copies. A trusted Chat instruction-role selector uses developer for supported targets or system for compatibility fallback, before encoding and without error-triggered retry; existing ordered history stays ordered |
| Target-local media detail | P8 prefers exact image/file detail carriers. Omission needs proven target-default equivalence; other approximations need evidence-backed named directional rules. No blanket media-option omission or inferred auto/absence equivalence |
| Model-visible filenames | P8 requires an exact filename carrier, including filename/title mapping, or rejection when a name is present. No omission or automatic text injection; empty and absent remain distinct |
| Reasoning-context defaults | P8 permits omission only for proven equivalent target defaults; otherwise preserve exactly or reject. Do not simulate context controls by deleting messages, required replay or compacted history |
| No default-strict-to-plain conversion | P8 rejects `DefaultStrictAsPlain`. Omitted Responses normalization defaults are not non-strict tools; preserve their source semantics or reject, without removing explicit constraints |
| Predicted-output optimization | D3 selects named omission of Chat prediction after source validation until supported. Record the loss without changing generated text or fabricating usage |
| Registered reasoning-view presence | Absent/null/empty plaintext views make no assertion; nonempty duplicates must agree. Explicit structured parts and replay ownership remain intact |
| Equal downstream contracts | Chat Completions, Responses and Messages receive equal consideration for static delivery, streaming and history replay; a missing carrier needs an approved projection or explicit rejection |
| Separate task contracts | Generation, Embeddings, Images, Speech and Transcription are not one universal message type |
| Pure protocol boundaries | Encode from final typed input; no network in codecs or post-encoding JSON patches |
| Functional ownership | Keep root layers; organize internals by domain, without compatibility aliases or plugin frameworks |
| Layered capabilities | Source validity, acceptance through approved adaptation and faithful target support are distinct. Do not advertise lossy acceptance as native support or infer public promises from a union of candidates |
| Temporary routing freeze | P7 keeps routing, affinity, candidate policy and failover frozen until the selected Generation IR, codec and protocol-conversion work is complete. Only then reopen routing/integration design; incompatible integration stays explicitly blocked in the meantime |
| Shared model identities across protocols | Compatible API-key, OAuth and native Messages bindings share one public model and ordered route. Endpoint protocols and credential domains stay distinct; narrower candidates reject requests they cannot represent |
| SIWC cross-Provider fallback | A mixed route may contain one SIWC endpoint and advance to another Provider before publication within the route budget. SIWC pools remain single-account with no credential fallback; standalone SIWC routes remain single-attempt with fallback disabled |
| Case-sensitive model labels | Public IDs may preserve upstream ASCII capitalization, but resolve only registered routes. Provider prefixes remain rejected; Provider, endpoint and route IDs keep their lowercase grammar |
| Declared adaptation | Named candidate-local losses; known fields strict, unknown protocol fields ignored |
| Cache storage intent is protected | Cache keys may be omitted with evidence, but explicit retention needs an exact target carrier. Do not erase a storage constraint or repair a malformed request cache container through adaptation |
| Native carrier availability is not activation | Native `representation_contract()` describes codec carriers; default `semantic_contract()` keeps binding admission unchanged. Frozen public/topology checks may remain narrower until the scheduled integration review |
| Candidate-local tool errors | Messages admission preserves tool-result error flags regardless of the route. Native targets retain them; OpenAI targets apply a named loss on their private copy |
| Explicit downstream identity | A closed client protocol and its adapter own decoding and delivery carriers. Messages keeps its Chat-candidate activation policy without a dummy OpenAI adapter |
| Codec dispatch and execution | Adapters own upstream decoder and downstream encoder dispatch. Execution owns attempt identity, budgets, EOF validation and publication; candidate-transition rules are pure and independent of I/O |
| Dataflow and scoped state | Typed stages have separate semantic, protocol, attempt and delivery authorities; optional stages do not introduce a generic pipeline framework |
| Delta coalescing (planned) | Optional, disabled-by-default delivery optimization after client projection and before encoding. Final-only clients retain existing materialization; original intake validation and publication safety remain authoritative |
| Typed startup activation | One `TaskEntries` input selects Generation and each standalone task explicitly. Empty lists activate nothing; embedding does not discover catalogs, credentials or environment proxies |
| Official intake fields | Decode registered source semantics accurately. Unsupported preferences need selected omission rules; critical content and execution intent cannot masquerade as unknown fields. Optionality alone does not authorize dropping billing, storage or quality controls |
| Append-only history projection | Earlier content does not change when a later turn is added |
| Reported tool results | Responses carries function/custom results in static output and item snapshots. Missing call references stay unreported; request history still requires matching preceding calls. Chat and Messages reject ordinary tool results as assistant output, not their supported history forms |
| Scoped opaque replay | Restore only compatible native values; foreign readable reasoning becomes text |
| Compacted context is required history | A distinct ordered compaction item preserves opaque content and identity. Incompatible targets reject it rather than apply the foreign-reasoning strip rule |
| Failover, not balancing | Legal candidates, bounded advancement and exact-credential affinity |
| Conversation routing | No inferred turn, branch or message routing |
| Gateway-only history | [In-memory TTL history](#stateful-history) defaults to enabled for OpenAI Responses compatibility; `store: false` opts out. Continuation, retrieval and deletion remain synchronous, without Provider-held state or background execution |
| Honest delivery | Publication forbids advancement; success requires validated EOF and handoff |
| Agent-shaped failures | Sanitized error classes match pinned client behavior; refusals are not retries |
| Delivery-local identity | Generated client IDs/timestamps are not upstream facts. Responses allocates a gateway-owned ID; only a stored, fully delivered snapshot makes it a history key |
| Stateless Google Interactions | `store: false` and full history; no `previous_interaction_id` or revision header. Signatures keep their native formats |
| Separate credential lifecycle | Private file store and ordered pools; no inference-triggered login or refresh |
| Independent CLI paths | Default configuration lives in the home-relative application directory, with credentials in its `credentials` child. Explicit config and credential paths override independently; no legacy path discovery or automatic migration |
| Agent-sized bounds | Bodies, IR budgets and deadlines fit whole-history agent sessions and long turns: tens of MiB and one hour per exchange. Upstreams enforce their own maxima |
| Inert, credential-free media | Inline data or URLs only; no gateway fetch or file service, and no client-supplied Provider file references |
| Target carriers and protected content | Prefer official carriers or declared extensions. Selected role approximations change only the target representation; they do not invent content, assistant/tool speakers or call/result links. Do not drop task content or media merely to make a target accept the request |
| Target-local image restrictions | Formats, encoded sizes, counts and unavailable per-use options are checked or projected at the protocol boundary, not encoded as Provider-specific IR fields |
| Envelopes as grouping | Ordered message envelopes are the only IR grouping. Chat's call ownership is a view derived in the OpenAI codec, which also enforces Chat-only rules such as no calls on a refusal |
| Schemas as data | The IR keeps any bounded JSON-object schema an official API accepts. Each target checks its selected subset without rewriting constraints. Google protocols keep distinct structured-output shells; native strict-tool checks do not restrict client parsing. An unnamed output schema gets the fixed name `response` only on OpenAI targets, which require one |
| Strict tool constraints stay protected | Messages strict tools reject unsupported constraints and documented case-sensitive string enum/const fidelity gaps. Native request-wide schema complexity includes output-only schemas and combined strict-tool/output schemas. No SDK constraint-removal helper or implicit default-strict conversion is used |
| Deferred stop sequences | Nonempty client `stop` and `stop_sequences` are rejected, not treated as unknown fields. No stop control enters the IR until implemented; a native `stop_sequence` stop still reports a finished turn |
| Client-owned output caps | The gateway neither caps nor invents `max_tokens`; the upstream enforces its model limit. Only a protocol that requires the field fills a fixed default |

Pinned clients and protocol sources are in [References](references.md). Observed client behavior is an oracle, not permission to weaken security or fabricate semantics.
