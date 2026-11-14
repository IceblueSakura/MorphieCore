# Official SDK source guide

Checked **2026-10-27** using public GitHub release metadata and isolated release checkouts. This guide locates durable contracts, consumers and fixture candidates for OpenAI, Anthropic and Google. It does not update [source pins](../../tests/upstream/sources.lock.json), package locks or [imported assets](../../tests/upstream/assets.lock.json). No SDK tests or Provider requests were run for this review.

## Contents

- [Source selection and licenses](#source-selection-and-licenses)
- [Tool-contract release recheck (2026-11-04)](#tool-contract-release-recheck-2026-11-04)
- [Google function-argument carriers (2026-11-09)](#google-function-argument-carriers-2026-11-09)
- [How to read an SDK](#how-to-read-an-sdk)
- [OpenAI](#openai)
- [Anthropic](#anthropic)
- [Google Gen AI](#google-gen-ai)
- [Reuse boundary](#reuse-boundary)

## Source selection and licenses

These were the latest non-prerelease releases returned by GitHub on the observation date, not a continuously maintained latest-version promise. The immutable links identify the inspected bytes.

| Source | Observed release | Inspected revision | Project regression pin |
|---|---|---|---|
| OpenAI Python | [3.39.0](https://github.com/openai/openai-python/releases/tag/v3.39.0) | [`f97798b`](https://github.com/openai/openai-python/tree/f97798bc86e41f4a5d18644113ef94cf58722778) | 3.37.0 |
| OpenAI Node | [7.45.0](https://github.com/openai/openai-node/releases/tag/v7.45.0) | [`af8336b`](https://github.com/openai/openai-node/tree/af8336b8c13bd5402868cf2ca0d9c64530c3848f) | 7.43.0 |
| Anthropic Python | [2.3.0](https://github.com/anthropics/anthropic-sdk-python/releases/tag/v2.3.0) | [`202db72`](https://github.com/anthropics/anthropic-sdk-python/tree/202db72c87d7c6eaa72129c67ce67fd343a3a20c) | 2.1.0 |
| Anthropic TypeScript | [1.3.0](https://github.com/anthropics/anthropic-sdk-typescript/releases/tag/sdk-v1.3.0) | [`cb5fba4`](https://github.com/anthropics/anthropic-sdk-typescript/tree/cb5fba419808e3844fa6407e59577ab7ba346b79) | 1.1.0 |
| Google Gen AI Python | [2.33.0](https://github.com/googleapis/python-genai/releases/tag/v2.33.0) | [`92f72b3`](https://github.com/googleapis/python-genai/tree/92f72b3a236a5bc56e8e3327ab9563f77dd119c3) | Same revision |
| Google Gen AI JavaScript | [2.31.0](https://github.com/googleapis/js-genai/releases/tag/v2.31.0) | [`1748ce7`](https://github.com/googleapis/js-genai/tree/1748ce7031d2ab4f71c42e24f02a5a0fa6e43251) | Same revision |

All six root licenses are **Apache-2.0**. Anthropic retains `LICENSE-MIT` for previously MIT-published versions and applicable pre-change contributions; this does not make all new code dual-licensed. Check the actual file and notices when importing an asset. Dependency/vendor licenses were not audited.

OpenAI's intervening releases add delegation/cache controls and broaden Decision types; Node 7.44.0 explicitly notes a typing break. The latest pair adds a model identifier. Anthropic's intervening releases add threads/reports, tool-runner changes and beta-query cleanup. These are review inputs, not automatic admission of new APIs. Use [Development](../development.md#source-links-and-updates) for any baseline promotion.

## Tool-contract release recheck (2026-11-04)

GitHub's latest non-draft, non-prerelease metadata and resolved release tags identify the following current stable sources. This focused inspection reads tool/history types, not every intervening API change. The regression source/asset locks and installed SDK packages remain independent; implementation must integrate selected directional expectations through the existing asset/SDK process.

| Source | Stable release | Immutable tool-contract source |
|---|---|---|
| OpenAI Python | [3.45.0](https://github.com/openai/openai-python/releases/tag/v3.45.0) | [`9b3458a`](https://github.com/openai/openai-python/tree/9b3458a2ef563493111a0a4265d4e9e7e14fe840/src/openai/types) |
| OpenAI Node | [7.50.0](https://github.com/openai/openai-node/releases/tag/v7.50.0) | [`42563fe`](https://github.com/openai/openai-node/blob/42563fe08083ed7af6feba046063e3100fa7ef50/src/resources/responses/responses.ts) |
| Anthropic Python | [2.4.0](https://github.com/anthropics/anthropic-sdk-python/releases/tag/v2.4.0) | [`5752c13`](https://github.com/anthropics/anthropic-sdk-python/tree/5752c132041af71055584632caab5ca2325cae5e/src/anthropic/types) |
| Anthropic TypeScript | [1.4.0](https://github.com/anthropics/anthropic-sdk-typescript/releases/tag/sdk-v1.4.0) | [`ea8d7de`](https://github.com/anthropics/anthropic-sdk-typescript/blob/ea8d7de0e47614ae71044fbf7bf12041318ba37e/src/resources/messages/messages.ts) |

Both current OpenAI SDKs retain developer-only input `additional_tools`, optional-nullable input item ID, required output item ID and eight output roles.

Input function-result status remains optional-nullable; function-call input and result-output status do not acquire that nullability.

Current Anthropic named/any choice types still document exactly one call only with `disable_parallel_tool_use:true`.

These shapes agree with the corresponding earlier regression references; they do not resolve collision precedence, output-role replay or strict-schema grammar accounting.

The [tool-contract qualification](2026-10-27-client-ir-boundaries.md#tool-contract-qualification-2026-11-04) owns those consequential distinctions and source-access limits.

## Google function-argument carriers (2026-11-09)

The user selected the installed local SDKs as sufficient source evidence for this slice after official-page fetches timed out. Inspected Python **2.33.0**, locked by `tests/sdk/uv.lock`, and JavaScript **2.21.0** in `node_modules/@google/genai`; the latter is not the independent 2.31.0 regression-source pin above.

| Source | Exact distinction |
|---|---|
| Python `google/genai/types.py`, `FunctionCallingConfigMode`; JS `dist/genai.d.ts`, same enum | `ANY` constrains output to function calls; `VALIDATED` permits calls or natural language. Neither supplies an at-most-one call carrier |
| Python `FunctionCallingConfigMode.NONE` | Explicitly prohibits function calls, with the same model behavior as not passing declarations. This supplies no-call semantics without deleting the declarations or requiring argument-validation mode |
| Python `FunctionDeclaration.parameters_json_schema`; JS `FunctionDeclaration.parametersJsonSchema` | Object JSON Schema carrier, with `additionalProperties:false` in the declaration example; distinct from typed `parameters` and from text `responseJsonSchema` |
| Python `google/genai/_gaos/types/interactions/function.py`, `Function.parameters` | Function-input subset explicitly says `minLength`, `maxLength`, `minProperties`, `maxProperties`, `uniqueItems`, `contains`, `minContains`, `maxContains` are ignored and not enforced |
| Python Interactions tool-choice types; JS `ToolChoiceType` | Separate request-global `auto`, `any`, `none`, `validated` spellings; no per-function `strict` member |

Inspected file fingerprints (BLAKE3; local package paths, not new vendored assets):

| File | Digest |
|---|---|
| JS `dist/genai.d.ts` | `72e862f939d6a9412d1b4a71524a1db836d39185e487a65d7e77dd9c95c8515b` |
| Python `google/genai/types.py` | `d98838fa852ba67f4f5c9da47978fa29dc7362da08fa7a64356e9a7fe9ac8816` |
| Python `google/genai/_gaos/types/interactions/function.py` | `9eaabd74c6b0469d708fafb5440ad0fb08b1e05a94181fd354a84468a40f0ed7` |

The function-input declaration is not a promise of every text-output keyword.

Native discriminators independently exercise basic closed-object/string/numeric-enum constraints and show length failures under generateContent ANY/VALIDATED and Interactions any.

ANY also produces two calls from one declaration.

Further nested-schema controls isolate ignored boolean enums and rejection of integer enum `9007199254740993`; fractional enum `1.25` and ordinary nested objects/arrays succeed separately.

Root `$ref` object schemas also succeed natively.

The 2026-11-09 concentrated recheck separately exercises required nullable `anyOf` values and matched root-reference/inline schemas under native required modes on both APIs.

The subsequent no-call/reference repair checks root and nested references through real managed continuation, and independently confirms native none on both APIs.

The Google SDK's function parameter carrier requires an object-described schema; text-output reference sibling/cycle documentation is not authority for function-input enforcement.

The selected implementation therefore inspects only root `$defs` and single-token local references to the already-qualified subset, preserving the document and rejecting cycles, unqualified sibling/resource semantics and over-deep paths.

Existing `SchemaDocument` bounds plus `serde_json::Value::pointer` suffice; no URI fetcher, schema converter or general resolver dependency is introduced.

[Architecture](../architecture/adaptation-and-dialects.md#google-strict-function-arguments) owns the exact scope, [Development](../development.md#overall-acceptance-status) the overall acceptance status, and [Roadmap](../roadmap/unscheduled-issues.md#unscheduled-issues) the remaining limitations.

## How to read an SDK

Read **request parameters → serialized HTTP → raw response/event types → accumulator → returned history** separately. Required-nullable is not optional; an SDK default is not necessarily an HTTP default. Parsed helpers, derived convenience fields and permissive runtime construction are not protocol guarantees. Prefer official service documentation for semantics and both language SDKs for discrepancies; retain the disagreement instead of selecting whichever accepts more input.

The [protocol reset note](2026-10-23-stable-protocol-contracts.md) retains API migration evidence. The [request review](2026-10-26-standard-request-boundaries.md) and [expressiveness review](2026-10-27-client-ir-boundaries.md#protocol-expressiveness-check) retain independent counterexamples, not exhaustive support matrices.

## OpenAI

| Question | Source entry points at the inspected revisions |
|---|---|
| Request/history versus output unions | Python [types](https://github.com/openai/openai-python/tree/f97798bc86e41f4a5d18644113ef94cf58722778/src/openai/types); Node [Chat](https://github.com/openai/openai-node/blob/af8336b8c13bd5402868cf2ca0d9c64530c3848f/src/resources/chat/completions/completions.ts) and [Responses](https://github.com/openai/openai-node/blob/af8336b8c13bd5402868cf2ca0d9c64530c3848f/src/resources/responses/responses.ts). In Python, compare `*_param.py` with response models, not just shared names |
| SSE transport and closure | Python [`_streaming.py`](https://github.com/openai/openai-python/blob/f97798bc86e41f4a5d18644113ef94cf58722778/src/openai/_streaming.py); Node [`core/streaming.ts`](https://github.com/openai/openai-node/blob/af8336b8c13bd5402868cf2ca0d9c64530c3848f/src/core/streaming.ts) |
| Materialized tool/history state | Python [`lib/streaming`](https://github.com/openai/openai-python/tree/f97798bc86e41f4a5d18644113ef94cf58722778/src/openai/lib/streaming); Node [`ChatCompletionStream.ts`](https://github.com/openai/openai-node/blob/af8336b8c13bd5402868cf2ca0d9c64530c3848f/src/lib/ChatCompletionStream.ts) and [`responses/ResponseStream.ts`](https://github.com/openai/openai-node/blob/af8336b8c13bd5402868cf2ca0d9c64530c3848f/src/lib/responses/ResponseStream.ts) |
| Embedding wire versus returned values | Python [`_parsing/_embeddings.py`](https://github.com/openai/openai-python/blob/f97798bc86e41f4a5d18644113ef94cf58722778/src/openai/lib/_parsing/_embeddings.py); Node [`lib/embeddings.ts`](https://github.com/openai/openai-node/blob/af8336b8c13bd5402868cf2ca0d9c64530c3848f/src/lib/embeddings.ts) |
| Independent fixture candidates | Python [`tests/lib`](https://github.com/openai/openai-python/tree/f97798bc86e41f4a5d18644113ef94cf58722778/tests/lib), especially `test_embeddings.py` and streaming tests; Node [`tests/lib`](https://github.com/openai/openai-node/tree/af8336b8c13bd5402868cf2ca0d9c64530c3848f/tests/lib), especially `embeddings-request.test.ts` and `ChatCompletionStream.test.ts` |

Durable cautions:

- **Omitted embedding encoding is not float on the wire.** Both SDKs request Base64 by default and decode it to numeric arrays; explicitly requested Base64 remains encoded. Python's numeric response annotation does not settle the wire contract. Keep [numeric conversion evidence](2026-10-22-embedding-image-contracts.md#embeddings) separate from the convenience result.
- **Custom Chat SSE remains source-disputed.** Node declares and accumulates `custom.input/name`; Python's [chunk type](https://github.com/openai/openai-python/blob/f97798bc86e41f4a5d18644113ef94cf58722778/src/openai/types/chat/chat_completion_chunk.py) remains function-only. Node's synthetic fixtures establish an implemented consumer, not server emission; Python typing alone does not establish runtime rejection. See the [directional analysis](2026-10-27-client-ir-boundaries.md#custom-tools-source-disagreement-versus-a-real-carrier-gap).
- **Accumulation is not transparent wire capture.** Parsed arguments and derived text belong to helper results. Compare raw events with final objects independently; item completion and response completion are distinct.
- Public OpenAI defaults to `/v1`; Azure version selection is a separate client contract. Both SDKs default to two retries. Python explicit/context-managed close and Node early-iterator abort are useful cancellation references, not evidence that a gateway may replay after publication.

## Anthropic

| Question | Source entry points at the inspected revisions |
|---|---|
| Stable request/output/event unions | Python [`types`](https://github.com/anthropics/anthropic-sdk-python/tree/202db72c87d7c6eaa72129c67ce67fd343a3a20c/src/anthropic/types): `message_create_params.py`, `message_param.py`, `content_block_param.py`, `content_block.py`, `raw_message_stream_event.py`; TypeScript [`messages.ts`](https://github.com/anthropics/anthropic-sdk-typescript/blob/cb5fba419808e3844fa6407e59577ab7ba346b79/src/resources/messages/messages.ts). Keep `types/beta` separate |
| Transport-event filtering | Python [`_streaming.py`](https://github.com/anthropics/anthropic-sdk-python/blob/202db72c87d7c6eaa72129c67ce67fd343a3a20c/src/anthropic/_streaming.py); TypeScript [`core/streaming.ts`](https://github.com/anthropics/anthropic-sdk-typescript/blob/cb5fba419808e3844fa6407e59577ab7ba346b79/src/core/streaming.ts) |
| Tool JSON, signature and usage accumulation | Python [`_messages.py`](https://github.com/anthropics/anthropic-sdk-python/blob/202db72c87d7c6eaa72129c67ce67fd343a3a20c/src/anthropic/lib/streaming/_messages.py); TypeScript [`MessageStream.ts`](https://github.com/anthropics/anthropic-sdk-typescript/blob/cb5fba419808e3844fa6407e59577ab7ba346b79/src/lib/MessageStream.ts) |
| Literal fixtures and consumers | Python [`tests/lib/streaming`](https://github.com/anthropics/anthropic-sdk-python/tree/202db72c87d7c6eaa72129c67ce67fd343a3a20c/tests/lib/streaming), including `fixtures/tool_use_response.txt`; TypeScript [`fixtures`](https://github.com/anthropics/anthropic-sdk-typescript/tree/cb5fba419808e3844fa6407e59577ab7ba346b79/tests/lib/fixtures) and [`MessageStream.test.ts`](https://github.com/anthropics/anthropic-sdk-typescript/blob/cb5fba419808e3844fa6407e59577ab7ba346b79/tests/api-resources/MessageStream.test.ts) |

Durable cautions:

- `messages.create(stream=True)` yields SDK-filtered message events, not raw SSE bytes: pings/unnamed events are omitted and error events raise. `raw_events`/`rawEvents` expose a different layer.
- Accumulators parse partial tool JSON, replace signatures, enrich stop events and update cumulative usage. A parsed partial object does not prove complete input; `incomplete_partial_json_response.txt` ends with `max_tokens` and `message_stop` despite unfinished arguments. SDK assignment behavior does not authorize repeated native signature deltas.
- Both SDKs default to `anthropic-version: 2026-10-21` and two retries. Explicit/context-managed close and iterator abort must be tested separately from successful accumulation. Stable versus beta feature selection is not inferred from a shared type name or a tool runner.

## Google Gen AI

| Question | Source entry points at the inspected revisions |
|---|---|
| Public types versus backend wire | Python [`types.py`](https://github.com/googleapis/python-genai/blob/92f72b3a236a5bc56e8e3327ab9563f77dd119c3/google/genai/types.py), [`_transformers.py`](https://github.com/googleapis/python-genai/blob/92f72b3a236a5bc56e8e3327ab9563f77dd119c3/google/genai/_transformers.py) and [`models.py`](https://github.com/googleapis/python-genai/blob/92f72b3a236a5bc56e8e3327ab9563f77dd119c3/google/genai/models.py); JavaScript [`types.ts`](https://github.com/googleapis/js-genai/blob/1748ce7031d2ab4f71c42e24f02a5a0fa6e43251/src/types.ts) and [`_models_converters.ts`](https://github.com/googleapis/js-genai/blob/1748ce7031d2ab4f71c42e24f02a5a0fa6e43251/src/converters/_models_converters.ts) |
| API version, retry and stream lifetime | Python [`_api_client.py`](https://github.com/googleapis/python-genai/blob/92f72b3a236a5bc56e8e3327ab9563f77dd119c3/google/genai/_api_client.py); JavaScript [`_api_client.ts`](https://github.com/googleapis/js-genai/blob/1748ce7031d2ab4f71c42e24f02a5a0fa6e43251/src/_api_client.ts) |
| Automatic tools and continuation | Python `models.py`; JavaScript [`models.ts`](https://github.com/googleapis/js-genai/blob/1748ce7031d2ab4f71c42e24f02a5a0fa6e43251/src/models.ts) and [`_afc.ts`](https://github.com/googleapis/js-genai/blob/1748ce7031d2ab4f71c42e24f02a5a0fa6e43251/src/_afc.ts) |
| Version-specific Interactions | Python [`_gaos`](https://github.com/googleapis/python-genai/tree/92f72b3a236a5bc56e8e3327ab9563f77dd119c3/google/genai/_gaos), plus the distinct OpenAPI artifacts in the project source lock. Do not substitute generateContent or Vertex types |
| Literal stream/test candidates | Python [`test_async_stream.py`](https://github.com/googleapis/python-genai/blob/92f72b3a236a5bc56e8e3327ab9563f77dd119c3/google/genai/tests/client/test_async_stream.py) and [`tests/models`](https://github.com/googleapis/python-genai/tree/92f72b3a236a5bc56e8e3327ab9563f77dd119c3/google/genai/tests/models); JavaScript [`api_client_test.ts`](https://github.com/googleapis/js-genai/blob/1748ce7031d2ab4f71c42e24f02a5a0fa6e43251/test/unit/api_client_test.ts) |

Durable cautions:

- Shorthand inputs are grouped into inferred user/model content. Backend converters move fields and reject unsupported combinations; a common SDK type does not prove identical Developer API/Vertex wire support. Compare [Discovery/protobuf/SDK evidence](2026-10-22-embedding-image-contracts.md#google-wire-source-reconciliation) when they disagree.
- Function callables can enable automatic function calling, normally bounded to ten calls; automatic continuation is another path. Disabling AFC alone does not establish one HTTP request. Inspect serialized requests and count transmissions with both helpers controlled.
- Defaults are Developer API `v1beta` and Vertex `v1beta1`, with explicit overrides. Ordinary retries are off without retry options; an empty retry-options object selects five total attempts. Python's aiohttp transport additionally has a one-off retry for selected transport failures. These implementation details must be rechecked when constraining a test run.
- Python async cleanup closes/releases the response; the inspected JavaScript stream cleanup releases the reader lock. The latter is not equivalent evidence of upstream cancellation. Neither a generated result nor a continuation-hop sum establishes [ordinary SSE score aggregation](2026-10-17-generation-contracts.md#streamed-score-scope-2026-10-20).

## Reuse boundary

Use source tests to discover independent wire expectations and failure cases, then select a small licensed subset through the existing [asset process](../development.md#upstream-test-sources). Keep official contract, SDK consumer behavior, local synthetic regression, installed-Agent acceptance and live Provider evidence distinct. Do not copy automatic retries, tool execution, coercion or successful termination into the gateway merely because a helper performs them.
