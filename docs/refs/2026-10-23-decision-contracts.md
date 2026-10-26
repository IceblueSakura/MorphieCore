# Decision model contracts

Checked 2026-10-23. Official documentation, model schemas and SDK source describe dedicated typed decision APIs, not ordinary reasoning or JSON-mode Generation. This note records source evidence and conflicts, not design approval or runtime support. [Architecture](../architecture.md#decision-api-direction) owns the selected dual-SDK direction; [API expansion design](../api-expansion-design.md#decisions) owns proposals; [Roadmap](../roadmap.md#decision-api-closure) owns gates.

No inference calls, credentials, SDK executions or implementation changes were used for this research. SDK links below point to mutable branches/documentation, not pinned acceptance releases. Search snippets and extracted HTML sometimes lagged directly retrieved Markdown/schema content; conflicts are retained rather than treated as live behavior.

## Source index

| Family | Primary sources inspected | Evidence boundary |
|---|---|---|
| OpenAI | [Decisions guide](https://developers.openai.com/api/docs/guides/decisions), [create reference](https://developers.openai.com/api/reference/resources/decisions/methods/create), [Node resource](https://github.com/openai/openai-node/blob/master/src/resources/decisions.ts), [Python request](https://github.com/openai/openai-python/blob/main/src/openai/types/decision_create_params.py) | Guide/reference/SDK disagree on some newer input and cache fields; no pinned release or serving-profile test |
| TypeSafe | [System One schema](https://docs.typesafe.ai/api.md), [models](https://docs.typesafe.ai/models.md), [confidence](https://docs.typesafe.ai/confidence.md), [structured questions](https://docs.typesafe.ai/primitives/advanced.md), [images](https://docs.typesafe.ai/images.md), [Box](https://docs.typesafe.ai/primitives/box.md) | Current schema is broader than older HTML and some narrative pages |
| TypeSafe SDK | [SDK index](https://docs.typesafe.ai/sdk), [Python client](https://docs.typesafe.ai/sdk/python/api/clients/sync), [advanced usage](https://docs.typesafe.ai/sdk/python/usage/advanced), [JavaScript SDK](https://docs.typesafe.ai/sdk/javascript), [model-list schema](https://docs.typesafe.ai/api/list-models.md) | Configuration, compression, typed consumption and companion API evidence; actual URL joining and version coverage remain untested |
| Perplexity | [direct quickstart](https://docs.perplexity.ai/docs/decisions/quickstart.md), [OpenAPI reference](https://docs.perplexity.ai/api-reference/decisions-post.md), [Node schema/resource](https://github.com/perplexityai/perplexity-node/blob/main/src/generated/api.ts), [Python resource](https://github.com/perplexityai/perplexity-py/blob/main/src/perplexity/resources/decisions.py) | Direct Markdown differs from cached HTML on models, confidence and question grouping |
| Cloudflare | [Clef](https://developers.cloudflare.com/workers-ai/models/clef/), [Flash Markdown](https://developers.cloudflare.com/workers-ai/models/clef-flash/index.md), [Omni](https://developers.cloudflare.com/workers-ai/models/clef-omni/), [input schema](https://developers.cloudflare.com/workers-ai/models/clef/schema-input.json), [output schema](https://developers.cloudflare.com/workers-ai/models/clef/schema-output.json), [Omni schema](https://developers.cloudflare.com/workers-ai/models/clef-omni/schema-input.json) | Hosted schemas, platform transport and self-hosted reference behavior are distinct |
| Cloudflare transport/reference | [Run API](https://developers.cloudflare.com/api/resources/ai/methods/run/), [Workers binding](https://developers.cloudflare.com/workers-ai/configuration/bindings/), [TypeScript SDK](https://github.com/cloudflare/cloudflare-typescript/blob/main/src/resources/ai/ai.ts), [reference model code](https://huggingface.co/Cloudflare/clef/blob/main/joint_schema_model.py), [platform errors](https://developers.cloudflare.com/workers-ai/platform/errors/) | SDK unwraps REST `result`; reference confidence algorithm does not attest hosted internals |

## Wire families

| Service | Native operation | Required request owners | Answer association |
|---|---|---|---|
| OpenAI | `POST https://api.openai.com/v1/decisions` | `model`, `input`, `questions[]` | Ordered answers, optional/nullable names |
| TypeSafe | `POST https://api.typesafe.ai/v1/systemone` | `model`, `state`, named `questions` | Answers keyed by question name |
| Perplexity | `POST https://api.perplexity.ai/v1/decisions` | `model`, `state`, named `questions` | Answers keyed by question name |
| Cloudflare | `POST https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/run/{model}` | Model-specific body includes `model`, `state`, named `questions` | Model result uses named answers; REST envelope differs from Workers binding |

All four REST paths use Bearer authentication. That does not make credential scopes interchangeable. Cloudflare's account path and path/body model selectors are trusted binding data. Perplexity's endpoint name is not evidence of OpenAI Decisions compatibility. Third-party `/v1/decide` sites are not TypeSafe protocol authority.

The three shared question meanings are a true/yes probability (`predicate` in OpenAI, `noul` elsewhere), a categorical distribution (`choice`), and an ordinal distribution with expected zero-based index (`score`). These are not free-form text or application action execution. Multi-question evaluation is not a batch of independent states: an array-valued `state` remains one structured evidence value.

## OpenAI

The guide documents `gpt-6-luna` and 1–400 ordered questions. Choice options are 2–255 unique string or Boolean values with optional descriptions. Score levels have labels and optional descriptions; score is the probability-weighted level index. The inspected sources did not establish a definitive score-level count limit.

Input may be a string or ordered evidence items. User messages admit text, images and JSON; developer/system messages admit text; assistant messages and function/custom-tool calls/outputs provide history evidence. Decisions does not execute those tools. The guide excludes files, audio and item references.

SDK schemas expose `input_json` with object or array-of-objects data, bounded to 32 container levels, 10,000 values and 256 KiB compact UTF-8 JSON. Nested `role` or `image_url` keys are ordinary data. Newer question instructions accept content parts, including JSON in the SDK; older reference material shows string-only instructions. Images may use data URLs or public HTTP(S) URLs. A reference limit of 128 images needs rechecking against newer question-level media support.

Results include ordered `predicate`, `choice`, `score` or per-question `refusal` answers. Choice returns the selected typed value, full distribution and confidence. Score returns numeric indices/labels, distribution, expected score and confidence. Refusal does not imply that every other question failed. No ordinary Generation finish reason, continuation token or tool execution is implied.

Usage includes input/output/total counts and cache/reasoning details in the inspected schema. Output-token charging and token-count reporting are distinct. Cache controls and `safety_identifier` are separate request controls; an opaque safety identifier is not authenticated caller identity. The guide describes shared-input caching, while inspected SDK TTL support and announcement retention claims differ.

No standalone Decisions `stream` parameter or SSE grammar was found. SDK lazy-body `with_streaming_response` wrappers are not model-event streaming. A separate voice integration is not authority for this endpoint.

## TypeSafe

The models page lists `jev-latest` → `jev-1.13.0` and `jev-preview` → `jev-1.14.0`. Responses are documented to report the answering version rather than merely echoing an alias. Alias movement can change threshold behavior. Default context budgets are 64k for state plus all questions, and 32k for state plus any one question. The page describes no separate question-count ceiling. An explicit `TypeSafe-Danger-Zone-Long-Context: true` header raises both budgets to 262k; this is not a default or a gateway byte-based admission formula.

Current OpenAPI admits string/object/array state. Instructions and Choice/Noul criteria can be structured or null. It admits 1–255 Choice entries and 1–10 Score entries; Score entries exclude null in this schema. Older HTML requires instructions and at least two Score levels; the advanced-structure page also differs on nullable Score entries. Resolve these baseline conflicts before admitting edge cases.

Questions are documented as evaluated independently against shared state. Results contain `model`, named `answers`, and input/output token usage. Choice has an option map, selected name and confidence. Score has a string-indexed distribution, expected score, original rubric `legend` and confidence. Noul is a probability and has no separately returned confidence.

### Images and Box

Images use an `assets` map and opaque placeholders replaced wherever they occur as or inside strings in state, instructions or criteria. JSON bodies carry image format/Base64; multipart places the JSON `request` first and each image in a uniquely named part referenced by exactly one asset. Reuse is through the placeholder, not by assigning one part to multiple assets. These are reference-expansion semantics, not arbitrary JSON passthrough.

Images/Box require `TypeSafe-Danger-Zone-Images: true`. Image guidance describes PNG/JPEG and explicit size/expansion limits; image support is newer than the Models page's text-only table. The service removes metadata without applying EXIF orientation, making implicit gateway image conversion consequential.

Box needs Jev 1.14 and returns normalized top/bottom/left/right edges, with nullable confidence in the schema. It has no `not_found` result: even an absent target receives a box. The official recommendation is a separate Noul existence question. A Box answer therefore cannot be interpreted as successful object detection without additional evidence.

### SDK and discovery boundaries

The Python SDK accepts an API-root `base_url`, explicit retry policy and compression configuration. It documents zstd request compression for bodies of at least 1 KiB when available. Ordinary JSON-only probes will miss this behavior. Decompression needs a separate expanded-size budget; disabling compression is a client restriction, not proof of default compatibility. These Python facts do not establish identical JavaScript defaults.

The Python SDK documents warning and skipping unknown answer kinds, and ignoring unknown response fields. A returned response object can therefore conceal a missing typed answer. Acceptance must compare requested question identities with the typed answers actually consumed.

TypeSafe model discovery uses `GET /v1/models` with `models:[{name,description,release_date}]`; all three entry fields are required. OpenAI uses the same suffix with `object:"list",data:[...]`, already implemented by MorphieCore's [model view](../../src/gateway/ingress/models.rs). A shared root cannot serve both contracts without an explicit discriminator. The [namespace proposal](../api-expansion-design.md#sdk-surfaces-and-discovery) avoids SDK-specific header sniffing.

Documented errors include validation `422`, rate limiting `429` and overload `529`; image/multipart paths also describe `400` and `413`. SDK default retries and body-inclusive debug logging require care in acceptance: disable retries for bounded Provider runs and do not enable business-payload logging merely to inspect SDK behavior.

## Perplexity

The direct quickstart lists `pplx-decider-v1.2` and `pplx-decider-flash-v1.2`, with 256K context and 1–128 named questions. Choice admits 1–255 options and Score admits 1–10 levels. Instructions and some criteria are optional/structured/null; prose conditions, such as Noul needing instructions or criteria, are not fully expressed by schema `required` arrays.

The response `model` echoes the requested name. Retired version-looking names may route to replacements; neither that field nor a version-shaped alias proves an immutable serving revision. Usage reports input/output counts, without the OpenAI cache-detail shape.

The direct quickstart distinguishes execution:

- v1.2 can evaluate multiple questions together; questions can influence one another. Overflow uses consecutive groups, processed sequentially, with state billed per group.
- Flash v1.2 evaluates multiple questions independently and bills state again for each question.

Older extracted HTML described grouping differently. Confirm the selected model's behavior before activation. Gateway splitting/merging can change both meaning and cost, even when answer keys remain unchanged.

Images may appear within structured state using image-URL marker objects, but only PNG/JPEG/WebP data URLs are accepted; HTTP(S) image URLs are rejected. The documented body ceiling is 32 MiB and image count is limited per question. Image-size violations may surface as delayed `504`, not merely synchronous validation. Preserve media position and do not silently fetch/resize inputs.

The reference defines synchronous JSON, not SSE. It rejects unknown root fields upstream; that is separate from MorphieCore's established codec unknown-field policy. Errors may be JSON, empty bodies (`404`/`405`) or HTML (`504`). Branch on HTTP status, preserve safe rate-limit information and do not depend on a universal error-body shape.

## Cloudflare

Hosted model IDs are `@cf/cloudflare/clef`, `@cf/cloudflare/clef-flash` and `@cf/cloudflare/clef-omni`. Direct current model sources describe context windows of 65,536, 24,576 and 64,000 respectively. Older Flash material retained the larger context/pricing; self-hosted weight capacity does not prove hosted limits.

Hosted schemas require 1–64 named questions and instructions, with 2–255 Choice options and 2–10 Score levels. Structured descriptions are admitted. Model outputs follow Noul/Choice/Score shapes with usage. Some constraints live in descriptions rather than structural JSON Schema keywords.

Clef/Flash hosted input exposes up to four embedded images, not remote image URLs. Omni additionally exposes bounded audio/video clips. Descriptive references to video in other model material are not sufficient when the hosted schema lacks that input. Workers binding returns model results directly; the REST SDK unwraps `result`, so the model-output schema alone is not the HTTP envelope.

Context documentation says questions/media consume capacity first and text state is truncated to the remainder; excess questions/media cause failure. Truncation direction, structured serialization and reliable no-truncation controls remain unresolved. A conservative byte limit cannot attest exact token fit. This is a material admission issue, not incidental transport normalization.

No Clef-specific SSE contract or confirmed Batch capability was found. Generic Workers AI streaming/batch documentation must not activate those features. Platform errors distinguish validation, size limits, timeouts, access, allocation and capacity failures; no per-question partial-error contract was found.

## Confidence is not a universal contract

All four return probability-like values, but that does not establish empirical calibration on a customer's domain. Noul/Predicate is P(yes/true); threshold selection belongs to the application. Choice and ordinal confidence summarize a distribution and must remain distinct from that distribution and from correctness guarantees.

| Source | Choice confidence | Score confidence |
|---|---|---|
| TypeSafe documented formula | `(p_max - 1/n) / (1 - 1/n)` | `max(0, 1 - spread / baseline)`, with `spread = sum(p_i * abs(i - peak))`, `baseline = sum(abs(i - (n-1)/2)) / n` |
| Perplexity current quickstart | Same normalized maximum | Same spread, but `baseline = sum(abs(i - peak)) / n`; explicitly defines single-option/level confidence as 1 |
| Cloudflare reference implementation | Maximum option probability | Maximum level probability; hosted implementation not established |
| OpenAI inspected guide/reference/SDK | Confidence returned; no formula established | Confidence returned; no formula established |

For levels 0,1,2 and probabilities 0.1,0.2,0.7, expected score is 1.6. The documented TypeSafe formula gives 0.4, Perplexity gives 0.6 and Cloudflare's reference algorithm gives 0.7. These are calculated counterexamples, not observed inference results. A threshold of 0.5 changes an application's action despite identical distributions.

Native reports should not be overwritten to hide this difference. A client-specific derivation is a separate design choice, must retain source facts, and cannot be called calibration. Rounding also matters: Cloudflare reference output rounds values, so exact floating-point sum equality is not a sound universal validator. TypeSafe's schema/formula combination leaves single-option/level handling to recheck.

## Capability-union source recheck

Rechecked on 2026-10-23 while designing the IR, against the [OpenAI Node resource](https://github.com/openai/openai-node/blob/master/src/resources/decisions.ts), [guide](https://developers.openai.com/api/docs/guides/decisions) and [Cloudflare Omni schema](https://developers.cloudflare.com/workers-ai/models/clef-omni/schema-input.json). These remain mutable source observations, not a new pinned baseline or live verification.

- OpenAI function/custom calls expose `call_id`, `name`, exact string `arguments`/`input`, optional `id`, `namespace` and `status`. Outputs carry matching `call_id`, text or ordered text/image output, optional `id` and `status`. Status includes in-progress/completed/incomplete; it is historical evidence, not gateway execution state. Assistant messages have optional commentary/final-answer phase and text/refusal content.
- OpenAI shared input text/image parts can carry `prompt_cache_breakpoint:{mode:\"explicit\"}`. Question instructions cannot. Request mode defaults to implicit; implicit mode writes an implicit boundary plus up to three explicit ones, while explicit-only mode writes up to four explicit boundaries and uses no cache if none exist. TTL is currently `30m`, documented as a **minimum** lifetime; the backend may retain entries longer. The cache key partitions reuse and is not identity. Input cache-read/write counters are included in the input aggregate.
- Cloudflare Omni explicitly places images before state. Its separate audio/video arrays preserve their own order but the inspected schema does not establish one global cross-modality sequence. Video sampling is documented at two frames per second; soundtracks are heard when every video has one. These are target behavior, not selectable generic request controls. State truncation remains documented without an exact local no-truncation test.
- The [Perplexity quickstart](https://docs.perplexity.ai/docs/decisions/quickstart.md) explicitly says questions within a native group see the state and every other question. This is peer-question content visibility, not merely correlated answers or a dependency on another answer. Context overflow can change the grouping, so it is not an unconditional all-question visibility guarantee.
- TypeSafe's [image reference rules](https://docs.typesafe.ai/images.md) specify that repeated criteria in answers, such as a Score `legend`, contain the original reference rather than the image. Normalizing media into resources must retain owner-bound source spellings for those client echoes. Structured object keys stay text, rather than being replaced as asset occurrences.

Existing-code review also constrains reuse. Generation's [content union](../../src/semantic/task/generation/content/item.rs) lacks a standalone JSON evidence part and includes reasoning/replay/compaction owners inappropriate for Decision. Its [structured value](../../src/semantic/task/generation/content/structured.rs), resource traversal and usage reports carry Generation-specific limits and validation. The [Embedding adapter](../../src/adapter/embeddings.rs) and [lowering expectations](../../src/protocol/embedding/projection.rs) offer a smaller source-report/delivery pattern. These observations support neutral leaf reuse and independent Decision owners, not an automatic extraction or refactor.

## Implementation evidence still needed

- Pin official SDK releases/schema revisions and reconcile nullable fields, score minima, question-media support, cache controls and model aliases.
- Verify default SDK paths, compression, retry/error behavior and typed answer completeness, including companion model listing.
- Select directional projections for role-bearing evidence, structured rubric, typed option values, refusal and confidence. Preserve same-protocol meaning before declaring cross-family support.
- Establish trustworthy per-target limits/grouping and no-truncation behavior; do not infer them from SDK permissiveness or broad context marketing.
- Keep offline codec checks, SDK loopback acceptance, bounded Provider observations and statistical quality/calibration evaluation separate. No part of this note establishes those runtime claims.
