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

JSON `0.1` is not exactly binary32 `0.100000001490116119384765625`. A small standard-library `struct`/`Decimal` check confirmed that arithmetic distinction during this review; it does not validate a Rust conversion implementation. Arbitrary exact JSON-number reports cannot silently pass through binary32 and remain unchanged. A Base64 design must select numeric authority, exact conversions and signed-zero behavior rather than assuming that wire encoding is always numerically neutral.

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
