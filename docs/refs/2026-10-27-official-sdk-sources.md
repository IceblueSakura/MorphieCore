# Official SDK source guide

Checked **2026-10-27** using public GitHub release metadata and isolated release checkouts. This guide locates durable contracts, consumers and fixture candidates for OpenAI, Anthropic and Google. It does not update [source pins](../../tests/upstream/sources.lock.json), package locks or [imported assets](../../tests/upstream/assets.lock.json). No SDK tests or Provider requests were run for this review.

## Contents

- [Source selection and licenses](#source-selection-and-licenses)
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
