# Stage 5 Unscheduled Proposals Backlog

This document owns detailed unscheduled Stage-5 operation proposals and separately deferred discovery, Agent integration, cache-prewarming and service-policy directions. The latter retain their own scheduling decisions; inclusion here does not assign them to Stage 5.

**All stage-5 tasks are currently unscheduled:** scope placement is not a start or delivery commitment. See [Proposals Framework](index.md) for shared boundaries and [Roadmap](../roadmap/index.md) for active execution priorities.

## Contents

- [Models discovery interfaces](#models-discovery-interfaces)
- [Centralized agent authorization](#centralized-agent-authorization)
- [1C design: Messages citations](#1c-design-messages-citations)
- [Deferred tool discovery and web search](#deferred-tool-discovery-and-web-search)
- [Context compaction](#context-compaction)
- [Native batch jobs](#native-batch-jobs)
- [Realtime](#realtime)
- [Deferred cache prewarming](#deferred-cache-prewarming)
- [Model-name service-policy carriers](#model-name-service-policy-carriers)

The [stage-5 backlog](../roadmap/index.md#stage-5-backlog) has no implementation queue or delivery commitment. Native cache-only prewarming and model-name service-policy selection also retain separate scheduling decisions. These branches share no automatic approval or activation.

## Models discovery interfaces

The [model capability directory](../architecture/adaptation-and-dialects.md#generation-text-model-discovery) defines fact ownership, task-specific contracts, independent capacities, modalities and interface projection; [Usage](../usage/api-reference.md#generation-text-model-discovery) owns implemented endpoints and [OpenAPI](../openapi.json) owns machine schemas.

Enhanced `/v1/morphiecore/models` descriptors and Pi integration remain Generation Text-only. Embedding, Decision, independent Images/Audio/Video and Realtime descriptor-union schemas do not activate inference, bindings or client integration. Standard `/v1/models` already lists activated standalone-task labels, and `/v1/morphiecore/capabilities` publishes configured Embedding contracts. These startup-derived declarations are not live upstream probing or effectiveness certification. Further non-Generation descriptor/client integration requires selected operation contracts; no version/revision maintenance or pagination system is selected.

## Centralized agent authorization

The converged [Agent identity and cluster diagnostics baseline](../architecture/runtime-and-routing.md#agent-identity-and-cluster-diagnostics) owns the selected direction: one deployer, no business user/tenant model, credential-independent Agent attribution and sensitive diagnostics under mandatory authentication redaction. [Per-Agent JSONL sessions](../architecture/history-and-replay.md#agent-session-files) with explicit cross-Agent reading/copying are now a current-stage design direction, not an implemented store or selected migration. Do not reinterpret OAuth account identity as Agent identity or rebuild a separate user/ownership hierarchy here.

API-key selection uses the versioned private [Agent-key schema](../usage/gateway-configuration.md#agent-api-keys) first; OAuth remains deferred. Unresolved integration choices are the authorization-server product, verified OAuth authorization-instance binding to `agent_id`, managed registry persistence/management, client correlation carriers, token permissions and revocation policy, and diagnostic storage/export/retention. Rauthy remains an evaluation candidate, not a selected dependency; the [dated research](../refs/2026-10-23-downstream-oauth.md) owns versioned evidence and limitations. [Roadmap](../roadmap/index.md#agent-authorization-and-diagnostics-follow-up) owns the remaining dependency order and acceptance gates. This is not a stage-5 API assignment, OAuth implementation approval or change to current loopback ingress.

## 1C design: Messages citations

This is an unscheduled stage-5 proposal, not a prerequisite for stages 1–4. Citation-bearing requests/responses retain their existing rejection until the design is selected, scheduled and implemented. The selected document-context and point-anchor additions do not supply the missing citation enable flag, quote or request-document association below.

Counterexample: a request holds two PDF documents with `citations: {"enabled": true}`. The response cites `{"type":"page_location","document_index":1,"document_title":"B","cited_text":"…","start_page_number":2,"end_page_number":3}`, and the client returns that text block with its citation. The IR has page coordinates but cannot hold the per-document flag, the cited text or a reference to the second request document: `Citation.source` resolves only in its owner's resource table.

Protocol facts (pinned SDK; [Citations](https://platform.claude.com/docs/en/build-with-claude/citations) and [Search results](https://platform.claude.com/docs/en/build-with-claude/search-results), read 2026-10-13):

- `document_index` is 0-based over all document blocks in the request, across messages. `search_result_index` is 0-based over all `search_result` blocks, across messages and tool results.
- Character and block ranges are 0-based; pages are 1-based. All ends are exclusive.
- Citations are enabled on all or none of the documents, and likewise for search results. They are incompatible with structured output.
- Messages cites whole text blocks, splitting text at citation boundaries. `cited_text` is not billed.

Proposed 1C-a, documents only:

| Concept | IR change |
|---|---|
| Enable flag | `FileDescription.citations: bool` per use. Messages carries it; other targets reject `true` because it changes the output contract |
| Cited source | `Citation.source` becomes `CitationSource::Resource(ResourceId)` or `CitationSource::RequestDocument(ordinal)`, counting the request's file uses in order |
| Cited text | `Citation.quote: Option<Text>`, kept exactly |
| Location | Existing `SourceCoordinates` text, page and block ranges; the claim is `Whole` |
| Other targets | OpenAI has no request-document citation. Reject an incompatible target rather than drop required evidence |

Exit condition: one hand-written Messages oracle each way for a cited PDF, one streamed `citations_delta`, and an OpenAI target rejecting the flag. A bounded real call must confirm whether documents inside `tool_result` count toward `document_index`.

Later 1C-b: `search_result` input parts and `content` documents of text and image blocks; neither has a carrier outside Messages. `web_search_result_location` belongs to Provider web search. The alternative is to keep 1C rejected: pi never sends citations, and opencode sends them only from part metadata.

## Deferred tool discovery and web search

These belong to the unscheduled stage-5 backlog. Select IR ownership and execution/replay contracts and explicitly schedule a slice before implementation. Neither blocks stages 1–4, coverage or P7; stage placement is not a delivery commitment.

- **Tool discovery.** Complete `tool_search` and its call/output items, building on deferred-loading and namespace types. Ordinary `additional_tools` preservation is now separately selected in the [tool root repair](../roadmap/unscheduled-issues.md#conditional-stage-3-follow-ups), without activating discovery. Later tools must not be retroactively added to earlier history through top-level settings. Exit condition: discovery results keep order, definitions and references through decode, encode and replay. No gateway tool registry or search executor.
- **Native OpenAI `web_search`.** Complete the selected built-in path: declaration and controls, Provider-executed observations, reported sources and URL citations, streamed lifecycle and history, reusing observation and citation foundations. Exit condition: the Responses path preserves operations, results, citation associations and honest terminals in JSON and SSE; targets without a carrier reject. The gateway never fetches URLs or searches itself.

Unsupported discovery and search semantics stay rejected, not converted to text or ordinary function calls. These slices do not reopen generated media, file services, new bindings, Realtime or Embeddings.

## Context compaction

Stage-5 future work, currently unscheduled. Existing Generation compaction-item intake/replay is not deferred with this operation.

Use a distinct request and result. The result contains returned ordered context, resources, reported accounting and operation metadata. It is not a normal assistant response.

The official compact response can retain user messages followed by a compaction item. [Generation response validation](../../src/semantic/task/generation/validate.rs) excludes user/instruction messages from ordinary output. Do not weaken that validation to reuse `GenerationResponse`.

Reuse typed conversation content and [compacted-history replay](../architecture/history-and-replay.md#compacted-history). Preserve the opaque result's compatible deployment/model scope; incompatible replay rejects rather than strips required history.

Keep three operations separate: caller-supplied plaintext context editing, Provider-executed compaction, and automatic compaction scheduling. Adding `/responses/compact` does not select automatic triggers, mutate stored conversations, enable Provider-held continuation or introduce background Generation.

Exit evidence: independently specified compacted context in both directions, faithful subsequent input replay, incompatible-target rejection and honest failure handling.

## Native batch jobs

Stage-5 future work, currently unscheduled. Stage-4 native media jobs retain their own lifecycle requirements without implying this general Batch API.

| Method and path | Proposed operation |
|---|---|
| `POST /v1/batches` | Submit a batch |
| `GET /v1/batches` | List accessible batches |
| `GET /v1/batches/{batch_id}` | Retrieve state |
| `POST /v1/batches/{batch_id}/cancel` | Request cancellation |

Recommend Provider-native batch execution with durable gateway coordination. A gateway scheduler running synchronous tasks is a different product and remains an open alternative, not an interchangeable implementation detail.

Decode each JSONL record through an allowlisted operation codec into its task contract. Validate method/path, public model, resource references and `custom_id`; project only to a trusted compatible native batch target. Re-encode typed requests rather than forwarding arbitrary JSONL to a selected origin.

Define mixed-model/deployment admission explicitly. Do not silently split a batch across credentials or Providers, alter record identity, or turn unsupported records into a local execution loop. Stream bounded validation and result processing rather than loading the full batch into Generation memory.

Preserve per-record outcome, request/result association, job state and output/error file references. Native batch endpoint support is a separate capability: image operations appear in the current OpenAI contract, but that does not establish native batch support for audio operations.

Select the durable acceptance point and crash-recovery contract. A submission timeout may follow successful remote creation; persist enough private identity to reconcile before retrying. Do not claim exactly-once submission without upstream evidence.

After acceptance, a disconnected client does not cancel the job. Explicit cancellation distinguishes requested, confirmed and already-completed work. A cancelled or expired job may still have valid completed records; job failure must not fabricate record outcomes.

Exit evidence: accepted-job recovery, lost submission acknowledgement, partial results, identity mapping, explicit cancellation, output/error file access and no accidental resubmission.

## Realtime

Stage-5 future work, currently unscheduled. No concrete session API, transport or event contract is selected. Choose the client scenario, media directions, interruption/cancellation, reconnection, session/resource ownership and bounded delivery before implementation. Ordinary streamed Generation and stage-4 audio/video operations do not establish Realtime support.

## Deferred cache prewarming

Native cache-only prewarming is not a prerequisite for first-stage completion, coverage or P7. Existing cached-context references keep their separate contract.

The [pinned pi cache warmer](https://github.com/earendil-works/pi/blob/4270522473e677ddba7b093caa0064a3df51d459/packages/coding-agent/src/core/cache-warmer.ts) replays a real request with a one-token cap and keeps refresh usage outside history. That is ordinary generation owned by the client strategy, not native cache-only support; no gateway-owned warmer is selected.

When reopened, select the cache-only operation and result contract from pi's strategy and official native carriers. Exit condition: exact codecs preserve the operation, usage and reported cache facts, distinguish prewarming completion from output and reject targets without carriers. `max_tokens: 0` is not a zero-length answer; never replace explicit no-output prewarming with one-token generation.

## Model-name service-policy carriers

Service-policy carriers may be upstream model names rather than a dedicated field. A trusted, evidence-backed binding declaration must associate an alias with the complete policy it represents; never infer a tier from a suffix or accept a client-supplied upstream selector. A different model version, quantization or context contract is not automatically a tier of the same model. The current [endpoint contract](../../src/topology/endpoint.rs) fixes the upstream model at binding time; choosing predeclared variants during preparation is an integration design question, not a codec-only string rewrite.

When scheduled, add an independent alias-carrier oracle and an incompatible-policy rejection. Preparation must agree with preflight on the effective deployment, and cache/replay identity must follow the selected wire model. A selected alias proves request projection, not delivered service; record a reported tier only from an authoritative report.

