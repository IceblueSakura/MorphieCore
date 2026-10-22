# Embedding and Image contracts

Sources checked on 2026-10-22 for the prioritized Embedding and Images design. Official web pages are mutable and disagree in places. The existing OpenAI Python research pin is `547ecb1e4fc233f19030736c29fee4374dfbaca1` (3.35.1, Apache-2.0); reusing it does not claim a new latest-version check. No SDK, gateway or Provider acceptance was run for these observations.

[API expansion design](../api-expansion-design.md#operation-contracts) owns proposals; [Architecture](../architecture.md#independent-task-boundaries) owns accepted boundaries; [Roadmap](../roadmap.md#stage-4-scope) owns priority and gates. This note records evidence, not a second implementation queue.

## Embeddings

### Official and pinned sources

- [Create embeddings](https://developers.openai.com/api/reference/resources/embeddings/methods/create)
- [Embedding guide](https://developers.openai.com/api/docs/guides/embeddings)
- Pinned Python [resource](https://github.com/openai/openai-python/blob/547ecb1e4fc233f19030736c29fee4374dfbaca1/src/openai/resources/embeddings.py) and [response parser](https://github.com/openai/openai-python/blob/547ecb1e4fc233f19030736c29fee4374dfbaca1/src/openai/lib/_parsing/_embeddings.py)

The API admits scalar text, a text batch, one integer-token sequence or a batch of integer-token sequences. It documents nonempty text, a per-input model token limit (currently described as 8,192), and 300,000 aggregate tokens per request. Its additional “any array must be 2048 dimensions or less” wording does not clearly distinguish batch size from token-sequence length; it is not a sufficient independent token-array-limit oracle.

`dimensions` is a positive optional control supported by third-generation and later models, not a universal option. The guide identifies 1,536/3,072 default dimensions for `text-embedding-3-small`/`large`, and `cl100k_base` for third-generation inputs. Model-specific tokenization compatibility is distinct from equal vector dimensions.

`encoding_format` admits float or Base64; raw API omission means float. Each response vector has an independent input index; the envelope reports model, `prompt_tokens` and `total_tokens`.

Both pinned Python sync and async resources replace caller-omitted encoding with `base64`. The parser converts strings to numeric lists only when the caller omitted encoding, using `array.array("f", ...)` or NumPy `float32`. Explicit float/Base64 returns its selected representation unchanged. This is direct source evidence for SDK-default wire requests, not a successful SDK loop.

The parser establishes binary32 practice but uses native-endian numeric interpretation; it does not independently specify portable wire byte order. Pinning byte order needs explicit profile evidence and independent byte fixtures before implementation.

### Current code and design counterexamples

- [Semantic input](../../src/semantic/task/embedding.rs) is bounded `Vec<String>`; exact `serde_json::Number` values are authoritative. Local limits are 128 inputs, 256 KiB text, 8,192 dimensions and 60,000 aggregate vector values.
- [Codec](../../src/protocol/openai/embeddings.rs) and [adapter](../../src/adapter/embeddings.rs) accept text/float only. A pinned SDK call omitting encoding therefore encounters a known admission gap.
- [Target admission](../../src/lowering/embeddings.rs) checks aggregate vector values only for explicit dimensions, with no default-dimension contract. At the [catalog's](../../src/topology/catalog/embeddings.rs) 1,536-dimensional model, 40 inputs imply 61,440 values: omitted dimensions pass that preflight while explicit 1,536 rejects. Both describe the same output size.
- Existing result validation preserves order and unique reported indexes and validates count/dimensions against the request. Extend it with admitted defaults rather than replacing indexes with array positions.
- Standard response parsing uses the shared closed-object helper, rejecting unknown fields. This differs from the intended strict-known/ignore-unknown upstream policy; tolerant unknown parsing must not weaken known types, duplicate-key rejection or bounds.

JSON `0.1` is not exactly binary32 `0.100000001490116119384765625`. A small standard-library `struct`/`Decimal` check confirmed that arithmetic distinction during this review; it does not validate a Rust conversion implementation. Arbitrary exact JSON-number reports cannot pass through binary32 and remain unchanged. The later user selection permits standard rounding on a delivery copy under the [named encoding policy](../architecture.md#embedding-encoding-compatibility), superseding the earlier exact-only recommendation; float→float source precision remains unchanged.

### Google Embedding selection

Official sources rechecked on 2026-10-22 (Google documentation CC-BY-4.0; samples Apache-2.0):

- [Stable Embedding 2 model](https://ai.google.dev/gemini-api/docs/models/gemini-embedding-2) and [Embedding 001 model](https://ai.google.dev/gemini-api/docs/models/gemini-embedding-001), pages dated 2026-04-28
- [Embedding guide](https://ai.google.dev/gemini-api/docs/embeddings), page dated 2026-09-04
- [Native Embeddings REST](https://ai.google.dev/api/embeddings), page dated 2026-08-28
- [OpenAI compatibility](https://ai.google.dev/gemini-api/docs/openai), page dated 2026-10-13

Both models are stable, default to 3,072 dimensions and document a 128–3,072 range. Embedding 2 has an 8,192-token input limit versus 2,048 for 001, supports multimodal inputs and automatically normalizes reduced-dimensional output. The selected initial profile is text-only. The guide says the two embedding spaces are incompatible and migration requires re-embedding; no cross-model failover is justified.

Embedding 2 does not accept `task_type`; text task instructions are caller-authored prefixes rather than a native task control. Do not inject those prefixes silently. For multiple inputs, separate `Content` objects produce separate vectors; multiple parts in one content can produce an aggregate vector. Native synchronous `batchEmbedContents` takes one `EmbedContentRequest` per input and guarantees response vectors in request order. This is distinct from `asyncBatchEmbedContent` and the general Batch API.

Native `ContentEmbedding.values` is a numeric array; no Base64 output control or integer-token input is documented. The current native schema places `outputDimensionality` and other controls under `embedContentConfig`, retaining deprecated top-level controls and older examples. It also exposes `autoTruncate` without documenting its default or an embedding-result truncation flag. Model cards' dimension minimum disagrees with older API examples requesting 10 dimensions; profile limits must follow selected model evidence rather than copy those examples.

The current REST response schema includes `usageMetadata` with `promptTokenCount` and modality details, but no total-token counter or reported serving-model identity. Presence in a schema is not a guarantee that every model response contains usage. These facts need independent semantic/result and downstream projections; filling a required “reported model” from the request or absent counts with zero would invent source facts.

Google's OpenAI-compatible endpoint is documented at `/v1beta/openai/embeddings`, but its embedding examples still name `gemini-embedding-2-preview` while the model/guide sources name stable `gemini-embedding-2`. The inspected compatibility section does not establish complete token-input, dimensions, encoding or usage behavior. Its examples using a default OpenAI SDK request do not prove that its response is Base64 or that the stable slug has identical support. Those contracts remain unverified, not proven unsupported.

Selection therefore favors the native synchronous contract for design, not an assertion that it works with current credentials. No Google model enumeration, embedding inference, SDK or Provider acceptance was performed.

### Google wire source reconciliation

A deeper same-day recheck compared current REST/Discovery against versioned implementations. Public-source downloads only; no credentials or inference. Google documentation is CC-BY-4.0 and the SDK/protobuf sources below are Apache-2.0.

| Source | Pinned observation |
|---|---|
| [Developer v1beta Discovery](https://generativelanguage.googleapis.com/$discovery/rest?version=v1beta) | Revision `20261020`, downloaded 2026-10-22; SHA-256 `5f91d2b54b79def5f0c5d81205411c5fad922401123f89ca53392bbf20349b31`. This fingerprint identifies the observed bytes, not an immutable URL |
| [Python converter](https://github.com/googleapis/python-genai/blob/c97ae02ff87b5138bee29100b56c04c649233fed/google/genai/models.py#L749-L796) | Revision `c97ae02ff87b5138bee29100b56c04c649233fed`; Developer API writes legacy per-entry dimensions and raises for `auto_truncate` |
| [Python response converter](https://github.com/googleapis/python-genai/blob/c97ae02ff87b5138bee29100b56c04c649233fed/google/genai/models.py#L1009-L1030) | Copies `embeddings`, `metadata` and SDK HTTP metadata, but not `usageMetadata` |
| [JS request converter](https://github.com/googleapis/js-genai/blob/38f6ebcda2c39edc588f4274f4abdd2d41e31e9c/src/converters/_models_converters.ts#L886-L945) and [response converter](https://github.com/googleapis/js-genai/blob/38f6ebcda2c39edc588f4274f4abdd2d41e31e9c/src/converters/_models_converters.ts#L1231-L1260) | Same legacy-control, Developer API truncation rejection and missing usage-conversion boundaries |
| [Public v1beta protobuf](https://github.com/googleapis/googleapis/blob/bc7e3baa28fbb223fa93782e130260fab8205bfc/google/ai/generativelanguage/v1beta/generative_service.proto#L1232-L1309) | Revision `bc7e3baa28fbb223fa93782e130260fab8205bfc` predates the newer nested config/usage fields; absence here does not refute the newer Discovery/REST contract |

REST and Discovery agree on the following candidate native baseline:

```json
{
  "requests": [
    {
      "model": "models/gemini-embedding-2",
      "content": {"parts": [{"text": "synthetic first input"}]},
      "embedContentConfig": {"outputDimensionality": 768, "autoTruncate": false}
    },
    {
      "model": "models/gemini-embedding-2",
      "content": {"parts": [{"text": "synthetic second input"}]},
      "embedContentConfig": {"outputDimensionality": 768, "autoTruncate": false}
    }
  ]
}
```

This is a source-backed request example, not an executed fixture. The fixed path is `/v1beta/models/gemini-embedding-2:batchEmbedContents`, with `x-goog-api-key` auth. Each entry's model must match the path; config is per entry, not batch-wide. Do not send old and nested controls together: precedence is undocumented. Single `embedContent` uses singular `embedding`, while this synchronous batch returns plural `embeddings` in input order. The gateway can use one-entry batches without a separate scalar executor.

The deeper schema review adds these distinctions:

- `ContentEmbedding.values` is an array of `format:float`; `shape` is also a known field for soft-token tensors. Dense text support is not tensor/soft-token support, and a shape-only response cannot pass as an empty vector.
- `EmbeddingUsageMetadata` contains optional `promptTokenCount` and `promptTokenDetails`, both at request/batch scope. Each detail has modality/token count. No total, serving-model/version or truncation-result field is documented. Omission does not assert zero; do not infer aggregate equality from the detail array.
- No synchronous `requests.maxItems` is declared in the inspected current schemas. The [older first-party SDK's 100-item limit](https://github.com/google-gemini/generative-ai-python/blob/7a7cc5474ddaa0255a4410e05361028a24400abd/google/generativeai/embedding.py#L32) is useful conservative precedent, not model-specific acceptance.
- Both inspected SDKs use synchronous `batchEmbedContents` for Developer API embedding, but their converted response can hide newer reports. “SDK usage missing” cannot establish “wire usage missing.”
- `autoTruncate` has no documented Developer API default or explicit false→error guarantee. The guide's silent-truncation warning is in PDF handling and does not settle the text-only case.
- [Vertex/Agent Platform embedContent](https://docs.cloud.google.com/gemini-enterprise-agent-platform/reference/rest/v1/projects.locations.publishers.models/embedContent) has different resource paths and reporting, including a truncation report and broader usage. Its `promptTokensDetails` spelling, defaults and SDK mode are not the selected Developer API contract.

The implementation baseline should therefore use direct native HTTP parsing, pin current nested controls and keep activation behind a bounded no-truncation/configuration check. SDK lag is not permission to silently omit a requested protection or treat an old alias as equivalent.

### Compatibility and existing-target evidence

The public [agentgateway issue #2451](https://github.com/agentgateway/agentgateway/issues/2451), opened 2026-07-07, reports successful Google OpenAI-compatible **001** responses without `usage`; [fix #2455](https://github.com/agentgateway/agentgateway/pull/2455) makes that parsing optional. This is useful integration evidence, not an observation of stable Embedding 2. Official compatibility cookbook vector access likewise does not validate every required OpenAI response field.

The subsequent user selection accepts [named Google ancillary projection](../architecture.md#google-embedding-compatibility), not fabricated zero accounting. A source input count can justify an explicitly derived input-only total; a missing count cannot. Bound model labeling is execution provenance, not an upstream model report. Unsupported `user` omission is a recorded loss, not an invented local attribution service.

Two current-model source checks constrain the shared work:

- [DashScope synchronous API](https://help.aliyun.com/en/model-studio/text-embedding-synchronous-api), rechecked 2026-10-22, documents default 1,024 dimensions, 20 text inputs and **float-only** output for its compatible interface. The model table and generic dimension prose differ in model-specific allowances; retain the catalog-selected widths until a scoped recheck. This makes float→Base64 gateway conversion essential, not an optional Google enhancement.
- [OpenRouter Embeddings](https://openrouter.ai/docs/api/api-reference/embeddings/submit-an-embedding-request), rechecked 2026-10-22, admits text/token alternatives and both float/Base64 in the request schema. That is codec evidence, not every target's tokenizer/encoding acceptance. Preserve reported indexes and current accounting; do not import `input_type`, routing preferences or other nonstandard controls into the selected downstream contract implicitly.

## Images

### Official and pinned sources

- OpenAI [Generate](https://developers.openai.com/api/reference/resources/images/methods/generate), [Edit](https://developers.openai.com/api/reference/resources/images/methods/edit) and [guide](https://developers.openai.com/api/docs/guides/image-generation.md)
- OpenAI [Generate events](https://developers.openai.com/api/reference/resources/images/generation-streaming-events) and [Edit events](https://developers.openai.com/api/reference/resources/images/edit-streaming-events)
- Pinned Python [Edit parameters](https://github.com/openai/openai-python/blob/547ecb1e4fc233f19030736c29fee4374dfbaca1/src/openai/types/image_edit_params.py) and [partial-image event](https://github.com/openai/openai-python/blob/547ecb1e4fc233f19030736c29fee4374dfbaca1/src/openai/types/image_gen_partial_image_event.py)
- Grok [generation](https://docs.x.ai/developers/model-capabilities/images/generation.md), [editing](https://docs.x.ai/developers/model-capabilities/images/editing.md), and the earlier [Imagine source review](2026-10-22-grok-imagine.md)

### OpenAI observations and disagreements

Generate accepts JSON. Edit's current reference describes JSON `images` entries with URL/data-URL or File IDs; its examples and pinned SDK also use multipart `image`/`image[]`. The reference describes a mask reference with exactly one source form. These are distinct wire carriers for one edit intent, not permission to accept unscoped upstream File IDs.

The documented maximum is 16 source images; pinned SDK input types describe PNG/WebP/JPEG below 50 MB each. Source order matters: a mask applies to the first input. The guide explicitly says masking is prompt-based guidance and may not follow the exact shape.

Mask limits conflict: pinned SDK says valid PNG below 4 MB and matching image dimensions; guide says same format/size below 50 MB with alpha. Do not merge those into a claim of verified 50-MB arbitrary-format masks. A conservative local subset and its source-format/byte units must be selected explicitly.

Defaults include one output, static delivery, PNG, automatic size/quality/background and JPEG/WebP compression 100. Transparent backgrounds require PNG/WebP. `response_format` is not a GPT Image format selector: GPT Image returns Base64, while output encoding uses `output_format`.

The guide and Edit/pinned SDK sources cover newer 2.5 models and xhigh/max quality while the inspected Generate reference contains older model lists and narrower enums. Their union is not a universal model profile. Current guide constraints for modern custom sizes include multiples of 16, aspect at most 3:1, edge at most 3,840 and 655,360–8,294,400 pixels; model/profile evidence must select where those apply.

`input_fidelity` is high/low on documented supported models, low-only on mini, and must be omitted on `gpt-image-2`, which the guide describes as automatically high. Those statements do not establish every newer model's effective support or authorize a universal omission rule.

Static `ImagesResponse` requires image creation time `created`; optional top-level reports describe format, dimensions, background, quality and usage. It has no standard serving-model field. Optional usage shape is separate from whether a concrete model reports it.

Generate and Edit SSE have separate event names, each with `partial_image` and `completed`. The full schemas carry Base64, event `created_at`, output format, size, quality and background; settings can remain Auto. Completed carries usage. Some endpoint examples omit fields required by the full event schemas; shorthand examples are not an independent relaxation oracle.

`partial_images` is optional/nullable, 0–3, and fewer previews may arrive before the final. The inspected parameter descriptions establish zero's final-only behavior but not an omission default or null normalization. The events expose `partial_image_index`, not a final-output index. These sources do not resolve preview-to-final association for multiple outputs. Event time also does not assert static image creation time.

The pinned Edit parameters and current completed-event schema include safety-identifier semantics not admitted by the current local image profile. Effective reported identity must not be inferred from a request or placed in image output controls.

### Grok and current code boundaries

Grok's guide warns that its JSON Edit contract differs from the OpenAI SDK's multipart call. Its URL/data-URL inputs can be passed to its native carrier without a gateway downloader. No equivalent OpenAI mask/input-fidelity or progressive preview grammar is established by the inspected Grok pages.

Grok's generation guide returns temporary URLs by default and admits explicit Base64. This chooses transport representation, not a guaranteed PNG encoding. Native aspect ratio/resolution tiers are not exact pixels. The guide describes automatic quality as currently low for generation and medium for editing; keep explicit Auto delegated rather than freezing that behavior. Creation time is not required in the inspected native result; model/moderation reports and sparse native accounting need their own carriers. The [earlier note](2026-10-22-grok-imagine.md#images) retains REST/guide discrepancies.

The [current image task](../../src/semantic/task/image_generation.rs) is static Generate with bounded Base64 artifacts and an 8-MiB aggregate decoded final budget. Edit inputs, masks and stream events do not exist there. Optional creation/model/usage facts do exist; they must not be synthesized at delivery.

The current [prepared upstream request](../../src/execution/attempt.rs) owns materialized bytes; [image preparation](../../src/execution/images.rs) builds JSON. There is no existing image multipart/spool ownership contract to inherit. Architecture's [inert-media policy](../architecture.md#media) also leaves actual image validity and pixel dimensions upstream; local mask inspection would require a separately selected exception, not merely a new upload parser.

The [standard encoder](../../src/protocol/openai/images.rs) requires `created`, forbids a reported model, requires compatible top-level artifact reports and rejects streaming. The [Grok codec](../../src/protocol/grok/images.rs) can honestly preserve absent creation time/model/native accounting, but no public binding is active. Existing [accounting projection](../../src/lowering/images.rs) does not fix missing timestamps, serving-model carriage or default-PNG incompatibility. Adding a binding alone therefore cannot establish faithful OpenAI delivery.

The static image topology currently assumes an API key and one operation. Those restrictions require selected integration changes, not duplicate public model identities or weakened trusted-Provider checks. Video deferral, Files authorization and stage-5 hosted image tools are unchanged.
