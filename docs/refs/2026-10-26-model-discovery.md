# Model capacity and discovery evidence — 2026-10-26

[Architecture](../architecture.md#generation-text-model-discovery) owns the selected design. This note records source evidence, not runtime health or authorization to activate tasks.

## Capacity sources

Official pages read on 2026-10-26:

| Model | Context window | Independent input maximum | Published output maximum | Source |
|---|---:|---:|---:|---|
| `gpt-6.1-sol` | 1,050,000 | 922,000 | 128,000 | [OpenAI model specification](https://platform.openai.com/docs/models/gpt-6.1-sol.md) |
| `gpt-6-astra` | 1,050,000 | Unknown | 128,000 | [OpenAI model specification](https://platform.openai.com/docs/models/gpt-6-astra) |
| `grok-4.7` | 500,000 | Unknown | No separate text output limit stated | [xAI guide](https://docs.x.ai/developers/grok-4-7) |
| `qwen3.8-max`, `qwen3.8-flash` | 1,000,000 | Unknown | Unknown | [Model Studio](https://help.aliyun.com/zh/model-studio/text-generation-model), explicitly `100万` |
| `gemini-3.8-flash` | Unknown | 1,048,576 | 65,536 | [Google model specification](https://ai.google.dev/gemini-api/docs/models/gemini-3.8-flash) |

These are model/API planning facts, not account entitlements, reserved capacity, a measured attention-quality boundary or a promise that all maxima fit together. Google's input maximum is not a published total context window. Grok's wording does not justify an infinity sentinel. OpenAI's published output maxima are not inserted defaults or a new inference limit in this metadata slice; existing bound output declarations remain authoritative.

New numeric seeds are limited to exact IDs on their native catalog bindings. Model Studio evidence does not certify TokenPlan; native model pages do not certify OpenRouter/OpenCode Go capacities. SIWC/Grok subscription service limits are not established by public API model pages and remain unknown unless explicitly supplied by trusted deployment configuration. A configured upstream-model, origin or endpoint-path replacement invalidates inherited context/input planning facts unless the deployment explicitly declares replacements.

Xiaomi V2.6 and DeepSeek Flash sources use `1M`/`128K`/`384K` notation; no decimal/binary expansion is selected here. The inspected Claude Haiku 5.5 overview likewise uses `1M`/`128K`; its 300K Batch-only opt-in is not the ordinary Messages capacity. Other catalog models remain unknown rather than borrowing family, weight-file, client-default or unrelated endpoint limits.

Catalog declarations and candidate configuration retain context, input and output as independent optional values. Missing evidence never means unlimited. Common route capacity requires comparable facts for every candidate. A deployment may narrow known declarations or supply an unknown one explicitly; discovery never calls an upstream or runs a tokenizer.

## Installed Pi domain

Pi 1.4.0's `pi-ai` model union has chat, classifier and image operations, not embedding/audio/realtime operations. Its chat user/tool-result content supports text and images. `api/openai-responses-shared.js` emits inline data URLs with explicit `detail: "auto"` both in user input and in function/custom-call output arrays. Image input must therefore be assessed at both placements and with that detail, not inferred from a vision flag or an omitted-detail witness.

Pi's image metadata is not a universal enforcement layer. `inputLimits.images.resize` is client preprocessing; count/request-byte fields do not automatically constrain every request path. The gateway's finite discovery domain is not a general size/count guarantee. Schema fields and client-generated payload behavior require separate checks.

Current public model admission intentionally remains narrower than official model specifications in places, including the existing `gpt-6.1-sol` image flag. This slice reports those effective domains; it does not broaden admission from native marketing/specification claims.

## Other tasks

Embedding already has independent typed contracts and runtime behavior. Reusing its discovery vocabulary does not connect it to the enhanced model list or Pi. Image/Audio/Video/Decision/Realtime schema components are metadata shapes only, with no new routes, callbacks, codecs or session lifecycle. Realtime transport and interruption/recovery contracts remain unimplemented and separately selected.
