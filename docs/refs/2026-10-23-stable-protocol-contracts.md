# Stable-release protocol contracts

Checked **2026-10-23** against official releases, SDK source and current API documentation. The [source lock](../../tests/upstream/sources.lock.json) owns exact release revisions; [Architecture](../architecture.md) owns accepted design and [Roadmap](../roadmap.md#standards-reset-and-redesign) owns execution. This note records source evidence, not a second plan or run report.

## Source selection

Use registry stable versions and matching release tags, not default-branch HEAD. OpenAI Python 3.37.0 resolves to `e8a0fd28bbdc1699c97ff0f602b1af3dd378382d`, whereas the earlier research checkout used `81792ff`; the four inspected Chat/Responses response/event types agreed, but `_models.py` differed. Similar version strings do not prove identical SDK behavior.

Vercel's selected package tags all resolve to `594495a5af70547d9d7a13d6df334ae30db49622`, not the previously inspected `e623e8c` main checkout. Its fixtures supplement official contracts; SDK warning-and-repair behavior is not protocol authority.

At the selected Anthropic Python **and TypeScript** releases, `LICENSE` is Apache-2.0, not the older MIT license. Acquisition metadata must follow the selected source. The imported licenses and byte digests are recorded in [assets.lock.json](../../tests/upstream/assets.lock.json); public availability alone is not license review.

## Anthropic API 2026-10-21

Sources: [migration guide](https://platform.claude.com/docs/en/api/migration-2026-10-21), [Python 2.0.0 release](https://github.com/anthropics/anthropic-sdk-python/releases/tag/v2.0.0), [selected 2.1.0 source](https://github.com/anthropics/anthropic-sdk-python/tree/9313f13a5a643ceed10fafba6785eab8e04b6e6e).

- `temperature`, `top_p` and `top_k` are removed from Messages/Batch requests; sending even a former default value returns 400. The guide states that no replacement restores a nondefault sampling effect. This is API-version semantics, not merely a model-specific SDK restriction.
- Documents directly in assistant messages reject; documents nested in returned tool results remain accepted. Nonempty text citations in user messages reject; assistant citations and document citation enablement remain separate.
- SDK API keys now use `Authorization: Bearer`; the service still accepts either Bearer or `X-Api-Key`. A client holds one credential. Preserve ingress rejection of ambiguous credentials rather than accepting both indiscriminately.
- `MessageParam` becomes role-discriminated. Dict input remains valid; generic construction/subclassing assumptions require review.

The gateway currently sends the older upstream version. Installing new SDKs and updating a loopback harness do not migrate its protocol admission, encoders or native binding acceptance.

## OpenAI Responses strictness

Source: [function calling](https://developers.openai.com/api/docs/guides/function-calling#strict-mode).

Explicit `strict:true` rejects incompatible schemas. When Responses omits `strict`, the service attempts normalization and falls back to best effort when normalization is impossible; the returned tool then reports `strict:false`. Chat omission remains non-strict. Preserve requested policy separately from observed effective strictness. An old local assertion that every non-normalizable omitted-strict schema must reject is not the current service contract.

## Google API versions and semantic differences

Sources: [API versions](https://ai.google.dev/gemini-api/docs/api-versions), [Interactions v1 schema](https://ai.google.dev/static/api/interactions-v1.openapi.json), [v1beta schema](https://ai.google.dev/static/api/interactions.openapi.json), [v1 Discovery](https://generativelanguage.googleapis.com/$discovery/rest?version=v1), [v1beta Discovery](https://generativelanguage.googleapis.com/$discovery/rest?version=v1beta), [thinking](https://ai.google.dev/gemini-api/docs/thinking), [Interactions overview](https://ai.google.dev/gemini-api/docs/interactions-overview).

- SDK types span versions. Interactions v1 omits sampling controls that v1beta retains as deprecated. Current generateContent documentation also describes model families that ignore sampling; serializability does not prove effective control.
- Interactions documents signatures on thought/built-in-tool steps, not ordinary function calls; current v1/v1beta `FunctionCallStep` schemas omit `signature`. Prior observed ordinary-call signatures remain upstream compatibility evidence, not public-standard authority. The documented stream can start with an empty signature and later supply its delta; this does not permit discarding a completed required signature.
- Function/output schema descriptions explicitly identify ignored constraints. `VALIDATED`/`ANY` and matching schema JSON do not establish equivalence to every OpenAI strict constraint; tool choice and schema adherence are separate requirements.
- generateContent introduces `responseFormat.text.{mimeType,schema}` while retaining deprecated carriers. Source normalization must check conflicts and preserve one semantic owner.
- generateContent `store` controls request logging. Interactions `store` controls retained interaction resources/continuation. Neither is the gateway's private diagnostic-capture switch; do not merge these different meanings.
- Google Python 2.33.0 adds audio/document/video function-result subcontent, while the inspected public Interactions schemas enumerate text/image result arrays. Record this source disagreement; SDK breadth alone does not activate a public carrier.

The [Gemini 3 guide](https://ai.google.dev/gemini-api/docs/gemini-3) still documents `thinking_budget` backward compatibility, while the [3.8 migration guide](https://ai.google.dev/gemini-api/docs/whats-new-gemini-3.8) recommends `thinking_level`. Both selected Discovery versions declare `ThinkingConfig.thinkingBudget` as `int32`. These distinguish native wire support from preferred controls and model admission; they do not independently establish a live model budget guarantee. The gateway's exact-first budget check must agree with its explicit target admission, not infer support merely because an integer serializes.

Google web schemas are mutable; their version, revision and content fingerprints are recorded separately in the source lock. A changed digest requires review, not silent expected-output regeneration.

## Consumer and fixture evidence

Official Python SDK base models accept extra fields, but this alone does not establish public extension compatibility. Anthropic's stream accumulator copies known delta fields into its final Message; unknown late extensions are not automatically retained there. Test raw-event visibility and final-object behavior independently. No inspected standard defines a generic successful gateway-adaptation warning field.

Useful upstream entry points include Anthropic `tests/lib/streaming/fixtures`, OpenAI Responses/Chat stream and returned-history tests, Google model tests and Vercel provider fixtures. Their selected paths and licenses must be checked during acquisition. SDK tests against their own mocks are not evidence that MorphieCore ran; connect raw fixtures and real consumers through the gateway boundaries.

Old private captures and installed-client observations remain sensitive compatibility evidence, not normative or publishable fixtures. Public fixtures also require content and license review before import.

## Official contract difference inventory

Rechecked **2026-10-23**. This is a bounded inventory of current Generation request, result, event, history and HTTP seams, not an enumeration of every official field. Source-derived findings are distinguished from executable probes. Runtime behavior is unchanged by asset integration; scheduling belongs to the roadmap.

Primary release evidence:

- [OpenAI Chat types](https://github.com/openai/openai-python/tree/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/chat), [Responses types](https://github.com/openai/openai-python/tree/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses), and the strictness guide above.
- [Anthropic request/result types](https://github.com/anthropics/anthropic-sdk-python/tree/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/types), [stream accumulator](https://github.com/anthropics/anthropic-sdk-python/blob/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/lib/streaming/_messages.py), and the version-specific migration guide above.
- Google versioned schemas above, especially `GenerationConfig`, `FunctionCallStep`, `FunctionResultStep`, `ThoughtStep`, `ThoughtSignatureDelta`, `FunctionResponseBlob` and `GenerateContentRequest`; [pinned Python types](https://github.com/googleapis/python-genai/blob/92f72b3a236a5bc56e8e3327ab9563f77dd119c3/google/genai/types.py) and `_gaos/types/interactions`.

### OpenAI

| Boundary / minimal example | Current behavior and evidence | Classification |
|---|---|---|
| Chat request `prediction: {type:content, content:hello}` | [request intake](../../src/protocol/openai/chat/request.rs) now retains bounded typed text until target selection; supported Chat is exact, other targets record omission | Corrected; source/edit/isolation requirement in `standard_contracts::prediction_survives_same_protocol_projection`; catalog support remains explicit |
| Chat history `assistant` with `name:planner` | [item intake](../../src/protocol/openai/chat/items.rs) rejects a non-null participant name despite the official assistant-message carrier | Semantic-owner gap; executable named-participant requirement |
| Responses function schema containing `oneOf`, with `strict` omitted | [strict-schema check](../../src/protocol/openai/lowering/request.rs) now preserves omission and original schema for native service fallback; explicit true still checks the strict subset | Corrected; executable requirement contrasts false, true and omitted without inventing an effective false report |
| Streamed Responses failure before the first upstream frame | [ingress failure delivery](../../src/gateway/ingress/http.rs) constructs delivery without request presentation; [required settings](../../src/protocol/openai/responses/envelope.rs) are completed only with that context | Source-traced delivery gap: required `tools`, `tool_choice`, `parallel_tool_calls`; focused HTTP reproducer remains for delivery work |
| Public Chat controls/reasoning extensions and Responses synthetic encrypted reasoning | [startup](../../src/gateway/startup/config.rs) selects MorphieCore Chat; [attachment carriers](../../src/protocol/attachments/carriers.rs) manufacture reasoning owners | Public-dialect/continuation redesign, not official replay authority |
| Chat `n:2` or multiple streamed choice indices | [request envelope](../../src/protocol/openai/chat/envelope.rs) and [event intake](../../src/protocol/openai/events/decode/chat.rs) support one candidate | Explicit coverage exclusion; multi-candidate execution is not activated |
| Chat `moderation` or non-null input/output moderation reports | Recognized controls/reports reject in the same envelope and item/event owners | Known safety-report coverage gap, not unknown-field discard or acceptance |

### Anthropic

| Boundary / minimal example | Current behavior and evidence | Classification |
|---|---|---|
| `anthropic-version:2026-10-21` versus `2023-06-01` | [ingress](../../src/gateway/ingress/http.rs) does not select a versioned contract; [target preparation](../../src/protocol/anthropic/messages/target.rs) sends the older version | HTTP/version migration gap, not fixed by SDK installation |
| Messages request `temperature:1`, `top_p:1` or `top_k:1` | [client intake](../../src/protocol/anthropic/messages/client/request.rs) still admits these removed fields | Public-version admission gap; executable requirement includes former defaults |
| Assistant history text with a valid nonempty citation array | [shape validation](../../src/protocol/anthropic/messages/shape.rs) rejects all nonempty text citations, rather than distinguishing user/assistant | Explicit citation coverage gap; stage-5 expansion remains deferred |
| Ordinary function with `allowed_callers:[direct]` | Client intake rejects any `allowed_callers`, including direct-only dispatch | Basic tool fidelity gap; executable requirement, not hosted-tool activation |
| Tool `eager_input_streaming:null`; thinking `display:null` | [wire request](../../src/protocol/anthropic/messages/wire/request.rs) / client intake reject; selected SDK types permit null | Eager streaming has a documented null default; display remains SDK/schema discrepancy pending service evidence |
| Gateway-only state emitted as empty signed thinking | [static delivery](../../src/protocol/anthropic/messages/client/response.rs) and [SSE delivery](../../src/protocol/anthropic/messages/client/sse.rs) append a manufactured block | Continuation owner redesign; preserve real upstream signatures |
| Extension supplied only in a late `message_delta` | SDK accepts extra fields but its final-message reducer copies an enumerated set | Consumer-contract distinction, not a generic successful warning carrier |

Bearer/X-Api-Key singleton authentication already aligns at [ingress auth](../../src/gateway/ingress/auth.rs); ambiguous credentials remain rejected. Direct assistant documents already reject. Documents nested in supported tool results and the SDK's system-message role must not be accidentally prohibited by the migration.

### Google

| Boundary / minimal example | Current behavior and evidence | Classification |
|---|---|---|
| Interactions v1 request with temperature/top-p | [encoder](../../src/protocol/google/interactions/request.rs) emits fields absent from v1 `GenerationConfig`, though retained/deprecated in v1beta | Version-specific carrier gap; service rejection versus ignoring is unverified |
| generateContent sampling for the registered Gemini 3.8 model | [encoder](../../src/protocol/google/generate_content/request.rs) serializes controls that current Discovery describes as ignored on Gemini 3.6 Flash and newer | Model-effectiveness distinction, not a generic serialization failure |
| Interactions `function_result.result: {ok:true}` | [item projection](../../src/protocol/google/interactions/items.rs) rejects a structured object allowed by both public schemas | Structured-result mapping gap |
| Interactions audio/document/video function-result subcontent | SDK union is broader than the public schemas' text/image arrays | Source disagreement; retain current unsupported-media boundary |
| Ordinary-call signature versus thought `signature:""` followed by a delta | Item projection emits call signatures absent from public schemas; [SSE intake](../../src/protocol/google/interactions/sse.rs) rejects transient empty signatures | Separate compatibility evidence from official thought continuation; do not strip protected replay as an inventory repair |
| generateContent `responseFormat.text` | Encoder uses older `responseMimeType` / `responseJsonSchema`; new MIME enum uses `APPLICATION_JSON`, not `application/json` | New-carrier/alias conflict inventory; deprecated carriers are not proven invalid |
| Interactions schema `minLength` / `uniqueItems` | Public descriptions say these constraints are ignored; output-schema guards and explicit strict-tool rejection already exist | Do not advertise equivalence to OpenAI strict guarantees |
| generateContent `store` versus Interactions `store` | First controls logging; second controls retained interaction resources. Interactions encoder deliberately uses false | Distinct semantic owners; no implicit mapping from diagnostic capture or generic retention |
| Inline generateContent tool-result image | [item encoder](../../src/protocol/google/generate_content/items.rs) invents `displayName` and `$ref` labels absent from the public `FunctionResponseBlob`; SDK marks display name unsupported in Gemini API | Compatibility mechanism requiring authoritative review, not standard losslessness |
| Opening Interactions event omits `service_tier`; terminal reports `standard` | SSE intake treats the first report as changed state | Fixture-reproduced lifecycle gap; executable requirement retains original upstream payloads |

Google remains upstream encoding/SSE intake only. Missing native downstream/static directions, stored-resource services and hosted operations are coverage boundaries, not implicitly activated work.

## Imported asset boundaries

[The asset manifest](../../tests/upstream/assets.lock.json) records unchanged source bytes and derived framing digests. [Development](../development.md#upstream-test-sources) owns commands, extraction rules and test selection.

- Official OpenAI tool/length streams exercise argument fragments, tool identity, trailing usage and incomplete terminal truth. They remain older-model fixtures maintained by the selected current SDK release, not live behavior of every current model.
- Official Anthropic basic/tool/omitted-usage fixtures lack terminal SSE blank lines. The thin transport adapter appends only the delimiter; independent tests require the untouched inputs to fail strict EOF. No semantic payload is rewritten.
- Anthropic's `unknown_events_response.txt` includes a known block-stop before message-start, not just unknown extensions. It is a rejection input, despite the SDK's permissive accumulator.
- Vercel chunks are JSON payload lines, not raw HTTP/SSE. The adapter adds framing and derives event names from existing discriminators without inventing sequence numbers or terminal events.
- Vercel's custom-tool stream omits required `sequence_number` and is a negative grammar case. Its static response has `store:true`, outside the current stateless upstream binding; the unchanged `output` array is separately exercised as returned Responses history. No claim of retained upstream response acceptance follows.
- Google text and Interactions static fixtures are companion reference objects, not paired expected output for their streams: IDs, text, usage and signatures can differ. The original Interactions stream exposes the late-service-tier defect and remains an explicitly failing requirement.
- EventSource Parser framing extracts include only literal `onChunk` arguments from three named functions. No TypeScript, encoder, timer, random generator or generated DONE sentinel executes. Independent expected multiline values and every two-way byte split exercise MorphieCore's parser.

These assets do not close the full text/SSE gate. The checked-in pending requirements assert desired behavior and fail when explicitly executed; they are not tests that consider a known defect successful. Additional HTTP failure, interleaved multi-call, actual-loss-extension and transport-backpressure cases remain with their responsible redesign slices.
