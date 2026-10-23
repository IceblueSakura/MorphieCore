# Stable-release protocol contracts

Checked **2026-10-23** against official releases, SDK source and current API documentation. The [source lock](../../tests/upstream/sources.lock.json) owns exact release revisions; [Architecture](../architecture.md) owns accepted design and [Roadmap](../roadmap.md#standards-reset-and-redesign) owns execution. This note records source evidence, not a second plan or run report.

## Source selection

Use registry stable versions and matching release tags, not default-branch HEAD. OpenAI Python 3.37.0 resolves to `e8a0fd28bbdc1699c97ff0f602b1af3dd378382d`, whereas the earlier research checkout used `81792ff`; the four inspected Chat/Responses response/event types agreed, but `_models.py` differed. Similar version strings do not prove identical SDK behavior.

Vercel's selected package tags all resolve to `594495a5af70547d9d7a13d6df334ae30db49622`, not the previously inspected `e623e8c` main checkout. Its fixtures supplement official contracts; SDK warning-and-repair behavior is not protocol authority.

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

Google web schemas are mutable; their version, revision and content fingerprints are recorded separately in the source lock. A changed digest requires review, not silent expected-output regeneration.

## Consumer and fixture evidence

Official Python SDK base models accept extra fields, but this alone does not establish public extension compatibility. Anthropic's stream accumulator copies known delta fields into its final Message; unknown late extensions are not automatically retained there. Test raw-event visibility and final-object behavior independently. No inspected standard defines a generic successful gateway-adaptation warning field.

Useful upstream entry points include Anthropic `tests/lib/streaming/fixtures`, OpenAI Responses/Chat stream and returned-history tests, Google model tests and Vercel provider fixtures. Their selected paths and licenses must be checked during acquisition. SDK tests against their own mocks are not evidence that MorphieCore ran; connect raw fixtures and real consumers through the gateway boundaries.

Old private captures and installed-client observations remain sensitive compatibility evidence, not normative or publishable fixtures. Public fixtures also require content and license review before import.
