# Architecture

MorphieCore translates client and Provider protocols through one typed semantic IR. It is a gateway, not an agent loop: planning, agent memory and tool execution belong to the caller.

This page owns accepted principles, domain contracts and decision records. [Roadmap](roadmap.md) owns scope, priority and acceptance status; [API expansion design](api-expansion-design.md) owns unselected alternatives. Code and tests describe implemented behavior, not authority over official contracts. Configuration and commands belong to [Usage](usage.md) and [Development](development.md).

## Contents

- [System boundaries](#system-boundaries)
  - [Protocols](#protocols)
  - [Shared Provider metadata](#shared-provider-metadata)
  - [Layers and data flow](#layers-and-data-flow)
- [Semantic contracts](#semantic-contracts)
  - [Semantic IR](#semantic-ir)
    - [Instruction authority and canonical roles](#instruction-authority-and-canonical-roles)
  - [Independent task boundaries](#independent-task-boundaries)
- [Adaptation](#adaptation)
  - [Source and target boundaries](#source-and-target-boundaries)
  - [Request and reported meaning](#request-and-reported-meaning)
  - [Reasoning controls](#reasoning-controls)
  - [Media](#media)
  - [Adaptation evidence](#adaptation-evidence)
    - [Domain evidence interface](#domain-evidence-interface)
    - [Atomic evidence and output batches](#atomic-evidence-and-output-batches)
    - [Typed conversion failures](#typed-conversion-failures)
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
  - [Conversation and cache affinity](#conversation-and-cache-affinity)
  - [Streaming and delivery](#streaming-and-delivery)
  - [Credentials and security](#credentials-and-security)
  - [Agent identity and cluster diagnostics](#agent-identity-and-cluster-diagnostics)
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

| Direction | Protocols |
|---|---|
| Downstream | OpenAI Chat Completions, OpenAI Responses, Anthropic Messages |
| Upstream | OpenAI Responses/Chat and compatible dialects, Anthropic Messages, Google generateContent, Google Interactions |

Generation supports JSON and SSE. Google upstreams use streaming intake even for downstream JSON delivery. Interactions uses stateless, full-history requests.

All three Generation client protocols use the public model's configured ordered candidates. A Messages client does not imply a native Anthropic upstream. Standalone Embeddings, Images, Speech and Transcription have separate contracts; only selected [stage-4 slices](roadmap.md#stage-4-scope) expand them.

**Accepted standards reset (2026-10-23).** Public Chat, Responses and Messages use their current official versioned baselines. Future generateContent and Interactions ingress has the same requirement, without activating those endpoints now. Provider-specific syntax and observed exceptions belong to trusted upstream adapters. The explicitly selected [Chat reasoning profile](#chat-reasoning-extensions) is a separately configured semantic extension, not the official Chat default or a Provider-selected dialect. The [safe-to-ignore diagnostic extension policy](#public-capability-and-adaptation-notice), selected on 2026-10-24, permits namespaced MorphieCore response fields alongside the official baseline. They are optional diagnostics, not official protocol fields, model-generated content or a replacement for standard semantics. Standard SDK Agent loops must work without interpreting or returning diagnostics.

Resolve current stable SDK/package releases and corresponding source revisions before the redesign. Record API versions, revisions and beta selections separately; an SDK superset is not a protocol-version contract. The [release-reset sequence](roadmap.md#standards-reset-and-redesign) supersedes the old mapping baseline as a regression oracle. Historical successful loops remain scoped evidence, not current conformance.

**Messages latest-version policy (selected 2026-10-24).** The latest stable official Messages API, explicitly pinned for each verified baseline, defines the downstream surface. Known removed fields reject at intake, even at former defaults; do not ignore, translate or forward them as compatibility input. Older upstream acceptance does not justify retaining removed Messages decoder/encoder paths, aliases or legacy-version branches. Remove obsolete mappings and their acceptance assertions rather than maintaining a version-compatibility layer. Shared IR controls still needed by other protocols remain; selected cross-protocol projection rules cannot restore removed Messages wire fields. Transport headers and Provider tolerance do not widen this public contract.

The selected HTTP baseline is `2026-10-21`, owned by the Messages protocol module. After authentication, Messages POST requires exactly one matching `anthropic-version` before body collection, including `?beta=true`; missing, duplicate, combined or unsupported values return the standard Messages 400 envelope without echoing header contents. Other APIs and method/resource errors do not negotiate a Messages version. Trusted upstream preparation and the manual Anthropic probe use the same fixed version, never an ingress-selected value or an error-triggered downgrade. Offline/SDK agreement is not live Provider acceptance.

Provider facts live in [providers.json](../src/provider/providers.json), validated by [provider/catalog.rs](../src/provider/catalog.rs). Public models and bindings remain in [topology/catalog.rs](../src/topology/catalog.rs) and [gateway bootstrap](../src/gateway/startup/bootstrap.rs). Registration does not prove activation or live account access.

### Shared Provider metadata

`src/provider/providers.json` is the single repository-owned source for Provider origins, operation paths, authentication and default credential-pool names. Both the gateway and `morphiecore-upstream` consume its typed definitions. The file is embedded with `include_bytes!`, validated once and retained immutably; editing it requires rebuilding. There is no external Provider-file lookup, merge policy or hot reload.

The top-level object is keyed by Provider ID:

| Field | Contract |
|---|---|
| `base_url` | Trusted origin only, without an API prefix, query, fragment or userinfo |
| `auth` | `{ "kind": "bearer" }`, `x_api_key`, `x_goog_api_key`, or `{ "kind": "oauth", "profile": "openai" \| "grok" }` |
| `credential_pool` | Default local pool name; never a secret, account alias or business-request selector |
| `endpoints` | Closed operation map: `models`, `chat_completions`, `responses`, `messages`, `generate_content`, `interactions`, `embeddings`, `images`, `speech`, `transcription` |
| Endpoint `path` | Full concrete path relative to `base_url`; omission of an operation means undeclared, not inferred |
| Endpoint `auth` | Optional override of the default header scheme; cannot change credential kind or OAuth profile |
| `sources` | Nonempty list of public HTTPS references; evidence starting points, not live acceptance |

Unknown fields/operations, duplicate JSON keys, unsafe paths and invalid authentication declarations reject. The catalog's OAuth profile must match its Provider ID. Metadata contains no credentials and grants no new model admission. API dialects, model capabilities and request controls stay in their current owners.

One resolver constructs Google's model-bearing path from its collection entry, validated model name and static/streaming choice. The gateway retains its fixed SSE query; other protocols use their declared paths directly. No arbitrary path template language is introduced.

Catalog model bindings derive their default credential locator from the same Provider metadata. Gateway deployment overrides still modify only a private compiled definition; they do not mutate the registry. Explicit library-created definitions may omit a default pool and supply their own trusted task declarations. Manual diagnostics use the default pool's first member only, whereas gateway candidate ordering and fallback retain their existing rules.

### Layers and data flow

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

Generation validation owns ordered dependency traversal and aggregate charging; `semantic/task/generation/validate/controls.rs` owns tool/output controls. Stream reduction consumes state, keeping global guards and private `reduce/items.rs` transitions in order; rejected streams cannot resume. Delivery, body acknowledgement and credential lifecycle remain separate owners.

Catalog binding families own declarations and endpoint/public-model construction. `topology/catalog.rs` assembles them in configured order.

Protocol implementation is organized by the wire responsibility being reviewed:

| Review boundary | Owner |
|---|---|
| OpenAI complete task requests, settings and history validation | `openai/{chat,responses}/request.rs`; the protocol `mod.rs` files expose the public task entry points |
| Typed Responses continuation | `openai/responses/{managed,history}.rs` assembles retained owners with new input; `fidelity/history.rs` transfers verified owner-local associations under private identity remapping |
| OpenAI message/item mapping reused by input, static output and events | `openai/{chat,responses}/items.rs`; these are not whole-request codecs |
| OpenAI shared shape admission and content construction | `openai/shape.rs` owns readers and field policy; `content/{intake,call}.rs` owns item construction and function-call mapping |
| OpenAI event state, poisoning, reduction and profile dispatch | `openai/events/{decode,encode}/mod.rs`; private Chat and Responses grammar/rendering modules operate on that same state, without a second reducer |
| OpenAI representability and candidate-local projection | `openai/lowering/{request,response}.rs`, with shared constraints in `checks.rs`, final encoding guards in `carriers.rs` and incremental checks in `events.rs` |
| Native Messages wire DTOs versus semantic mappings | `anthropic/messages/wire/` versus `anthropic/messages/{request,response,settings}.rs` and `anthropic/messages/items/`; native target binding is not client admission |
| Messages client admission versus delivery | `anthropic/messages/client/{request,response}.rs`; `client/mod.rs` exposes the deliberate client API, `client/projection.rs` owns delivery projection and `client/sse.rs` owns outgoing stream state |
| Messages shared accounting and terminal meaning | `anthropic/messages/{accounting,terminal}.rs`; native mappings and client JSON/SSE use these owners, not helpers belonging to another front end |
| Google request settings versus history content | `google/{generate_content,interactions}/request.rs` owns whole requests; each API's `items.rs` owns its history and media projection |
| OpenAI wire prefix fingerprints | `openai/cache_affinity.rs`; the disposable routing index and affinity decisions remain in `gateway` |
| Shared fidelity state, bounds and owner pruning | `fidelity.rs` owns the aggregate; private `fidelity/{replay,response_records}.rs` implementations group replay binding and output-only report associations |
| Public replay projection | `replay_projection.rs` produces wire copies; execution authorizes managed continuation and rejects unsupported stateless replay before projection |

Paths are relative to `src/protocol/`. Public entry points stay at protocol façades. Private helpers introduce no independent fidelity stores: fidelity has one aggregate and managed continuation composes typed history. Request transforms stay with their owner; stream grammar, reduction and materialization share a boundary. Chat and Responses own their envelopes/SSE codecs. Native Anthropic and Google adapters do not borrow OpenAI switches.

Execution is typed dataflow with scoped state machines, not a generic pipeline or detached workers. Stages may transform their output, never another stage's authority or the trusted candidate set:

| Owner | State authority |
|---|---|
| OpenAI event codec | Rejection invalidates accepted semantic state, retaining metadata, fidelity and successful-prefix evidence |
| Attempt intake | Withheld terminals belong to stream/finished intake; terminal publication requires strict EOF validation |
| Delivery projection | Encoded finalization is separate from rendered output and acknowledged lifecycle completion |
| Messages SSE | Checked native usage is separate from the published opening snapshot |
| Delta coalescer | One prepared text run retains its context and fixed deadline under the [merge contract](#optional-delta-coalescing) |

## Semantic contracts

### Semantic IR

The IR is the union of evidenced official protocol meanings, with equivalent meanings sharing one authoritative owner. It is neither a protocol DTO nor a union of vendor fields. One official API is sufficient evidence for a concept; representation and binding activation remain separate.

Generation semantics come from the official OpenAI, Anthropic and Google APIs. One official API is enough to justify a concept. Preserve its meaning in the IR even when a target needs a declared lossy projection; reject losses of protected semantics or cases without an approved adaptation. Provider dialects and adaptation policies belong to protocol codecs, not semantic types. Reasoning activation, numeric budgets and qualitative effort are not interchangeable. Realtime and standalone tasks do not determine Generation completeness.

- **Ordered items:** instructions, messages, tool calls/results, reasoning, compaction and Provider tool observations share one ordered history. A message is one container, not the whole conversation.
- **Text-only instructions:** request-wide and ordered application-instruction parts contain plain text only; [canonical roles](#instruction-authority-and-canonical-roles) distinguish semantic authority from legacy wire spelling. Reject media-bearing instructions at source decoding, including reported instruction echoes before any approved omission; do not drop media or move it to another role. Typed instruction parts retain identity and cache ownership without resource references. Request-wide instructions precede every ordered item. An instruction item after conversation content keeps its position; targets without an ordered carrier reject it rather than hoist it.
- **Instruction composition and grouping:** Messages and Google compose request-wide instructions with the contiguous leading instruction prefix in source order. Messages/generateContent keep separate text blocks; Interactions uses its named paragraph-join rule. Composition changes the target container, not retained root/ordered ownership. Native grouping splits at ordered instruction/effort/tool-change boundaries and preserves known containers; partial envelopes cannot absorb intervening owners, cross known boundaries or erase empty containers. Canonical role selection does not reopen paused Google work.
- **Role-scoped content:** input media belong to user turns and tool results; generated assistant media need their own output owner, not an input resource use. Citations and probabilities belong to assistant text, refusals to assistant messages. No official request carrier attaches them to user input, so validation rejects these states instead of leaving them to target projection.
- **Explicit links:** calls and results link by call ID, never position or name. Request program callers name a preceding program call; linked tool calls/results preserve the same caller. Response-only snapshots may remain self-describing. Item IDs, part IDs, wire IDs and stream indexes remain distinct.
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

#### Canonical ownership

The accepted redesign removes competing writable representations, not legitimate distinctions:

- `UsageReport.counts` holds aggregate `UsageCounts`; modality observations live only in `details.modalities`. Flat `Usage` values are derived operation/delivery views. Scalar intake shorthands split their modalities when constructing a report. Editing or deleting a breakdown entry cannot restore an older flat value. Sum disjoint modalities only within their reported bucket; tool-input overlap still needs its own premise.
- `ReasoningRequest.presentation` owns visibility and summary granularity. Hidden output is not a granularity enum; nullable summary preference is not an explicit hidden-output guarantee. Activation, effort scope, budget, readable text and opaque continuation remain distinct.
- `Part.cache` owns prefix boundaries as `RequestLifetime` or `Ttl(PrefixCacheTtl)`; tool-definition/call/outer-result/change owners retain their own explicit TTL fields. Request lifetime policy, maximum retention and automatic placement remain independent. Fidelity may retain representation evidence, never resurrect a deleted semantic directive. `ResourceDescription::Image.size_policy` owns input-image size enforcement per use, independently of shared resource identity and image detail. Allowing downsizing is permission, not a resize request or a numeric threshold.
- Keep request intent, opening/terminal observations and completed delivery views separate. Derivation and estimation never overwrite source facts.
- Keep runtime attempts and the execution-owned adaptation report outside the IR. Reports, logs and wire extensions are projections of that report, not competing semantic containers.

The [roadmap](roadmap.md#standards-reset-and-redesign) tracks remaining mapping and history migration. A representation record is not a second writable semantic owner.

#### Instruction authority and canonical roles

**Selected and migrated on 2026-10-25.** Use the latest reviewed and pinned official standard as the canonical contract, rather than preserving historical wire spelling as a separate semantic tier. Source pins still require explicit review; this is not automatic adoption of mutable documentation or every newly released feature. The [current/legacy/Provider research](refs/2026-10-23-stable-protocol-contracts.md#instruction-authority-research-current-legacy-and-provider-dialects) supplies the evidence and limits; [Development](development.md#verification-gates) owns executable evidence.

For the selected standard OpenAI profile, caller-supplied `system` is a legacy application-instruction spelling and `developer` is canonical. Both normalize to one application/operator instruction channel, above user input; neither represents the vendor-reserved system authority described in the Model Spec. Messages operator system uses the same application channel. Root placement does not create another authority tier. No new generic hierarchy engine or parallel raw-role semantic owner is selected.

**IR-level requirement:** application instructions have canonical Developer meaning inside semantic IR, not only when encoding OpenAI output. `Instruction` inherently owns ordered application instructions; the old `InstructionAuthority::{System, Developer}` distinction is removed without a single-variant replacement enum. `GenerationSettings.instructions: Presence<RootInstructions>` owns request-wide text parts and their cache boundaries, addressed within the root rather than as ordered-history identities. Root and ordered instructions retain separate scope/lifetime, not separate authority. This remains a union of current official semantic meanings, not a copy of OpenAI DTOs or permission to discard other protocols' distinct capabilities.

Provider deviations belong exclusively to codecs and trusted dialect projection. In particular, DeepSeek compatibility maps canonical Developer instructions to its effective system carrier on the candidate copy; it must not add a DeepSeek role, compatibility flag or alternative authority tier to IR, or change the standard codec's canonical developer output. A trusted native dialect decoder interprets actual source meaning before constructing canonical IR; spelling alone cannot promote effective user content. The dialect adapts to the IR standard, never the reverse.

Source decoding determines meaning once, independently of candidate targets. Validate the original source contract, including text-only content for standard application instructions; an alias does not admit malformed content, new roles or wider model capabilities. Trusted native DeepSeek Responses developer is effective-user content and admits its documented images, not arbitrary standard file/video parts. Preserve instruction text, item order, identities, participant labels, lifecycle and cache ownership. Original spelling, if needed for bounded codec evidence, is not a second authority owner and cannot force legacy output.

| Source protocol/profile | Accepted instruction representation | Canonical semantic meaning |
|---|---|---|
| Standard OpenAI Chat | Explicit `system` or `developer` message | Ordered application instruction; canonical explicit OpenAI output is `developer` |
| Standard OpenAI Responses | Explicit `input` system/developer item | Ordered application instruction; canonical explicit OpenAI output is `developer` |
| Standard OpenAI Responses | Top-level `instructions` | Current-request root application instructions, not an ordered history item |
| Messages | Root system string or text blocks | The same current-request root application instructions in both forms; keep block/cache ownership. An empty block list normalizes to explicit empty root text |
| Messages | Admitted mid-conversation system | Ordered application instruction at its original position, subject to native scope and placement |
| An explicitly supported trusted Provider source profile | Documented dialect role | Decode that dialect's effective meaning, not its apparent OpenAI spelling; a DeepSeek Responses developer item means user, not application authority |

The last row does not activate a Provider-specific downstream endpoint. Public Chat/Responses ingress uses its standard profile; neither a requested target/model nor business content selects a source dialect. User, assistant and tool-result meanings remain distinct. Application↔user authority conversion is not authorized by this policy.

**Mixed roles normalize in place.** A standard input sequence `system(A), user(U), developer(B)` becomes `developer(A), user(U), developer(B)` in canonical OpenAI form. Keep both instructions even when they conflict; do not merge, deduplicate, reorder, solve the conflict or synthesize `A > B` from the old system spelling. This is the project's selected canonical interpretation, not a claim of universal historical aliasing or identical mixed-role model behavior.

**Root and history lifetimes stay independent.** Managed Responses continuation takes root settings only from the current request. Omitted/null/empty root values follow the source contract and never delete retained ordered instructions. Projecting root instructions into a leading Chat developer message does not make them retained history; projecting an eligible ordered prefix into Messages root system does not make it disappear on continuation. With root `R` and ordered instruction `P`, a continuation omitting root retains `P`, not `R`. A client that explicitly resubmits an instruction message supplies ordered input; the gateway must not guess from its text that it was once a root projection.

| Target contract | Root application instructions | Ordered application instructions |
|---|---|---|
| Current standard OpenAI Chat | Leading developer message | Developer messages at their existing positions |
| Current standard OpenAI Responses | Native `instructions` for one unmarked text part; otherwise an exact temporary leading developer item when representable | Developer input items at their existing positions |
| Messages | Root system text/string or blocks; compose the eligible leading prefix without changing semantic owners | Mid-conversation system only at supported native positions; do not hoist later instructions |
| Reviewed legacy application-system target | Native root field if supported; otherwise a leading system message | System messages where the target supports ordered instructions |

Canonical OpenAI spelling also applies to explicit instruction items in supported history/retrieval surfaces and report conversions; it is not permission to rewrite generated assistant output into instructions. Prefer a native root field only when it preserves required parts and metadata. A plain string is not an automatic carrier for cache-marked blocks or participant labels. Use an already selected scoped projection or reject when protected content, cache, lifecycle or scope cannot be represented. Keep Messages tool-pair and placement restrictions; temporary `clear_at` remains unselected, not a permanent developer-message alias.

**Provider egress is a separate, trusted target mapping:**

| Reviewed target operation | Selected application-instruction carrier | Limit |
|---|---|---|
| DeepSeek Chat | System messages, including a leading message for root instructions | No assumption that its undocumented developer role works; no new model admission |
| DeepSeek Responses | Native `instructions` for representable root text; system input items for ordered instructions | Never encode application instructions as developer: the dialect treats that spelling as user |
| DashScope/Token Plan Chat under the shared Bailian dialect | System messages for supported application instructions | Preserve operation/model admission; QVQ's ignored system and other unsupported slots are not faithful carriers. The shared billing dialect is selected policy, not independently measured Token Plan normalization |
| DashScope/Token Plan Responses under that dialect | Native root instructions and explicit developer where the reviewed target supports them | Published role support does not establish every model or mixed-role behavior; effective target admission remains necessary |
| Zhipu Chat | System messages for root and ordered instructions | String content only; no implicit text-part composition |
| Zhipu Responses | Native root instructions and developer input items | Materialize `type:message` on message items; do not import Chat's role/string restrictions |
| NVIDIA hosted Chat, `nvidia/nemotron-3-super-120b-a12b` | At most one initial system message for application instructions | String content, no extra instruction fields; reject later/multiple instructions rather than move or merge them. This exact bound model/operation policy does not extend to NVIDIA Responses |
| Xiaomi Chat/Responses | Developer for ordered instructions; Chat leading developer or Responses native root field | Keep documented operation restrictions; do not infer unsupported server-side continuation |
| OpenRouter reviewed routes | Canonical developer and supported native root instructions, unless an evidenced trusted route rule selects another effective carrier | A compatible schema alone is not route-level fidelity; unassessed models/routes gain no new guarantee |

Other Provider-specific mappings remain unassessed, not implicitly selected by an OpenAI-compatible label. The mapping is selected by operation and effective target contract, not brand alone, model-name heuristics or an upstream-error-triggered retry. If no application-instruction carrier is established, reject that candidate instead of lowering instructions to user or adding prompt text to simulate priority. Source canonicalization is request-owned; egress adaptations affect only candidate-private copies.

The 2026-10-25 [Provider cross-check](refs/2026-10-23-stable-protocol-contracts.md#vercel-provider-instruction-cross-check-2026-10-25) selects conservative correction of the evidenced wire boundaries. Zhipu Chat, Bailian Chat and the exact NVIDIA Chat binding reject instruction arrays when their native carrier requires a string. Keep independently representable single-part owners in place; do not join their text, collapse root/history owners or erase cache/participant identity to make them fit. These checks run in the same encoder used by candidate preflight, before upstream I/O. No paragraph-join policy or broader NVIDIA schema conformance is selected. Go, ModelBest, Grok, OpenRouter and NVIDIA hosted Responses evidence gaps remain separate from these repairs and receive no new mapping.

Equivalent spelling normalization is not an authority-loss warning; obsolete developer-as-system approximation receipts are removed. Non-exact composition, scope or target behavior still requires its applicable named evidence, including Interactions paragraph joining; accepted role syntax alone never proves losslessness. Preserve source/profile attribution without duplicating source-normalization evidence on each candidate. This supersedes the earlier requirement to retain distinct legacy OpenAI role authority and the root-only Chat selector limitation, including use of the broad P8 role wording to justify application→user conversion. It adds no bindings, Google scope or live Provider acceptance.

#### Participant ownership

`Instruction.participant` and `Message.participant` own the bounded, model-visible label distinguishing participants of the same authority/role. One owner carries the label for all its parts and attached calls; tool names, call IDs, attribution hints and grouping envelopes do not own it. A named instruction followed by a named call-only assistant message illustrates why envelopes are insufficient: instructions have no envelope, while the empty assistant content message already owns its calls.

Chat request/history `name` maps exactly for system, developer, user and assistant roles. Absence stays absent, empty strings remain explicit, and supplied null/non-string values reject. No name grammar or 64-character tool-name bound is inferred from another field. Charge labels to the existing per-value and aggregate text limits. Retained owner IDs protect participant identity alongside authority/role/phase, including a call's effective label inherited through its message association. Initial construction establishes items and envelopes atomically; item/context edits and direct envelope replacement cannot reattribute retained calls, while unnamed regrouping remains possible. Reattribution needs a new owner, and automatic plain-message summaries cannot erase named participants. Client-managed selection preserves the label.

Other protocols have no selected exact carrier and reject rather than rewrite text or omit the label, including final Responses encoding and managed-history composition. This is request-history semantics, not a generated-output observation: response validation and stream snapshots do not acquire a participant report. If a caller adds a name to a manifest-bearing returned message, keep its visible named history and omit incompatible restoration with the existing receipt instead of reconstructing unnamed owners. No binding, route or response-schema expansion follows.

#### Tool examples and document context

Tool examples and document context have bounded IR and native Messages carriers. The reviewed Go/Haiku binding opens their separate requirements after the [bounded deployed checks](development.md#native-messages-input-acceptance). Configured public admission requires every selected candidate to opt into each feature on native Messages; generic profiles, other bindings and mixed routes remain closed.

Bounded, ordered input examples belong to `FunctionTool.input_examples`, including definitions introduced by `ToolChange`. They are model-visible input, not schema constraints or executable calls. Preserve absent versus an explicit empty list and exact object values/order without rewriting the schema or appending examples to the description. Messages validates root and by-value definitions before presence conversion, rejecting null lists and non-object elements. Both placements charge example trees to the shared request node budget as well as their byte and per-example bounds. The separate `ToolExamples` requirement prevents ordinary tool support from implicitly activating examples; other protocol encoders reject without a selected carrier.

The [source recheck](refs/2026-10-17-generation-contracts.md#anthropic-input-and-cache-recheck-2026-10-21) requires object examples and documents upstream schema validation. Existing `SchemaDocument` checks data shape and bounds, not instance validity. Keep these responsibilities explicit: no automatic schema/example repair, claim of local instance validation or new general validator follows from this owner selection. Native rejection remains an error, not a successful adaptation or reason for silent retry.

Document context belongs to each file use's `FileDescription.context: Presence<Text>`, not to shared resource identity. Two uses of the same source may have different contexts. Native Messages retains absent/null/empty/text through ordinary message and tool-result history without another instruction or adjacent text. `DocumentContext` admission stays separate from file support; unsupported targets reject rather than discard even explicit null. Context bytes remain redacted from Debug and participate in owner-bound report dependencies.

Context is model-visible text, not citable document content or an arbitrary JSON object. Handle absent/null/empty from the pinned source rather than truthiness; a nullable SDK type alone does not prove runtime equivalence. Examples/context edits change model-visible input and cannot preserve an unchanged-prefix claim merely because the underlying tool name or resource source is unchanged.

Messages carriers come first. A target without an accurate carrier rejects either concept; no omission or prompt-rewriting approximation is selected. Existing bounds, owner-edit validation, candidate isolation and history-prefix stability apply. These owners do not select document citations, tool discovery, hosted execution or File services.

#### Citation point anchors

`ClaimAnchor::Point(TextPoint)` holds a bounded insertion boundary in one text part, with an explicit UTF-8-byte or Unicode-scalar unit. It is distinct from whole-text and range anchors and from a reported list ordinal. Two citations may name the same insertion position without sharing a list position.

Map confirmed `file_citation.index` insertion semantics to that anchor only after resolving native units. The [source recheck](refs/2026-10-17-generation-contracts.md#openai-citation-positions-2026-10-21) confirms the existing `547ecb1` baseline but not its Unicode unit. Native JSON/history, annotation events and final snapshots reject file citations; output preflight also rejects the obsolete ordinal shape and unmapped points. `file_path.index` remains a distinct reported file-list index; neither its spelling nor event `annotation_index` makes it a text point. Its existing annotation-added SSE exclusion remains unchanged.

Static, event and final-snapshot representations must retain the same source/text-part association. Arrival-time checks bound the annotation; full-text position validation waits for owner closure, not the current text prefix. Text and resource edits follow existing dependency invalidation, not a stale position restored from fidelity. Synthetic Unicode tests do not prove native units. This local correction selects neither complete Messages citations nor source/quote extensions, Files or hosted search.

#### Responses item and event observations

An output `created_by` is a bounded, item-owned Responses report for function/custom results and compaction, not a caller, credential or execution authority. Its record binds the item and referenced resources, including model-visible filenames. Unrelated resources do not invalidate it; owner edits do. Repeated non-null actors must agree, including independently encoded events after publication. Missing later observations cannot erase a published actor. Request history has no actor field, so native input encoding never forwards it.

Summary interruption and compaction sampling progress are event facts. `PartInterrupted` closes a finished summary value without changing item lifecycle, response outcome or replay finality. Responses emits `reasoning_summary_part.done.status: "incomplete"`; other target protocols without a selected carrier reject. Repeatable `CompactionProgress` requires an open compaction owner and supplies no content or completion evidence. Static/history schemas have neither observation: do not invent fields or a retained event log, and do not claim that JSON retrieval can reconstruct their event history.

`ResponseError.original_type` retains a bounded original error classification on failed response snapshots. It is separate from the standalone SSE `type: "error"` discriminator; that event has no original-type carrier and rejects an independently populated classification rather than overwriting it.

#### Google foreground intake

On 2026-10-20 the user selected lifecycle IDs as optional source observations: `id` on created/completed resources and `interaction_id` on status updates may be omitted. Missing IDs neither invent nor erase a source fact. Supplied IDs remain strict: reject null, wrong types, empty strings and over-limit values. Bind the first real ID and reject later changes before accepting usage. The execution-owned HTTP attempt, not the availability of a source ID, owns the stream.

Creation may also omit status; a supplied opening status must be `in_progress`, never a default for absence. Started acknowledges native stream creation, not a reported execution state. Model is still required; no model fallback was selected. Update/final statuses remain required; updates accept the six public v1 values as observations. Other known opening states and unselected SDK status additions are unsupported, not malformed public enum values. Non-null continuation tokens reject at each actual owner, including status updates. Optional event IDs do not activate resumption. Service reports follow the separate [processing-class contract](#service-policies-and-reports). Function-call signatures follow the separately selected [attachment contract](#reasoning-replay).

Status observations, generated outcome/progress and delivery publication are separate authorities. On 2026-10-20 the user selected final-envelope authority: validate update fields and checkpoint usage without inferring a transition graph or equality with an earlier status. Do not retain a replayable status-event archive or emit semantic Progress/Terminal from updates, including `failed`/`cancelled` observations. The final envelope alone determines outcome/progress under the existing step/call constraints; strict EOF is required before publication. Interactions accepts an optional DONE sentinel only after the terminal envelope; it neither requires that extra sentinel nor treats it as completion authority. Neither a schema union nor `interaction.completed` alone proves success. This does not add background execution, remote cancellation or retry policy.

Supplied lifecycle `steps` reconcile against a bounded source-ordinal registry, including empty output steps that have no semantic member. Omitted/null steps make no snapshot assertion; an explicit list asserts the source-step count and order. Compare supplied content, call identity/object arguments and thought/call signatures against closed owners before terminal publication, without adding content, signatures or closure events. Optional content/signature omission does not erase earlier facts. Multipart content is unsupported at both step start and snapshot because deltas carry no content-part index; do not concatenate unknown boundaries. Non-null step errors remain unsupported. Source ordinals are not compacted semantic indexes, and reconciliation uses an ordered scan rather than repeated owner searches.

Both Google request encoders reject explicit tool execution reports: `is_error` cannot distinguish nonexecution, failure and successful execution. Ordinary results without an execution report preserve their error flag independently. Capabilities and direct encoder checks agree; no execution fact is inferred from the flag or result text.

generateContent preserves structured JSON values under its existing `functionResponse.response.output`/`error` envelope without stringification. Interactions preserves structured objects directly in `function_result.result`, including independent absent/false/true error flags; unsupported structured scalar/array carriers reject. Representation support does not open structured-result admission on existing model bindings.

An empty signature on an opening Interactions thought is an unfilled placeholder, not replay authority. Later nonempty signature deltas retain the existing protected owner and cannot overwrite a supplied signature. Empty call signatures, malformed signatures and contradictory terminal snapshots still reject.

generateContent retains bounded [terminal explanations](#explanatory-terminal-reports), but still rejects aggregate probability reports and model-specific token IDs; probability and token-identity expansion remain paused. Nonempty URL-context observations also reject; absent/null or empty URL metadata supplies no retrieval fact. Chosen-token probabilities require an actual number, including explicit zero; missing is not defaulted to zero. Sparse alternative-token facts retain their existing optional IR fields, and unknown token bytes stay unknown. See the [source boundaries](refs/2026-10-17-generation-contracts.md#google-foreground-contracts-2026-10-20).

#### Scoped accounting

`UsageReports` is the sole stored accounting source. It retains the latest bounded snapshot per operation or response-local execution step; steps never synthesize an operation total. Checkpoints replace their scope without summation. A final observation seals that scope but does not certify exhaustive billing. Content edits retain the reports.

`UsageReport` separates reported counters, accounting premises and sparse modality/cache/tool breakdowns. Missing counts remain unknown; source-defined default zeros are marked separately. Duplicate categories, overflowing known sums and breakdowns exceeding their own known parent reject. Sparse lists do not imply exhaustive coverage, and tool-request counts are not tokens.

Split input/tool-input premises also constrain two known counts of the same named modality, even when parent totals are absent: additional counts must add without overflow, and included counts cannot exceed their input counterpart. Unknown modality labels do not establish identity. Failed snapshot replacement leaves every scope and its byte charge unchanged; successful replacement clears omitted facts rather than merging old fields.

The existing `usage()` accessor derives an operation view from that source rather than storing independently mutable totals. generateContent has an explicit prompt + tool-input + output + thought relationship. Interactions retains these source buckets but leaves tool-input and total inclusion unreported because the official examples and generic descriptions do not establish one universal equation. Missing thoughts do not erase a reported non-reasoning output count; they leave the inclusive output view unknown.

Google modality reports and Interactions opening/status/step/terminal snapshots use this contract. `step.stop.usage` is cumulative operation usage; `step_usage` is local to the step. Unknown category/count fields in the optional Interactions breakdown shape remain unknown, not zero. The source contract and conflicting examples are recorded in [protocol notes](refs/2026-10-17-generation-contracts.md#scoped-accounting-contract).

OpenAI native encoding requires its established input/output/total relationship. Native reports without that premise may have no exact scalar view, even while the source retains counts. The named `generation.usage-report-view.v1` projection records loss of source scope, breakdowns or provenance on a delivery copy. Strict-complete direct codec projection still rejects report loss. Gateway delivery separately applies [schema completion](#downstream-schema-completion), without changing source reports or introducing custom SSE events.

Messages preserves cache-write TTL breakdowns, cumulative search/fetch request counts and thinking-token subsets. Input excludes disjoint cache reads/writes; thinking stays within output and is not subtracted into a purported visible-text count. Optional null deltas leave prior observations intact; non-null nested reports replace whole objects. Both children of the native TTL/tool-count object are required: sparse generic reports cannot invent the missing count. A reported TTL sum must equal its known parent, including after a parent update.

Native Messages JSON stores a final operation report; SSE emits opening/delta checkpoints and a final report at message stop, while successful terminal publication remains behind strict EOF. Client Messages delays its opening until the first usable accounting observation or content owner, without buffering content. TTL detail has only an opening carrier: later changed or newly supplied TTL detail rejects rather than entering a fabricated delta field. Tool counts and thinking counts use their real delta carriers. Exact native details survive JSON and SSE; other scoped details use the existing named delivery loss. OpenAI derivation preserves thinking-token counts while omitting unrepresented TTL/request breakdowns with receipts.

An absent delta field cannot erase an optional observation already published in opening usage; such delivery transitions reject. A final report with no usable scalar view cannot resurrect an older checkpoint. Static client projection records checkpoint-to-final observation sealing on its private copy, matching its JSON carrier; exact native static encoding requires a final report. Accounting checkpoints remain lifecycle-only for the existing bounded pre-content hold, not a reason to publish and disable failover.

#### Cached context references

`GenerationRequest` may name one existing cached input base, separately from settings and ordered items. The reference carries bounded opaque identity and an optional reported expiry with subsecond precision. It is required task context, not an advisory optimization. Item edits preserve it; history composition may share an identical base but rejects incompatible bases or retroactive introduction after selected input.

Boundary fidelity records associate the complete reference with a logical issuer scope. This is compatibility evidence, not resource authorization, credential selection or a trusted origin. A changed identity or expiry needs a new association. Known expiry requires an explicit caller-supplied clock and rejects at the expiration instant; missing expiry remains unknown, not unlimited retention. Pure codecs read no clock and perform no I/O.

The generateContent carrier encodes the bound reference without expanding its content. Cached instructions/tools cannot be overlaid implicitly. Other carriers reject without support; default binding admission and public HTTP schemas remain unchanged. Cache creation, fetching, renewal, resource authorization and cache-only prewarming are separate work. See [Google's resource contract](https://ai.google.dev/api/caching).

#### Safety policy and reports

`GenerationSettings` holds requested per-category probability filtering. Target default, disabled filtering and no category blocking are distinct choices. A caller identity hint is not a safety policy. Threshold names do not establish equivalent calibration or enforcement across targets or models.

`GenerationResponse` holds ordered `SafetyReport` observations independently of content and terminal details. Report events distinguish input from output assessment; they can represent blocked input without an output item. Successive assessments remain separate rather than overwriting or requiring agreement with an earlier assessment. Their arrival does not identify an exact content span. Reports never come from requested thresholds, and probability is not severity.

Report counts, ratings, messages and aggregate response storage use the existing semantic bounds. Content edits and native client-view reconstruction preserve reports. Report debug output is redacted. Reports cannot follow the terminal; an assessment alone never computes the outcome. Codecs translate explicit native blocking signals separately under the existing terminal contract.

The classic generateContent codec encodes probability policies and reads prompt feedback and candidate assessments. Omitted proto3 rating scalars use the [source-defined defaults](https://github.com/googleapis/googleapis/blob/8cc2927c2ea64e6876568683050deabf2b52ce56/google/ai/generativelanguage/v1beta/safety.proto), not guesses from the request. An empty native rating list is not an inferred safe verdict. Existing native envelope identity requirements remain unchanged; reports do not synthesize missing identities or models. This does not add Vertex severity/method carriers, Google static intake or Google client endpoints.

Other request carriers reject these policies. Downstream carriers have no assessment slot. An assessment-only report, with no block reason or message and every rating explicitly unblocked, is omitted under `generation.omit-safety-assessment.v1` in JSON and SSE. Blocking reports and safety-bearing settings echoes still reject rather than being treated as incidental settings; rejection prevents a later successful terminal. Default admission and public HTTP schemas stay unchanged; model-effective enforcement and blocking-report delivery remain integration work.

### Independent task boundaries

Standalone task types retain their own request, result and lifecycle contracts. [Operation proposals](api-expansion-design.md#proposed-ownership) describe pending expansion; they do not extend Generation execution.

#### Decision API direction

**Direction selected on 2026-10-23; not implemented.** Provide stable access through both standard TypeSafe and OpenAI Decision SDKs, with one typed Decision task and protocol codecs/dialects adapting mainstream upstream interfaces. The initial research covers TypeSafe, OpenAI, Perplexity and Cloudflare. Neither downstream family is merely an optional alternative to the other; no third custom public decision protocol is selected.

Keep the two official wire baselines distinct. Shared task meaning belongs in semantic IR; dialects describe evidenced deviations within a wire family, not model identity, transport authority or interchangeable statistical calibration. Decision is an independent operation, not structured-output Generation or gateway execution of an action. Existing [adaptation rules](#protocol-baselines-and-provider-dialects) still prohibit raw request bags, hidden critical loss and fabricated results.

The subsequent 2026-10-23 instruction selects a **capability-union design**, following Generation's ownership discipline: each distinct meaning has one interpretable typed owner, not parallel vendor fields or a lowest-common-denominator schema. Prefer exact carriage, then consider justified derivation, named approximation or simple estimation; reject when no acceptable projection exists. Preserve source intent and reported facts independently of delivery. This permits designing alternatives, not inventing probabilities, approving every lossy rule or activating the union on every target. The [concrete IR and projection proposal](api-expansion-design.md#decision-semantics-and-compatibility) still requires review.

This selection establishes the interoperability goal, not universal field/Provider compatibility, an approved IR layout, SDK release coverage or runtime activation. [Decision proposals](api-expansion-design.md#decisions) own unresolved endpoint namespaces, semantic mappings, result delivery and execution policy; the [roadmap](roadmap.md#decision-api-closure) owns its stage-4 closure and acceptance gates.

#### Token counting policy

Prefer upstream-reported counts for the selected model/target and effective input projection, preserving any documented estimate semantics. If those reports are known to be inaccurate, a documented coarse heuristic such as character-based estimation is allowed. MorphieCore does not perform tokenizer-based exact counting or reconstruct hidden model templates. This does not constrain how an upstream implements its own counting.

Keep upstream-versus-estimated provenance and approximation meaning explicit. A rough heuristic disagreeing with an upstream count is not itself evidence that the upstream is inaccurate. Estimates are not inference usage or billing facts; never use them to replace reported Generation usage. Only the separately selected [downstream presentation policy](#downstream-schema-completion) may complete absent delivery counters. One target's count neither reserves the next Generation route nor guarantees equality across fallback candidates.

Counting must not trigger generation. Concrete reliability criteria, estimation rules/input coverage and client-visible provenance remain part of selecting the [counting profile](api-expansion-design.md#token-counting). A text heuristic cannot silently treat non-text media as zero. This policy activates no endpoint or binding; implementation and acceptance are [deferred](roadmap.md#deferred-token-counting).

#### Independent Embedding path

**Implemented under the renewed authorization of 2026-10-23.** Embedding remains independent of Generation. Its task owns ordered samples, indexed dense vectors and source usage facts; the adapter owns client encoding/identity, optional reported model/ID and the separate delivery view.

```text
auth → bounded wire intake → activated model / trusted tokenizer scope → sample IR
  → one immutable PreparedEmbedding (target body, expectations, delivery policy, receipts)
  → credential resolution → one HTTP attempt → bounded complete native report through EOF
  → per-sample validation → delivery projection → encoding / byte limit → acknowledged handoff
```

Each sample owns its input and optional dimensions. Ordered text parts mean one joint input, not several independent inputs; token IDs carry a logical vocabulary scope. OpenAI global dimensions broadcast at decode, with no batch-default/local-override precedence. A global-only target rejects differing sample controls rather than splitting a request. Public text requests currently use one text part per sample. Other modalities, task prefixes, contextual chunks and quantized/tensor output remain unselected.

Only the trusted OpenAI-model binding admits `cl100k_base` IDs: ordinary ranks 0–100255, no holes or special/control tokens, at most 8192 IDs per sample and 60000 in aggregate. Integer carriers acquire their scope before entering the IR; aliases do not identify a tokenizer. No tokenizer execution, token-ID-to-text conversion or token-counting endpoint is introduced. Text token limits are enforced upstream, not estimated from bytes.

Value invariants live in the [task](../src/semantic/task/embedding.rs); transport ceilings live in the [adapter](../src/adapter/embeddings.rs); model admission and delivery rules live in [lowering](../src/lowering/embeddings.rs). The [execution plan](../src/execution/embeddings.rs) carries trusted routing/auth inputs outside the IR. Codecs stay pure. Default dimensions resolve once for both preflight and result checks without injecting an explicit wire control. The output-value budget is the sum of effective per-sample dimensions. Source results retain order and must cover all sample indexes exactly once; dimensions are checked by index, not array position. OpenAI responses additionally require uniform widths.

Routes preserve embedding-space identity: equal dimensions or model labels do not prove compatible vectors. There is one target and one nonfallback API-key source, no Generation-style failover, retry, hidden fan-out or batch splitting. Request/response bytes, numeric counts and exchange deadlines remain independent bounds. Failed upstream heads preserve status/Retry-After and release immediately rather than waiting for an optional error body. Success still requires complete EOF, cancellation-safe release and acknowledged body handoff.

##### Embedding encoding compatibility

All four float-JSON/Base64 report→delivery combinations are supported. Encoding is a wire preference, not task semantics. Prefer matching native output when supported; compatible upstream profiles also accept another valid known representation, never malformed binary data.

| Upstream vector → downstream encoding | Policy |
|---|---|
| Float JSON → float JSON | Preserve exact reported JSON numbers |
| Base64 → float JSON | Decode finite binary32, widen exactly to binary64, then use round-trip JSON formatting |
| Base64 → Base64 | Preserve finite binary32 bits using canonical standard padded little-endian bytes |
| Float JSON → Base64 | Round original decimals directly to binary32, nearest/ties-to-even, under `embedding.float-json-to-binary32.v1` |

`ExactJson | Binary32` retains one authority per vector. No decimal→binary64→binary32 double rounding, clamping, nonfinite values or subnormal flushing is allowed. Reject overflow and nonzero-to-zero underflow; preserve signed zero. Conversion occurs on delivery copies before publication. Native numeric storage is not narrowed merely to satisfy another client's encoding.

##### Google Embedding compatibility

Google uses native synchronous `batchEmbedContents` for stable `gemini-embedding-2`, including single-input batches. Each entry carries nested `embedContentConfig` with `autoTruncate:false` and optional dimensions; native order establishes sample indexes. The selected text profile is default 3072 dimensions, range 128–3072 and a 100-input ceiling, intersected with the shared 60000-value budget. A nonempty tensor `shape` is not a dense vector. Neither the Google OpenAI-compatible endpoint nor native downstream Google ingress is activated.

The user extended explicit ancillary compatibility to activated Embedding profiles on 2026-10-23, then selected complete OpenAI response schemas with approximate usage when reported facts cannot support derivation. Keep strict native OpenAI parsing/encoding as a distinct codec boundary. Optional source model, response ID, input/total counters and modality details remain immutable reported facts; only the separate delivery view can derive, estimate or omit unsupported details.

| Rule | Delivery or request projection |
|---|---|
| `embedding.bound-model-label.v1` | If source model is absent, use the bound public model as a delivery label, not an observed serving revision |
| `embedding.omit-response-id.v1` | Omit an ancillary native ID unsupported by the selected client envelope |
| `embedding.input-only-total-from-prompt.v1` | Derive total only from a reported input aggregate and an attested input-only basis, as in Google |
| `embedding.input-from-input-only-total.v1` | Interpret DashScope's reported input-only total as input; this is the opposite direction, not missing-fact inference |
| `embedding.estimate-missing-usage.v1` | Complete missing scalar fields from a request-local approximation, preserving every reported scalar and the source report. Supersedes usage omission |
| `embedding.omit-usage-details.v1` | Omit a supplied detail field when both scalar reports are already usable |
| `embedding.omit-user-hint.v1` | Validate and omit an unsupported opaque hint on the target copy; never inject it into text/headers or claim local abuse monitoring |

OpenRouter report permissiveness does not attest an input-only accounting basis. Complete real reports and attested input-only derivations take precedence; otherwise compatible delivery estimates only missing counters. Known null, negative, overflowing or wrong-type fields still reject; with both counts present, total must be at least input. Usage is batch-scoped, never multiplied by vector count.

The fixed v1 approximation sums per-sample estimates: text uses `max(1, ceil(total UTF-8 bytes of its parts / 4))`; scoped token inputs use sequence length. Round joint parts once per sample, but independent samples separately. Prepare this positive fallback once with the admitted request, without retaining extra payloads. With neither counter reported, deliver the estimate as both counts. With only input reported and no attested derivation, estimate total as `max(estimate, input)`; with only total reported, estimate input as `min(estimate, total)`. Preserve reported zeros, even when they constrain both delivered counts to zero; zero is never the missing-report default. Details alone do not establish an aggregate.

One applicable usage rule records fixed actions such as `total_derived`, `input_interpreted`, `input_estimated`, `total_estimated` and `details_omitted`; it does not emit duplicate receipts for the same response. Request and delivery receipts accumulate in opt-in body-free diagnostics. Capabilities advertise `usage_may_be_estimated`; no custom fields enter the standard usage object. The source basis remains unchanged, including `Unspecified` when delivery is estimated.

Every successful downstream response has complete `usage` with integer `prompt_tokens` and `total_tokens`. The delivery type makes usage mandatory; strict native codecs still reject incomplete source reports rather than estimating without request context. The heuristic is uncalibrated, language-dependent and neither billing truth nor a hard quota/admission guarantee. No tokenizer execution, calibration service, extra Provider call or deferred counting endpoint is introduced. These rules never authorize changing input, guessing tokenizer scopes, truncating/reshaping vectors or accepting incomplete transport. See the dated [source and acceptance evidence](refs/2026-10-22-embedding-image-contracts.md#embeddings).

## Adaptation

### Source and target boundaries

#### Protocol baselines and Provider dialects

Use two codec responsibilities in each direction:

```text
decode: wire → dialect normalization → protocol baseline + declared extensions → IR + fidelity
encode: IR + eligible fidelity → target baseline + declared extensions → dialect representation → wire
```

This is the selected boundary, not a claim that all existing codecs implement it. The baseline is one official API and its selected source revision, not another cross-protocol IR. Keep Chat Completions, Responses, Messages, generateContent and Interactions distinct. Model restrictions and Provider aliases do not redefine the baseline.

A dialect may normalize an equivalent alias, with conflict checks against the canonical field. A declared semantic extension enters typed IR through an explicit decoder; it cannot hide required content in a restoration-only sidecar. Non-semantic reports and representation records stay bounded, source- and owner-bound outside the semantic task. Unknown fields never become restorable extensions.

Trusted upstream adapters select dialects. Apply the same upstream extension admission to requests, static reports, streamed events and returned history, while retaining their direction-specific unknown-field policies. Public codecs select official contracts independently of the upstream. Legacy MorphieCore Chat controls and replay carriers are migration work, not additions to the official schema.

Encode from the current typed meaning. A source extension cannot override canonical output, restore a deleted owner or cross an incompatible target. Dialect representation precedes final serialization; there are no post-encoding JSON patches. Projection need not be invertible: named approximations remain losses, not equivalent mappings.

Introduce this seam incrementally around concrete mappings. Do not add a universal protocol DTO, arbitrary extension bag, plugin registry, second stream reducer or whole-stream buffering. Existing framing limits, conflict validation, EOF-gated terminals and replay authority remain unchanged.

#### Protocol tree ownership

Organize protocol code as vendor → wire API → responsibility: `openai/{chat,responses}`, `anthropic/messages`, and `google/{generate_content,interactions}`. The vendor name is where the wire API is defined, not which Provider serves the request, so a compatible endpoint reuses the standard codec plus its dialect instead of copying one. Inside an API, the same module names carry the same role: `wire` holds DTOs, `request`/`response`/`items`/`settings` map to and from the IR, `sse` owns framing and stream lifecycle, `target` owns capabilities and admission, `accounting` owns usage, `replay` owns opaque carriers, and `dialect` owns named deviations. An operation that a protocol does not have keeps no placeholder module.

A shared rule belongs to the nearest common owner: one API, then one vendor, then `protocol`. Dialect code differs by API, so `openai/chat` and `openai/responses` may each name the same Provider; only a rule that must change together is shared. Model or endpoint capability, adaptation policy, Provider identity, origin and credentials stay outside the dialect.

OpenAI's shared dialect selection lives in `openai/dialect/trusted.rs`; Chat/Responses Bailian request rules live in `openai/dialect/bailian.rs`, while the Responses-only SIWC check lives in `openai/responses/dialect/siwc.rs`. Adapters invoke those checks at their required validation stages; moving the implementation does not move its authority or execution order.

Where a direction is missing, the tree says so instead of imitating a fuller sibling: a whole re-encode is not a static response, and a truncated dependency is not a mirror of the source that defined it. Shared records still live at protocol level; the [report views](../src/protocol/reports.rs) are one example, because a protocol that cannot express a lifecycle change must not import that rule from a sibling API.

##### Shared Aliyun billing dialect

DashScope and Aliyun TokenPlan select one `Bailian` wire dialect for their existing Chat/Responses bindings. Billing plans do not own separate parser flags. Readable reasoning and the [history-reading switch](#chat-reasoning-extensions), Chat creation-time normalization/empty continuation call IDs, and response-only `schema_` report aliases apply equally; conflicting identities and aliases still reject. Current Qwen3.8, GLM5.3/Prime and Kimi-K3 Chat bindings use the documented `max_completion_tokens` total ceiling on both plans, not an inferred legacy-cap rewrite.

The [compatible-request subset](refs/2026-10-19-bailian-compatible-requests.md) uses existing IR owners only. Explicit activation, positive Chat thinking allocations and top-k have native carriers without filling defaults. Qualitative effort aliases are model/protocol-bound and recorded, never token presets. Native reply-only caps cannot become total caps unless thinking is explicitly off; GLM5.3's documented legacy total cap is a separate intake exception. Unsupported model values, ignored controls and unlisted Responses guarantees reject before upstream publication. Function output order is not repaired. Native function tables do not establish strict generation or default schema normalization; do not confuse these with Qwen's separate structured-output support.

Client obfuscation preferences remain local delivery intent: Bailian Chat receives only the documented usage option and Responses receives no `stream_options`. Independent answer/thought ceilings remain [unselected design](api-expansion-design.md#independent-output-ceilings-unselected); this slice changes no IR, public admission, bindings or stored-history ownership.

Model catalogs, trusted origins, paths, credentials, billing and replay issuers remain separate. Shared syntax does not authorize exchanging keys, pooling subscriptions or replaying account-bound opaque values across the two origins. TokenPlan is not Coding Plan; see the [dated source note](refs/2026-10-19-same-protocol-inventory.md#shared-aliyun-dialect).

##### Subscription Responses

Grok and OpenAI SIWC use public Responses endpoints, not Chat or a CLI subscription proxy. Both keep upstream history stateless with `store:false` and use body `prompt_cache_key`, not product session headers. SIWC always requests upstream SSE even for a JSON client, groups function/custom definitions and call history in namespaces, and rejects its unsupported controls under the [final typed check](../src/protocol/openai/responses/dialect/siwc.rs).

OpenAI keeps only standard API-key codecs and public SIWC. The legacy ChatGPT/Codex product-backend dialect, combined product-accounting path and unused backend-header carrier are removed, without compatibility aliases. Codex as a downstream client remains a separate concern. This boundary does not activate a direct Platform API-key model binding; the built-in OpenAI binding remains SIWC.

Only SIWC's selected compatibility profile permits absent SSE Content-Type and an empty terminal output reconstructed from already closed event owners. Grok does not inherit those exceptions or `[DONE]` tolerance. Missing owners, conflicting terminal snapshots, incomplete framing and data after termination remain errors. These compatibility observations are not universal guarantees of the public API.

Ordinary usage counts retain their typed meaning. xAI context counters/cost ticks and SIWC program/attribution/zero-tool reports stay bounded in source-bound fidelity, never summed into billed usage or treated as invocation authority. Known numeric reports are strict; unknown nested members are filtered before retention. These reports can return only to the same dialect/issuer and unchanged response dependency. MorphieCore client delivery has no carrier for them: that is an explicit non-lossless report exclusion, not exact same-protocol delivery. Identity omission and any opted-in retention degradation retain their existing named rules.

#### Field handling

| Class | Handling |
|---|---|
| Must preserve | Preserve instructions, order, tools, arguments, call/result links, required continuations, stop reasons and core usage; reject an unrepresentable target |
| Degradable | Apply an approved advisory omission or behavioral approximation to a target-private copy and record it; never silently patch the source |
| Unknown | Ignore unregistered protocol fields; keep known fields strict and never forward unknown input |

Upstream responses are strict on known types, values and relations. Unknown fields are ignored. Unknown Responses event types are skipped and counted; registered content and terminal events remain strict.

Downstream generation requests ignore unregistered fields; diagnostics record their count, not untrusted names or values. Unknown content or role discriminators are rejected because ignoring them would lose content. Protected known semantics that a target cannot express return an error. Trusted configuration and frozen-task requests stay closed.

Parsing direction is explicit: client input, upstream reports and encoder checks have different policies. Nested parsing inherits that direction; no ambient or thread-local policy is used.

Messages request-level `output_config.task_budget` and tool-use/result `toolset_name` reject before permissive filtering can discard them. Task budgets remain unsupported even at zero total. Explicit null budget and null/empty dispatch identity also reject: nullable SDK syntax does not establish equivalence to omission. Only absence passes these unsupported owners; genuinely unknown extensions remain ignored. This selects rejection, not task-budget or toolset execution support.

Check actual protocol owners: request-level task budget and tool-use/result dispatch identity, including upstream calls and returned history. Do not recursively treat identically named argument/schema properties as controls. Full-message and SSE `message_delta.delta.stop_details` readers also type-check and reject non-null beta credit/prefill/retry-target reports, including a supplied false prefill claim. The [pinned beta source](refs/2026-10-17-generation-contracts.md#refusal-detail-boundaries-2026-10-21) explicitly uses null for unavailable facts; those nulls pass without retaining credit or retry authority. This field-handling repair does not implement fallback execution or the pending stable refusal-detail owner.

#### Client Responses history shorthand

The user selected an input-only compatibility rule on 2026-10-20. A client Responses assistant-history `output_text` part may omit `annotations`, like the already supported assistant string shorthand. Intake materializes the empty annotation list and records `responses.default-history-annotations.v1` once as a body-free `request.decode` insertion receipt. This is not an ignored field or a report of upstream annotation completeness.

Explicit null, malformed arrays and supplied citations retain strict validation; existing citations are never erased. Closed request decoding, upstream JSON and all SSE snapshots continue to require the official array. The rule does not apply to other roles or content kinds, reconstruct discarded citations or change routing. Encoding the resulting IR emits the canonical array. The [client/source evidence](refs/2026-10-17-generation-contracts.md#responses-lifecycle-reports-2026-10-19) distinguishes this selected shorthand from the pinned OpenAI parameter schema.

#### Candidate-local projection

Each candidate starts from the unchanged source IR. Static and streaming paths use the same named rules.

OpenAI request projection carries one candidate-local owner through the existing ordered transforms, borrowing the source until an edit is needed. Settings-only edits avoid a separate history replacement. Source validation and intermediate target checks retain their positions; final-only validation must not sanitize invalid source controls into acceptance.

Semantic support, wire representation, execution properties and client promises are separate capability layers. Public capability is not the union of all candidates.

For a fixed target, earlier history projects byte-identically as later turns are added; cache markers may move. Rules must not reinterpret earlier content based on the current turn or request position. This protects cache prefixes and preserved-thinking replay.

Native container boundaries may be omitted by a named client projection. Chat coalesces unannotated text into its scalar wire value without changing bytes; metadata-bearing merges reject. Ordered content, exact arguments and call/result identity remain intact. Required replay attachments need genuine native carriers or execution-authorized typed managed Responses continuation; this wire view is not native ownership.

#### Model-effective values

Trusted target caps own optional allowed-effort and maximum-output declarations in [ModelLimits](../src/protocol/model_limits.rs), separate from protocol carriers and semantic feature admission. Omission adds no restriction or capability. Declaration validation rejects empty/duplicate effort sets and zero maxima; configured candidates inherit catalog declarations and may narrow, never widen them. Effort scopes and existing conversion tables remain unchanged.

The shared preflight/preparation encoder checks final typed controls after selected mappings and carrier projections, including retained ordered efforts and inserted defaults. Messages checks the projected native request so its default ceiling and per-message efforts cannot bypass the guard. Reject violations without clipping, remapping to another allowed effort or changing source IR. A model maximum never supplies a missing ceiling; [Messages default insertion](#missing-output-ceilings) is a separate policy. These declarations neither rank routes nor publish broader capabilities or prove upstream enforcement.

### Request and reported meaning

#### Accurate IR and declared loss

The accepted order is **lossless mapping → bounded semantic approximation → meaningful simple estimation, otherwise rejection**. Validate the source first and retain its intent and observed facts. For each target, test exact model-supported carriage before applying a configured fallback; the presence of a conversion policy never preempts an available exact carrier. Work only on target-private copies. Exact arithmetic with established premises is derivation, not estimation; equivalent normalization is not loss. Omission is a classified lossy mapping, not permission to discard fields at intake.

Estimation applies only to quantities with an explicit bounded method and domain. Never estimate identities, opaque signatures, tool success, authentication authority, safety intervention or successful termination. Invalid source requests still reject. An adaptation report does not authorize withdrawing an explicit output-schema/tool-choice guarantee, disclosing hidden reasoning or raising a caller's output ceiling. Codecs remain direct typed transformations, not a generic rule engine.

All previously accepted conversion rules are reopened under this order. The following corrections are selected; implementation and source-specific coverage belong to the roadmap:

| Rule family | Accepted treatment |
|---|---|
| Prediction and advisory controls | Preserve in IR until target selection; exact native carriage first, then named omission |
| Reasoning allocation/presentation | Preserve supported budget, effort and visibility; only then apply evidenced directional approximation or bounded estimates |
| Responses omitted strictness | Follow current official normalization-with-best-effort-fallback semantics; explicit `strict:true` remains a different guarantee. Record effective reports separately from request intent |
| Tool-result errors | Preserve native error meaning; absent a native carrier, prefer bounded model-visible error wrapping over deleting the only failure indicator |
| Instructions, sampling and schema/media controls | Seek accurate carriers and supported combinations before rejection. Use named order/authority-preserving composition, fixed-sampling omission and evidenced directional detail mappings; do not infer equivalence from matching names |
| Output ceilings and attribution | Consider downward ceiling adjustment only for source-valid upper bounds, never an increase or a changed minimum/target. Advisory attribution may omit when no valid carrier exists; never rewrite security identity to fit |
| Ordered configuration/tool changes | Prefer native history carriers. Flattening must declare its historical/prefix loss and cannot claim byte-stable prior context |
| Replay and phase | Preserve legitimate semantic owners; separate incompatible opaque state from visible content. Do not fabricate reasoning owners to transport manifests; necessary opaque context cannot be estimated |
| Accounting and ancillary reports | Preserve actual values, then justified derivation, then field-specific estimates. Report actual omissions/approximations/estimates at their delivery boundary |

Classify losses by their effect on task constraints, output quality, execution and security, not by whether a field is optional in an SDK. A verbosity preference may be omitted when no target carrier exists. A required output schema, tool selection, call/result link, content item or honest terminal cannot be discarded merely to complete a request. Billing, storage and disclosure controls also need an explicit policy even when they do not change answer quality.

Cache grouping keys are advisory. Prefer an exact cache-retention carrier. A trusted target may explicitly select `cache.omit-day-retention.v1` for `"24h"` only after its default storage policy is reviewed; the policy defaults off, including on all registered bindings. It relinquishes extended reuse, not a promise of shorter storage, no storage or ZDR. Explicit `in_memory`, null and unapproved targets still reject without a carrier. Do not substitute another protocol's TTL or erase active cache options. Automatic caching does not establish a compatible storage policy; null request cache-options containers remain invalid.

Explicit prefix-cache breakpoints belong to the content or tool-definition owner, not a separate request-wide reference plan. Keep TTL intent typed and independent of resource identity, cache affinity and replay. Owner edits carry or remove their own marker; no fidelity record restores a deleted marker. Request-wide automatic caching is a separate control.

Responses boundaries inherit the request's minimum lifetime policy; do not materialize a default TTL on every part or equate that lifetime with legacy maximum retention. Messages boundaries supply 5m/1h, with omitted native TTL normalized to 5m, never request inheritance. Missing exact carriers reject in both directions. The existing `explicit_cache` admission flag covers owner-supplied TTLs; inherited Responses boundaries keep their existing codec admission, without activating the native Messages capability or widening a binding. Native automatic-default suppression considers all boundaries.

Responses may report inherited boundaries on inner function/custom-result text/image/file parts. Preserve them in static output, SSE snapshots and returned history; a marker-only change is an output update, not stale fidelity. Other generated owners, outer result TTLs and explicit-TTL response parts remain invalid. Null reported boundaries mean no directive; supplied request null remains invalid. Native four-marker bounds, TTL ordering and automatic-cache conflicts stay unchanged.

| Treatment | Contract |
|---|---|
| Exact representation | Preserve meaning; check source conflicts before alias normalization |
| Advisory loss | Omit selected verbosity preferences or predicted-output optimization through named rules; other fields need individual classification |
| Behavioral approximation | Use selected effort/budget, summary-granularity and role mappings on target copies. Preserve visibility and output ceilings; preserve supported sampling and omit fixed/ignored sampling only under a target policy. Record changes and validate target restrictions |
| Meaningful estimate | Fill only eligible missing quantities using a named bounded method; preserve actual zeros and identify the estimated fields downstream |
| Protected meaning | Preserve task content, constraints, execution identity, security boundaries and result truth; reject when no faithful carrier exists |

Decode according to the source protocol before target projection. Known types, variants and relations remain strict even when a later target may omit the field. A known but unsupported critical field is not an unknown field. A target profile's restriction must not prevent the client decoder from expressing a valid source concept already supported by the IR.

Keep source validation, acceptance with approved adaptation, and faithful target support distinct. Project before checking the final target representation; do not advertise an omitted feature as native support. Capability assessment belongs to IR/codec contracts, not to a new route-ranking or fallback policy. The [temporary routing freeze](roadmap.md#temporary-routing-freeze) permits integration to remain disconnected while these contracts change.

Request controls, upstream-reported settings and downstream delivery have different authority. Sent controls do not prove effective settings. A response-only echo can be omitted under a selected delivery rule without discarding generated content; validate source facts and repeated snapshots before that omission. JSON, SSE and retained delivered history must agree on the applied loss.

#### Downstream schema completion

Selected on 2026-10-23: the admitted Generation Text surface returns complete downstream schemas, using **upstream facts → justified inference → simple estimates**. This supersedes omission of mandatory downstream fields, not strict source decoding, unsupported-feature admission, protected content or terminal honesty. Unresolvable semantic choices still require human selection.

`ResponseMetadata.delivery` is a separate presentation owner, never populated by source decoders or inserted into IR reports. The gateway prepares request-local settings and an input estimate, then completes final accounting only after validated EOF. JSON, terminal SSE and stored retrieval use the same completion. Native codecs and library callers without this explicit delivery context retain their exact/fact-preserving contract.

- `responses.infer-required-settings.v1`: preserve each reported `tools`, `tool_choice` and `parallel_tool_calls`; fill only missing fields from the admitted request. Absent request settings use Responses defaults `[]`, `"auto"` and `true`. These are downstream request echoes, not evidence of Provider-effective settings. Opening and terminal response envelopes both use this rule.
- `generation.complete-delivery-usage.v1`: retain usable actual counters, including zero; use checked arithmetic for known disjoint cache categories, inclusive reasoning and total-minus-known-component. Unknown source total relationships do not become an asserted OpenAI sum. Complete the target's sum from its selected input/output instead, while retaining the original total in source reports. Diagnostics distinguish `derive` from `estimate`.
- The v1 text estimate is `ceil(UTF-8 payload bytes / 4)`, with minimum input one and empty output zero. Count text, refusals, visible reasoning, tool names/arguments/results, request instructions and tool descriptions/schemas once, not repeated SSE snapshots. Exclude transport wrappers, opaque replay tokens and binary/media payloads; this is an uncalibrated **text-only approximation**, not a media estimate, tokenizer, billing fact or quota guarantee.
- Missing cache categories use zero estimates. Unknown cache/tool-input overlap uses the larger known category rather than asserting disjointness. Missing reasoning uses visible reasoning bytes, bounded by output. Known sibling/detail counts bound missing-parent estimates. Responses always completes its required cache-read, cache-write and reasoning children; optional unsupported breakdowns remain omitted under existing rules.
- Messages retains exact native usage and its optional TTL/tool details when representable; otherwise its mandatory final input/output uses the separate completed view. Opening zeros are provisional checkpoints, not final accounting. Chat emits the requested final empty-choices usage chunk before `[DONE]`; absent upstream accounting no longer suppresses it.

Input-items listing uses returned-resource shapes rather than request shorthand: instruction content is an `input_text` array; accepted instruction/custom-call/tool-result history defaults to `completed` only when no explicit lifecycle exists. This does not change continuation encoding or invent successful tool execution.

Messages authentication errors use the Messages error envelope before any body polling; method and unknown Messages-resource errors use the same protocol envelope. Tool argument streaming buffers only a bounded leading JSON-whitespace prefix until the first token, preserving concatenated bytes without exposing an unparseable prefix to SDK accumulators.

#### Reported text configuration

Reported text configuration preserves absent, null and object containers through the existing `TextOptions.presence: Presence<()>`; an empty object stays distinct from absent/null. Format and verbosity children require an object container. Native Responses JSON/SSE and stored terminal snapshots retain these observations without inserting format or verbosity defaults; repeated settings comparisons include container presence before any delivery omission.

Null is a reported state, not a request to disable text or an inferred default configuration. Request-side null text containers and formats remain invalid, including direct IR construction: Chat `response_format:null`, Responses `text.format:null` and supplied null JSON-schema descriptions reject. Nullable reported formats/descriptions retain their separate report parser rather than inheriting request validation. This is a local repair of the reported-fact contract, not a mechanical presence expansion across all settings. The [dated source evidence](refs/2026-10-17-generation-contracts.md#responses-text-container-presence-2026-10-20) and [stable-source recheck](refs/2026-10-23-stable-protocol-contracts.md#remaining-mapping-recheck) distinguish SDK-observable state from unproven Provider behavior.

#### Message phase transfer

Responses preserves assistant `commentary` and `final_answer` exactly, independently of lifecycle and terminal meaning. Missing/null labels stay unlabelled. These labels affect history interpretation, not merely display; never infer them from text, tool calls or completion.

Requests to Chat, Messages, generateContent and Interactions may omit phase on a target-private copy under `[chat,messages,gemini,interactions].omit-message-phase.v1`. This is lossy visible-history transfer, not equivalent continuation. Source content, order, call/result links and phase remain unchanged.

Responses uses its native phase field. Chat and Messages JSON/SSE delivery preserve visible content/order but omit phase under `generation.omit-delivery-message-phase.v1`. Phase-only answers never acquire fabricated reasoning/thinking owners. This loss is not equivalent continuation, and a diagnostic notice cannot restore phase or establish client retention.

Returned native Responses labels remain authoritative; other client histories do not reconstruct omitted labels. See the [source evidence](refs/2026-10-17-generation-contracts.md#compatibility-policy-review-2026-10-18).

#### Explanatory terminal reports

Messages `stop_details` uses `TerminalDetails.refusal: Presence<RefusalDetails>`; Google candidate `finishMessage` uses the separate `explanation: Presence<TerminalExplanation>` owner. Keep explanations separate from generated text, execution errors, outcome and stop/incomplete causes, not in a generic report bag. An explanation neither proves success nor turns a refusal into a failed attempt.

Validate source types, bounds, presence and repeated/final reports before projection. Preserve native reports accurately in existing directions, with JSON/SSE and applicable retained-history agreement. No missing Google client or static direction is added by this decision.

Messages retains absent/null/object details and each child's absent/null/value state. The five pinned policy categories map to typed semantic classes; unknown categories and variants reject. A report has a 64-KiB aggregate budget and a non-null refusal detail requires the real refusal stop. SSE null/absent observations may precede the first concrete report; repeated concrete reports must agree, explicit null cannot erase one, and omission is no update. Final validation and strict EOF remain mandatory. Native/client JSON and SSE carry the report; request history has no native report field and does not turn it into model input. Report explanations are Debug-redacted but included in dependency fingerprints.

generateContent intake accepts `finishMessage` only with a reported `finishReason` on the selected single candidate, whose optional index must be zero. Absent and empty-string observations remain distinct; explicit null and wrong types reject. The finish reason alone determines outcome/cause; prose never overrides it. The existing strict post-finish frame rejection remains in force, including repeated equal reports. Refusal details and terminal explanations share a 64-KiB observation budget; their Debug is redacted and actual prose bytes bind response dependencies.

A cross-protocol delivery copy may omit explicitly classified explanation-only information under a named, body-free receipt when no accurate carrier exists. Remove omitted reports from the typed delivery snapshot as well as its wire representation, including snapshots retained after SSE delivery. Never use this permission to omit refusal state, safety blocking, terminal causes, usage or execution policy, or to fabricate a successful terminal. Preserve source IR; native report carriage is not replaced by omission merely for convenience.

Google explanations have no selected Messages/OpenAI carrier. Client delivery omits them under `messages.omit-terminal-explanation.v1` or `openai.omit-terminal-explanation.v1` in JSON and SSE; direct native Messages encoding rejects them instead of silently losing them. Existing EOF withholding and terminal-cause projections are unchanged. No explanation is appended to content, errors or retained model input.

Classify fields before omission. OpenAI delivery rejects a non-null refusal category because it lacks the selected policy-class carrier. Only details with no reported policy class may omit under `openai.omit-explanatory-refusal-details.v1`, including an explicit no-detail observation; the incomplete/filter outcome remains unchanged. Unsupported non-null beta `fallback_credit_token`, `fallback_has_prefill_claim` and `recommended_model` reject before stable parsing, including a supplied false claim. Do not retain/redeem credit tokens, authorize prefill or choose a retry target through this report slice. See the [pinned fields](refs/2026-10-17-generation-contracts.md#refusal-detail-boundaries-2026-10-21).

The narrow Google pause exception also selects source-version review of `serviceTier` and `service_tier`. Reuse the existing service-class owner only when the selected API/model profile and equivalent meaning are established. Service classes are outside the explanatory-omission permission: incompatible or unresolved reports reject. Do not infer a service report from a request, map Google processing class to Messages capacity merely by name, or add new service-policy semantics without another decision.

The [current v1 OpenAPI](refs/2026-10-17-generation-contracts.md#google-text-report-source-recheck-2026-10-21) establishes service fields on created/completed resources, not status updates; beta/SDK `deferred` does not widen that profile. Candidate `finishMessage` requires reported finish-reason evidence. Source discovery resolves field existence, not lifecycle drift, explicit-default normalization or body/header agreement. Existing strict consistency remains the default; no new header carrier, request-policy mapping or usage rule is selected.

#### Service policies and reports

Service selection uses closed request-policy variants, separate from the reported service class. They belong to request execution intent and response metadata, not generation settings or token accounting. Keep native spellings in codecs; do not infer a report from the requested policy, configured target or model alias.

[Messages service tiers](https://platform.claude.com/docs/en/api/service-tiers) distinguishes standard-capacity-only intent from preferring available committed capacity. [OpenAI Responses](https://developers.openai.com/api/reference/resources/responses/methods/create) resolves `auto` through project settings. These policies, and their reported capacity/processing classes, are not interchangeable merely because native names overlap. Reject conversions without a selected exact carrier.

Messages carries a service report in full-message usage, including stream opening usage. The [pinned delta usage](https://github.com/anthropics/anthropic-sdk-python/blob/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f/src/anthropic/types/message_delta_usage.py) has no such carrier. Reject tier-bearing deltas and downstream reports introduced or changed after opening; never manufacture an opening value or emit a successful terminal after rejection. Other repeated native reports must agree, except the explicitly selected [Responses lifecycle relation](#lifecycle-settings-and-service-reports); absent reports stay absent.

The selected Google profiles map reported `standard`, `flex` and `priority` to the existing `StandardProcessing`, `FlexibleProcessing` and `PriorityProcessing` classes. generateContent reads `usageMetadata.serviceTier` into the single report owner, not accounting; the first supplied concrete report may arrive late, but later supplied usage snapshots must repeat it. Frames without usage metadata make no new service assertion. Interactions v1 reads optional `service_tier` independently on created/completed resources into bounded opening/terminal slots. Opening-only, terminal-only, both absent and matching supplied classes are valid; two supplied conflicting classes reject. Missing fields neither copy nor erase another phase's observation. A status-update tier has no selected carrier and rejects. Downstream phase/carrier restrictions remain independent.

Null/wrong types, unknown classes, `unspecified` normalization and beta `deferred` reject. No report is inferred from a request or copied into a missing phase. OpenAI delivery uses the existing exact `default`/`flex`/`priority` report spellings; Messages delivery rejects every processing class rather than dropping it or relabeling it as capacity. Source consistency precedes projection and successful EOF release. This report-only repair adds no request policy, header carrier, usage rule, model, or native Google client/static direction; see the [dated source evidence](refs/2026-10-17-generation-contracts.md#google-text-report-source-recheck-2026-10-21).

Model names may encode a service policy only through an evidence-backed trusted binding declaration. Keep canonical model identity, semantic policy and wire spelling distinct. Effective deployment identity must agree across preparation, cache and replay. Request-dependent alias selection remains deferred with routing integration.

#### Lifecycle settings and service reports

The user selected bounded phase-tagged reports and resumed the Responses acceptance slice on 2026-10-20. That selection alone did not close the overall Generation review or select another routing policy.

Response context has one typed report owner for settings and service class. Native Responses and Interactions use at most one opening and one terminal report; protocols without this lifecycle distinction retain their single report. Do not keep another mutable current-settings value, raw snapshots or an event archive. All retained reports share a bounded budget. Instructions, content, execution, usage and terminal facts retain their existing owners.

Opening/progress reports agree within their phase. Validate native types and differences before normalization. Cross-phase equality is the default; the selected OpenRouter Responses contract additionally admits only `summary: auto → detailed`, absent context → `all_turns`, absent mode → `standard`, and service `auto → default`. These independently checked transitions may coexist; other fields must agree. Null is a reported value, not absence or a wildcard. Terminal reports never inherit missing fields from opening reports or request controls. Same-phase drift, concrete-to-different-concrete drift and unselected transitions reject. Interactions' independently optional service observations use their [source-specific relation](#service-policies-and-reports), not an inferred change from a missing field. The [dated evidence](refs/2026-10-17-generation-contracts.md#responses-lifecycle-reports-2026-10-19) distinguishes observations from the selected interpretation.

Responses SSE emits the actual opening and terminal reports in their respective carriers. A terminal-only source does not supply an opening report. JSON and stored retrieval select the terminal snapshot, with named evidence when an opening observation is omitted; strict-complete delivery rejects that loss. Stored typed metadata and retrieval bytes describe the same delivered view, not an unseen opening archive. Terminal absence is not permission to synthesize a successful snapshot. Source validation precedes every projection, and this slice does not approve changing service reports across Chat/Messages carriers.

Metadata updates must preserve already emitted observations, including an absent opening report once a queued/started carrier is emitted. Rejection poisons the encoder; EOF, cancellation, publication and replay restrictions are unchanged. Phase reports do not establish terminal success or permit replay after publication. A gateway-classified failure may emit `response.failed` without an upstream terminal report; it carries no invented settings or service class.

#### End-user attribution

Request projection implements shared opaque end-user attribution through OpenAI Chat/Responses `safety_identifier`, Messages `metadata.user_id` and generateContent `labels.safety_identifier` on admitted targets. These carriers overlap in abuse attribution; they are not authentication, credential, workspace, billing-project or conversation selectors. The [source comparison](refs/2026-10-17-generation-contracts.md#cross-provider-user-attribution-2026-10-19) records their distinct bounds and purposes. Google Interactions has no selected equivalent carrier.

Unified meaning does not collapse independently supplied controls. Retain general user attribution, an explicitly supplied safety identifier and cache grouping separately, reusing the existing typed identity/cache owners rather than an arbitrary metadata bag. OpenAI's deprecated `user` combines attribution and legacy cache bucketing; preserving only one use is not full semantic equivalence.

Apply these rules on the target-private copy:

1. Validate all known source fields before selection, including lower-priority fields. Same-protocol faithful paths preserve each native field and meaningful presence independently; do not automatically modernize a lone OpenAI `user` into new safety/cache fields.
2. When projecting into another protocol's single attribution carrier, prefer an explicit safety identifier; otherwise use the general end-user identifier, including the attribution meaning of legacy `user`. Never replace a supplied safety identifier with a derived fallback. An explicit null blocks gateway fallback; preserve null where representable, otherwise record its omission without filling another ID. This does not assert that null disables Provider attribution. A permitted empty value is not absence.
3. Different `user` and `safety_identifier` values are valid independent facts, not conflicting aliases. If the target cannot retain both, carry the higher-priority value and record omission of the other under `generation.omit-identity-hints.v1`, without identifier values in evidence. Competing explicit aliases for the same semantic field at the same priority must agree in value and presence or reject; object order never resolves a conflict. Duplicate JSON keys still reject.
4. Validate the selected value against the target's length and character constraints. An invalid value rejects; never truncate, lowercase, hash or fall back to a different identity to make it fit. Targets without an admitted carrier retain named omission. All-absent input stays absent; do not derive identifiers from credentials, prompts or conversation IDs.
5. Keep `prompt_cache_key` independent. Neither a user ID nor a safety ID automatically becomes a cache key, and a cache key never becomes attribution. Preserve an explicit cache key through its own carrier or named cache projection. Cross-protocol transfer that loses legacy `user` cache bucketing records `cache.omit-legacy-user-bucketing.v1` separately from identity loss; this decision selects no automatic `user`-to-cache-key conversion.

For example, source `user: "legacy_a"`, `safety_identifier: "usr_b"` and `prompt_cache_key: "template_c"` retain all three on an exact OpenAI path. A single-carrier Messages or generateContent projection selects `"usr_b"` for attribution, never `"template_c"`; source values remain unchanged and unrepresented identity/cache facts receive their respective loss evidence.

Native Messages `metadata.user_id` uses the bounded general-user owner, preserving absent/null/empty and up to 512 characters. OpenAI native limits are 256 characters for `user` and 128 for safety identity; shared storage permits 512/128 independently of source/target checks. Messages attribution projects to OpenAI safety identity, never legacy `user` or cache bucketing; a value over the safety bound rejects even when Messages can carry it. Chat↔Responses preserves both native OpenAI fields independently. Empty/absent metadata without a user ID makes no attribution claim.

GenerateContent emits only the fixed `labels.safety_identifier` entry. Values permit 0–63 Unicode codepoints in `[\p{Ll}\p{Lo}\p{N}_-]`, including uncased international letters, without normalization. Null has no label representation and receives named identity omission without fallback. Interactions and targets without an identity carrier omit unrepresented fields explicitly. A known OpenAI legacy `user` that cannot be retained also loses its cache-bucketing meaning, including on carrier-less OpenAI targets, and receives the independent cache-loss receipt.

[Attribution projection](../src/protocol/attribution.rs) uses the existing typed identity owners and fixed, value-free loss evidence. Current wire contracts register one spelling per semantic slot: no simultaneous same-priority aliases are exposed. Duplicate raw keys reject; arbitrary metadata entries are not identity aliases. Future aliases must uphold the conflict rule above, not add object-order precedence. The existing `source=None` typed-entry convention preserves native target slots for OpenAI/Messages; it is not a newly admitted wire source. No binding activation or generic metadata/label forwarding is implied.

#### Selected request approximations

The old D3 intake-time omission of Chat `prediction` is superseded. Preserve its typed meaning until target projection, use an exact native carrier when supported, and otherwise omit the optimization with a receipt. Predicted content is not an output constraint or a source of fabricated prediction-token reports.

`GenerationSettings.prediction` contains bounded ordered text parts, not wire JSON. Standard Chat has a native carrier; trusted `TargetCaps.prediction` independently declares model support. Existing catalog bindings keep it closed without that evidence. Unsupported targets record `generation.omit-prediction.v1` at request projection, never at intake. String/single-part normalization does not change the predicted text.

Native tool-error flags remain exact. Without an OpenAI carrier, `openai.wrap-tool-result-errors.v1` prefixes the tool output with `[tool result is_error=true]\n` or `[tool result is_error=false]\n`. It preserves the supplied Boolean, not an inferred success or execution conclusion. Text keeps its payload; multipart results gain a separate bounded text part before their unchanged parts/resources; structured values retain exact JSON serialization inside the textual wrapper. The source, call links and execution facts remain unchanged. Added bytes/parts count against normal limits; overflow rejects instead of truncating.

P8 permits approximation of summary/display granularity, such as concise or detailed summary intent to a target's single summary mode. Preserve the requested show/hide behavior independently: hiding must not become disclosure, and requested display must not be silently disabled. An absent preference remains absent. These control approximations do not enable reasoning, fabricate summaries, discard generated content or remove required replay. Reject a target that cannot preserve the visibility contract.

The selected [canonical instruction contract](#instruction-authority-and-canonical-roles) supersedes P8's broad developer/system/user approximation permission for application instructions. Standard historical system normalizes to the application channel; trusted target mapping selects developer or operator-system without lowering that channel to user. Do not invent assistant/tool speakers, repair orphan results or accept an invalid source role. Preserve text, relative order and root/ordered lifetime.

Trusted dialect `system_instruction_messages` selects the effective application carrier for both root projections and ordered instructions. Standard codecs emit developer; DeepSeek and Bailian Chat select system on the wire copy. Keep target policy outside semantic IR and public business input, validate before encoding, and never retry alternative roles after an upstream error.

**DeepSeek parallel-tool policy selected on 2026-10-25.** Both registered Chat and Responses targets omit `parallel_tool_calls` from prepared requests. Explicit false rejects at candidate projection before dispatch, including the equivalent Messages `disable_parallel_tool_use:true`; it must not become a prompt-only request or an ignored guarantee. Explicit true is permission, not a serial constraint: remove it on the candidate copy with `deepseek.omit-parallel-tool-permission.v1`. Absence stays absent; source IR and other candidates remain unchanged. This selects no developer/system conversion and does not certify parallel output or live Provider behavior.

P8 keeps image detail and file detail separate. Prefer exact carriers. Omit an explicit detail only when the selected target's default is proven equivalent, or apply a named, directional approximation backed by evidence for that target. An absent carrier alone does not justify omission, and explicit `auto` is not automatically equivalent to absence. Undeclared or unsupported conversions reject. These rules do not permit dropping media, transcoding it or moving it to another role.

P8 protects the model-visible filename. Preserve it through an exact carrier, including OpenAI `filename` and Messages document `title`; reject a target without one when a filename is present. Do not omit the name or inject it into surrounding text. An empty name remains distinct from absence.

P8 permits omission of an explicit reasoning-context control only when the selected target's default is proven equivalent. Otherwise preserve its exact carrier or reject, including when equivalence is unknown. Keep absent controls absent. This policy does not permit deleting messages, necessary reasoning replay or compacted history to simulate a context setting, and does not apply to a Messages document's `context` field.

Keep omitted normalization-default strictness distinct from omitted non-strict defaults and explicit `strict:false`. Current Responses semantics attempt normalization and permit best-effort fallback when normalization is impossible; the old unconditional rejection is superseded. Preserve that native behavior first, then assess cross-protocol equivalence. Explicit `strict:true` cannot become best effort merely to obtain a response.

The reverse direction has an exact carrier: an omitted non-strict source default becomes explicit `strict:false` on Responses. Materialize that default without changing source IR/schema. Native normalization-with-fallback requires its own evidenced projection, not unconditional conversion to plain tools.

Schema normalization and explicit strict tools have separate semantic requirements and capability checks. Supporting one does not imply the other. Native Responses keeps omitted `strict` absent and sends the bounded original schema, including non-normalizable schemas, for service normalization/fallback. Mandatory local strict-subset admission applies to explicit `strict:true`. Cross-protocol normalization-default rejection remains until an evidenced equivalent projection exists.

OpenAI terminal delivery uses selected approximations. Chat's `stop` officially covers a natural stop and a matched stop sequence, including beside forced tool calls, so natural and sequence-matched causes coarsen to it under `chat.coarsen-completion-cause.v1`; an ambiguous stop is exact. Responses `completed` has no cause carrier, so `responses.omit-completion-cause.v1` omits any cause. A recitation stop becomes OpenAI `content_filter` under `openai.recitation-as-content-filter.v1` and stays incomplete. OpenAI has no context-window reason: Chat reports `length` under `chat.context-window-as-length.v1`, and Responses stays incomplete with a null reason under `responses.omit-context-window-reason.v1`. Generic Messages still rejects ambiguous stops and recitation; its selected Google exception is defined below. These rules never turn a non-success outcome into success.

Strict Messages encoding uses the same terminal contract for JSON and SSE:

| Outcome and reported progress | Required evidence | Messages terminal |
|---|---|---|
| Completed, turn finished | Natural cause, no calls | `end_turn` |
| Completed, turn finished | Matched sequence, no calls | `stop_sequence` with that sequence |
| Completed, awaiting tool results or progress unreported | Completed pending calls, no completion cause | `tool_use` |
| Incomplete, progress unreported | Max-output, context-window or content-filter reason | `max_tokens`, `model_context_window_exceeded` or `refusal` |

Other combinations reject without a separately selected delivery projection. Response completion alone does not prove natural completion or end-of-turn. Completed pending calls establish required tool results independently of a reported disposition; `tool_use` carries those dependencies without inventing a source progress report. Messages output has no ordinary result items that could already resolve those calls. Unknown completed terminals without pending calls and explicit continuation reject in the strict codec.

Messages has no refusal block, and `stop_reason: "refusal"` reports classifier intervention rather than a model-authored refusal message. Messages delivery therefore keeps refusal content as text under `messages.refusal-as-text.v1`. A source without an output ceiling gets the required Messages `max_tokens` under `messages.default-max-tokens.v1`.

These are selected policies, not claims that all bindings support every conversion. Concrete mappings and default-equivalence checks use trusted target policy with source evidence, not client-selected fallback logic or upstream-error discovery. Native targets reject uncalibrated detail/context controls and filenames without carriers; a named rule alone does not establish approval.

#### Google Messages terminal projection

On 2026-10-21 the user selected basic-loop closure followed by a pause in further Google API expansion, accepting incomplete protocol coverage. Only the existing Gemini 3.8 Flash bindings opt into this delivery profile; generic Google capabilities default off. Select it for Messages from the actual candidate-local request with no stop sequences. Both Google encoders preserve nonempty stop controls and insert no stop defaults. A client field or reported model label cannot activate the policy.

| Validated source terminal | Messages delivery-copy projection | Evidence rule |
|---|---|---|
| generateContent completed, awaiting tool results, completed calls, `NaturalOrStopSequence` | Omit the completion cause; strict encoding yields `tool_use` | `messages.google-tool-stop.v1` |
| generateContent completed, turn finished, no calls, `NaturalOrStopSequence` | Resolve the natural/provided-sequence alternative using the known no-stop request; strict encoding yields `end_turn` | `messages.google-stop-without-sequences.v1` |
| Interactions completed, turn finished, no calls, no completion cause | Explicit compatibility approximation to `end_turn`, not proof of a reported natural cause | `messages.interactions-completed-as-end-turn.v1` |

Preserve the original decoded response and terminal. Change only completion details on a delivery copy, never outcome, progress, content, signatures or accounting. JSON and SSE use the same pure decision with direction-specific receipts; streamed terminals still require validated EOF. No configured-stop match is guessed. Unreported progress, continuation, incomplete/failed/cancelled outcomes and other causes receive no new mapping. Usage/report/content validation, attempt isolation, cancellation and no-post-publication-replay remain unchanged. Direct Messages codecs and Chat/Responses delivery retain their previous rules.

### Reasoning controls

#### Reasoning activation and effort scope

The IR separates activation from a scope-qualified effort control. [Reasoning types](../src/semantic/task/generation/content/reasoning.rs), [settings](../src/semantic/task/generation/request/settings.rs) and their codecs own the contract; target-specific integration limits are in [Roadmap](roadmap.md#integration-handoff).

Put activation on `ReasoningRequest` and one optional `EffortControl { scope, level }` on `GenerationSettings`. The initial scopes are `Reasoning` and `Response`. One scoped control is sufficient for the selected contracts; do not add two parallel effort fields without a counterexample requiring both at once. Keep protocol-relevant absence and null separate from active intent.

Activation distinguishes disabled from enabled reasoning; enabled permits reasoning and does not promise a nonzero token count. Execution/allocation mode, numeric budget and replay stay separate from presentation visibility/granularity. `EffortLevel` contains only positive levels: native off spellings normalize to activation, not a minimum effort. An omitted activation does not invent enabled behavior.

Disabled activation plus response-wide high effort is valid IR. Disabled activation plus a positive reasoning-only effort, adaptive/budgeted allocation or numeric thinking budget is contradictory. Standard/pro execution mode is not an activation switch; validate its combinations against the target contract rather than banning it solely because reasoning is disabled. Validate official wire variants separately before constructing this contract.

OpenAI non-off effort and Google levels have reasoning scope. Anthropic `output_config.effort` has response scope and must not change reasoning-object presence by itself. Preserve source scope in the IR. P1 selects target-private cross-scope approximation through trusted directional policy with loss evidence; equal level labels are not an equivalence. Concrete target mappings must be declared, including identities, and undeclared entries reject rather than silently inherit a same-label conversion. A binding without a selected policy retains rejection.

P2 gives explicit disabled activation priority when the target can represent off but cannot represent independent response-wide effort. Preserve off and omit only that effort through a named rule. A target with no off carrier still rejects; the approximation never enables reasoning.

Requirements and target contracts must distinguish response-wide effort from reasoning support. Reported settings use the same scoped types without acquiring request intent that was not reported. No Provider identifiers or mapping tables enter semantic types, and no compatibility-only effort field survives the migration.

##### Google response-effort projection

On 2026-10-21 the user selected request-side effort adaptation and static projection diagnostics only, not a new terminal or accounting policy. The existing Gemini 3.8 Flash generateContent and Interactions bindings select `google.gemini-3.8-flash.response-effort-to-thinking.v1`: response-scoped low/medium/high map to reasoning-scoped low/medium/high. This is a scope approximation, not equivalent response length, tool-call frequency, thinking depth or cost. The [source comparison](refs/2026-10-17-generation-contracts.md#google-messages-client-boundary-probes-2026-10-21) owns the evidence.

Apply the existing directional scope policy to the candidate-private request and record original/projected controls once. Already reasoning-scoped requests bypass this conversion; absent controls remain absent. Preserve activation, mode and the explicit output ceiling. Unmapped levels, disabled thinking, unselected numeric budgets and unsupported ordered controls still reject; do not saturate, compose presets or fabricate effective-setting reports.

Native Google contracts keep `response_effort:false`. Public/canonical input admission separately permits the selected approximation; topology promises use validated policy-adjusted input capabilities, while native codec ceilings remain unchanged. Final model-value checks run after projection in the same encoder used by preflight and preparation. Invalid policy declarations cannot establish a topology promise. Startup inherits the catalog scope policy when numeric conversion is omitted; an explicit numeric conversion replaces, rather than chains with, that policy.

This request-side policy changes neither usage nor terminal rules; Messages delivery has the separately selected [Google terminal profile](#google-messages-terminal-projection). Request acceptance alone is not full-loop acceptance.

#### Reasoning level calibration

The existing level interface performs explicit, hand-authored calibration for the same model under trusted binding/dialect selection. It is not a universal adapter for all reasoning controls. P1/P2 select scope approximation and disabled-priority handling; P3 selects preset-based numeric-budget conversion inspired by [pi's pinned implementation](refs/2026-10-15-generation-projections.md#pi-thinking-budget-presets). Request-policy machinery exists for OpenAI and native projections, and OpenAI intake has directional reported-level calibration. Those interfaces do not establish model-effective support or activate a binding.

Different Providers may expose different labels for one model. A selected rule may map `low` to `medium`, for example. Qualitative level-to-level calibration still requires explicit pairs; equal labels or ordinal proximity do not establish equivalence. P3 separately selects nearest-budget quantization as a named numeric approximation, defined below. Preserve original numeric intent and validate the target's mode, minimum and output ceiling.

P4 preserves supported sampling controls. A named target policy may omit controls known to be fixed or ignored by that model. Unknown effectiveness does not justify guessing or dropping them; keep the rejection until a valid carrier or explicit target policy exists. Protocol grammar and model-effective support remain separate.

The reviewed Gemini 3.8 Flash bindings select ignored-sampling omission for temperature, top-p and top-k on a candidate-local copy, with field-presence losses and `gemini.omit-sampling.v1`/`interactions.omit-sampling.v1`. Current Discovery documents these controls as ignored on Gemini 3.6 Flash and newer; Interactions v1 has no temperature/top-p/top-k request carrier. Generic generateContent retains native controls; generic Interactions rejects them. Other bindings do not inherit the reviewed model policy, and business model labels never select it. Preflight and preparation use the same encoder.

Encode and decode mappings are directional. A many-to-one encode mapping has no automatic inverse; normalization of an upstream report needs its own explicit rule. Preserve request intent and reported effective settings separately, never synthesizing a reported setting from the request. Apply each mapping once at its boundary, record its named provenance, and validate the result against the target contract.

##### Thinking-budget quantization

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

Each target assesses the particular control before invoking the selected conversion, including native scope, model levels, numeric carriers and effective output ceiling. It does not retry the entire encoder after an arbitrary error. Apply this independently to root and ordered controls: an already-native root control must not reject merely because an ordered control needs scope conversion. Native Messages can retain budget plus response effort, or disabled thinking plus response effort. A configured fallback is not an instruction to erase either exact value.

For generateContent, wire integer representability alone does not declare native model budget support. The exact-first predicate and budget-admission guard share the same capability decision: an explicit native budget declaration or the already selected numeric-target policy. Current catalog defaults remain closed; this introduces no new binding. Positive native budgets obey the selected Discovery `int32` carrier.

Retain the original budget and record the selected table, matched level and representative tokens as adaptation evidence, not reported model effort. Quantization does not bypass scope or target-level admission, invent a missing table entry, activate a binding or change route policy.

P3b rejects a request when its applicable explicit output-token ceiling is smaller than the thinking budget. This applies to both client-supplied numeric budgets and budgets generated by a selected level-to-budget conversion. Preserve the budget and ceiling; do not lower the budget, select a cheaper preset, raise the ceiling or disable thinking to make the request fit.

Passing this comparison is necessary, not sufficient. Equality is accepted only if the target contract permits it; stricter budget/output relations, protocol minima and required answer room still apply. Reject a target-invalid combination without automatic adjustment. No extra gateway-defined answer reservation or budget-reduction policy is selected. Ordinary preset lookup and nearest-budget quantization remain unchanged.

Rules cannot expand accepted content types, bypass model restrictions, select a route or modify the source IR. No public request override is selected. Qualitative level calibration remains hand-authored Rust policy. Trusted startup configuration may supply a public model's six numeric thinking budgets; each candidate explicitly selects budget-to-effort or effort-to-budget conversion. Omission selects no conversion. The existing algorithms, target restrictions and output-ceiling checks remain authoritative; there is no generic rule engine. P6's diagnostic switches configure capture, not semantic mapping.

##### Initial mapping plan

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

The code interface is [ReasoningEffortMapping](../src/protocol/reasoning.rs): hand-authored encode/decode pairs, effort scope and declared native levels selected by `ApiKeyBinding.effort_mappings` for an exact OpenAI protocol member. OpenAI endpoint construction carries that mapping into request encoding and static/SSE metadata intake; adapter selection validates its canonical-model identity. Bindings without a selected mapping use their protocol's representation rules, not a generated mapping. Native request-policy projection is separate from this qualitative encode/decode mapping.

Activation is outside the positive-level table, so a table cannot turn off into on. Absence and null never create a level. The interface does not add Provider-only level labels to the standard IR.

Selected policies calibrate settings and ordered configuration items independently. Responses intake checks native configuration snapshots across added, done and terminal boundaries before normalization. Request projection maps a private ordered copy; a configuration budget without an item carrier rejects. Always-reasoning bindings also reject disabled configuration history. A settings policy must not silently bypass historical native-level admission.

Validate duplicate keys, output levels, effort scope and binding identity at the trusted target boundary. Existing topology compilation also checks these declarations; its policy is frozen for this migration. Preflight and preparation project afresh from unchanged input through the same pure codec path when connected. Calibration precedes the final supported-level check, but cannot bypass source validation or final target restrictions.

Validate native types, alias conflicts and source-snapshot consistency before a many-to-one normalization can hide a disagreement. Responses intake compares repeated reported settings before effort calibration, separately from item-snapshot validation. Normalize only fields actually reported. Static and streamed metadata use the same rule, and downstream delivery does not reapply the source Provider's encode map.

##### Implementation owners

| Owner | Responsibility |
|---|---|
| [Semantic reasoning types](../src/semantic/task/generation/content/reasoning.rs), [settings](../src/semantic/task/generation/request/settings.rs), [requirements](../src/semantic/task/generation/requirements.rs) and [contract](../src/semantic/task/generation/contract.rs) | Activation and scoped effort only; no model-specific level policy |
| [Protocol reasoning helper](../src/protocol/reasoning.rs) | Explicit qualitative mapping pairs plus separate P3 preset lookup and nearest-budget quantization with highest-level ties; no inferred qualitative inverse |
| [Catalog binding declarations](../src/topology/catalog/bindings.rs), [catalog construction](../src/topology/catalog.rs), native catalog constructors and [endpoint compilation](../src/topology/endpoint.rs) | Select policy by canonical model and binding, validate it and pass it through representation caps to the adapter; do not reuse a Provider-wide switch for model-specific levels |
| [OpenAI adaptation](../src/protocol/openai/dialect/adaptation.rs), [request projection](../src/protocol/openai/adapter/request.rs), [Messages projection](../src/protocol/anthropic/messages/request.rs), [generateContent projection](../src/protocol/google/generate_content/request.rs) and [Interactions projection](../src/protocol/google/interactions/request.rs) | Preserve scope and apply a selected level policy once to candidate-private typed representation before final validation and serialization; no Provider-wide level calibration switch |
| [Upstream adapter](../src/adapter/upstream.rs), [Chat response intake](../src/protocol/openai/chat/response.rs) and [Responses response intake](../src/protocol/openai/responses/response.rs), [event intake](../src/protocol/openai/events/decode/mod.rs) and native decoders | OpenAI carries its selected decode mapping into actual reported-setting parsing, including SSE. Any future native report calibration needs independent policy propagation; do not put it in `ReplayTarget` or discover it from a response model string |
| [Fidelity](../src/protocol/fidelity.rs) and existing named request rules | Record bounded normalization provenance and actual encode adaptation, without restoring unsupported controls or synthesizing effective settings |

Native `Endpoint::adapter()` currently carries profile/replay labels rather than all representation caps. Native request conversions do not supply an implicit inverse for reported settings. If a concrete native binding needs reported-level calibration, select an independent decode policy and propagate it explicitly into intake before claiming support; request projection alone does not establish it. The ordinary downstream client decoder uses its declared client contract, not a guessed Provider policy chosen from business JSON.

Keep [candidate preflight](../src/execution/plan.rs) and transport free of mapping tables. Preflight already invokes endpoint encoding, so preparation must reuse that same codec path rather than add a second execution-layer mapper.

### Media

Image, audio, video and file resources are inert: inline data or a URL, never fetched, decoded or uploaded by the gateway. Each kind is explicit, because a URL source carries no media type. An inline file keeps its declared MIME, and targets that dispatch by MIME may carry it.

- Chat video uses the registered extension `{"type":"video_url","video_url":{"url":…}}`, shared by OpenRouter, DashScope, Zhipu, vLLM and ModelBest. Responses video uses `{"type":"input_video","video_url":…}` from Open Responses and OpenRouter, in user messages only. Only targets whose dialect declares the carrier receive video. Responses clients may send `input_video` in tool results, but no bound target carries tool-result video: OpenAI and Interactions results have no video part, and Gemini function responses admit only PNG, JPEG, WebP, PDF and plain text.
- Native codecs reject explicit image/file detail without a declared carrier or selected target rule, and filenames without exact carriers. No target-default equivalence is inferred from a missing slot. Messages document `context` uses the [per-use owner and separately gated carrier](#tool-examples-and-document-context); other model-visible options without an owner remain rejected.
- A file name is the model-visible document name: OpenAI `filename` and Messages document `title` map to each other. A present name needs an exact carrier; do not drop it or inject it into text.
- Instructions are text-only, including Responses system/developer messages and Messages mid-conversation system messages. Reject image, audio, video and file parts before target selection, even when a shared SDK content union admits them; user and tool-result media retain their separate contracts.
- Tool results may carry media of any kind. Admission needs tool-result media and the kind's input modality. An inline file whose MIME type is image, audio or video also needs that modality, in addition to file input. Responses carries tool media; standard Chat does not, so Chat targets reject it.
- generateContent preserves text tool-result parts under `functionResponse.response.output` or `error`, but rejects tool-result media: the reviewed schema/SDK does not establish a usable `FunctionResponseBlob.displayName` carrier. Do not manufacture `$ref` linkage, drop media or turn it into text. Ordinary user media and the separately supported Interactions text/image result carrier remain unchanged.
- Provider file IDs and file URIs are bound to the gateway credential's workspace or project. Client-supplied references are rejected.
- User and tool-result media uses admit only inline data or URLs. Generic opaque/namespaced identities remain available for citation sources, not model-input media.
- Google protocol boundaries reject media URLs on the Files API host, even when a client labels them as ordinary image or file URLs. Public URLs stay inert.
- Image and file formats, encoded sizes and counts are target constraints, including tool-result media. Pixel dimensions and actual image validity remain upstream checks; the gateway does not decode or transcode images.
- A Responses `prompt_cache_breakpoint` on an image or file uses the [owner-local boundary](#canonical-ownership) and reaches Responses targets; Chat and native Messages/Google targets reject it rather than infer a TTL. Native Chat content-part `prompt_cache_breakpoint` also rejects at intake until its carrier is selected, before permissive client filtering can discard it.
- A Messages image with `transformations.oversized_image: "error"` sets the image use's `RejectOversized` policy and keeps that exact carrier on Messages targets. Other targets reject the guarantee because they may downsize. Omitted/default `downsize` means `AllowDownsize`; null outer transformations also retain the default, but a supplied null `oversized_image` rejects. Encoding reads the current use, never a stale part-ID record.
- A failover route may include candidates narrower than the public contract. Requests they cannot represent skip them, which reduces failover rather than admission.

Carrier tables and sources: [Multimodal carriers](refs/multimodal.md).

### Adaptation evidence

Every selected loss produces bounded typed evidence with a stable rule, direction, semantic field category and action. Safe control summaries show enum values and presence; numeric request summaries allow only thinking budgets, representative preset tokens and output-token ceilings. Keep request intent, sent controls, upstream reports and delivered facts distinguishable. Evidence cannot restore omitted values, change semantic meaning or select an upstream.

Keep the bounded typed projection evidence needed for adaptation correctness; operational reporting follows [Observability](#observability), not a parallel diagnostic model or generic rule engine. Record losses independently once per affected control or aggregate repeated event occurrences within existing budgets. A prior refusal receipt cannot suppress a later usage-view receipt. These summary records do not contain bodies, prompts, schemas, arguments or opaque values; full business content belongs only to the separately controlled capture below. Arbitrary input strings and unknown field names are not automatically safe summary labels.

Evidence belongs to a successfully checked projection boundary. A rejected batch adds no new receipts; earlier successful prefixes retain theirs when later intake or encoding fails. Late-reported facts follow the same rule. Native grouping and accounting transformations record evidence before their source ownership is removed. Encoding evidence alone proves neither network transmission nor effective Provider behavior.

Required output evidence is established by explicit request controls, the source contract or continuation requirements. Preserve it or reject an incompatible target. Selected incidental settings echoes and auxiliary reports may be omitted after source validation, with evidence. Absence of a request flag alone does not prove that citations or other content are incidental; deferred citation support remains deferred.

Chat omits incidental settings echoes because its response has no settings carrier. Responses omits unsupported independent reasoning-control echoes while retaining representable settings. Strict-complete delivery rejects either loss. These rules never drop generated reasoning, probabilities, citations or continuation values. Messages rejects unsupported probabilities and citations; its retained usage uses the same explicit cache/total view as delivery.

#### Domain evidence interface

**Internal interface selected and implemented on 2026-10-24.** `lowering::generation` owns `Conversion<T>`, `EvidenceDelta` and the single `Effect` enum. `execution::report` owns bounded request/attempt observations and checked-batch associations; codecs retain no mutable execution report. This is not a public diagnostic schema, new conversion permission or production telemetry activation. The logical shape below is schematic: effects use specific enum variants rather than optional payload bags.

```text
Conversion<T> {
    value: T,
    evidence: EvidenceDelta
}
EvidenceDelta {
    effects: bounded Effect entries,
    coverage: selected-boundary coverage,
    omitted_entries: saturating count
}
Effect {
    rule: RuleId,
    boundary: ConversionBoundary,
    locus: closed Field plus typed Owner,
    change: Change,
    summary: optional typed SafeSummary
}
```

`RuleId` is a versioned compile-time identifier with no runtime registration or duplicate revision field. An effect's logical locus combines `Field` with `Owner`: root, scoped item/part, tool-definition ordinal or usage scope/counter. Reuse semantic IDs and accounting enums; do not accept arbitrary JSON pointers, schema keys, tool names, wire IDs or strings from input. A locus describes the affected source owner, not a generated wire alias; target-only fields use the explicitly identified delivery view. One-to-many transformations may report a bounded set/range or a counted aggregate, never silently substitute one unrelated owner.

Boundaries are closed request/response/event decode, project and encode operations. The pure producer stamps the boundary where meaning changed; an outer serialization check cannot relabel a projection as an intake effect. The execution wrapper supplies the attempt binding, view and checked unit, not fields in the semantic IR. Views distinguish request intake, source response intake, upstream request, downstream JSON, downstream SSE and retained JSON. Static rules/profile declarations stay in protocol owners; the report references their stable IDs without becoming a rule registry or an executor.

| Interface owner | Responsibility |
|---|---|
| `lowering::generation` shared vocabulary | Pure conversion result/delta, field/locus, change/method and boundary types; no execution, Provider or telemetry dependency |
| Protocol codecs/lowering | Construct effects and typed leaf failures at the responsible transformation; no mutable report, I/O or publication state |
| `protocol` shared source-failure vocabulary | Body-free error-envelope meaning/basis shared with HTTP intake; trusted dialect classifiers remain at their current responsible boundaries |
| `adapter` | Contextual codec/representation failure and protocol dispatch; transport no longer needs to recover meaning from strings |
| `execution` | Request/attempt report, atomic checked-unit acceptance and view-specific batch associations around existing lifecycle owners |
| `gateway` / transport | Actual send/head, publication gate, body acknowledgment and interruption observations; log/wire projections cannot change those facts |

| `Change` family | Meaning and required basis |
|---|---|
| `Normalize` | Equivalent representation; record only where a selected normalization/provenance question warrants it, not for every exact field |
| `Derive` | Exact result with established premises and a named formula/method |
| `Default` | Source-defined default or explicitly selected target-policy insertion; distinguish those bases and never label either as an explicit upstream report |
| `InferFromRequest` | Delivery presentation copied from known request intent; not evidence that the model actually used or reported it |
| `Omit` | A selected meaning has no retained target carrier; identify that field/owner |
| `Approximate` | Selected behavioral mapping or coarsening; name its method |
| `Estimate` | Eligible unknown quantity filled by a selected bounded method; identify each estimated counter, not the whole response |

Use enum variants carrying their necessary basis/method, rather than independent optional strings that permit an estimate without a method. `SafeSummary` has specific bounded control/presence/accounting variants, not `serde_json::Value`, arbitrary `Debug` or before/after IR snapshots. Preserve the existing request-summary allowlist. Defaulting a genuinely unknown token quantity to zero is not a source default or exact derivation; it remains an estimate only where the existing policy permits one.

One transformation can produce several effects. Exact input/cache arithmetic may derive an aggregate while separately omitting source breakdown/provenance; these must not become one generic usage loss. Change kind is not severity: exact normalization/derivation are not warnings, and recording an omission or estimate grants no authority to discard protected meaning. Retain source reports in `UsageReports`/response context and delivery values in `GenerationDelivery`; evidence explains the difference without a second writable value/provenance store.

The returned delta replaces parallel `RequestControlLoss`, request rule-name lists, `LossEvidence`, `ProjectionStage`, adapter-specific omission lists and audit-only fidelity normalizations. Human-readable names and log/wire codes are projections, not separately maintained receipts. Replay bindings, native identity/dependency proof and genuine source reports stay in their current owners. History retention or client echo must not reapply old diagnostic effects as new authoritative source observations.

`Change::Default(DefaultBasis)` distinguishes source-contract defaults from selected-policy insertions. Completion's `UsageMethod` names each exact arithmetic, selected estimate or required-counter completion independently; one estimated counter does not label the whole usage record as estimated. Existing profile defaults remain explicitly selected policy, not newly observed native fields.

Accepted request-intake evidence is request-owned with its original boundary, view and checked-unit identity, not assigned to the first candidate. Preparation can reference it but cannot re-emit or recount it for every attempt, rebind it to the upstream-request view, or discard it when preparation fails. Only newly produced candidate/request-encoding effects remain provisional until that preparation succeeds. Source-response intake similarly stays attempt-owned and is not recounted for every downstream or retained view.

Effects aggregate only within their request/attempt, boundary, view, rule, locus, change/basis and safe-summary identity. Counts describe checked conversion occurrences, not tokens or distinct model facts. Emit deltas once and consume the returned unit once; do not repeatedly scan cumulative encoder lists. Preserve separate checked, no-carrier and handoff counts when aggregating. Coalescing repeated deltas must not cause unbounded per-event state or merge different owners/methods.

Both entry count and retained bytes, including indexes and safe summaries, have a finite optional allowance in the responsible request/attempt/delivery resource envelope. Exhaust that allowance by truncating diagnostics, not by rejecting otherwise valid business data or allocating without bounds. A saturated optional evidence delta returns the business value plus explicit omitted-count/coverage state, not a fabricated lossless result. Reserve failure/terminal summaries outside the optional effect-entry allowance. Coverage is `assessed-within-declared-boundary`, `partial` or `unassessed`, independently of truncation; assessed coverage means the selected fields/rules at that boundary, not complete official-protocol conformance. Empty or truncated evidence proves neither native support nor losslessness. Export loss remains a separate telemetry property.

Current allowances are 64 entries/16 KiB per delta or report evidence owner and 32 live batch associations per attempt. Association saturation loses detailed attribution, never mandatory body acknowledgments. Request and attempt results, semantic task outcome, source classification, public code/status and observed routing policy remain separate fields; successful recovery leaves no failed-attempt cause on the successful request. Planning rejection retains its typed request-level cause without inventing an attempt or exposing endpoint IDs. Advancement is observed only when a successor starts; a cooled successor's selection observation cannot overwrite the failed attempt's health. Generation byte totals project acknowledged report counters, not a second producer-resumption counter.

#### Atomic evidence and output batches

Pure transformations return `Result<Conversion<T>, ...>`; their effects are candidates until the responsible outer boundary accepts the whole returned unit. That boundary includes source validation, selected projection, final representation checks and applicable serialization/framing/byte limits. Its success hands immutable evidence to execution; its failure returns a typed cause and **no new success receipts**.

For static output, the checked unit includes the fully serialized bytes and corresponding typed history. Request preparation also includes body limits and trusted header preparation. For SSE, it is one caller-visible encoding invocation and all the frames it returns, not each successful inner event. If event A encodes and event B fails inside that invocation, discard its temporary bytes/effects together; do not return an internal checked-prefix receipt. Earlier independently returned successful units remain. Rejected stateful encoders may be poisoned instead of restoring their semantic state; this transaction requirement applies to publication/evidence, not to cloning an entire reducer.

Keep the existing preparation/coalescing owner but distinguish these encoding outcomes explicitly:

```text
Ready(EncodedBatch { nonempty frames, evidence })
NoCarrier(EvidenceDelta)     // intentional transformation completed
Deferred                    // no checked output yet; owner retains pending state/effects
```

A queue operation may return ready prepared units and retain a pending run simultaneously. `Deferred` therefore describes the particular encoded unit, not the whole stream. Empty `Vec<Bytes>` alone cannot distinguish buffering, waiting for a future metadata carrier, a no-op, and completed omission. An assessment-only report may deliberately have no carrier; a control projected into an opening frame that has not yet been serialized is still deferred. Neither outcome publishes bytes merely to expose a notice.

Pending effects follow the frozen `PreparedEvents`/metadata context through coalescing; a later source snapshot cannot reattribute them. Hold buffers retain bounded batch associations instead of flattening away evidence ownership. If serialization of a held/coalesced unit later fails, discard that unit's provisional effects. Successful already checked units abandoned before publication remain checked-but-not-handed-off in their attempt report, not losses delivered by the next attempt.

Execution observes progress around existing I/O calls; it does not add a second retry/delivery state machine:

| Observation | Owner and permitted conclusion |
|---|---|
| Preview/preflight | Pure planning result only; never an actual attempt, prepared request or sent effect |
| Checked input | Successful intake/reduction unit; terminal/body completeness still waits for the existing EOF checks |
| Prepared request | Complete bounded upstream request exists; no send or Provider acceptance is implied |
| Send started / response head observed | Transport invoked / actual response status available; neither proves all request bytes arrived or controls took effect |
| Checked output / no carrier | Complete output unit accepted / intentional transformation completed; no I/O handoff is implied |
| Publication gate set | Preserve the existing conservative freeze before enqueue; it forbids replay even if the first handoff fails |
| Body handoff | Existing body-consumer acknowledgment of one complete `Bytes` item; not socket flush, client parse or remote persistence |
| Final delivery | Existing finalized encoding, verified upstream EOF and final handoff conditions all hold; does not replace semantic task outcome |

Use the responsible internal request scope or existing execution attempt identity plus a bounded monotonic checked-unit ordinal; neither wire response IDs nor naked codec-local indexes establish that binding. A non-cloneable execution batch handle associates effects with its total frame count and acknowledged prefix. Per-frame effect maps are not required initially: conservatively count carrier-associated effects as fully handed off only after the complete batch is acknowledged. A partial batch records its prefix and uncertain effect-to-frame attribution, not a blanket delivered-loss claim. `NoCarrier` never becomes fully handed off by the vacuous equality `0 == 0`. Completed units can fold into bounded aggregates; retain detailed progress only while necessary for held/in-flight output.

Mandatory I/O acknowledgment/completion bookkeeping remains with its existing lifecycle owner, independently of optional evidence capacity. Exhausting diagnostic association/index capacity cannot skip an acknowledgment, lose cancellation handling or reject business output. Retain bounded incompleteness/counts when detailed attribution is unavailable; do not infer delivered effects from the surviving subset.

Previously returned successful content must be drained at its existing boundary before a following barrier. `finish_stream` and `fail_stream` must not hide such content inside a new all-or-nothing terminal/failure operation. A terminal serialization failure adds no terminal receipts and cannot erase the earlier checked prefix. Gateway failure frames are error-delivery units, separate from an adapted successful Generation terminal; their handoff does not call success-only completion logic. Publication/health behavior on cancellation remains unchanged.

Retained JSON generated after SSE is a distinct projection view, not a reconstruction of stream events or a reason to add JSON-only losses to the delivered-SSE view. Storage staging/publishing keeps its existing owner. The same report supplies view-specific summaries; it does not require equal loss sets across JSON, SSE and retrieval. A terminal response can describe only evidence checked before its serialization, never certify its own eventual handoff. Cancellation may prevent any final diagnostic notice.

#### Typed conversion failures

Keep semantic failed/refused/incomplete/cancelled outcomes in `GenerationResponse` and source `ResponseError`/terminal details. They are not automatically malformed input, codec rejection or transport failure. The failure interface below describes a rejected local conversion or a standalone source error envelope, not every non-success task result.

```text
ConversionFailure {
    boundary: ConversionBoundary,
    locus: optional Field,
    cause: Codec(CodecError) | Representation(RepresentationError)
}
SourceFailureEvidence {
    kind: shared closed source-failure kind,
    basis: status | envelope code/type | named message pattern | unrecognized,
    status_hint: optional ProtocolStatusHint,
    observation: complete | interrupted | over-limit | intentionally-unread
}
```

Evolve the current adapter error boundary into the contextual `ConversionFailure`; retain its codec/representation sum rather than adding a second competing error tree. Pure codecs and lowering continue to return their own typed leaves. Tag the first responsible decode/project/encode boundary while the cause is still available and preserve it through outer wrappers. Execution adds the attempt and retains its own transport, lifecycle, timeout, cancellation and delivery failures in `AttemptError`. Never infer the rejecting boundary from a tracing stage or reconstruct causes from `Display`, public error messages or generic `invalid_request`.

Retain `GenerationError`, `EventError`, `ContextError` and `RepresentationError` leaf identities. Replace Generation's string-only invalid/unsupported labels with closed shape/relationship/carrier reasons and a safe locus; distinguish bounded resource failures such as depth/nodes, bytes, events and projection limits. Execution-only `Protocol`/`Delivery` labels likewise become closed reasons. Add variants at migrated responsible sites, not a speculative generic error catalogue or an arbitrary context stack. Unknown names/values and raw parse text never become diagnostic labels.

`CodecError::Representation` preserves its original typed representation cause and responsible boundary until adaptation wraps it in `ConversionFailure`. `JsonError::Limit(JsonResource)` preserves bytes/depth/nodes through codec translation. Execution distinguishes request-body, attempt and outer-exchange deadlines and records body-consumer closure separately from source failure. Body-free failure serialization is an allowlisted projection; the internal report retains the typed cause rather than its rendered label.

Replace `StreamFailure(u16)` with `SourceFailureEvidence`. Share the existing source-failure vocabulary and pure classifiers between protocol error-envelope decoding and HTTP-error intake; do not duplicate it in the report. Raw bounded error bytes remain ephemeral classifier/capture input. Actual HTTP status belongs to the execution response head, separately from an SSE code/type or a protocol's suggested status mapping. `ProtocolStatusHint` is constructed only by the responsible protocol mapping and retains distinctions needed by existing policy, including hints that share an overloaded classification; it is not an arbitrary source-supplied integer or a second observed response head. In particular, an error event inside HTTP 200 is not an observed HTTP 429/400. Record incomplete or deliberately unread error-body evidence independently of recognition: a complete unknown envelope remains unrecognized; a status-only classification may be known even when its body was not read.

Execution derives its existing fallback/health decision and the public error presentation from typed facts before rendering. Preserve `ErrorClass`, status/authentication precedence, credential scopes and no-replay rules. Diagnostics are not retry authority. Richer SSE classification does not silently enable new advancement, health or public-error behavior: preserve the current policy decision during migration and select any behavioral correction separately. For example, HTTP 429 quota evidence currently survives a body classifier, whereas a status-only SSE path cannot distinguish quota from throttling; the new interface retains that distinction without itself approving a different recovery policy.

Keep each failed attempt's original cause even if a later attempt succeeds. Within an attempt retain an already observed source failure when cancellation or failure-frame encoding/handoff later fails; record the later handling failure separately within a finite allowance rather than overwriting the source cause or constructing an unbounded chain. Final request success contains no inherited prior-attempt terminal error. Public `ApiError` is a presentation selected alongside an internal execution decision, not the owner of diagnostics; neither report truncation nor exporter failure can change that decision.

#### Public capability and adaptation notice

The capability snapshot describes supported request domains, delivery and history contracts, not unions of candidate feature bits or model-effective guarantees. Unlisted cases remain unassessed. Keep `/v1/models` standard and hide credentials, deployment origins and private candidate identities.

**Actual-loss reporting was selected on 2026-10-23; the subsequent 2026-10-24 module selection resumes unified ownership and publication.** The [domain interface](#domain-evidence-interface) supplies independent log and wire projections. `AdaptationNotice` is a bounded optional `morphiecore` field, not serialization of the internal report or a stable public schema. It admits at most 16 entries/2048 bytes, reports omitted coverage and preserves standard output when the optional notice exceeds the complete payload budget. JSON attaches it only after checked serialization; SSE uses existing final Chat JSON, Responses terminal-response and Messages `message_delta` carriers, never an invented event. Retained JSON is a separate checked view, not required diagnostic parity with SSE. A terminal notice cannot certify its own eventual handoff. Error, stop, refusal, annotation, safety and usage fields are not generic warning containers. Standard fields retain their official meaning; an extension cannot establish full conformance merely because an SDK tolerates it.

**Safe-to-ignore diagnostics selected on 2026-10-24.** MorphieCore may expose additional namespaced response fields for a future deployer-owned Agent Runtime to display or analyze. With those fields ignored or removed, the supported standard SDK Agent loop must still preserve content, tool calls/results, failure and terminal meaning, and promised continuation. Clients need not parse, display, aggregate, retain or return the extensions. SDK acceptance, raw-event visibility and final-object retention are different properties; none implies the others. This is an official-protocol baseline with optional diagnostics, not a claim that every closed-schema consumer accepts extra fields.

Expose selected diagnostic views, not serialized internal objects: actual losses and estimates, supplementary error context, public routing-policy labels and observed routing outcomes are eligible. Keep configured policy separate from what actually happened; enabled failover is not evidence of a failover. Routing observations remain execution-owned and do not change candidate selection, affinity or health policy. Do not expose credentials, private origins/configuration, candidate identities, business values or internal Provider/model rule names. Use deliberately selected fields and bounded values, not an arbitrary passthrough bag.

The current notice aggregates closed field, local owner, boundary and change categories with occurrence counts. Internal rule/method IDs stay in the body-free log projection, not the wire summary; aggregation therefore does not imply one particular internal method. Wire field organization remains unselected beyond this safe subset.

One bounded execution-owned adaptation report is authoritative. Pure codecs return immutable evidence; commit it only after the responsible transformation and serialization checks succeed. Distinguish prepared, sent and delivered effects. A response reports its producing attempt, not rejected/abandoned candidates; valid published prefixes survive later failure. Diagnostics and JSON/SSE/stored retrieval project this report rather than maintaining separate mutable ledgers.

Actual-loss entries identify the affected fields and use short descriptions and selected codes, with method identifiers for estimates. Exact carriage and derivation are not warnings. Bound diagnostic bytes/counts within existing budgets; absence or truncation never proves that no adaptation occurred. Public field layout, naming, grouping, aggregation and a stable consumer-facing schema remain provisional under the [current scheduling decision](roadmap.md#standards-reset-and-redesign); internal evidence-interface convergence does not finalize those wire contracts or activate their dedicated tests.

Attach streaming fields to valid protocol envelopes; preserve standard event order and terminal grammar. Do not fabricate content, reasoning, usage chunks, errors or success solely for a notice. Receipt-only output must not prematurely publish a held candidate. Headers alone cannot describe late observations, and cancellation cannot guarantee final notice delivery. A client may discard diagnostic fields during parsing or aggregation; no cross-SDK final-object retention or next-request echo is promised.

Errors still use standard HTTP/error/terminal mechanisms; an extension only supplements them. Optional diagnostics cannot authorize a prohibited loss, withdraw an explicit guarantee, carry the only copy of a required signature/continuation value, or grant tool execution authority. Future Runtime use for automatic retry, routing, loss rejection or history recovery requires a separately selected control contract with defined completeness and missing-field behavior.

The `X-MorphieCore-Adaptation: possible` header and partial snapshot remain conservative declarations, not actual-loss evidence. The optional notice above is implemented without a universal consumer-retention claim. Extension-specific tests and information organization/aggregation are deferred to the deployer's final production-preparation decision in the [roadmap](roadmap.md#standards-reset-and-redesign); no public receipt store, new Google ingress or routing change is implied.

## History and replay

### Reasoning replay

Real semantic replay owners, dependency checks and trusted issuer isolation remain required. Opaque native values may use the tagged `mc1.` envelope below, but gateway manifests must not appear as fabricated reasoning/thinking owners. Any additional semantic extension requires its own contract and client-retention evidence. Internal opaque state stays separate from public retrieval, without a lossy typed-history → temporary-wire → typed-history composition step.

**Native-only delivery selected and implemented on 2026-10-24.** Preserve genuine native reasoning/signature carriers. Signed ordinary text/calls without a standard downstream carrier remain available only in typed gateway-managed Responses continuation; incompatible stateless delivery rejects. No client encoder emits or restores `mca1.` manifest owners. Responses/Messages reject recognized legacy manifest values; official Chat does not interpret private reasoning fields. Execution-owned storage authorization, not upstream `store` metadata, permits richer typed history. Do not assume unknown-field retention by pi/opencode. Public retrieval is not a stateless export of hidden managed continuation.

Opaque upstream values reach clients in a tagged envelope:

```text
mc1. + base64url JSON: {o: issuer fingerprint, f: native format, d: payload}
```

Runtime fingerprints bind Provider, upstream model and the normalized trusted deployment origin/path. They do not include public aliases, credentials or ingress keys, and reveal no URL or credential locator. Changing a deployment does not authorize replay of another deployment's opaque values. Clients must treat the whole value as opaque.

Once an item's replay value is final, item closure may repeat it unchanged but cannot remove, downgrade or rewrite it. Final replay does not imply a completed artifact. Partial-to-final replacement preserves format and intake origin.

1. Restore the native payload only for the matching deployment/model and carrier.
2. Transfer visible history to an incompatible target by omitting complete foreign reasoning, including its readable thinking and opaque value. Do not present another model's thinking as an assistant answer.
3. Preserve visible messages, media and real call/result associations. A tool turn need not have a final assistant answer, but incomplete owners or partial replay cannot be repaired by omission. Compaction and cached context remain required input.
4. Never send bare, malformed or unsupported-version envelopes upstream.
5. Reject damaged compatible replay or an unavailable required-context carrier. A different native replay format is an explicit transfer case, not permission to weaken compatible replay checks.

The selected pi-inspired transfer uses `[chat,responses,messages,gemini,interactions].transfer-visible-history.v1`. It changes only the target copy and records the loss; it is not equivalent reasoning continuation. Dependencies bound to the selected target are checked before foreign removal, so transfer cannot hide a missing or edited compatible replay owner. Responses call-item wire IDs associated with removed reasoning are forgotten; semantic call IDs and results remain unchanged. A mixed assistant run with retained replay and potentially shared call-item aliases rejects rather than guessing which dependency may be removed. No tool results, validator-bypass values or replacement user messages are synthesized.

Reasoning without provenance is not automatically foreign. `reasoning_as_text` remains disabled by default and controls the existing explicit conversion of unattributed readable reasoning when no exact native carrier is available. It does not enable or disable foreign-history transfer, nor change reasoning display controls. Valid native carriers remain available. A changed item kind gets a derived identity in the candidate copy, not the issuer's identity.

Text conversion preserves the reasoning item's lifecycle. A partial or in-progress owner never becomes completed merely because its readable text has a carrier. Responses can retain that status; targets without a partial-history carrier, including native Messages, reject it.

The envelope is not encryption, authentication or local proof. It cannot choose a route, origin or credential. The issuing upstream validates its payload.

Some native thinking is account-bound. Exact-credential affinity helps preserve it, but deployment/model compatibility alone cannot guarantee replay after credential fallback.

Carriers include Responses `encrypted_content`, upstream Chat encrypted reasoning details, Anthropic thinking signatures and redacted thinking `data`, Gemini `thoughtSignature` and Interactions signatures.

Redacted thinking is an opaque-only reasoning item with its own replay format, so the native block type survives the envelope. Messages clients receive it as `redacted_thinking` whose `data` is the envelope; a streamed reasoning block opens on its first delta or at its end, once a redacted value can be recognized.

generateContent signatures on ordinary text and function calls belong to `Part.replay` and `ToolCall.replay`, with a distinct `GoogleGenerateContentSignature` format. Thought state remains on its reasoning owner; Provider-tool replay stays separate. Fidelity binds part identity, ordinal, text, lifecycle and value, or the call's identity, name, exact arguments, context, lifecycle and value. Sealing checks recorded obligations even when an owner or token was removed.

On 2026-10-20 the user selected the observed Interactions v1 `function_call.signature` carrier: existing `ToolCall.replay` with a distinct `GoogleInteractionsV1FunctionCall` format. It is call-only, inadmissible on message parts, reasoning or compaction. The bounded payload stays codec-local, with existing call dependencies and deployment/model compatibility; envelopes do not add authenticity. Retain the signature unchanged, finalize on call closure, preserve it in typed managed Responses history and emit it only on the native function-call step. Thought state keeps `GoogleInteractionsV1Thought`; neither format borrows generateContent state. Snapshot signatures compare against observed closed owners and cannot introduce new replay. No raw step bag, semantic owner, hosted tool or generic lifecycle framework is added.

- An attachment is never a tail-anchored or call-ID-anchored surrogate reasoning item. Moving or dropping its visible owner cannot silently move or erase required state. Signature-only surrogates returned from earlier gateway history are dropped with `gemini.omit-legacy-signature.v1`; other malformed thought payloads still reject.
- An explicitly observed empty signed text part keeps its text owner. A signature on an empty streaming fragment may also close accumulated text. Both differ from a missing carrier.
- Distinct parts within one generateContent frame retain their boundaries; unsigned preceding text never becomes part of a later signed owner.
- A signed thought closes its owner. Later thought text starts another item, so adjacent signed parts never overwrite one another.
- Interactions call signatures are an explicitly selected observed-v1 profile, despite their absence from the inspected v1 reference and pinned SDK type. Built-in tool signatures remain a separate deferred carrier.
- Preserve every compatible native signature, even when an upstream sample accepts an omitted one. An empty thought remains ordered reasoning, not a movable attachment.
- Foreign part/call replay is removed only from completed owners with final values; visible text and real calls/results remain. Affected call wire aliases are cleared without changing semantic call identity.
- generateContent and Interactions are separate formats. Foreign state is omitted under the selected transfer policy, not translated.

Historical pi parser/request-builder checks exercised superseded manifest paths. Their probe and restoration machinery were removed, not accepted as official conformance. Equal-owner, exact-number, lifecycle, dependency and isolation safeguards remain in managed-history and native-replay tests. Installed-client reacceptance is separate; see the dated [protocol notes](refs/2026-10-17-generation-contracts.md#signed-google-parts).

### Chat reasoning extensions

Public Chat defaults to the official Standard dialect. Generated reasoning and signed replay without an official Chat carrier reject in JSON and SSE; no readable-view leakage, fabricated owner or silent omission is allowed. Standard `reasoning_effort` and usage reasoning counters remain separate supported facts.

**Explicit reasoning profile selected and implemented on 2026-10-24; installed-client and Provider acceptance remain separate.** The trusted public-model `chat_profile: readable_reasoning` selects a narrow Chat semantic extension, independently of every upstream dialect. It carries readable `reasoning_content` in assistant JSON, SSE deltas and returned history, plus the independent `preserve_thinking` history-reading intent. Standard effort controls remain authoritative; this does not enable private activation/budget aliases, top-k, video, structured reasoning details or opaque replay. Reject unsupported former private fields at their actual request/history positions, even at null/false/empty values, rather than silently discarding them. Never reinterpret tool arguments with the same names.

The initial shared pi/OpenCode profile carries real unsigned readable reasoning only. Signed/structured owners and reasoning that resumes after answer/tool output remain unsupported rather than being flattened or hidden. Native signed Messages and managed Responses retain their separate contracts. Client retention and complete tool continuation require actual host-installed Agent evidence; this semantic extension does not inherit the diagnostic-test deferral.

Known incompatible output requirements reject before upstream dispatch when established by trusted binding evidence and effective request controls. Selected Bailian Qwen3.8 Chat bindings require a readable-capable downstream unless the prepared native request explicitly disables thinking. This is a binding output requirement, not parser capability or a claim that every generation contains nonempty thoughts. Check the final candidate-local wire after effort/activation adaptation; never change it to satisfy the downstream. Candidate order and rejection policy remain unchanged. Runtime JSON/SSE representation and honest failure checks remain necessary for unpredictable outputs.

The following OpenRouter shapes remain upstream dialects only:

| Carrier | Meaning |
|---|---|
| `reasoning`, `reasoning_content` | Readable reasoning view |
| `reasoning_details` | Ordered reasoning items for replay |
| `reasoning.text` | `text`, optional `signature` |
| `reasoning.summary` | `summary` |
| `reasoning.encrypted` | Opaque `data` |

Details may also carry `id`, `format` and `index`. Signatures and encrypted data use the replay envelope above. Target-native history returns details unchanged.

One assistant message carries the reasoning items ahead of it. A replay value closes its item; adjacent items without values merge. The readable view concatenates readable details.

Streamed readable deltas carry both readable views and no empty opener. Client input does not rely on returned indexes or format labels because target clients merge fragments.

For registered plaintext views, absence, null and an empty string make no text assertion. Nonempty duplicate views must agree with each other and with the readable structured view. Malformed known values still fail. An empty details list has no parts; explicit structured parts, including empty text, retain their content, order and owners. This normalization does not change visibility controls or relax post-terminal lifecycle checks.

Register upstream extension shapes here one at a time, not in the public official Chat baseline or the readable-only profile. The old `_openbridge` attachment is rejected and never emitted.

DashScope's upstream Bailian Chat carrier is readable `reasoning_content` plus the independent `preserve_thinking` control, not OpenRouter encrypted details. These controls are not registered on the public official Chat baseline. `ReasoningRequest.read_history` preserves the distinct history-reading meaning without inventing effort, activation or retention. Do not map it to `CurrentTurn` merely by name. Readable reasoning cannot resume after text/call output begins.

### Compacted history

A compaction item owns opaque context at one position in the ordered history. It is not a readable summary, reasoning or a message with an invented role. Responses preserves its identity and `encrypted_content` in input, static output and streamed item snapshots. Only a completed opaque value can enter replay history; an unfinished stream cannot become a static compaction item.

Compaction uses the [replay envelope](#reasoning-replay) with a distinct format tag. The gateway restores the native payload only for the matching deployment/model and Responses carrier. Bare, malformed, foreign or unbound values are rejected, never stripped: unlike optional reasoning, removing compacted context would lose history. Chat Completions, Messages and Google targets reject it in the absence of a carrier.

Compaction admission is separate from reasoning support. Codec support does not activate existing model bindings. This contract receives and replays compacted history; it does not schedule compaction, expose `/responses/compact` or store conversations. The [official guide](https://developers.openai.com/api/docs/guides/compaction) and [pinned SDK](references.md#pinned-sources) define the selected wire contract.

### Stateful history

Downstream Responses offers stateless full-history requests and gateway-managed `previous_response_id` continuation. The gateway retains canonical request input and the delivered response projection, with replay envelopes intact. The Responses codec composes these typed owners directly with newly decoded input before combined IR validation and candidate preparation; retained history is never encoded to temporary public wire and decoded again. Storage facts stay in protocol metadata, outside Generation IR and upstream execution hints.

For signed ordinary text/calls, the retained typed projection also keeps hidden replay and native grouping unavailable on public Responses wire. Public creation, retrieval and input listing expose only standard carriers; they cannot export this richer state for stateless replay. The wire projection does not mutate retained owners. `store: false` may read a predecessor but cannot deliver a new response requiring hidden continuation state. Genuine reasoning and compaction retain their native carriers and existing stateless checks.

Retained input stays in its sealed client-bound representation, captured before the single execution opening pass. Its existing identities and associations remain stable. Imported output receives collision-free private item/part/resource/group identities; verify source dependencies before recomputing identity-dependent receipts. New input has its own unused scope. Resource/citation edges, exact arguments, native grouping and empty-envelope append positions survive. Output-only creator and response-wide reports do not become request history. Legacy manifest input cannot rewrite a retained prefix.

Storage defaults to true, matching OpenAI's Responses default. Omitted or null `store` retains the new response; explicit `store: false` opts out. A request may read a stored predecessor without storing its successor. Top-level instructions and other settings come from the new request, not its predecessor; instruction items in history remain history.

The synchronous API includes response retrieval and deletion and cursor-paginated input items. Only fully delivered terminal responses become visible. Unknown, deleted and expired references have the same not-found result. Stored branches own complete snapshots: deleting or expiring an ancestor does not invalidate an already stored descendant. Retrieval is JSON only; there is no event log or stream resumption.

History lives in bounded process memory with a TTL. Expiration or restart invalidates stored references; there is no disk durability or restart recovery. The selected [Agent session-file direction](#agent-session-files) still needs its concrete storage contract; it does not change this implementation or justify a prebuilt backend framework.

Capacity accounts for typed payloads, retrieval bytes, owner records and nested part allocations, including empty parts and signatures absent from public wire. Accounting does not require public representability or serialization of retained input. A final snapshot reserves capacity before its terminal frame is queued. The body publishes it synchronously at the final acknowledged handoff, so an immediately following request can resolve it. Dropped or failed delivery releases the reservation. TTL starts at publication, reads do not extend it, and expired entries are reclaimed on store access. Unexpired entries are not evicted.

Upstream calls remain stateless, using full history and `store: false` where supported. Neither Provider-only nor hybrid Provider-held continuation is planned. Downstream storage intent never enables upstream storage. Cache affinity remains separate from history ownership.

The scope is conversation history management, not execution persistence. No `background: true`, detached generation jobs, scheduler or agent loop is planned. JSON and SSE requests keep the existing cancellation and honest-delivery rules.

### Agent session files

**Direction selected on 2026-10-25; included in the current downstream-integration stage, not implemented.** Organize retained sessions in separate directories per stable authenticated Agent, with each directory containing a group of JSONL files when session storage is enabled. Names, client-supplied paths and credentials cannot select the storage namespace. This is session storage, not the operational JSONL log or exchange-capture sink.

Cross-Agent work should explicitly read an earlier session or copy it into a new working session before continuing, rather than silently changing the original session's Agent. Keep original provenance and distinguish the new caller's work; submitted client history alone does not prove the authorship of every message. Directory organization does not itself define an ACL, cluster store or new public API.

Reading history as context and copying resumable typed history are different operations. Public transcript JSON may omit hidden signatures/grouping required by managed continuation. No read or copy may strip protected state, make foreign Provider/account-bound replay valid or treat copied text as complete executable continuation.

The next design must select session identity, JSONL record meaning, explicit read/copy behavior, storage opt-in and its relation to protocol `store`, finite retention/capacity, private-file safety and interrupted-write/publication semantics. Concrete migration, file layout/record schema, persistence implementation and endpoint activation are not selected here. Existing Responses storage defaults/TTL and shared HTTP permissions remain unchanged; Chat/Messages gain no `store` field through this direction.

## Native Messages projection

Native requests preserve thinking, tool-use/result order and call IDs. Messages owns its profile, cache controls, limits and session headers.

Consecutive same-role native messages form one logical turn for tool-result adjacency and prefix validation, without changing their wire grouping. Optional null usage-delta counters leave earlier reports intact; numeric zero replaces them. Native response encoding and client delivery share the completion-evidence checks.

### Request rules

These describe current native behavior, not completion of the broader [loss-policy migration](roadmap.md#ir-and-codec-migration). The [canonical instruction contract](#instruction-authority-and-canonical-roles) owns role and root-history semantics. A richer reasoning conversion requires a model-bound trusted request policy; absent policies retain rejection. Default-strict-to-plain conversion remains rejected by decision. Selected P8 behavior follows the [request policy](#selected-request-approximations).

- Disabled, adaptive and ordinary manual-budget thinking have distinct native carriers. Manual budgets require at least 1024 tokens and strictly less than the explicit output ceiling, independently at source intake and final native encoding. No clipping, cap increase or manual-interleaved exception is selected. Trusted target policy disables manual thinking on adaptive-only Go/Haiku; carrier availability does not widen that binding.
- Response-wide effort maps to `output_config.effort` independently of thinking. Reasoning-only effort is rejected, not broadened to response-wide effort.
- Summary intent maps to summarized or omitted display only with a representable activation/allocation. Display alone does not enable thinking. Uncalibrated reasoning context rejects. Messages has no temperature/top-p/top-k wire carrier. Generic projection rejects those IR controls; the existing explicit fixed-sampling Go/Haiku policy may omit temperature/top-p from other source protocols with a receipt, and rejects top-k. This does not restore removed Messages input fields.
- Client `thinking.display` is preserved independently of effort and budget, using the same display parser as native intake.
- Root instructions and every contiguous leading instruction compose into system blocks in source order, retaining individual cache markers and their separate semantic ownership. System is the native operator carrier for canonical application instructions, not an authority approximation; later instructions are not hoisted.
- Later instructions, `ToolChange` items and response-scope `ConfigurationUpdate` items become [mid-conversation system messages](https://platform.claude.com/docs/en/build-with-claude/mid-conversation-system-messages) at their envelope boundary. A run of them forms one system section and shares a message; an effort sets that message's `output_config` and closes it. Content placement follows the native rules: after a user turn, before an assistant turn or at the end; an effort-only message may appear anywhere. An item inside a merged turn rejects, and turn-scoped `clear_at` rejects. The profile default keeps `ordered_instructions` and `tool_changes` closed; a reviewed binding opens them.
- Per-message effort, `thinking.block_binding` and tool-change blocks are beta carriers that the trusted binding enables (`Carriers`). Without one, `messages.flatten-message-effort.v1` sets top-level effort to the level in effect for the requested reply, `messages.omit-block-binding.v1` drops tolerant `drop_block`, and `messages.collapse-tool-changes.v1` declares the final available tool set, withholding never-surfaced deferred tools. Explicit `"error"` binding has no fallback and rejects. Non-Messages targets omit `drop_block` and eager tool input (`generation.omit-eager-tool-input.v1`) the same way. `anthropic-beta` values are derived from the encoded body; client headers never select them.
- Returned `input_transformations` are validated, ignoring unknown entry types as documented, then omitted under `messages.omit-input-transformations.v1`: their paths index the upstream request.
- Call IDs outside the native grammar map to stable digests; collisions fail rather than merge calls.
- Owner-local cache markers preserve their placement and 5m/1h TTLs. Do not add unrequested automatic caching beside explicit markers: four explicit markers plus automatic caching, or an automatic TTL differing from the tail marker, would conflict. Generic codecs preserve absent automatic policy. Go/Haiku explicitly selects the existing marker-free 5m default under `messages.default-automatic-cache.v1`, on a candidate copy after source admission. Caller-selected automatic caching has its own [all-native admission](#caller-selected-automatic-cache-admission). Cache hits still depend on upstream thresholds.
- A JSON-schema output format maps to `output_config.format`, which always constrains decoding. Its name is dropped by a named rule; a JSON-object mode or a schema description has no carrier and is rejected.
- A source without an output cap uses the [trusted target default](#missing-output-ceilings), if declared, under `messages.default-max-tokens.v1`. Preserve an explicit client ceiling; pure codecs invent no fallback.
- Ordinary root/by-value tools normalize nullable `eager_input_streaming` to the existing target-default owner, not explicit false; nullable tool or supported content-wrapper `cache_control` creates no marker or TTL. Explicit booleans and real cache boundaries retain their existing owners. `tool_result.is_error`, `tool_choice.disable_parallel_tool_use` and supplied `output_config.effort`/`format` remain non-null; null rejects before optional ownership can erase it. Other known non-null fields remain strict; no beta-header forwarding is implied.
- OpenAI stateless controls are not forwarded as native fields. Requesting an encrypted reasoning carrier selects delivery, not native thinking.

Exact rules live in [request.rs](../src/protocol/anthropic/messages/request.rs) and [carriers.rs](../src/protocol/anthropic/messages/carriers.rs).

### Missing output ceilings

The user selected these configuration and migration details on 2026-10-20. Messages `TargetCaps.default_output_tokens` owns an optional positive `u64` insertion policy, separate from `ModelLimits.maximum_output_tokens`. It is not a client field, model capability or routing policy. [Usage](usage.md#model-configuration) owns configuration syntax.

Candidate omission inherits the binding default; a positive value replaces it; explicit `null` clears it. Overrides may raise or lower a default because it is not a restriction on explicit client intent. Reject malformed values and any presence of this field on a non-Messages candidate. Existing model-limit presence semantics remain unchanged.

Resolve model-limit inheritance/narrowing before validating the default. Zero or a default above the effective declared maximum rejects at startup/endpoint compilation and in direct target encoding, even for a request that supplies its own valid ceiling. No default is valid configuration; an uncapped request then rejects locally. A missing maximum supplies neither a default nor evidence of unlimited upstream output. For example, narrowing a maximum to `8192` while inheriting `32000` requires overriding or clearing the default, not taking `min`.

The existing Go/Haiku Messages binding explicitly declares `32000` to preserve its prior missing-cap behavior. This is gateway policy retained from the [pinned client fallback](refs/2026-10-17-generation-contracts.md#messages-missing-ceiling-evidence-2026-10-20), not an upstream default, model maximum, measured optimum or live enforcement claim. Generic `TargetCaps::new` has no default; future bindings select their own values. Other protocols, including Chat for the same model, do not inherit this Messages policy.

Projection order:

1. Validate original source semantics. Native Messages still requires positive `max_tokens` at intake; a target default cannot repair missing/null/zero native input or an invalid explicit source budget/cap relation.
2. Apply existing selected reasoning conversions and carrier projections on a candidate copy. Only then fill an absent effective cap from the trusted default. Preserve every explicit cap, including one larger than the default; never use the model maximum or a thinking preset as a fallback.
3. Validate effective settings and project the typed native request. Pure Profile/IR-to-wire encoding requires a resolved ceiling and rejects absence; it contains no fallback constant or compatibility shim.
4. Apply final model-value and native grammar checks. Retained ordinary manual budgets require `1024 <= B < C`; the candidate-settings check may already reject `B > C`, while native validation rejects equality. Do not clip budgets, raise caps or infer tokens from effort.
5. Successful insertion records `messages.default-max-tokens.v1` exactly once per encoder invocation. Explicit caps record no insertion. Preflight and preparation use the same deterministic encoder; source IR, stored client history and successor candidates remain unchanged. Transport does not resolve defaults.

The ordering preserves budget-to-effort conversion that removes an otherwise conflicting numeric allocation, and checks budgets created by effort-to-budget conversion against the inserted cap. It selects no new mapping or manual-thinking target; Go/Haiku remains adaptive-only. A synthetic target with `B=2048` and `D=1024` rejects without changing a successor target's ability to encode `D=4096`. A retained budget equal to the inserted cap rejects; a larger cap may pass without changing the budget.

Counting does not inherit Generation's required output cap. Its separately selected effective-input projection shares only count-relevant rules, not full generation preparation.

### Owner-local cache admission

The selected `opencode-go-haiku-messages` binding admits existing owner-local cache markers; the generic profile constructor remains closed. The canonical model ceiling includes that capability, but the shared catalog public contract and OpenRouter Chat binding do not inherit it. At configured startup, publish explicit-cache admission only when every selected candidate is an opted-in native Messages target. Mixed routes remain cache-closed instead of admitting requests by skipping an incompatible candidate; membership, order, fallback, affinity and replay identity do not change.

The Messages capability view distinguishes valid owner-local 5m/1h breakpoints from caller-selected request-wide automatic policy. Four-marker bounds, TTL ordering and owner validity still apply; no marker is relocated, dropped or downgraded. The [model and forwarding evidence](refs/2026-10-17-generation-contracts.md#messages-owner-local-cache-admission-2026-10-19) is not proof of a deployed proxy's effective caching, retention or account-compatible signed replay.

### Caller-selected automatic cache admission

**Selected on 2026-10-21; Go/Haiku activation verified on 2026-10-22.** Reuse existing request-wide automatic-cache controls. Publish support only when every selected candidate is an opted-in native Messages target with verified support; do not infer it from owner-local markers or the marker-free default. The [deployed gate](development.md#native-messages-input-acceptance) observes 5m/1h write/read and write-class reports, not elapsed retention.

Preserve explicit 5m/1h intent and existing marker-count, TTL-order and automatic/marker conflict checks. Absence keeps the existing target default policy; explicit input is neither replaced by that default nor silently omitted on mixed routes. Preflight, preparation and capability publication must agree without changing route membership, order or affinity. Cache-use reports do not prove measured retention or invoice accuracy. No prewarming, cached-context lifecycle or new File service is selected.

Placement follows the nearest eligible tail, not simply the last serialized block. Root deferred definitions do not supply that tail. Shared validation distinguishes no eligible owner from an unmarked eligible owner: the former adds no virtual breakpoint. Keep four-marker rejection distinct from same-TTL placement no-op. Native regression retains examples/context and unchanged root tools when inline definitions are appended; this does not prove a deployed cache hit or eliminate the documented first-inline-definition miss. A client's local `cacheRetention:"none"` preference is not an upstream disable field when its wire request merely omits markers; it does not override the selected target default. [Input/cache](refs/2026-10-17-generation-contracts.md#anthropic-input-and-cache-recheck-2026-10-21) and [client-source](refs/2026-10-17-generation-contracts.md#production-client-profiles-2026-10-21) evidence define these verification boundaries.

### Response rules

Native observations stay unchanged. OpenAI delivery projects a copy:

- Chat requires leading thinking, then text, then calls; it rejects unrepresentable interleaving. Responses retains independent item order.
- Structured tool arguments become exact JSON strings; call/result links remain intact.
- Max-token limits, context-window exhaustion and refusal are incomplete with distinct reasons. Unsupported server-tool continuation is rejected.
- Cache-inclusive usage is computed only from complete reported operands; missing usage is not invented.
- Delivery generates required IDs/timestamps only on its own copy, stable within the response. They are not upstream facts; a stored Responses snapshot uses its gateway ID as a local history key.
- Text, thinking and tool arguments stream incrementally. Full signatures are enveloped after block validation.
- For an eager call ending at `max_tokens`, unfinished object-JSON prefixes retain exact fragments/call identity as `StructuredPartial` and an incomplete item. Native SSE closes the block without inventing a completed/executable value, then publishes only the real incomplete terminal after strict EOF. Duplicate keys, malformed prefixes and byte/depth/node overflow still reject before closure. Successful `tool_use` with invalid input, JSON delivery and request-history replay remain rejected; no argument repair, substitute object, fabricated tool result or automatic retry is selected.

Messages delivery rejects citations until that carrier is implemented. Static and streamed output use the same refusal to discard annotations.

**Readable reasoning compatibility selected on 2026-10-25.** Preserve ordinary cross-protocol continuation before preserving readable reasoning's exact type and segmentation. For unsigned readable reasoning, Messages joins each item's parts in order without trimming, inserting separators, summarizing, or mixing reasoning into the answer. The delivery copy represents the result as one Summary (or no parts for empty text), matching returned Messages history; source IR, item order and lifecycle remain unchanged. `messages.readable-reasoning.v1` records the item-local type/boundary approximation. This preserves text carriage, not measured model quality or a reversible raw/summary distinction. It neither reveals hidden reasoning nor discards opaque continuation. Native signed thinking and redacted blocks retain their exact path; replay-bearing content that needs this normalization rejects instead of rebinding a signature. SSE preserves sequential part order and the single-value byte bound, rejecting overlapping reasoning parts rather than concatenating arrival order.

Messages delivery checks tool-call IDs and names against its public history grammar before publishing the call owner, including incomplete calls. Unrepresentable labels reject without renaming identities, so a successful call cannot become an unavoidable next-turn grammar error.

Messages block starts preserve source item order. Text/thinking owners may open lazily, but cannot start a new block after a later owner's block has already started; reject that interleaving rather than reversing the SDK's final content array. Deltas for already opened blocks may interleave. This is an explicit streaming representation boundary, not a new buffering scheduler or a claim that every cross-protocol item schedule is representable.

Messages final usage requires reported input/output counts and a known accounting relation. `output_tokens_details.thinking_tokens` maps to the reported reasoning subset in both directions and is never derived. Cache-exclusive input is preserved. Inclusive input needs a reported cache-read count. An unreported cache-write category stays absent rather than zero, and the remaining input keeps any such writes. With both counts reported, at least one must be zero: the IR does not promise that nonzero cache-read and cache-write subsets are disjoint. Missing or ambiguous counts fail projection instead of becoming zero. SSE opening counters are provisional; they never replace the required final report.

## Runtime and trust

### Failover

#### Legal candidates

Startup compiles a public model to a route with legal endpoints and all authorized credential-pool members, at most 64 expanded candidates per entry. Business input cannot create candidates or name an executor. Request representability is checked before selection; it does not widen public admission.

Compiled order is stable. Runtime policy promotes an available conversation preference and defers unavailable targets, then rechecks health atomically before dispatch. Cooling is a dispatch gate, not a last resort. This is failover, not load balancing.

The exact executor/cache identity includes ingress principal, credential provenance, public model, Provider, deployment origin/path and upstream model. API-key rotation changes it; OAuth token refresh preserves the verified principal. A different credential is a different cache domain. Health uses separate private digests so public aliases do not isolate a shared failure.

#### Advancement

Advancement requires enabled policy, an unpublished body, a legal remaining candidate and remaining attempt/time budget. Scan the stable suffix under every earlier failure restriction; an ineligible adjacent key cannot block a later Provider, and another Provider's failure cannot erase an earlier exclusion.

The pool's runtime ceiling applies per endpoint: `max_attempts` with credential fallback enabled, otherwise one. The route ceiling spans all endpoints. Beginning credential resolution consumes an attempt; representability, health and policy skips do not. Neither budget resets or multiplies, and no candidate is revisited. Transport has no hidden retry or redirect. A retained tail member can be the first attempt even when the pool or route ceiling is one.

| Failure | Advancement |
|---|---|
| Transport/connect failure, upstream timeout, server failure | Next key or endpoint where its credential/route policy permits |
| Locally unavailable credential | Next credential of that endpoint where enabled; no new cross-Provider permission |
| Exhausted quota | Next key or another Provider, where enabled |
| Rate limit | Another Provider; do not rotate keys for unknown quota scope |
| Authentication/permission failure, context overflow, request projection failure | Do not advance |
| Safety refusal | Deliver it; never retry on another model |

A non-API 2xx reply, such as HTML or an empty body, is not success. Invalid or truncated upstream intake can advance while unpublished. Downstream projection failures do not acquire retry permission.

Authentication/permission HTTP statuses remain authoritative: error wording cannot reinterpret them as quota, and reading an optional error body cannot turn them into a timeout. For other failed heads, a body-read timeout retains the known status classification.

#### Health and recovery

| Failure | Health domain |
|---|---|
| Authentication, locally unavailable credential | Credential provenance + Provider + trusted origin |
| Permission, quota, upstream server/transport/intake failure | Exact credential + Provider + deployment origin/path + upstream model |
| Unknown-scope rate limit | Configured pool membership + Provider + deployment origin/path + upstream model |

Pool identity is a digest of its ordered-independent member provenance set, not its display name. An unknown rate limit gates that deployment's pool across requests, without inventing a global Provider/account quota scope. Health keys come only from bound candidates; they and the bounded conversation table remain in process memory.

Before `not_before`, no dispatch occurs. At expiry, one foreground request atomically acquires recovery ownership across all affected scopes. The lease lasts through EOF and final handoff; cancellation releases it. No background probes, waiting queues or repeated candidate cycles run. If every otherwise-legal remaining target is cooling or probing, return sanitized HTTP 503 `upstream_unavailable` with the earliest candidate's remaining delay (one second when only a probe is pending). Policy/budget exhaustion instead preserves the last attempted failure.

Standard `Retry-After` accepts seconds or an HTTP date within one day. A valid delay, including zero, is used without a one-hour truncation. Without it, consecutive transient upstream failures back off from one second to one minute; credential, quota and rate failures from one minute to thirty minutes. Failures during an active cooldown do not multiply the backoff; thirty minutes after the previous cooldown resets the sequence. Active deadlines never shorten. No Provider-specific reset dialect is guessed.

Each attempt captures health generations. Successful cleanup and affinity publication require all captured generations still to match; a late success cannot erase a newer failure. Already observed upstream failures survive interrupted error-frame delivery. Slow downstreams, cancellation alone, local projection failures and semantic refusals do not create upstream health failures.

Diagnostics record bounded skip/stop reasons, scope classes, probe ownership and delay sources, never health keys, credential fingerprints, conversation IDs or business content.

#### Routing unit

The declared conversation is the routing unit. Turns, messages, branches and inferred subtasks are not routing keys.

After honest completion, later requests for the same public model and conversation prefer that executor. Successful failover updates affinity without automatically returning to the old target when it recovers. The table holds at most 4,096 pairs for one hour after completion; reads do not extend it. There are no weights, rotations, concurrency lanes or load-spreading rules.

### Conversation and cache affinity

The explicit `X-MorphieCore-Conversation-Id` header takes precedence over agent session headers. See [Usage](usage.md#conversation-and-cache-affinity) for accepted headers and constraints.

The gateway derives Provider-specific cache/session carriers; it does not read Provider session headers as conversation authority. Standard `prompt_cache_key` is a separate cache-grouping intent.

Without a declared conversation, a short-lived prefix-digest index offers best-effort cache-group reuse. It stores no bodies, selects no credential and restores no history. Opaque-reasoning histories are excluded.

Implicit cache scope includes the complete configured Agent ID/key-table namespace, credential source and public model, not a per-Agent boundary. Admitted Agents share that namespace under the single-deployer access policy. It is independent of the replay fingerprint. Losing the index loses cache hits, not correctness.

### Streaming and delivery

When another candidate could answer, lifecycle-only frames are held before publication. Content, a 64 KiB bound or a real 20-second timer releases held frames, including during upstream silence. Empty buffers produce no invented keepalive or terminal. This lets a pre-publication failure advance without exposing the attempt.

Publishing any body frame freezes advancement. Body handoff acknowledges commit. Completion requires validated upstream EOF and the final handoff.

After publication:

- Never splice another candidate into the body.
- A classified upstream failure may produce a protocol failure frame.
- Other failures abort delivery.
- Never invent a successful terminal or replay the request.

A Responses stream with context overflow or exhausted quota can return HTTP 200 with a standard `error` event even before normal output. Gateway-classified failures use this event's open code field, not the closed model-reported `response.error.code` enumeration. After publication the error follows the existing sequence; before publication it is the only event with sequence zero. It never manufactures opening/failure Response reports or successful termination. The former Codex-specific `response.failed` shape is superseded; Codex acceptance remains unscheduled.

Upstream streaming and client delivery are independent. JSON clients receive a bounded aggregate; SSE clients receive incremental output. A native stop event is not transport EOF.

Bodies, frames, event counts and deadlines are bounded. Client disconnect, timeout or shutdown cancels upstream work. Held-stream keep-alives remain [deferred](roadmap.md#only-if-needed).

#### Optional delta coalescing

Delta coalescing is implemented and disabled by default. Prepared batches retain shared immutable metadata and fidelity, with projection and replay sealing performed once. The disabled path encodes each batch immediately.

The goal is fewer downstream events, repeated envelopes and body handoffs without changing semantic content. It does not reduce upstream traffic or token usage, and fewer handoffs do not imply proportionally fewer network packets.

```text
upstream decoding and original-event validation/reduction
  → attempt-local intake with terminal withheld until validated EOF
    → streaming: client projection and prepared event context
      → optional bounded delta coalescer → client encoder → body delivery
    → final-only: existing materialized response → client projection → JSON delivery
```

Final-only delivery already supports a declared forced-stream upstream. It uses the existing authoritative reduction and final response, not an infinite coalescing window, an event log or a second full-response accumulator. Unexpected upstream SSE remains subject to the endpoint's declared delivery contract.

##### Ownership and equivalence

| Owner | Responsibility |
|---|---|
| Upstream decoder and semantic reducer | Validate every original event and retain existing byte, event and semantic limits |
| Client projection | Prepare each event once, in order, with stable identity, metadata, fidelity and replay context |
| Delivery-local coalescer | Keep one bounded adjacent run for one attempt; no network, clock source, credential or complete response state |
| Gateway intake | Drive reads, monotonic-time flushes and body handoff in the existing task; retain deadlines, backpressure and cancellation |
| Client encoder and body | Generate downstream sequence numbers, validate target state and preserve publication/acknowledgement gates |

Successful streams must be equivalent after normalizing delta segmentation: ordered content, identities, probabilities, annotations, replay, usage and terminal outcome remain unchanged. Event count, sequence numbers, padding and timing need not match byte-for-byte. Failure paths preserve published prefixes and honest failure; cancellation is not a promise to deliver buffered data.

Preparation and encoding need a separate seam. Do not buffer raw events and later call the existing combined operation against newer attempt metadata or fidelity. Retain the context needed by the pending run, or flush it under its original context before an update. Budget retained context as well as text; avoid copying a full sidecar per delta. Never rerun stateful native projection during flush. Existing target-encoder validation remains in place.

##### Initial merge contract

The first implementation merges only adjacent, nonempty `Delta` fragments of `PartKind::Text`, with the same projected item/part identity, unchanged encoding context and empty logprobs. Concatenate exact strings; never sample or keep only the latest fragment. Other deltas, including reasoning, tool arguments, audio and opaque signatures, pass through unchanged.

Every noneligible event is a flush barrier. This includes resources, annotations, probability snapshots, final text metadata, usage, progress, part/item closure and terminal events. Empty deltas also remain barriers until their observable equivalence is established. Keep only one pending run, not independently buffered queues per part.

The coalescer has empty, pending and closed states. An eligible event starts or extends a pending run; a barrier or size/time threshold emits that run before subsequent events. Cancellation or abandonment discards its state. No state crosses attempts, clients or requests.

##### Scheduling and failure boundaries

1. Preserve the first content-bearing batch's existing publication path without intentional coalescing delay. Do not release held lifecycle frames while hiding that batch in a new buffer. Existing pre-content fallback holding remains a separate policy.
2. Start a fixed flush deadline when a run becomes pending; later fragments do not reset it. An actual timer must flush during upstream silence, and processing multiple events from one network chunk must not starve it. Existing elapsed-time checks on event arrivals are not such a timer.
3. Flush before exceeding the pending-byte limit. Account conservatively for JSON escaping and envelope overhead so merging cannot make individually valid events exceed target frame limits. Original upstream limits are unchanged.
4. Await the existing bounded body handoff. Do not read ahead in a detached task to bypass a slow client. The flush deadline bounds intentional buffering, not downstream/network blocking.
5. Publication forbids advancement; acknowledgement advances the delivery lifecycle. Do not flush merely to manufacture commitment or create a new fallback permission. A failed unpublished attempt loses its buffer before legal advancement; cancellation discards it. On a published classified failure, handle already validated pending content in order before the failure frame when delivery remains possible; malformed intake still cannot create a success terminal.
6. Normal completion drains pending content before terminal encoding, but the terminal itself remains gated by validated upstream EOF. Final-only delivery, stored response snapshots and replay retain their existing authority.

The policy belongs to trusted startup configuration and defaults to disabled. Both finite time and accounted-byte bounds are required; invalid bounds fail startup. Retained context counts against the pending budget, and oversized contexts/fragments pass through rather than being truncated. Reported instruction-message trees bypass buffering. Conservative escaped-text and envelope allowances keep merged frames within the target event ceiling; the encoder still enforces aggregate output budgets.

Configuration is documented in [Usage](usage.md#delta-coalescing). It is not an IR field or a downstream request extension. The [offline baseline](development.md#offline-baseline), [SDK loopback gate](development.md#sdk-loopback-gate) and separately scoped real acceptance remain distinct; synthetic overhead reductions do not establish universal latency or packet-count improvements.

### Credentials and security

Credentials come only from the application's private store. A manager owns API keys, OAuth identities and ordered pools. Inference borrows access for a fixed identity; it never logs in or refreshes. Each borrow reads the store under its short transaction lock off the async workers; concurrent requests wait at most one second for that lock, while long-lived account operation locks still report `Busy` immediately.

Ingress resolves exactly one configured Agent key via Bearer or `x-api-key` before body collection. The immutable authenticated-Agent context exists independently of diagnostics and capture. Business data cannot select an Agent identity, origin, credential or auth/proxy header. Inbound headers are not passed through.

Transport uses trusted prepared requests, no redirects and no implicit HTTP retries. Environment proxies require explicit startup opt-in. Credentials, locators, private config and upstream bodies never reach clients.

### Agent identity and cluster diagnostics

**Agent baseline selected on 2026-10-23; API-key admission selected and implemented on 2026-10-25.** This is the primary reference for downstream authorization and future session attribution, usage statistics and diagnostics. MorphieCore, Agents and diagnostic infrastructure belong to one deployer. The business model has no users, tenants, owners or billing accounts: a registered **Agent binds one client instance** and is the attribution unit. An authorization server's human account is an approval mechanism, not a MorphieCore statistics dimension. [Roadmap](roadmap.md#agent-authorization-and-diagnostics-follow-up) owns remaining gates; ingress remains loopback-only.

#### Identity and authentication

| Identifier | Meaning and lifetime |
|---|---|
| `agent_id` | Trusted configured identity, independent of display name/key; survives restart, rename and key rotation when the ID is retained. Retired IDs must not be reassigned. A future managed registry must prevent deleted-name identity reuse; OAuth stability remains unimplemented |
| Credential/grant reference | Non-secret reference to a verified API key or OAuth authorization bound to that Agent; one Agent may rotate or hold multiple credentials |
| `run_id` | Optional client execution identity, not authentication. Client-supplied labels and parent-run relationships remain declared correlation data |
| Conversation, request and attempt IDs | Business conversation, gateway request and upstream attempt boundaries; none is a substitute for Agent identity |

API-key authentication resolves to an immutable authenticated-Agent context at request admission; future OAuth must resolve to the same context. Keep it in gateway/runtime owners, outside semantic IR and Provider attribution hints. Names, hosts, IPs, request `user`/`metadata` and self-declared Agent IDs cannot establish identity. Requests with conflicting authentication credentials reject. If independent clients share a credential, their calls are indistinguishable and count as one Agent.

The implemented startup-only [Agent-key schema](usage.md#agent-api-keys) separates stable `agent_id`, secret `key`, mutable `name` and maintenance `desc` in a versioned record array. A nonempty bounded table has unique IDs and keys. Runtime authentication retains digests and uses constant-time comparisons over the complete table. Only the authenticated ID enters existing inference request diagnostics; names/descriptions never select identity or reach clients/Providers. All Agents retain the existing shared model/history permissions, with no per-Agent routing, quotas, registry API or history provenance. Startup replacement/removal rotates or revokes keys for new gateway instances, not already running instances or in-flight work.

The private health/cache namespace covers the complete configured ID/key table and existing upstream identities, not a per-Agent credential hash. Display edits and record order do not change it. Key-table changes may replace this private namespace, independently of stable Agent identity; Provider-bound replay restrictions and routing policy remain unchanged.

When the separately deferred OAuth slice is selected, use an existing authorization server with Device Flow for human-approved headless access; MorphieCore is the resource server. Agent2 obtains its own authorization after agent1 launches it; process parentage grants no automatic authority. A public OAuth `client_id` identifies an application, and `iss`/`sub` commonly identifies its approving account, not an Agent instance. Token bytes, `jti` and an unverified interpretation of `sid` cannot supply stable identity. Require a verified refresh-stable authorization-instance binding or a separately selected controlled registration mechanism. Credential-to-Agent linking is a trusted management action, never an email/name match.

API keys remain supported for simple or unattended clients. Reuse the authorization service for login, token issuance, refresh and revocation; do not build a user directory or custom OAuth server. Keep upstream Provider credentials separate. Rauthy is the first evaluation candidate, not a selected dependency; [dated research](refs/2026-10-23-downstream-oauth.md) owns server evidence and compatibility limitations. Access-token audience, permissions, independent revocation and old-token/in-flight behavior require explicit acceptance.

#### Shared history and usage attribution

Session organization follows the selected [per-Agent JSONL direction](#agent-session-files), with explicit reading or copying for cross-Agent work. Record the creating Agent and the actual Agent on every subsequent operation; handoff does not rewrite history authorship. Current admitted Agents still share the existing HTTP access policy, not tenant-private ACLs. Shared data access does not grant credential-management privileges or bypass Provider/account-bound replay restrictions. Key revocation does not delete history. Persistence implementation remains separate from attribution and does not replace today's [process-local TTL history](#stateful-history).

Fix attribution at request admission, not by looking up a credential's current binding during later aggregation. Usage belongs to the Agent making each request, not the conversation creator or parent Agent. Conversation and parent/child rollups are alternate views, never additional consumption. Preserve per-attempt source accounting and distinguish reported values, explicitly estimated values with provenance, and unknown usage. Do not sum cumulative streaming snapshots, treat failed/cancelled requests as free, or equate price-versioned cost estimates with Provider invoices.

Internal statistics may be best effort with visible loss and completeness indicators; diagnostic logs are not a durable billing ledger or strict quota authority. This baseline selects neither financial billing nor a reservation/settlement subsystem. Structured events and full-content capture use shared Agent/request/attempt references but retain separate storage and reliability policies under [Observability](#observability) and [exchange capture](#configurable-exchange-capture).

**Minimal diagnostic attribution selected on 2026-10-25.** Authenticated admission allocates one runtime-nonce/sequence request reference independently of diagnostics or capture. Inference traces, attempt logs/spans and each enabled capture row use that reference and the authenticated Agent, never a caller-provided identity. Exhausted sequence space leaves correlation unknown rather than reusing an ID or rejecting business data. IDs remain internal; no new response header, client turn inference, history-access audit or upstream header is introduced.

Each Generation attempt retains bounded, latest validated source usage observations by operation/step scope before delivery projection. Repeated observations replace rather than merge or sum; final accounting stage is not EOF, successful delivery or exhaustive billing. Earlier successful source units survive later failure/cancellation, while rejected units contribute no new observations. Static intake contributes only after successful typed completion. Pending generateContent accounting is observed without changing its terminal event carrier.

The execution report retains exact source counters, accounting premises, sparse breakdowns and source-default markers, not downstream completed/estimated usage. The existing intake evidence qualifies cache-write normalization: `cache_write_basis` distinguishes `source`, `selected_policy` and `unassessed`. Policy-filled cache writes, or writes whose provenance could be missing after evidence truncation, remain unknown in this source view; an explicitly reported zero stays zero. Streaming observations retain their source-unit evidence rather than re-reading the materialized response after those deltas were drained.

The allowance is 32 scopes and 16 KiB of accounted report storage per attempt; overflow increments omitted-observation counts without affecting inference. These count failed retention calls, not distinct upstream messages. A prior retained scope whose replacement cannot fit is explicitly marked `latest:false`, not advertised as current. Missing usage remains unknown. This is diagnostic evidence, not usage aggregation, metrics activation or durable accounting.

### Observability

Prefer `tracing` spans/events and established OpenTelemetry log, trace and metrics integrations where needed. Reuse ecosystem SDKs, subscribers and exporters with minimal application wiring; do not build a home-grown diagnostics library, event bus or parallel record framework.

**Rust stack selected on 2026-10-24.** Use `tracing`/`tracing-subscriber` for spans and structured events, `tracing-opentelemetry` for traces, `opentelemetry-appender-tracing` for scalar logs, and the typed OpenTelemetry Meter API when metrics are selected. SDK processors/readers own network batching and aggregation; OTLP/HTTP protobuf with an explicit blocking reqwest client remains the selected future network export path. [References](references.md#telemetry-dependency-selection) pins the reviewed releases; [ecosystem checks](development.md#telemetry-ecosystem-checks) remain independent of production acceptance.

**Local runtime wiring selected on 2026-10-25.** The existing opt-in diagnostics path constructs one runtime-scoped subscriber and SDK tracer/logger providers; it installs no global subscriber/provider, network exporter, metrics reader or Collector. `gateway::diagnostics::telemetry` owns real request/attempt spans through the existing request/body owner. Explicit parents and dispatcher-scoped emission/destruction preserve attribution across detached bodies without ambient-context inference. Only the registered request/attempt span names and attempt-finished event enter the bridge. Final status is set at closure, not at interim outcome updates; a recovered attempt error does not mark its successful request as failed.

The final safe `Record` projection, including the existing report, becomes one native SDK structured log body, not an escaped Debug string or a second report ledger. Scalar attempt-finished events use the tracing log bridge; events are not also copied into spans. The local SDK exporter only clones/moves owned records to the existing bounded writer. JSON conversion and file I/O stay on that worker, except constructing the native log body at request finalization. Optional null map members are omitted because SDK `AnyValue` has no null variant; unsigned counters above `i64::MAX` remain exact decimal strings with a declared body-encoding attribute. Unsupported null array members fail only telemetry encoding and increment its failure count.

Explicit resources, AlwaysOn sampling and complete span limits override discarded SDK environment defaults. Provider construction runs under a no-output dispatcher because this SDK still reads defaults internally and may diagnose invalid sampler settings. This is not a claim that the SDK performs no environment reads. `OTEL_*` cannot select a destination or exported resource in this local mode. Provider ownership follows all outstanding traces; ordinary `flush_diagnostics` drains the file queue without shutting providers down. Final SDK shutdown only enqueues to the local writer; cancellation alone is not proof that detached request owners have finished, and flush still requires caller-established quiescence.

Keep only bounded domain evidence and its checked/prepared/publication/handoff associations in the execution report, not a second general-purpose trace tree, event bus or metrics registry. Logs and optional wire summaries project that evidence independently of trace sampling, subscriber filtering and exporter availability. Set final request/attempt status explicitly; recovered attempt errors must not mark a successful request as failed. Use allowlisted scalar fields, not arbitrary `Debug`/error objects or automatic argument capture. Route diagnostic events to logs without duplicating every event into traces; keep SDK/exporter health outside that export bridge.

Telemetry providers belong to startup and shut down after request workers and bodies. Own finite item/attribute, queue, batch, HTTP size, retry and shutdown budgets; SDK queue counts alone are not byte bounds. Timed-out shutdown does not cancel an exporter, and a later empty flush does not prove an earlier failed batch was delivered. Select configuration precedence explicitly: the reviewed HTTP exporter still merges `OTEL_EXPORTER_OTLP_HEADERS` despite builder-supplied headers. Production integration must prevent unintended environment configuration or destinations rather than treating a builder call as complete isolation.

Organize observations around real request, client-turn and session boundaries. A client turn may span several tool-loop requests; upstream attempts belong to one request. Correlate turns/sessions only from established caller context, not prompts, timing or response ancestry. A conversation/cache key alone does not prove either boundary. Unknown associations stay unknown; correlation needs neither long-lived session spans nor a new agent/session store. Request instrumentation follows body completion, failure or cancellation, not merely the handler's return.

Attach logs and adaptation summaries to the relevant request/attempt context instead of maintaining disconnected record trees. Use privacy-safe correlation fields; request, run and conversation IDs belong in logs/traces, not unbounded metric labels. A bounded registered Agent population may use `agent_id` as a metric dimension under the [future attribution baseline](#agent-identity-and-cluster-diagnostics). Semantic reports and projection evidence remain authoritative; do not duplicate their owners or add IR fields solely for telemetry.

Keep operational telemetry body-free and preserve business behavior when export fails. [Private exchange capture](#configurable-exchange-capture) remains a separate disclosure contract; adopting OpenTelemetry does not authorize business-content export.

The retained JSONL file adapter is a local sink for standard SDK records, not a framework to expand or OTLP wire format. A future authorized Collector integration replaces SDK exporter construction, not report producers or request ownership. Instrumentation follows the [acceptance-driven follow-up](roadmap.md#observability-follow-up): add only signals that answer demonstrated investigation needs, not an exhaustive field/event catalogue.

The selected local maintenance keeps the existing 64-record try-send boundary and one writer. It retains two private, whole-record files of at most 1 MiB each, recognizes its versioned format, locks both data-file namespaces and rejects unrelated/empty files or aliases. Recognized incomplete tails are repaired with a byte-loss count; this is not power-loss durability. Healthy output records cumulative loss/flush health; failed file output uses fixed, body-free background stderr warnings. Neither request recording nor the two-second best-effort flush waits for stderr. See [operator procedures](usage.md#body-free-diagnostics).

Each authenticated request receives a gateway-generated runtime-nonce/sequence correlation ID independently of these sinks; candidate ordinals are scoped to that request. See [minimal attribution](#shared-history-and-usage-attribution). This is neither authentication nor evidence of a client turn/session and is not forwarded upstream. `first_upstream_delta_ms` measures the first decoded nonempty text/reasoning/argument delta relative to request start, including previous attempts. It is distinct from first bytes and actual client visibility; use the [controlled consumer-side observation](development.md#sustained-operation-acceptance). Completed-block CLI output is not a first-delta signal, and network release proves neither remote compute nor billing cancellation.

### Configurable exchange capture

The single deployer owns all downstream clients and explicitly authorizes sensitive business request/response headers and bodies in controlled private logs for diagnosis. P6's four independent capture boundaries remain: inbound client request, outbound upstream request, inbound upstream response and outbound client response. Body capture includes JSON, streamed application content, tool arguments/results and non-authentication opaque payloads when enabled. Capture is not a source of replay, protocol truth or public test fixtures.

Capture is opt-in and off by default. Use private local storage, restrictive file permissions and finite queue, per-exchange and aggregate retention limits. Record redaction, truncation, dropped records and incomplete streams explicitly; do not label a partial record as complete. Keep the current best-effort availability contract: capture failure does not change the business response. Enabling content capture does not authorize remote export or printing captured business content into tool output.

Credentials are excluded even in full mode. Redact Authorization/Proxy-Authorization, authentication cookies, API keys, token-bearing query values, known authentication body fields and credential material before enqueueing or writing. Configuration cannot disable this step. If a capture unit cannot be sanitized safely, omit it and record the reason. Complete business capture is not byte-for-byte credential-bearing wire capture.

Capture belongs at gateway/transport I/O boundaries; codecs remain pure and only return bounded typed adaptation evidence. Stream incrementally within existing lifecycle bounds, sanitize at safe parsing boundaries and do not buffer the whole response or wait for storage before publication. Associate capture with an exchange-local identity and existing attempt ordinal without changing routing, retry or cancellation.

`gateway/capture.rs` owns queue/retention accounting and observed-body lifetime. Its private `capture/redaction.rs` owns the pure credential-removal policy; sanitization still precedes enqueueing. Body-free diagnostics and opt-in business capture remain separate disclosure contracts.

The startup-owned [capture configuration](usage.md#private-exchange-capture) selects four independent header/body boundaries. Authenticated client traffic and Generation upstream I/O use a private exclusive-create writer. Static upstream JSON borrows the existing intake buffer; SSE uses bounded event framing. Unsafe serialized-argument fragments and URI headers are omitted explicitly, not logged as raw strings. Frozen standalone-task upstream capture remains outside these hooks.

Capture is normalized application evidence, not a wire archive. Source EOF is distinct from semantic completion, body handoff and retained completeness. Queue, exchange and aggregate limits are finite; observed drops and writer failures remain queryable even when storage cannot retain a status record. No capture failure changes request acceptance or permits replay.

**Future cluster diagnostics baseline (2026-10-23).** The deployer permits sensitive business histories, request/response headers, bodies and tool data in explicitly enabled diagnostic capture, including failed exchanges. The target may be a trusted, deployer-controlled cluster collector once configured and separately deployed; this is not permission to publish captures or send them to arbitrary third parties. Authentication redaction above remains mandatory. Retained bodies remain sensitive: arbitrary secrets embedded in business text cannot be assumed fully detected or removed.

Keep structured operational events separate from full exchanges, joined by Agent/request/attempt and established conversation references rather than duplicating history in every error event. Capture must start while the exchange is in progress; an error cannot recover already discarded input or deltas. The initial planning default is bounded streaming capture with short success retention and longer error retention, not whole-response buffering or capture enabled only after failure. Storage choice, concrete limits and retention periods remain implementation decisions; export/storage failure must not block or fabricate inference completion.

Distinguish a captured HTTP exchange, gateway-observed conversation history and a complete Agent execution trace. Local tools, omitted history and pre-compaction context never sent to MorphieCore remain unknown unless separately supplied through an explicit client integration. Report truncation, dropped records, redaction, crashes and interrupted streams honestly; capture does not become a replay store or prove full conversation completeness.

## Settled decisions

These records preserve accepted choices and their consequences, grouped by domain. Detailed contracts above remain authoritative; implementation status, remaining work and acceptance belong to the [roadmap](roadmap.md#current-status). A selected decision is not evidence of a working client loop.

The 2026-10-23 standards reset supersedes conflicting historical conversion, public-dialect, manifest, internal-only notice and old-baseline decisions below. Preserve their dates as evidence; do not use them to constrain the approved redesign.

### Scope and activation decisions

| Decision | Consequence |
|---|---|
| DeepSeek parallel-tool admission (2026-10-25) | User selected no upstream parallel-control field for either DeepSeek protocol; reject explicit serial intent before dispatch and omit explicit parallel permission with evidence. Instruction-authority review remains separate; no role rewrite or routing expansion |
| Messages readable reasoning compatibility (2026-10-25) | User prioritizes ordinary cross-protocol Agent continuation and reasoning carriage over exact readable type/segmentation. Preserve unsigned bytes/order through a declared item-local delivery projection; retained snapshots match Messages history. Native replay/visibility, source IR and call identity remain protected; no model-quality or broader Provider acceptance claim |
| Local telemetry wiring (2026-10-25) | Scoped tracing/OTel request and attempt lifetimes project the existing report into native structured logs and completed spans through the existing private writer. No global registration, ambient destination, metrics reader, new retention mechanism or network exporter; Collector/export-health integration remains separately selected |
| Minimal diagnostic attribution (2026-10-25) | Admission-owned Agent/request references join inference logs/spans and private capture independently of sink availability. Bounded per-attempt source accounting retains observed facts and explicit loss, not completed delivery estimates or billing totals; no routing, public diagnostic schema or aggregation change |
| Unified Generation evidence (2026-10-24) | One pure typed effect delta, contextual typed failures and an execution-owned bounded report replace parallel audit ledgers. Checked conversion, publication and body handoff remain distinct. Logs and optional wire views project the same evidence independently of telemetry availability; no retry/health/no-replay change or production exporter activation |
| Standards reset and breaking redesign (2026-10-23) | User accepted semantic-union IR with unique ownership, exact-first ordered fallback, official public protocol baselines, actual-loss reporting and review of all prior conversions. Pin current stable dependencies and release sources before implementation; old mapping outputs are not regression authority. Reuse upstream standard-based tests in the integrated redesign gate, preserving independent safety invariants |
| Standard classified stream errors (2026-10-24) | User selected standard Responses `error` events for gateway-classified failures, preserving exact codes without inventing model-reported `response.failed` objects. Supersedes the former Codex-only failure shape; pre-frame and published failures retain separate ordering, bounds and SDK checks |
| Native-only continuation delivery (2026-10-24) | User selected genuine native replay carriers and typed gateway-managed Responses continuation instead of fabricated manifest reasoning/thinking owners. Unrepresentable stateless signed text/call paths reject; no new continuation extension or client-retention assumption is selected. Historical Google cross-protocol stateless acceptance does not certify the replacement |
| Explicit readable Chat reasoning (2026-10-24) | User authorized the review plan: keep Standard Chat default, add an explicitly configured narrow reasoning semantic profile, reject unsupported former private controls/history, and preflight evidenced incompatible output requirements. Begin with unsigned readable reasoning and complete actual-client tool history; no global MorphieCore dialect, fabricated manifest, reasoning-as-answer conversion or diagnostic exemption. Actual Agent acceptance always uses the host's current installed versions, separately from source pins |
| Latest Messages surface (2026-10-24) | [Latest stable official semantics](#protocols) govern downstream functionality. Singleton HTTP version admission and trusted upstream/probe version now use `2026-10-21`; removed sampling paths are deleted instead of retained for older upstreams. Root/by-value nullable eager/cache defaults reuse existing owners. Offline/SDK checks and live upstream version acceptance remain distinct |
| Remaining Generation mappings (2026-10-24) | Complete the selected request/report null, leading-prefix, native-grouping, target-effective sampling and structured-result corrections with source-backed independent assertions and existing SDK consumers. User selected rejection of unproven generateContent tool-media linkage and independent optional Interactions phase reports. [Foreground intake](#google-foreground-intake), [request approximations](#selected-request-approximations) and [service reports](#service-policies-and-reports) own behavior; no routing, binding, modality or live-conformance expansion |
| Canonical accounting and control projections (2026-10-23) | Scoped modality breakdowns and one reasoning-presentation owner replace competing writable facts. Preserve prediction until target selection, native Responses omitted-strict fallback and already representable allocation controls. Replace tool-error deletion with a bounded model-visible status wrapper; source facts, execution truth and caller ceilings remain protected. Owner-local cache/image and continuation work retain their separate remaining gates |
| Five-stage development order | 1. Same-protocol lossless upstream/downstream IR codecs; 2. Basic agent tool interactions; 3. Cross-protocol stability and `count_tokens`; 4. Embedding/audio/image/video APIs and Files/multipart Uploads; 5. Unscheduled citations, tool discovery, hosted tools, server-side compaction, Batch and Realtime. Lossless claims are semantic and scoped; approved loss is not losslessness. Scope and exit conditions live in the [roadmap](roadmap.md#planned-order) |
| Historical stage-1 baseline (2026-10-19) | Prior evidence used OpenAI Python 3.35.1, Anthropic Python 1.15.0 and pi 1.3.0. The 2026-10-23 stable-release reset replaces these active pins and their mapping-oracle authority, without changing the dates or scope of prior observations |
| Installed-client stable release policy (2026-10-23) | Supersedes the 2026-10-21 major-only policy. Harnesses require selected stable release versions from the source lock; full version/build provenance and isolated execution remain necessary. Host binaries are not automatically changed; historical 2.x OpenCode behavior does not certify the selected release |
| Google response-effort slice (2026-10-21) | User selected only the [three-level request-scope approximation](#google-response-effort-projection) and body-free terminal/usage failure classification. Terminal compatibility proposals and accounting approximations remain unselected; do not claim Messages full-loop acceptance |
| Google basic-loop closure and pause (2026-10-21) | User subsequently selected the bounded [Messages terminal profile](#google-messages-terminal-projection), basic tool/final-history acceptance, then [paused further Google API expansion](roadmap.md#google-api-pause). Incomplete coverage is accepted, not reclassified as full support; no new usage approximation or optional report owner is selected |
| Generation Text production closure (2026-10-21) | User selected a bounded closure before stage 4: protected-field repairs, local input/citation/report additions, existing-profile structured-output/tool-choice acceptance and sustained-operation checks. Scope is the existing loopback pi/opencode gateway, not new Providers, clients, routing mechanisms or deployment authorization. The [roadmap](roadmap.md#generation-text-production-closure) owns packages and exit gates; this selection is not implementation or acceptance evidence |
| Google text-report exception (2026-10-21) | A later narrow exception to the pause selects [terminal explanations and service-source review](#explanatory-terminal-reports). Unproven service mappings still reject. Probability reports, native client/static directions, new models and other API expansion remain paused |
| Generation design baseline approval (2026-10-20) | User explicitly approved the six-area stage-1/2 baseline at `26b63eb`, closing the [overall design review gate](roadmap.md#generation-design-review-gate) for that scope. Inventory exclusions, real-acceptance limits, the routing freeze and deferred observability remain unchanged. No new binding, broader Provider acceptance or unselected stage-3 design is approved |
| Shared Aliyun billing dialect (2026-10-19) | User selected one [Bailian dialect](#shared-aliyun-billing-dialect) for DashScope and TokenPlan. No billing-plan parser fork; shared-model controls have the same wire meaning. Catalogs, origin/key authority, charging and replay isolation remain distinct |
| Bailian compatible-request boundary (2026-10-19) | User selected existing-IR-only adaptation. Preserve documented native controls exactly; reject unowned/ignored guarantees and independent ceilings that cannot be represented. No cap summation, clipping, JSON repair, budget/effort preset or IR expansion |
| pi Haiku request profile (2026-10-19) | R2 is pi's `anthropic-messages` API with the built-in Haiku 5.5 compat flags. The user's separate pi extension declares that model for the gateway; MorphieCore adds no client-specific code, and OpenCode Go is only the current upstream. Its strict, deferred and eager-input tools, block binding, per-message effort, mid-conversation system messages and tool changes get typed owners and exact native carriers. Missing carriers use the selected named projections in the [request rules](#request-rules); protected controls without an approved fallback, including explicit block-binding `"error"`, still reject. Trusted binding policy enables carriers, and the encoder derives beta headers from the final wire. On 2026-10-19 bounded Go checks accepted all three beta carriers, so the Go/Haiku binding enables them. Strict tools, ordered instructions and tool changes are published under the all-native rule for owner-local cache admission |
| Stage-3 counting (2026-10-20) | User selected upstream-reported counts first, with explicit coarse estimates such as character-based heuristics allowed for inaccurate upstream reports; no gateway tokenizer-based exact counting. The [counting policy](#token-counting-policy) preserves effective-input scope, provenance and separation from inference usage. Responses input-token counting remains a proposed companion surface. The [2026-10-21 deferral](roadmap.md#deferred-token-counting) suspends implementation without changing this policy |
| Stage-4 operation scope | Complete selected Decision/Embedding/media contracts and implement Files/multipart Uploads with selected storage, authorization and lifecycle contracts. Video is in scope, but exact operations and owners remain to be designed. Do not force standalone operations into Generation or imply Batch, Realtime or new binding activation |
| Dual-SDK Decision direction (2026-10-23) | User selected stable standard TypeSafe and OpenAI SDK access through shared Decision IR and upstream codecs/dialects. Both client families are required; initial research covers TypeSafe, OpenAI, Perplexity and Cloudflare. The [direction](#decision-api-direction) is selected, while detailed representations, compatibility policies and integration remain unselected; its own design gate precedes implementation; the later stage-4 scheduling decision removes cross-family precedence |
| Stage-4 equal priority (2026-10-25) | User places Decision design/implementation/acceptance closure inside stage 4, at equal priority with Embedding, Audio, Images, Files/Uploads and Video. No API family is a blanket prerequisite for another. Preserve actual resource/shared-owner dependencies and per-operation selection gates; the [roadmap](roadmap.md#stage-4-scope) owns scheduling. No new endpoint, binding or unresolved contract is activated |
| Decision capability-union principle (2026-10-23) | User selected Generation-style unique semantic ownership and named projections across the full evidenced Decision capability union, considering approximations, simple estimates and rejection when exact conversion is unavailable. The [IR proposal](api-expansion-design.md#decision-semantics-and-compatibility) covers media, Box and service/report semantics without implying their first-slice activation or approval of individual rules |
| Stage-4 design priority (2026-10-22) | User prioritized S4-1 Embedding and S4-3 Images, with design first. Subsequently authorized the converged [Embedding implementation sequence](roadmap.md#embedding-integration-targets), starting with the local exact-JSON/finite-binary32 owner, little-endian Base64 profile, pure request expectation and existing-model closure. The later 2026-10-25 scheduling decision supersedes this cross-family priority without changing selected Embedding semantics or approving unresolved Image/media/resource contracts |
| Google Embedding target and compatibility (2026-10-22) | User requested a Google Embedding target and gateway-side encoding compatibility, then selected all four float/Base64 combinations with standard binary32 rounding on delivery copies. The [encoding policy](#embedding-encoding-compatibility) supersedes the earlier exact-only proposal. Select stable `gemini-embedding-2` for the [stage-4 target list](roadmap.md#embedding-integration-targets), initially text-only; its native synchronous carrier remains a design recommendation. This is an Embedding design exception to the Google pause, not a Generation reopening, runtime binding or live acceptance |
| Google Embedding ancillary projection (2026-10-22) | User subsequently selected the explicit [Google compatibility profile](#google-embedding-compatibility): bound-model labeling, reported-input-only total derivation, omission of unavailable usage and omission of an unsupported `user` hint. Preserve source facts and record applied rules; missing usage is explicitly not strict OpenAI conformance. Numeric conversion approval does not imply truncation, token-ID emulation or vector reshaping |
| Embedding review and resumption (2026-10-23) | After the Embedding-only pause and design review, the user explicitly authorized Google/OpenAI implementation and named lossy compatibility. The [sample-centered contract](#independent-embedding-path), immutable target plan and separate source/delivery types supersede batch-global request controls and response-copy mutation. Activate native Google text only after nested-config/no-truncation evidence; scoped ordinary `cl100k_base` tokens require native OpenAI-model carrier evidence. Extend honest ancillary omission to trusted activated Embedding profiles, not strict native OpenAI codecs or unselected Google APIs. No guessing accounting bases, tokenizer conversion, truncation or vector-space substitution |
| Complete Embedding usage (2026-10-23) | User selected strict OpenAI success shapes instead of omitted usage. Preserve reports, prefer attested derivations, then use the named request-local approximation for missing counters; no default all-zero fallback. The delivery type requires both scalars while source presence/basis stays untouched. Fixed UTF-8-byte and scoped-ID counting needs no tokenizer execution, extra Provider call or counting-endpoint reopening. Existing error, numeric and lifecycle boundaries remain strict |
| Grok Imagine scope revision (2026-10-22) | User selected the `grok-imagine-image-2.0` parameter projection/dialect codec and then deferred all video integration, superseding the earlier synchronous-wait choice. The 2026-10-25 equal-priority rule replaces that scheduling hold, not the unresolved implementation gates. Future video interfaces should primarily follow Grok's native asynchronous API; task identity, persistence, authorization and cancellation remain design gates, not an approved implementation. Keep 1.5/1.5-lite as future targets and lite as the future video test model; exclude the retiring image-quality model. Runtime jobs and credential affinity stay outside semantic IR. See the [remaining gates](roadmap.md#grok-imagine-adaptation) |
| Grok static image projection | The pure `GrokImagineImage2` profile admits only existing request meanings with exact native carriers; it requests bounded Base64 artifacts and rejects unsupported explicit controls rather than approximating pixels as resolution tiers. Creation time, effective model and optional token counts retain reported presence. Monetary ticks convert exactly to USD; no missing counts or timestamps are synthesized. Strict OpenAI output still requires its creation time and representable reports, so this codec alone does not activate a gateway binding |
| Stage-5 scheduling | All stage-5 tasks are unscheduled future work, not a delivery commitment after stage 4. Explicit compaction is separate from automatic triggers; Batch and Realtime require their own task/session/job designs. Existing compaction-item replay and stage-4 media job requirements do not wait for those future APIs |
| Tool discovery and native web search (deferred) | Unscheduled stage-5 backlog; implementation designs and per-slice scheduling remain unresolved. Do not block stages 1–4, coverage improvement or the P7 boundary review on them. Preserve existing foundations and reject unsupported semantics; deferral does not claim complete official API coverage. Goals and reopening boundaries stay in the [roadmap](roadmap.md#deferred-tool-discovery-and-web-search) |
| Cache prewarming (deferred) | Client replay warming already has an owner in pi; no gateway-owned background warmer is selected. Defer native cache-only support with priority and scheduling undecided, without blocking first-stage completion, coverage improvement or the P7 boundary review. Native cache resource references remain in scope. Ordinary one-token generation is not native no-output prewarming; preserve existing cache handling without claiming that contract. See the [roadmap](roadmap.md#deferred-cache-prewarming) |
| P7 first reconnection scope (2026-10-19) | Select Qwen3.8 Flash through DashScope Chat and GPT-6 Luna through OpenRouter Responses, one existing binding at a time under the roadmap's bounded contracts and gates. Messages full-loop acceptance and Google model-value guards remain separate. No new binding, route policy or protected-semantic loss is selected |
| Chat acceptance target revision (2026-10-24) | After DashScope returned HTTP 400 `Arrearage`, the user selected existing `aliyun-tokenplan-cn-chat` for Qwen3.8 Flash instead of `dashscope-flash-chat`. Reuse Token Plan's declared origin and separate credential pool, with the same model identity, shared Bailian dialect, explicit readable profile and bounded gates. Preserve DashScope as an independent catalog binding; no automatic billing fallback, credential mutation, deployment change or new capability follows |
| R1 closure authorization (2026-10-19) | The user resumed R1a/R1b closure independently of the then-pending overall Generation review, then limited the newly observed report boundary to analysis. The separate 2026-10-20 lifecycle decision selects that report design; acceptance status belongs to the roadmap |
| Responses lifecycle report selection (2026-10-20) | User approved [bounded phase-tagged reports](#lifecycle-settings-and-service-reports), explicit terminal-only JSON/history projection and resumption of R1b implementation/acceptance. Only the evidenced closed transition relation is admitted; that selection did not close the overall review or approve cross-protocol tier drift or routing expansion |
| R2 sequencing (2026-10-19) | The user deferred R1b and resumed R2's existing native pi Haiku profile independently. Its readiness and acceptance did not require the then-unselected Responses lifecycle-report design. No R1b implementation, overall design approval, automatic-cache admission or routing expansion followed from that scheduling change |
| Messages owner-local cache admission (2026-10-19) | The user separately resumed this slice. Enable the existing Go/Haiku binding's owner-local markers and publish admission only for all-native, opted-in configured candidates. Do not inject automatic caching beside explicit markers; preserve the marker-free default. Automatic-policy admission and unrelated frozen features remain closed; strict tools follow the pi Haiku profile row |
| Caller-selected automatic TTL (2026-10-21) | User selected [narrow automatic-cache admission](#caller-selected-automatic-cache-admission) using existing owners, initially Go/Haiku and all opted-in native candidates. Preserve explicit TTL and existing conflict checks; missing input keeps current default policy. Selection does not activate admission or establish live retention |
| Native Messages input activation (2026-10-22) | Complete the selected examples/context/automatic-TTL slice on existing Go/Haiku after bounded deployed evidence. Each feature has independent all-native configured admission and capability/preflight/preparation checks; generic profiles, other bindings and mixed routes stay closed. Keep retention, invoices, citations and new bindings outside this activation |
| Routing phase boundary | Generation routing functional development is phase-complete and closed. The roadmap owns [controlled acceptance and separate real failover](roadmap.md#controlled-routing-acceptance). Necessary maintenance and evidence-backed acceptance fixes preserve the selected contract; public semantic admission, bindings, credentials, standalone execution and unresolved client reconnection remain unchanged |
| Initial production support boundary (2026-10-22) | Retain the declared [profile baseline](development.md#initial-production-profile-baseline), a single candidate and an effective attempt ceiling of one for initial real production claims. Synthetic recovery, codec fidelity and configured multi-candidate admission are not real failover acceptance; a stronger claim needs compatible real credentials/Providers and replay/sticky-continuation evidence. This is an acceptance boundary, not automatic rewriting of user configuration, a new binding or deployment approval |
| Protocol tree and dialect ownership (2026-10-20) | Protocol code groups by vendor, wire API and responsibility; the vendor name is where the wire API is defined, not which Provider serves it. Dialect is a named deviation inside that API, model capability and adaptation policy are separate, and trusted bindings select them. A shared rule moves to its nearest common owner; protocols keep their real asymmetries instead of gaining placeholder modules |
| OpenAI access paths (2026-10-20) | Retain standard API-key codecs and [public SIWC](#subscription-responses), not the legacy Codex product backend. Removing its dialect and unused carriers does not remove downstream Codex compatibility or activate new model bindings |
| Manual upstream diagnostics (2026-10-22) | `morphiecore-upstream` owns one-request model-list JSON retrieval without pagination and one-request native text probes outside the gateway/IR. Shared embedded Provider metadata fixes targets/auth/default pools; no CLI endpoint overrides or hot reload. Select the default pool's first member or an explicit compatible alias, without availability scanning/fallback. Expiry-triggered OAuth refresh remains outside gateway inference. No discovery-driven activation, automatic inference sweep or capability claim |
| Native carrier availability is not activation | Native `representation_contract()` describes codec carriers; default `semantic_contract()` keeps generic binding admission unchanged. The selected [all-native Go/Haiku contract](#owner-local-cache-admission) is a narrow configured exception, not activation of other carriers. Public/topology checks remain narrower elsewhere until the scheduled integration review |
| Transitional test scope | While IR/codec contracts are incomplete, defer success scenarios that assume unselected completion conversions or binding admission. Keep established local contracts and safety assertions, measure pruning coverage, and rebuild selected integration coverage after the contracts stabilize. A green reduced suite does not establish the deferred client loops |

### Semantic ownership decisions

| Decision | Consequence |
|---|---|
| One semantic core | Gateway and future agent library share the IR; callers own agent behavior. Beyond context edits and response continuation, agent-only machinery joins the IR when the library needs it |
| Official Generation vocabulary | OpenAI, Anthropic and Google define semantic concepts; dialect carriers and approximation policies stay in protocol modules. Semantic expression, codec support and activated bindings are separate claims |
| Two codec responsibilities | Normalize Provider dialects into an official API baseline plus declared extensions before semantic decoding; encode in reverse before serialization. Semantic extensions enter IR; eligible non-semantic fidelity stays scoped outside it |
| Input-image size-policy ownership (2026-10-24) | Implement the accepted canonical-owner redesign on each image use: independent `AllowDownsize`/`RejectOversized`, not resource identity, detail or fidelity. Preserve native user/tool-result carriage and rejection without an exact target carrier; edits cannot restore an obsolete directive. No pixel processing, new modality or binding activation |
| Stage-1 report/event ownership (2026-10-19) | User-selected [Responses observations](#responses-item-and-event-observations): output-only bounded actor records, event-only summary interruption/compaction progress, and original error classification. No actor authority, static progress field or event-log service |
| Tool examples and document context (2026-10-21) | User selected [local model-input owners](#tool-examples-and-document-context): ordered examples belong to function definitions, context to each document use. Accurate Messages carriage first; missing target carriers reject. No description/prompt concatenation, omission or resource-identity conflation |
| Citation insertion positions (2026-10-21) | User selected [point anchors](#citation-point-anchors), distinct from whole/range claims and list ordinals. Confirm source units and `file_path` separately; unresolved carriers reject. Full citations, Files and hosted search remain outside this correction |
| Explanatory report ownership and loss (2026-10-21) | User selected [bounded refusal details and terminal explanations](#explanatory-terminal-reports), accurate native carriage and named cross-protocol omission of explanation-only information. Outcome, refusal/blocking, terminal causes, usage and execution policy remain protected; no generic report bag |
| Reported text-container presence (2026-10-20) | Preserve absent/null/object using the existing bounded presence type, without default insertion or a parallel report owner. Request null remains rejected; [reported text configuration](#reported-text-configuration) keeps this repair separate from broad presence redesign |
| Eager partial-input boundary (2026-10-19) | User selected native SSE-only unfinished argument carriage under `max_tokens`, reusing partial arguments and incomplete lifecycle; strict JSON/history and successful-terminal rejection remain. Complete clients may still repair/drop raw fragments, so wire fidelity does not certify client history fidelity |
| Accurate IR with declared lossy projection | Prefer exact carriers, permit approved advisory omissions and behavioral approximations, and reject loss of protected meaning. Preserve the source and record target-local changes; implementation follows the roadmap |
| Equal downstream contracts | Chat Completions, Responses and Messages receive equal consideration for static delivery, streaming and history replay; a missing carrier needs an approved projection or explicit rejection |
| Text-only instruction scope | Request-wide and ordered system/developer instructions admit only plain text. Instruction parts are `Part<Text>`, not resource-capable input parts. Reject media in requests and reported echoes at source decoding; no role movement, silent loss or instruction resource table is selected. Every official instruction carrier, including Messages mid-conversation system messages, is text-only; a shared SDK content union is not evidence of role-specific semantics |
| Ordered instructions | An instruction after conversation content is a separate `ordered_instructions` feature. Chat and Responses carry it natively; Messages represents it as a mid-conversation system message; Google targets reject it. Codec representation does not widen default Messages admission; the reviewed Go/Haiku binding opens it |
| Role-scoped content | IR validation rejects input media in assistant messages and citations or probabilities on user text. Generated assistant media wait for a selected output owner |
| Participant ownership (2026-10-24) | [Instruction/message labels](#participant-ownership) preserve official Chat request names, including call-only assistant owners. Envelopes and fidelity never become alternate owners; unsupported target carriage rejects and response names remain unselected |
| Ordinary Messages dispatch (2026-10-24) | Omitted and explicit `allowed_callers: ["direct"]` normalize to inactive/default dispatch on root and by-value definitions, without a loss receipt. Recognize ordinary tool `type` absent/null/`custom` before filtering; reject hosted/versioned discriminators and non-direct dispatch instead of coercing them into functions. No hosted execution or new capability activation |
| Unowned message fields | Non-null Messages `clear_at` rejects rather than being ignored. Non-null per-message `output_config.effort` on a system message becomes an ordered `ConfigurationUpdate` and rejects on other roles. Chat `store: true` rejects because the gateway provides no completion storage |
| Protected controls and reports without owners | Messages geography/speed/fallback, Responses access programs and misalignment details, Google attribution and unsupported Interactions lifecycle/step-error carriers reject explicitly. Messages block-binding controls now have typed owners and selected carriers/fallbacks; input transformations use validated named omission under the [request rules](#request-rules). Rejection of the remaining gaps is a temporary representation boundary, not complete IR support; see [contract gaps](refs/2026-10-17-generation-contracts.md#protected-semantics-without-complete-owners) |
| Exact non-strict default materialization | Chat/Messages non-strict defaults can become Responses `strict: false` without schema rewriting. Responses normalization defaults cannot become plain tools |
| Client text-history annotations (2026-10-20) | User selected [input-only Responses shorthand](#client-responses-history-shorthand): omitted annotations become an empty list with named insertion evidence. Null, malformed or supplied citations and all upstream snapshots remain strict; no client-lost citations are reconstructed |
| Program caller dependencies | Request calls name preceding program calls and linked results preserve caller identity. Self-describing response snapshots do not relax complete request-history associations |
| Gemini non-content stops | `MALFORMED_FUNCTION_CALL`, `UNEXPECTED_TOOL_CALL`, `TOO_MANY_TOOL_CALLS`, `LANGUAGE`, `OTHER` and unknown finish reasons remain attempt failures. They never become success; pre-publication failover stays available and the frozen routing classification is unchanged. Recitation has its own incomplete reason |
| Chat custom-tool identity | Use the official nested JSON carriers and preserve raw input/grammar. Resolve a result's function/custom kind only from its preceding call ID. The pinned Chat SSE schema has no custom delta carrier: reject rather than invent one or cast custom calls to functions; see [protocol sources](refs/2026-10-16-chat-custom-tools.md) |
| Separate task contracts | Generation, Embeddings, Images, Speech and Transcription are not one universal message type |
| Shared ASR model identity | Token Plan and DashScope expose one `qwen-audio-3.0-asr-flash` identity. Startup explicitly selects one trusted binding; origins and credential domains stay isolated. No ASR failover is introduced |
| Pure protocol boundaries | Encode from final typed input; no network in codecs or post-encoding JSON patches |
| Functional ownership | Keep root layers; organize internals by domain, without compatibility aliases or plugin frameworks |
| Explicit downstream identity | A closed client protocol and its adapter own decoding and delivery carriers. All Generation clients share the configured candidate route; Messages needs no dummy OpenAI adapter |
| Official intake fields | Decode registered source semantics accurately. Unsupported preferences need selected omission rules; critical content and execution intent cannot masquerade as unknown fields. Optionality alone does not authorize dropping billing, storage or quality controls |
| Reported tool results | Responses carries function/custom results in static output and item snapshots. Missing call references stay unreported; request history still requires matching preceding calls. Chat and Messages reject ordinary tool results as assistant output, not their supported history forms |
| Envelopes as grouping | Ordered message envelopes are the only IR grouping. Chat's call ownership is a view derived in the OpenAI codec, which also enforces Chat-only rules such as no calls on a refusal |
| Schemas as data | The IR keeps any bounded JSON-object schema an official API accepts. Each target checks its selected subset without rewriting constraints. generateContent function parameters require an explicit object root when a schema is present; omitted schemas stay omitted. Google protocols keep distinct structured-output shells; native strict-tool checks do not restrict client parsing. An unnamed output schema gets the fixed name `response` only on OpenAI targets, which require one |
| Structured constraints stay protected | Messages strict tools and JSON output schemas share the native subset check, including rejection of unsupported constraints and documented case-sensitive string enum/const fidelity gaps. Request-wide complexity includes both owners. The [bounded Go/Haiku profile](development.md#constrained-messages-profile-acceptance) verifies numeric enum schemas, adaptive thinking, root/message effort and required/named strict tools; it does not activate unsupported schemas or other bindings. No SDK constraint-removal helper or implicit default-strict conversion is used |
| Client-owned output caps | Preserve explicit client ceilings. A protocol requiring an otherwise absent ceiling uses the selected named default policy; [Messages request rules](#request-rules) distinguish the current fixed insertion from the accepted target-configured replacement. No model-maximum fallback or automatic budget adjustment is selected |

### Projection and reasoning decisions

| Decision | Consequence |
|---|---|
| Independent activation and scoped effort | Activation belongs to reasoning; a single scoped effort control belongs to generation settings. Response-wide effort is not reasoning-only effort, and off is not a positive level |
| Compatibility before rejection | Keep every representable fact, because retained state makes conversion more stable and extensible. When a supported client cannot carry or return some state, prefer a documented loss over rejecting its requests. Do not invent content, filler or validator-bypass values. Each existing rejection changes only through a selected rule |
| Strict downstream Generation schemas (2026-10-23) | Complete required downstream fields under [schema completion](#downstream-schema-completion): reported values first, then request/accounting inference, then documented text estimates. Keep presentation separate from source facts; preserve EOF, errors, security and unsupported-feature boundaries |
| Selected compatibility losses (2026-10-18) | Superseded for block binding on 2026-10-19 by the typed owner in the pi Haiku profile row. Targets without an identity carrier omit `user` and `safety_identifier` under `generation.omit-identity-hints.v1`. Assessment-only safety reports and OpenAI context-window stops follow the named rules above |
| Unified user attribution (2026-10-19, revised) | Permit projection to OpenAI `safety_identifier`, Messages `metadata.user_id` and generateContent `labels.safety_identifier` under [the attribution contract](#end-user-attribution). Explicit safety identity takes precedence during cross-protocol projection; independently supplied source fields remain distinct and same-priority conflicts reject. Preserve same-protocol native fields and keep cache grouping separate. This supersedes the earlier prohibition on user-to-safety projection, not the trust boundary or binding gates; implementation remains pending |
| Phase transfer and return (2026-10-18) | Named non-Responses request omission preserves source history. The former Chat/Messages manifest return is superseded by native-only delivery. Chat and Messages omit phase with `generation.omit-delivery-message-phase.v1`. Responses remains exact. No inferred labels, changed terminals or new binding activation |
| Gemini citation attribution (2026-10-18) | Nonempty `citationMetadata.citationSources` remains rejected: URI/range/licence may be required output evidence even without a search tool. Complete citation owners and downstream carriers remain deferred; neither a loss receipt nor hidden capture substitutes for attribution |
| Explicit same-model level calibration | P1 permits declared directional scope approximation on target copies; level mappings stay model-local. No inferred nearest level, inverse or off-to-on conversion |
| Disabled activation priority | P2 preserves a supported off carrier and names omission of independent unrepresentable response effort; a target without off support still rejects |
| Preset-based thinking allocation | P3 uses six distinct shared budgets from 1024 to 24576; target-specific tables may contain duplicates. Numeric conversion minimizes absolute token distance and chooses the highest level on every tie. Preserve original intent and hard caps; this is explicit lossy quantization, not an inferred qualitative inverse |
| Thinking budget and output ceiling | P3b rejects explicit or conversion-generated budgets above the applicable client output ceiling. Preserve both values; stricter target relations still apply, without automatic reduction or cap increases |
| Messages manual-budget scope (2026-10-19) | Select ordinary manual budgets only for the pending carrier work: at least 1024 tokens and strictly less than the applicable output ceiling, enforced at Messages source/target boundaries. Defer the wider manual-interleaved budget-scope exception and reject requests requiring it; do not add a new budget-scope abstraction for this slice. Shared IR equality does not establish Messages validity. Preserve P3b and no-clipping rules; exact carriage needs a supporting target, and adaptive-only bindings remain unchanged |
| Messages missing output ceiling (2026-10-19; details approved 2026-10-20) | Use the [candidate-local insertion policy](#missing-output-ceilings): omission inherits, positive overrides, `null` clears. Go/Haiku declares `32000`; generic targets and pure codecs have no fallback. Reject contradictory default/maximum declarations before requests and retain final budget/model checks after selected conversions. Preserve explicit caps; no clipping or cap increase |
| Model-effective value limits (2026-10-19; configuration 2026-10-20) | Keep model-specific effort levels and output maxima in [trusted target caps](#model-effective-values), outside protocol-neutral IR and routing policy. Candidate configuration inherits catalog limits and may only narrow them. Check final controls, retained updates and inserted defaults through the shared encoder; preserve selected mappings, reject violations and add no clamping. This activates no binding and does not certify upstream enforcement |
| Bailian output ceiling | Current Qwen3.8 Flash/Max, GLM5.3/Prime and Kimi-K3 Chat bindings use the documented total `max_completion_tokens` on both billing plans. Qwen/Kimi legacy `max_tokens` is reply-only; GLM5.3's legacy cap already includes thinking. Responses total-cap admission is scoped separately. Do not infer these contracts for Coding Plan or unbound models; wire preservation does not prove upstream enforcement |
| Target-effective sampling | P4 preserves supported controls and permits named omission only for declared fixed/ignored behavior; unknown effectiveness is not permission to discard a control |
| Summary granularity and visibility | P8 permits approximate summary granularity while preserving show/hide intent. Do not enable reasoning, fabricate summaries or expose hidden content |
| Canonical instruction roles (2026-10-25) | User selected the latest reviewed standard for IR itself: application instructions have canonical Developer meaning, replacing the legacy System/Developer authority split. The implemented [mapping contract](#instruction-authority-and-canonical-roles) retains multipart root/ordered scope and confines DeepSeek and other Provider deviations to dialects; it supersedes the root-only selector and application→user fallback. Subsequent Provider repairs select string/placement rejection instead of implicit composition. Verification boundaries remain separate from real Provider acceptance |
| Target-local media detail | P8 prefers exact image/file detail carriers. Omission needs proven target-default equivalence; other approximations need evidence-backed named directional rules. No blanket media-option omission or inferred auto/absence equivalence |
| Model-visible filenames | P8 requires an exact filename carrier, including filename/title mapping, or rejection when a name is present. No omission or automatic text injection; empty and absent remain distinct |
| Reasoning-context defaults | P8 permits omission only for proven equivalent target defaults; otherwise preserve exactly or reject. Do not simulate context controls by deleting messages, required replay or compacted history |
| No default-strict-to-plain conversion | P8 rejects `DefaultStrictAsPlain`. Omitted Responses normalization defaults are not non-strict tools; preserve their source semantics or reject, without removing explicit constraints |
| Predicted-output optimization | D3 selects named omission of Chat prediction after source validation until supported. Record the loss without changing generated text or fabricating usage |
| Registered reasoning-view presence | Absent/null/empty plaintext views make no assertion; nonempty duplicates must agree. Explicit structured parts and replay ownership remain intact |
| Unsupported known request semantics | Chat `moderation` and content-part `prompt_cache_breakpoint` reject before unknown-field filtering. Messages tool examples have exact native carriers and a separately gated binding requirement. Non-null Chat moderation reports reject in JSON and every SSE chunk, including usage tails; null is inactive. Neither security policy/reports, explicit cache boundaries nor model-visible tool examples may be silently discarded |
| Layered capabilities | Source validity, acceptance through approved adaptation and faithful target support are distinct. Do not advertise lossy acceptance as native support or infer public promises from a union of candidates |
| Declared adaptation | Named candidate-local losses; known fields strict, unknown protocol fields ignored |
| Candidate-local tool errors | Messages admission preserves tool-result error flags regardless of the route. Native targets retain them; OpenAI targets apply a named loss on their private copy |
| Google tool-result truth and order (2026-10-20; media revised 2026-10-24) | Reject explicit execution reports without an exact carrier; retain independent error flags. The standards recheck supersedes the earlier `$ref`/`displayName` projection with [tool-media rejection](#media); text order and exact structured values remain. This is codec support, not real Provider or client-loop acceptance |
| Interactions status observations (2026-10-20) | Select [final-envelope authority](#google-foreground-intake), not an inferred transition graph or last-update equality policy. Validate identities/continuations before accepting usage. Updates never authorize tools, retry or successful termination; foreground opening, final step/call checks and EOF gates remain strict |
| Interactions sparse lifecycle metadata (2026-10-20) | Allow lifecycle IDs and opening status to be unreported without fabricating them. Validate provided IDs, bind the first real identity, retain it through later omission and reject contradictions. HTTP attempts own stream identity; Started acknowledges native creation, not an inferred `in_progress`. Update/final statuses, call/result identity, final-envelope authority and EOF gates are unchanged. No model fallback, call-signature carrier or service-report admission follows implicitly |
| Interactions function-call signatures (2026-10-20) | Use existing `ToolCall.replay` and a distinct call-only native format with current dependency/compatibility rules. Return unchanged signatures through the existing manifest and native function-call input, finalize only on closure and reconcile rather than create snapshot state. Keep thought/part/hosted owners separate; do not add a raw step bag or claim that envelopes authenticate state |
| Interactions partial snapshots (2026-10-20) | Compare supplied lifecycle fields against original streamed owners, retaining empty source ordinals and rejecting new/contradictory content, call facts, signatures and errors. Never republish snapshot content or select snapshot-only buffering. Unproven multipart boundaries remain unsupported; real opening/service-report admission is independent |
| Configurable reasoning parameters | Numeric thinking tables belong to public models; candidate-local conversion direction is explicit. Reasoning-as-text defaults to false and requires opt-in. Configuration cannot alter dialect implementations, infer qualitative mappings or widen semantic admission |
| Target carriers and protected content | Prefer official carriers or declared extensions. Selected role approximations change only the target representation; they do not invent content, assistant/tool speakers or call/result links. Do not drop task content or media merely to make a target accept the request |
| Target-local image restrictions | Formats, encoded sizes, counts and unavailable per-use options are checked or projected at the protocol boundary, not encoded as Provider-specific IR fields |
| Generation controls | Positive top-k, ordered exact stop sequences, independent presence/frequency coefficients and best-effort seeds have separate carriers. Absent, permitted null and empty stop lists mean no control; target limits remain codec-local. Messages has no sampling carrier; its explicit fixed-sampling policy omits temperature/top-p and rejects top-k. Reviewed Google bindings use their selected ignored-sampling policy. Ordinary manual budgets have a native carrier; adaptive-only bindings keep rejection. Unbound tokenizer-specific logit bias remains rejected |
| Service-policy intent and reports | [Closed policies and separate reported classes](#service-policies-and-reports), not speculative field combinations. Native names and model aliases never establish cross-service equivalence or effective reports; dynamic alias selection remains deferred with routing integration |

### History and cache decisions

| Decision | Consequence |
|---|---|
| Cache storage intent is protected | Prefer exact retention. Only `"24h"` may be omitted under an explicit, default-off trusted target policy after reviewing its default storage; `in_memory` stays protected. No registered target opts in. Do not infer equivalent TTLs or repair malformed/active request cache options |
| Append-only history projection | Earlier content does not change when a later turn is added |
| Scoped opaque replay and visible-history transfer | Preserve compatible replay. Omit complete incompatible reasoning on a target-private copy with a named transfer receipt; preserve visible content and actual call/results, reject partial or damaged compatible state, and never discard required compacted/cached context |
| Selected attachment return contract | Actual part/call replay stays in the IR. Each client encoder emits the bounded opaque manifest in its own carrier before validation. Restore original owner segmentation only when the visible projection and manifest agree; otherwise drop that carrier's part/call replay with a receipt and keep the visible history. A client that discards a whole opaque-only turn loses that turn's replay; the gateway accepts the resulting history. No immutable edit identity or unverified client compatibility claim |
| Replay dependencies | Validate recorded reasoning, part and call obligations before sealing. Bind owner lifecycle as well as content; missing owners/values reject. Partial replay cannot become final through omission or defaulting |
| Prefix-cache ownership (migration completed 2026-10-24) | One semantic boundary owner distinguishes request-inherited lifetime from owner-supplied TTL, including separate inner and outer tool-result boundaries. No fidelity marker survives deletion. Automatic caching and maximum retention remain separate; preserve native TTL admission and bounds. Unsupported targets reject rather than substitute TTLs, cache keys or retention |
| Compacted context is required history | A distinct ordered compaction item preserves opaque content and identity. Incompatible targets reject it rather than apply the foreign-reasoning strip rule |
| Gateway-only history | [In-memory TTL history](#stateful-history) defaults to enabled for OpenAI Responses compatibility; `store: false` opts out. Continuation, retrieval and deletion remain synchronous, without Provider-held state or background execution |
| Agent session organization (2026-10-25) | [Per-Agent directories of JSONL sessions](#agent-session-files) are the selected current-stage direction when storage is enabled. Cross-Agent work explicitly reads or copies earlier sessions, preserving provenance and replay restrictions. Concrete records, migration and persistence implementation remain unselected; current TTL/API behavior is unchanged |
| Stateless Google Interactions | `store: false` and full history; no `previous_interaction_id` or revision header. Signatures keep their native formats |
| Existing cached context | A typed optional reference belongs to `GenerationRequest` as its input base, not settings, an ordered item or advisory cache hints. Preserve resource identity and reported expiry; unreported expiry is not unlimited retention. Boundary records hold provenance and an explicit target clock checks known expiry. Never fetch, expand, renew or silently omit the reference. Default binding admission and public request schemas stay unchanged |

### Routing and security decisions

| Decision | Consequence |
|---|---|
| Stable entry paths | Provider entry paths are concrete unescaped ASCII. Reject encoded dot segments, separators, whitespace/control bytes and URL-escaped syntax before topology compilation; request-path normalization must not change the path used for cache/replay identity |
| Stable routing revision | Bind complete bounded pools; scan legal successors under accumulated restrictions; gate dispatch on generation-fenced cooling with exclusive foreground recovery. Preserve stable order, exact-credential affinity, one exchange budget and no replay after publication |
| Shared model identities across protocols | Compatible API-key, OAuth and native Messages bindings share one public model and ordered route. Endpoint protocols and credential domains stay distinct; narrower candidates reject requests they cannot represent |
| SIWC cross-Provider fallback | A mixed route may contain one SIWC endpoint and advance to another Provider before publication within the route budget. SIWC pools remain single-account with no credential fallback; standalone SIWC routes remain single-attempt with fallback disabled |
| Case-sensitive model labels | Public IDs may preserve upstream ASCII capitalization, but resolve only registered routes. Provider prefixes remain rejected; Provider, endpoint and route IDs keep their lowercase grammar |
| Codec dispatch and execution | Adapters own upstream decoder and downstream encoder dispatch. Execution owns attempt identity, budgets, EOF validation and publication; candidate-transition rules are pure and independent of I/O |
| Dataflow and scoped state | Typed stages have separate semantic, protocol, attempt and delivery authorities; optional stages do not introduce a generic pipeline framework |
| Typed startup activation | One `TaskEntries` input selects Generation and each standalone task explicitly. Empty lists activate nothing; embedding does not discover catalogs, credentials or environment proxies |
| Separate startup configuration | `gateway.json` owns service settings, `auth.json` owns versioned Agent-key records, and `models.json` explicitly selects catalog models and ordered candidates. Candidate-local trusted deployment declarations may override URLs, wire model names and credential pools without changing Provider identity or dialects. All Generation client protocols use the configured route; frozen tasks keep their single catalog binding |
| Failover, not balancing | Legal candidates, bounded advancement and exact-credential affinity |
| Conversation routing | No inferred turn, branch or message routing |
| Separate credential lifecycle | Private file store and ordered pools; no inference-triggered login or refresh |
| Agent-centered identity (2026-10-23; API keys selected 2026-10-25) | [Identity contract](#agent-identity-and-cluster-diagnostics): one deployer, no business user/tenant layer. Versioned private API-key records establish stable configured Agent identity; display edits and key replacement do not change that ID. OAuth, managed registration, history authorship, usage aggregation and cluster activation remain gated |
| Independent CLI paths | Default configuration lives in the home-relative application directory, with credentials in its `credentials` child. CLI credential/proxy overrides beat file settings; omitted credential paths retain their independent home-relative default. File-relative paths resolve beside the declaring file; no legacy discovery or automatic migration |
| Agent-sized bounds | Bodies, IR budgets and deadlines fit whole-history agent sessions and long turns: tens of MiB and one hour per exchange. Upstreams enforce their own maxima |
| Inert, credential-free media | Inline data or URLs only; no gateway fetch or file service, and no client-supplied Provider file references |

### Delivery and evidence decisions

| Decision | Consequence |
|---|---|
| Upstream-maintained protocol tests (2026-10-23) | Prefer official SDK and Vercel protocol/Provider fixtures, plus established SSE suites, over a parallel hand-maintained dataset. Evaluate codec/IR fidelity, multi-turn carriage and selected task/resource contracts, not model intelligence or tool execution. Track latest GitHub source candidates while keeping verified regression revisions reproducible; upstream expectations do not override security bounds or approved semantics. Reuse these assets as additional operation-level acceptance evidence. [Development](development.md#upstream-test-sources) owns source/update rules; [Roadmap](roadmap.md#test-supplementation) owns the current text/SSE prerequisite and later stage-4 acceptance gates |
| Ecosystem-first observability (2026-10-20; Rust stack selected 2026-10-24) | Use the tracing log/trace bridges, typed OTel metrics, standard SDK processors and OTLP/HTTP protobuf with an explicit blocking client. Retain domain evidence, not another telemetry framework. The test-only feasibility slice precedes production wiring; [Observability](#observability) owns configuration, lifetime and disclosure boundaries |
| Required and incidental output evidence | P5 preserves required output facts and allows selected incidental-report loss after validation; request intent never fabricates upstream reports |
| Configurable internal exchange capture | P6 permits opt-in business headers/bodies at all four I/O boundaries with separate controls, finite retention and best-effort writes. Credentials are always redacted; no switch permits credential-bearing logs |
| Cluster diagnostics planning (2026-10-23) | [Future capture baseline](#configurable-exchange-capture) permits sensitive business diagnostics in explicitly configured deployer-owned infrastructure, preserves mandatory authentication redaction and bounded best-effort capture, and distinguishes observed exchanges from complete Agent traces. No backend deployment or export is activated |
| Actual adaptation reporting (selected 2026-10-23; implemented 2026-10-24) | The [domain evidence interface](#domain-evidence-interface) replaces parallel internal receipts with returned deltas, typed causes and atomic checked-unit acceptance under one execution report. Optional bounded notices and independent log projections are implemented; public schema/consumer certification remains deferred. Preserve source facts, selected-attempt isolation, publication honesty and privacy; a possible-adaptation header is not an actual receipt |
| Safe-to-ignore diagnostic extensions (2026-10-24) | [Optional namespaced fields](#public-capability-and-adaptation-notice) may carry selected loss, error and routing diagnostics for a future deployer-owned Runtime. Standard SDK Agent loops remain independent of them; no client display, retention or echo is required. Field organization/aggregation and dedicated extension tests are deferred to final production preparation, not current delivery gates |
| Bounded text-delta coalescing | Startup-only, disabled-by-default optimization after client projection and before encoding. One pending run retains its prepared context and fixed deadline; invalid bounds fail startup. Final-only clients retain existing materialization; original intake validation and publication safety remain authoritative |
| Scoped source accounting | Retain raw buckets and latest operation/step snapshots independently; derive delivery views without summing snapshots or inferring unknown inclusion relations. Source-default zeros and optional counts remain distinguishable |
| Honest delivery | Publication forbids advancement; success requires validated EOF and handoff |
| Owner-local state representation | Each [pipeline owner](#layers-and-data-flow) retains its own rejection, finalization or acknowledgement authority; no shared dispatcher or global completion flag |
| Agent-shaped failures | Sanitized error classes match pinned client behavior; refusals are not retries |
| Delivery-local identity | Generated client IDs/timestamps are not upstream facts and remain stable once selected, even if an upstream ID is first reported later. Keep the source fact on its own metadata owner. Responses allocates a gateway-owned ID; only a stored, fully delivered snapshot makes it a history key |
| Terminal stop evidence | Completion cause is separate from outcome and turn progress, including Google STOP beside calls. Source reports never derive from sent controls or output text. Chat coarsens and Responses omits causes under the [selected terminal approximations](#selected-request-approximations); the [Google Messages profile](#google-messages-terminal-projection) changes only a delivery copy with recorded context/approximation. Strict native codecs, non-success outcomes and unknown-context rejection remain unchanged |
| Safety policy and reported assessment | Request probability-threshold settings belong to `GenerationSettings`; actual input/output assessments belong independently to `GenerationResponse` and report events. Preserve report order and source facts without deriving them from controls, confusing probability with severity or inferring an outcome. Default, disabled filtering and no category blocking remain distinct. Assessment-only reports are omitted under a named rule; blocking reports and unsupported policy carriers reject |

Pinned clients and protocol sources are in [References](references.md). Official contracts govern protocol expectations; observed client behavior is compatibility evidence, not permission to redefine those contracts, weaken security or fabricate semantics.
