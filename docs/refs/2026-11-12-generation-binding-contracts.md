# Generation binding contracts

Primary sources reviewed on **2026-11-12**. These are model/service contracts, separate from live account eligibility and complete Agent-loop qualification. Accepted projections belong in [Protocol adaptation](../architecture/adaptation-and-dialects.md); unresolved qualification belongs in the [issue register](../roadmap/unscheduled-issues.md).

## SIWC tool carriage

The official [preview limitations](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations.md) require HTTP `store:false`, `stream:true` and array input. Function/custom tools use namespaces or `additional_tools` input items. This is a carrier restriction, not an absence of client-executed tools. Hosted-tool availability and unsupported output ceilings remain independent service limitations.

The existing Responses developer `ToolBatch` represents the latter carrier. Moving flat root definitions into a leading batch preserves their availability before historical calls, names, schemas, strictness and direct call/result references. Existing inline declarations retain their order; the projection neither chooses a schema winner nor rewrites history. Root namespaces stay native. The same function/custom definition writer serves root and inline carriage.

## GPT model facts

The official [Astra](https://developers.openai.com/api/docs/models/gpt-6-astra.md), [Sol](https://developers.openai.com/api/docs/models/gpt-6.1-sol.md) and [Luna](https://developers.openai.com/api/docs/models/gpt-6-luna.md) pages list function calling, structured outputs and image input. The [function-calling guide](https://developers.openai.com/api/docs/guides/function-calling.md#strict-mode) defines strict function schemas through the structured-output carrier. Sol tool calling requires Responses; Luna Chat function calling requires reasoning off. These facts do not grant a proxy or billing-plan endpoint another service's capacity, pricing or guarantees.

Astra/Sol expose positive reasoning levels `low`, `medium`, `high`, `xhigh`, `max`. Luna additionally permits `none`. The [model guide](https://developers.openai.com/api/docs/guides/latest-model) explicitly excludes `none` for Astra/Sol. A generic SDK enum therefore cannot supply every model's activation or native level domain.

## Grok reasoning

The official [reasoning guide](https://docs.x.ai/developers/model-capabilities/text/reasoning.md) and [Grok 4.7 page](https://docs.x.ai/developers/grok-4-7.md) expose `low`, `medium`, `high`, `xhigh`. Reasoning cannot be disabled; omission uses the service default `high`. Mapping a non-native positive level to an existing level is distinct from replacing explicit off with active thinking.

Grok 4.7 returns Responses encrypted reasoning by default and documents unchanged next-request replay. The [Responses streaming contract](https://developers.openai.com/api/reference/resources/responses/streaming-events) supplies owner closure and native replay shapes independently of binding history assessment.

## Replay identity and carrier

A bound target's source identity and its protocol carrier are separate facts. Projection and final request/response/event checks must resolve the same identity. The explicit library origin wins for strict target resolution; otherwise the bound codec scope is used. Native reasoning then uses the separately bound service-source scope when present, while compaction and other strict attachments retain target-specific identity. An unbound reasoning scope alone grants no replay authority.

A catalog boolean that only suppresses the explicit target origin cannot establish that an upstream has no reasoning carrier: decoding can still issue a trusted opaque owner. Carrier qualification stays with the protocol/dialect, and unassessed history acceptance delegates to upstream validation within the existing trust boundary.
