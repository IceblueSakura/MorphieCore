# Decision model contracts expansion

Checked 2026-11-25. This note extends the [2026-10-23 Decision contracts](2026-10-23-decision-contracts.md) note with newly published provider APIs, primitive extensions, and codebase constraints observed while refining the unified Decision Semantic IR. [Architecture](../architecture/index.md#decision-api-direction) owns the selected dual-SDK direction; [Decision API Proposal](../proposals/decisions-api.md#decisions) owns proposed layouts and projections; [Roadmap](../roadmap/index.md#stage-4-pending-scope) owns gates.

Evidence is explicitly stratified into two tiers:
- **P1 (Primary schema/SDK evidence):** Direct OpenAPI `v1beta` schema, official `@google/genai` / `google-genai` SDK source, and official guide pages for Google Gemini Decision 1, alongside the earlier P1 baselines for OpenAI, TypeSafe, Perplexity v1.2, and Cloudflare Clef recorded on 2026-10-23.
- **P2 (Secondary/search-summary evidence):** Search-grounded summaries of public documentation, Hugging Face model cards, and aggregator notes for Perplexity v1.3 (`locate`), Qwen `decision-model-preview`, Microsoft `Microsoft-Decision-1` / `1.1`, Liquid AI `d1` / `d1.2`, Inception `mercury-decide`, and OpenRouter `/api/alpha/decisions`. P2 contracts and unresolved conflicts remain gated until primary OpenAPI/SDK specs or bounded live probes lock them.

As of 2026-11-25, no dedicated `/v1/decisions` or `/v1/systemone` decision-model endpoint was found for Anthropic, Mistral, DeepSeek, xAI, or Cohere.

## Contents

- [Wire families and ecosystem matrix](#wire-families-and-ecosystem-matrix)
- [Google Gemini Decision 1 (P1)](#google-gemini-decision-1-p1)
- [New and updated P2 provider observations](#new-and-updated-p2-provider-observations)
  - [Perplexity Decider v1.3](#perplexity-decider-v13)
  - [Qwen decision-model-preview](#qwen-decision-model-preview)
  - [Microsoft Decision 1 and 1.1](#microsoft-decision-1-and-11)
  - [Liquid AI D1 and D1.2](#liquid-ai-d1-and-d12)
  - [Inception Mercury Decide](#inception-mercury-decide)
  - [OpenRouter Decisions aggregator](#openrouter-decisions-aggregator)
- [Codebase and IR boundary observations](#codebase-and-ir-boundary-observations)
- [Updated implementation evidence still needed](#updated-implementation-evidence-still-needed)

## Wire families and ecosystem matrix

Three distinct native wire families now exist, plus one aggregator layer:

1. **System One family (`state` + keyed `questions` map → keyed `answers` map):** TypeSafe Jev (`POST /v1/systemone`), Cloudflare Clef (`POST .../accounts/{account_id}/ai/run/@cf/cloudflare/{model}` with REST `{result}` wrapper), Perplexity Decider (`POST /v1/decisions`, System One-style wire despite path name), Qwen (`POST /compatible-mode/v1/systemone`), Microsoft Foundry (`POST /providers/microsoft/v1/systemone`), Liquid AI (`POST /decisions/v1/systemone`), and Inception (`POST /v1/decisions` or `/v1/systemone`).
2. **OpenAI Decisions family (`input` string/conversation/history/JSON + ordered `questions[]` → ordered `answers[]`):** OpenAI `gpt-6-luna` (`POST /v1/decisions`).
3. **Google Gemini Hybrid Decisions family (Interactions multimodal `input` + System One-style keyed `questions` map → keyed `answers` map):** Google `gemini-decision-1` (`POST /v1beta/decisions`).
4. **Aggregator:** OpenRouter (`POST /api/alpha/decisions`, discovery via `GET /api/v1/models?output_modalities=decisions`).

| Model family | Standing | Endpoint / wire family | Supported primitives | Input carriers & modalities | Context / question / option limits | Confidence & refusal / blocking |
|---|---|---|---|---|---|---|
| TypeSafe `jev-1.13.0`, `1.14.0` | P1 | `/v1/systemone` (System One) | `noul`, `choice`, `score`, `box` (1.14+) | `state` (str/obj/arr) + `assets` PNG/JPEG placeholders in `state`/`instructions`/`criteria` (JSON or multipart, `TypeSafe-Danger-Zone-Images: true`) | 64k/32k default, 262k via `TypeSafe-Danger-Zone-Long-Context: true`; Choice 1–255; Score 1–10 (OpenAPI) / 2–10 (older docs) | `noul`: none; Choice normalized max; Score centered-uniform spread; `box` nullable confidence, no `found`; isolated; HTTP errors |
| OpenAI `gpt-6-luna` | P1 | `/v1/decisions` (OpenAI) | `predicate`, `choice` (str or bool), `score` (`label` + `description`) | `input` (str or ordered messages + historical function/custom calls/results + `input_json` + image URL/data URL with `detail`); `instructions` parts | 1–400 ordered questions; Choice 2–255; `input_json` ≤32 depth / 10k values / 256 KiB; explicit cache breakpoints (`30m` min TTL) | `predicate`: none; Choice/Score confidence (formula undocumented); isolated; per-question `refusal` |
| Cloudflare `clef`, `clef-flash`, `clef-omni` | P1 | `.../ai/run/@cf/cloudflare/{m}` (System One + REST `{result}`) | `noul`, `choice`, `score` | `state` + sidecar `images[]` (≤4, before `state`); `clef-omni` adds `audio[]` & `videos[]` (2 fps, no cross-modality audio/video order) | 65,536 / 24,576 / 64,000 ctx; 1–64 questions; Choice 2–255; Score 2–10; questions/media consume budget first and text `state` silently truncates | Reference `p_max` (hosted unattested); platform errors |
| Perplexity `pplx-decider-v1.2`, `v1.3`, `flash`, `27b` | P1 (v1.2) / P2 (v1.3) | `/v1/decisions` (System One wire) | `noul`, `choice`, `score`, `locate` (v1.3+, plain-text `state` only) | `state` + inline PNG/JPEG/WebP data-URL markers (≤32 MiB; HTTP URLs rejected); `locate` has no `criteria` | 256K ctx; 1–128 questions; Choice 1–255; Score 1–10 | Choice normalized max; Score peak-centered baseline (singleton=1); v1.2 groups share question text & bill `state` per group; `flash` isolated per question |
| Google `gemini-decision-1` | P1 | `/v1beta/decisions` (Hybrid) | `noul`, `choice`, `score` | `input` (`string \| Content \| Content[]`: text, image, audio, video, PDF `document`; inline ≤100 MB / PDF ≤50 MB / HTTPS ≤15 MB / Files URI; `resolution`); `instructions` text-only | 262,144 ctx; 1–1,000 strictly isolated questions; Choice 2–255 (UTF-8 sorted); Score 2–255 | `noul` clamped `[0.01, 0.99]`; closed-world `confidence` (`max(p,1-p)` / `p_max`); whole-request `prompt_feedback.block_reason` (HTTP 200, empty `answers`) |
| Qwen `decision-model-preview` | P2 | `/compatible-mode/v1/systemone` (System One) | `noul`, `choice`, `score` | `state` (text/structured); image support disputed in secondary summaries | 65,536 ctx; rec ≤16 questions; Choice ≤255; Score 2–255; fine-tuning | Reports `confidence` (formula unpinned); response keys conflict between System One and OpenAI-like secondary sketches |
| Microsoft `Decision-1` / `1.1` | P2 | `/providers/microsoft/v1/systemone` (System One) | `noul`, `choice`, `score` | `state` (text/JSON); `1.1` (Gemma 4 26B-A4) adds ≤8 images via `assets` | ~32K (32,767) ctx; Choice 2–255; Score 2–10 | Response keys and confidence formula unpinned |
| Liquid AI `d1`, `d1-ultra`, `d1.2`, `d1.2-ultra` | P2 | `/decisions/v1/systemone` (System One superset) | `noul`, `choice` (`multiple: true`), `score`, `text`/`number`/`date`, `spans`, `box`/`boxes`, `number`+`range` | `state` + `images` + `audio` (`d1.2`) | `spans`/`boxes` accept `max` 1–64 (default 16) | `box`/`boxes`/scalars return `found` probability; `range` returns median + `[p10, p90]` + `unit`; `multiple: true` returns independent marginals |
| Inception `mercury-decide` | P2 | `/v1/decisions` or `/v1/systemone` | `noul`, `choice`, `score`, `extract` | `state` (text/JSON) + image data URLs | 32K–64K ctx; Choice 1–255; Score 1–10 | `extract` returns `value`, `spans`, per-field `probabilities`, `confidence`; 0 output tokens |

## Google Gemini Decision 1 (P1)

Inspected directly via the `gemini-api-docs` OpenAPI `v1beta` document (`Operation: CreateDecision`, `CreateDecisionRequest`, `CreateDecisionResponse`, `NoulQuestion`, `NoulAnswer`, `ChoiceQuestion`, `ChoiceAnswer`, `ScoreQuestion`, `ScoreAnswer`, `Criteria`, `DecisionInputContent`, `DecisionInstructionContent`), official guide (`gemini-api-guides/decisions-api.md` and `gemini-decision-1.md`, released 2026-11-11), and official JavaScript/Python SDK docs (requires SDK `>= 3.2`, `api_version = "v1beta"`).

- **Endpoint & authentication:** `POST https://generativelanguage.googleapis.com/v1beta/decisions` with `x-goog-api-key`.
- **Shared `input` contract:** `string | DecisionInputContent | DecisionInputContent[]`, reusing Interactions `TextContent`, `ImageContent`, `AudioContent`, `VideoContent`, and `DocumentContent` (`application/pdf`). Tool-call, tool-result, and thought blocks are explicitly excluded. Adjacent text blocks concatenate without separator; empty `input` is rejected. Structured JSON objects/arrays must be serialized to strings and passed as text.
- **Media limits & `resolution`:** Inline base64 up to 100 MB per request (50 MB for PDFs); public/signed HTTPS URLs up to 15 MB per external URL; Files API URIs (`file.uri`) supported for larger/reusable media. `resolution` (`low`, `medium`, `high`, `ultra_high`) applies to images (`low`, `medium`, `high` to video); audio and document blocks do not accept `resolution`.
- **Question capacity & isolation:** 1 to 1,000 independent questions per request over shared input up to 262,144 tokens. Shared input is counted once regardless of question count; questions never see each other's content or answers, and question map keys are never visible to the model.
- **Text-only instructions:** `DecisionInstructionContent` resolves strictly to `Interactions.TextContent`. Passing media or structured blocks in `instructions` is invalid even when `input` carries 5 modalities.
- **Question & answer specifics:**
  - `NoulQuestion`: optional `criteria` has `true_description` and `false_description` strings (distinct from TypeSafe `true`/`false`). `NoulAnswer` returns `noul` clamped to `[0.01, 0.99]` and required `confidence = max(noul, 1 - noul)`.
  - `ChoiceQuestion`: `criteria` maps 2–255 non-empty option IDs (no control chars or leading/trailing whitespace) to descriptions (empty string allowed). Before evaluation, the API sorts option keys alphabetically in **UTF-8 byte order**, and exact ties break to the alphabetically first key. `ChoiceAnswer` returns `choice`, `probabilities` (summing to 1), and `confidence = p_max`.
  - `ScoreQuestion`: `criteria` is an ordered array of 2–255 non-empty level strings (`0..n-1`). `ScoreAnswer` returns `score` (`sum_k k * probability_k`), `probabilities`, `legend`, and `confidence = p_max`.
- **Whole-request policy block & modality accounting:** `CreateDecisionResponse` contains `answers`, optional `prompt_feedback`, and `usage`. If `prompt_feedback.block_reason` is set, the request was blocked by input policy (`answers` is empty on an HTTP 200 payload). `usage` reports `total_input_tokens`, `input_tokens_by_modality`, `total_output_tokens = 0`, and `total_tokens`.

## New and updated P2 provider observations

All items in this section come from secondary documentation summaries and model pages as of 2026-11-25 and require primary schema inspection or bounded live checks before production target admission.

### Perplexity Decider v1.3

`pplx-decider-v1.3`, `pplx-decider-flash-v1.3`, and open-weight `pplx-decider-27b` add the **`locate`** primitive for extractive span localization:
- Request: `{"type": "locate", "instructions": "..."}` without `criteria`, restricted to plain-text `state` (images not supported for `locate`).
- Response: `{"spans": [{"start": int, "end": int, "text": string, "score": float}]}`, returning `[]` when no span matches.
- **Unresolved before activation:** exact `start`/`end` offset unit (UTF-8 byte vs UTF-16 code unit vs Unicode scalar), endpoint inclusivity (half-open vs inclusive), whether `spans` can exceed or alter verbatim `state` substrings, and whether `v1.3` preserves `v1.2`'s native question-group visibility and per-group state billing.

### Qwen decision-model-preview

Alibaba Cloud Model Studio (DashScope) exposes `decision-model-preview` at `POST /compatible-mode/v1/systemone` (Beijing and Singapore regions; 65,536-token context; recommended ≤16 questions; 0 output tokens; fine-tuning supported). Supports `noul`, `choice` (≤255), and `score` (**2–255** levels, wider than TypeSafe's 10-level limit).

Two P2 conflicts must be resolved by direct probe or official DashScope OpenAPI retrieval before codec work:
1. **Response field names:** Official Alibaba summaries state System One compatibility (`noul`/`choice`/`score` + `legend` + `input_tokens`), whereas third-party gateway summaries show OpenAI-like sketches (`answer`, `score_label`, `prompt_tokens`).
2. **Image modality support:** Primary Alibaba summaries describe text-only input, while third-party parameter breakdowns list an optional `images` array.

### Microsoft Decision 1 and 1.1

Hosted in Microsoft Foundry (`POST /providers/microsoft/v1/systemone`; ~32K / 32,767 tokens; `noul`, `choice` 2–255, `score` 2–10). `Microsoft-Decision-1` is built on `Qwen3.5-9B`; `Microsoft-Decision-1.1` is built on `Gemma 4 26B-A4` and adds up to 8 images via `assets`. Exact wire response keys (`selected`/`level`/`probability` vs System One `choice`/`score`/`noul`), asset encoding rules, and confidence formulas remain unpinned.

### Liquid AI D1 and D1.2

Liquid AI (`d1`, `d1-ultra`, `d1.2`, `d1.2-ultra`) exposes `POST /decisions/v1/systemone` (text, image, and `d1.2` audio) compatible with `typesafe-sdk` for core types while expanding the primitive surface:
- **`choice` with `"multiple": true`:** Multi-label classification returning `choices` (options with probability ≥0.5) and independent per-option `probabilities` (marginal Bernoulli probabilities that do not sum to 1).
- **Extractive typed scalars (`text`, `number`, `date`):** Returns nullable parsed value, verbatim `text`, `[start, end]` `span`, and `found` probability (value becomes `null` when `found < 0.5`).
- **Multi-span (`spans`):** Optional `max` (1–64, default 16); returns items (`text`, `span`, `found`), `count`, `presence`, and `at_least` truncation indicator.
- **Object detection (`box`, `boxes`):** Normalized `[top, bottom, left, right]` coordinates plus `found` probability and `confidence`; `boxes` also carries `max`, `count`, `presence`, and `at_least`. Distinct from TypeSafe `box`, which has no `found` signal and always emits one rectangle.
- **Quantity estimation (`number` with `range`):** Returns median estimate alongside `[p10, p90]` uncertainty interval and optional `unit`.

### Inception Mercury Decide

`mercury-decide` uses a diffusion LM readout head (`POST /v1/decisions` or `/v1/systemone`, 32K–64K context, text/JSON + image data URLs, 0 output tokens) supporting `noul`, `choice` (1–255), `score` (1–10), and a keyed **`extract`** primitive (`criteria: {field: desc}` → `value`, `spans: {field: {start, end}}`, per-field `probabilities`, and aggregate `confidence`).

### OpenRouter Decisions aggregator

OpenRouter exposes `POST /api/alpha/decisions` and filters decision-capable models via `GET /api/v1/models?output_modalities=decisions`, adding billing metadata (`usage.cost`, `cached_tokens`). As an aggregator, model execution semantics, visibility, and calibration formulas inherit the underlying upstream target.

## Codebase and IR boundary observations

Inspection of `crates/morphiecore-semantic` establishes three concrete constraints for Decision IR realization:
- **JSON limits decoupling:** [`JsonLimits::STRUCTURED`](../../crates/morphiecore-semantic/src/value/json.rs#L28-L32) caps structured values at `8,192` nodes and `32` depth, and `parse_json` clamps budgets to [`JsonLimits::ENVELOPE`](../../crates/morphiecore-semantic/src/value/json.rs#L17-L21) (`64 MiB`). These bounds differ from OpenAI `input_json` (`10,000` values / `256 KiB` compact UTF-8) and Gemini inline request bytes (`100 MB`). Decision admission must distinguish gateway resource ceilings, native field counting units, and decoded media sizes rather than silently reusing Generation JSON constants and claiming full native range.
- **Shared leaf reuse candidates:** [`InputModality` and `ModalityTokenCount`](../../crates/morphiecore-semantic/src/task/embedding.rs#L162-L175) already match Gemini `input_tokens_by_modality` and can be shared neutrally without coupling Decision to Embedding. Similarly, [`CacheMode` and `CacheTtl`](../../crates/morphiecore-semantic/src/context.rs#L102-L110) fit `DecisionServiceHints` without importing [`ExecutionHints`](../../crates/morphiecore-semantic/src/context.rs#L168-L176) (`store`, `background`, `max_tool_calls`).
- **Presence serialization rule:** [`Presence<T>`](../../crates/morphiecore-semantic/src/value/presence.rs#L9-L16) serializes both `Absent` and `Null` via `serialize_none`, leaving field omission (`skip_serializing_if`) to the enclosing wire codec. Use `Option<T>` by default and reserve `Presence<T>` for fields where `null` and absent are observably distinct.

## Updated implementation evidence still needed

Native text-predicate diagnostics returned a matching answer from OpenAI `gpt-6-luna`, TypeSafe `jev-1.13.0`, and Google `gemini-decision-1`. OpenAI Python `3.37.0` and TypeSafe Python `0.7.6` consumed all three typed primitives in a mixed text request through the embedded gateway and their native Provider. TypeSafe also completed a native predicate loop with its default zstd compression enabled. A synthetic receiver established the SDK model listing envelopes and validation exceptions; gateway discovery is not yet qualified.

The fixed OpenAI and TypeSafe models both returned full distributions for a 40-option Choice and a 10-level Score. OpenAI kept Boolean `true` distinct from string `"true"` and both models echoed the requested rubric. One TypeSafe ten-level report differed from the expected score computed from its serialized probabilities by `0.05`, despite those probabilities summing to one. The native codecs use clipped half-hundredth intervals and the simplex constraint to admit rounded reports without rewriting the distribution or reported statistics. This is a gateway acceptance envelope, not a vendor guarantee of precision. TypeSafe also served singletons with a numeric confidence; the singleton formula is not asserted.

Live TypeSafe discovery listed `jev-latest` and `jev-preview`, not the fixed Jev versions, and returned offset timestamps for `release_date` despite documenting `YYYY-MM-DD`. Fixed versions `jev-1.13.0` and `jev-1.14.0` evaluated successfully when named directly. The [TypeSafe model changelog](https://docs.typesafe.ai/changelog.md) names `jev-1.14.0` and its publication date, `2026-10-14`; prefer that fixed model for initial public discovery rather than reusing an alias date.

Native Python clients also completed mixed three-primitive requests over structured evidence. OpenAI used an `input_json` part within user-owned history, whereas TypeSafe kept business JSON in `state`; keys such as `role` and `image_url` stayed inert data.

1. Complete the gateway companion operations using the Python releases in the [source lock](../../tests/upstream/sources.lock.json); pin JavaScript SDK consumers separately before claiming support.
2. Retrieve primary DashScope OpenAPI / run a bounded probe for `qwen/decision-model-preview` to resolve the response-key and image-support conflicts.
3. Lock Perplexity `v1.3` `locate` span unit (`UTF-8` vs `UTF-16` vs Unicode scalar), endpoint bounds, and `v1.3` question-grouping behavior before Wave 2 `TextLocate` admission.
4. Retrieve primary Foundry OpenAPI for `Microsoft-Decision-1.1` before adding any Microsoft binding.
5. Pin primary schemas for Liquid `d1.2` (`MultiLabelChoice`, `QuantityRange`, detection/extraction) and Inception `mercury-decide` (`extract`) before Wave 3 admission.
