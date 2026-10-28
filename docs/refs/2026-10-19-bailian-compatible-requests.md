# Bailian compatible-request subset

Checked **2026-10-19**. These are mutable official definitions, not live Provider acceptance or a complete DashScope API implementation. Accepted decisions belong to [Architecture](../architecture.md#shared-aliyun-billing-dialect); independent-ceiling proposals belong to [API expansion design](../api-expansion-design.md#independent-output-ceilings-unselected).

## Primary sources

- [Chat Completions reference](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-chat-completions): request/message/function tables, model-specific effort, thinking budgets, sampling, caps and usage.
- [Responses reference](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-responses): documented input/control/tool subset, effort aliases and cap scope. Its request section explicitly says parameters not listed are ignored.
- [Deep thinking](https://help.aliyun.com/zh/model-studio/deep-thinking): explicit activation versus default mode and thinking budgets. Prefer the specific model table over broader family statements when they differ.
- [Function Calling](https://help.aliyun.com/zh/model-studio/qwen-function-calling): ordinary function loops, explicit thinking-off forced selection, and parallel Chat calls.
- [Structured output](https://help.aliyun.com/zh/model-studio/qwen-structured-output): JSON Object versus JSON Schema; Qwen3.8 supports both. This does not establish strict function generation.
- [Context cache](https://help.aliyun.com/zh/model-studio/context-cache): owner-local markers and creation/TTL reports are distinct from ordinary implicit cached-token counts; this request correction does not activate explicit cache.
- [Text-generation API reference](https://help.aliyun.com/zh/model-studio/qwen-api-reference): native DashScope RPC is a separate `input`/`parameters` layout, not either compatible endpoint.

TokenPlan's shared dialect and separate authority follow the [accepted billing-domain boundary](../architecture.md#shared-aliyun-billing-dialect). No credential, origin, billing or replay-issuer equivalence is inferred.

## Observed definitions and selected treatment

| Domain | Official distinction | Existing-IR treatment |
|---|---|---|
| Current-turn activation | `enable_thinking` is a boolean; Responses also supports `reasoning.effort` with higher precedence | Preserve explicit on/off, never fill a default. Conflicting declarations reject rather than relying on precedence to hide intent. Native GLM5.3 cannot turn thinking off |
| Chat allocation | Positive `thinking_budget`; GLM5.3 ignores it and Kimi-K3 does not support it | Existing budgeted mode only, no qualitative effort mix, and no allocation exceeding an explicit total cap. Reject ignored/unrepresentable cases; no clipping or preset conversion |
| Qualitative effort | Qwen3.8 supports low/medium/xhigh; minimal aliases low and high/max alias xhigh | Model-bound named mapping on both protocols. Native intake normalizes declared aliases; projection records changed levels |
| GLM/Kimi effort | Chat GLM5.3 and Kimi-K3 list low/high/max; Responses GLM5.3 additionally maps minimal→low, medium→high, xhigh→max | Keep protocol-specific mappings. Native undocumented `none` is rejected before generic parsing erases its spelling. Semantic disabled activation may still use a documented boolean carrier where supported |
| Chat caps | Current bound Qwen3.8, GLM5.3/Prime and Kimi-K3 support total `max_completion_tokens`. Legacy Qwen/Kimi `max_tokens` excludes thinking; GLM5.3's includes it | Encode the total spelling for all current bindings and both plans. Native legacy intake accepts the GLM exception or explicitly disabled thinking; otherwise reject reply-only semantics |
| Responses caps | Qwen3.8 `max_output_tokens` includes thoughts; other models use a reply ceiling; minimum 16 | Accept a total ceiling only for Qwen3.8 or explicit thinking-off. An always-thinking GLM reply ceiling cannot represent the IR total limit |
| Sampling | Temperature [0,2), top-p (0,1]; Qwen3.8 thinking raises temperature below 0.6; top-k is positive ≤100 and unsupported on Kimi-K3 | Reject silent temperature adjustment and invalid values. Add the top-k codec carrier without widening catalog admission. Seed/probability bounds remain strict; current bound models do not gain unverified logprob support |
| Chat tools | Functions with descriptions; Qwen required selection is unsupported; named forcing is unsupported while thinking | Reject unsupported kinds/dispatch/strict intent and unsupported selection. Use the documented non-strict shell, not an implicit schema-normalization promise |
| Responses tools | Flat functions; only one function for required/specific selection; required subset uses `allowed_tools` | Explicit false or native non-strict omission only. Convert a single specific function to its exact required subset; do not weaken strict/default-normalization intent |
| Responses results | Function output is a string and must immediately follow its corresponding call | Reject arrays/media and non-adjacent results. Do not stringify, flatten or reorder them |
| Responses history | Summary reasoning input is documented; phase, encrypted/full-readable reasoning input and ordered configuration updates are not | Retain the selected summary carrier; reject unsupported history rather than sending a standard OpenAI item that may be ignored |
| Delivery | Chat documents `include_usage`; neither compatible interface establishes an upstream obfuscation control | Keep the client preference in delivery state. Send only Chat usage options; send no Responses `stream_options` |
| Unowned controls | Hosted search/execution, prefill, pixel/frame/cache controls and native RPC bags need their own contracts | Recognize and reject, including an HTTP `extra_body` bag. SDK `extra_body` is flattened before transmission, not a wire passthrough |

The public gateway owns typed activation/allocation/top-k spellings, not an arbitrary Provider bag. Its generic total-cap convention remains unchanged. The Bailian guard is a documented subset, not a universal model validator: physical/model capability does not make every protocol carrier available, and a client schema does not activate a binding feature.

## Evidence boundary

The [Bailian dialect](../../src/protocol/openai/dialect/bailian.rs) owns native checks and writers; current binding admission belongs to the catalog and Architecture. Directional offline tests do not establish effective temperature, hard token enforcement, replay validity or live plan acceptance. This source subset does not claim every cache/`x_details` report, hosted/citation/OCR output, standalone modality or native RPC stream. [Development](../development.md#verification-gates) owns verification, not a second field-support inventory.
