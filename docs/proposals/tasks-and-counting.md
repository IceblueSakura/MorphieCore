# Standalone Tasks and Token Counting Proposals

This document owns proposed designs for standalone counting surfaces (OpenAI Responses input tokens and Anthropic Messages counting) and Embeddings expansion.

See [Proposals Framework](index.md) for shared boundaries and [Architecture](../architecture/index.md#token-counting-policy) for accepted token counting policies.

## Contents

- [Token counting](#token-counting)
- [Embeddings](#embeddings)
  - [Embedding chain and ownership](#embedding-chain-and-ownership)
  - [Embedding inputs and admission](#embedding-inputs-and-admission)
  - [Embedding results and encoding](#embedding-results-and-encoding)
  - [Google Embedding profile](#google-embedding-profile)

## Token counting

Selected for stage 3 with cross-protocol conversion and deferred ([roadmap](../roadmap/backlog-and-frozen.md#deferred-token-counting)); the contract below is unstarted design reference. The Messages `count_tokens` surface is confirmed; the Responses counting companion still requires selection.

Model a count request from count-relevant conversation content: instructions, ordered history, tools, media and applicable settings. It is not generation with a zero output cap. A count result is not inference usage and must not contain fabricated generated output.

OpenAI documents `POST /v1/responses/input_tokens`; Anthropic documents `POST /v1/messages/count_tokens`. Reuse one task boundary while preserving distinct source semantics and result envelopes. Resolve any admitted gateway-owned history reference before target preparation; never forward a gateway ID as a Provider-owned reference.

The [upstream-first counting policy](../architecture/index.md#token-counting-policy) is selected: prefer upstream counts, allow explicit coarse estimates for inaccurate upstream reports, and do not build a tokenizer-based exact-counting path. Identify the model/target, effective input projection and count source; a Messages-shaped request does not imply an Anthropic model or counting method.

Select the concrete profile's upstream reliability criteria, coarse estimation rule and supported input coverage. A character-based rule must account for the projected instructions, history and tool definitions/results, not just the latest user text. For heuristic fallback, media require an explicit estimate policy or rejection, not a zero-cost assumption. Select client-visible source/approximation reporting compatible with the result envelope; do not invent standard API fields or promise billing equality.

Exit evidence: independent request/result oracles cover upstream preference and the selected inaccurate-report fallback, count-relevant controls and media, explicit scope/provenance, no generation side effects and no tokenizer execution. Reject input outside the selected counting profile rather than silently omit it. One candidate's result still neither covers every failover candidate nor reserves the next generation route.

## Embeddings

The sample-centered text/scoped-token IR, native Google integration, immutable preparation and explicit ancillary/numeric delivery rules are implemented. Their authoritative contract is [Independent Embedding path](../architecture/index.md#independent-embedding-path), not a pending proposal. This section retains only extension boundaries.

### Embedding chain and ownership

Preserve the independent single-target chain, source/delivery separation and one immutable prepared plan. Future operations must establish their own admission and response expectations before activation; no generic provider-options bag, hidden fan-out or Generation failover is proposed. Native error-body inspection is not selected: status and Retry-After remain authoritative even if a body stalls.

### Embedding inputs and admission

Future joint image/audio/video inputs need typed content owners, source validation and model-specific input processing controls. Keep independent samples distinct from joint parts within a sample. Do not infer joint semantics by concatenating text or reuse generation attachments without reviewing their contract.

Task purpose and explicit overflow/truncation controls would belong to each sample, not competing batch defaults. No task prefix injection or implicit truncation is selected. New tokenizer scopes need vocabulary, special-token, per-input and aggregate limits plus actual target-carrier evidence; the implemented ordinary `cl100k_base` subset does not attest other tokenizers.

### Embedding results and encoding

Quantized int8/uint8/binary vectors and multi-representation output require an explicit numerical representation independent of transport encoding. Base64 alone cannot identify element width or dtype. Tensor/soft-token results require a separate shape/index contract, not flattening into ordinary dense embeddings. Contextualized chunks need a distinct shared-context relation rather than being represented as independent batch samples.

Embedding-space identity remains a trusted model contract. Equal widths, labels or a hash of request options are not sufficient for substitutability. Retrieval indexes, chunking, contextual grouping and migration/re-embedding remain outside the current path.

### Google Embedding profile

The implemented target uses native synchronous `batchEmbedContents`, nested controls and explicit no-truncation; [dated source/protocol evidence](../refs/2026-10-22-embedding-image-contracts.md#google-wire-source-reconciliation) distinguishes current REST from lagging SDK converters. The Google OpenAI-compatible endpoint, native downstream APIs, `gemini-embedding-001`, multimodal ingestion and tensors remain unselected. A future extension must characterize its own configuration and reporting rather than inherit text-only acceptance.

