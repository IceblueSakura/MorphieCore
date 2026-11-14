# Client Request Guide

This guide covers sending requests to the three primary generation protocols (OpenAI Responses, OpenAI Chat Completions, Anthropic Messages), cache markers, tool calling, media input, stored responses, and cache affinity.

See [Gateway Configuration](gateway-configuration.md) for server setup and [API Reference](api-reference.md) for standalone endpoints and error handling.

## Contents

- [Send a request](#send-a-request)
  - [Responses](#responses)
  - [Chat Completions](#chat-completions)
  - [Messages](#messages)
  - [Shared request rules](#shared-request-rules)
  - [End-user attribution](#end-user-attribution)
  - [Messages profile and cache markers](#messages-profile-and-cache-markers)
  - [Chat custom tools](#chat-custom-tools)
  - [Unsupported controls and delivery](#unsupported-controls-and-delivery)
  - [Image input](#image-input)
- [Stored Responses](#stored-responses)
- [Conversation and cache affinity](#conversation-and-cache-affinity)

## Send a request

Set the SDK `base_url` to `http://127.0.0.1:8080/v1` and `api_key` to the Agent's `key` from [auth.json](gateway-configuration.md#agent-api-keys). Anthropic clients use the bare origin, `http://127.0.0.1:8080`.

Send JSON with `Content-Type: application/json` and exactly one credential header:

- `Authorization: Bearer <key>`, or
- `x-api-key: <key>`.

Both forms work at ingress. Repeated headers or both forms together are rejected, even if they carry the same token.

### Responses

```json
{"model":"configured-public-model","input":"Reply with exactly pong.","max_output_tokens":64}
```

When an admitted request includes `metadata`, Responses delivery can echo it if the upstream response omits that field. An upstream explicit null or object wins; maps are not merged. This is a request association, not proof of Provider storage or effective configuration. Send it again when needed on a later turn; stored history does not inherit it. See [delivery provenance and precedence](../architecture/adaptation-and-dialects.md#downstream-schema-completion).

Stateless Responses requires explicit `store:false`, no nonnull `previous_response_id`, and complete returned standard history. Google ordinary text/call signatures without a standard carrier reject; see the [field-level reasons](../architecture/history-and-replay.md#stateless-api-boundaries). No replay profile or client-specific extension is offered.

### Chat Completions

```json
{"model":"configured-public-model","messages":[{"role":"user","content":"Reply with exactly pong."}],"max_completion_tokens":64}
```

For pi's DeepSeek gateway profile, merge the provider entry from
[`pi-deepseek.example.json`](../../config/pi-deepseek.example.json) into test-owned or
operator-managed pi `models.json`. Set the gateway origin and resolve the Agent
key through the client's environment. The explicit client model declaration uses
MorphieCore's public `deepseek-v4.1-flash-1017` ID. The dynamic MorphieCore provider
can instead discover this identity and its route-common capacities automatically.

The declaration carries the native service's sourced context/output maxima and effort levels.
Both `compat.thinkingFormat:"openai"` and `thinkingLevelMap.off:"none"` are
required: pi then sends standard `reasoning_effort:"none"` for off, without
the native `thinking` extension discarded by public Chat intake. DeepSeek's
upstream adapter separately chooses native `max_tokens`. Workstation extensions
that generate model declarations should apply the same profile; installing or
activating such an extension remains an operator action.

For required-history tool calls through Standard Chat, explicitly select thinking
off in pi (`--thinking off`). Loading the profile alone does not change pi's default
thinking level. Active/default thinking returns `reasoning_history_required`;
MorphieCore does not switch it off on the runtime's behalf.

### Messages

Send exactly one `anthropic-version: 2023-06-01` or `anthropic-version: 2026-10-21` header. Both use the current body grammar. Missing, duplicate, combined and unsupported versions, as well as `anthropic-user-profile-id` and `anthropic-workspace-id` headers, return 400 before body collection. `anthropic-beta` headers are ignored; supported body fields and trusted upstream beta selection remain independent. Legacy `temperature`/`top_p` values in `[0,1]` and mathematically integral `top_k` in `[0,18446744073709551615]` are validated exactly, then omitted with named evidence. Equivalent decimal/exponent spellings such as `top_k:1.0` and `top_k:1e0` are accepted; no floating-point rounding can admit an out-of-range value. Null sampling defaults normalize without a loss receipt; malformed values still reject.

Completed thinking history must retain its nonempty native signature, and redacted thinking must retain its opaque data. Bare or damaged continuation cannot be authorized by deleting it; the gateway requires its existing issuer-bound envelope. A completed empty signature rejects. The historical cross-protocol unsigned [delivery projection](../architecture/adaptation-and-dialects.md#response-rules) is not a supported standard Messages continuation path; do not submit it as signed history, invent a signature or replace it with answer text.

Current installed pi and OpenCode Messages defaults need no version/beta header overrides. The isolated Go/Haiku gates preserve built-in model metadata and default thinking, changing only the test endpoint/key and operational output cap. They exercise actual read execution and signed-history continuation; see [verification](../development.md#installed-client-acceptance). `allowEmptySignature:true` does not establish a valid loop; return genuinely signed native history.

```json
{"model":"configured-public-model","messages":[{"role":"user","content":"Reply with exactly pong."}],"max_tokens":64}
```

### Shared request rules

Use `"stream": true` for SSE. Send the full history each turn, or use [stored Responses](#stored-responses). Chat Completions and Messages remain full-history APIs.

Instruction content is plain text only: request-wide instructions, Messages `system`, instruction text in mid-conversation Messages system messages, and Chat/Responses system or developer messages cannot contain images, audio, video or files. Media-bearing instructions reject at decoding rather than being dropped or moved to a user message. The same restriction applies to reported instruction echoes from an upstream.

Later system or developer instructions keep their position. Chat and Responses targets carry them natively; Messages uses mid-conversation system messages subject to native placement rules; Google targets reject them.

Standard Chat has no private reasoning/replay carrier. Uncarried reasoning owners are omitted from JSON, SSE and retained history under the [selected delivery rule](../architecture/history-and-replay.md#standard-chat-reasoning); visible text, tools, usage and source observations are preserved. Standard `reasoning_effort` controls computation, not reasoning-text visibility; usage reasoning counters do not imply readable output. Unregistered Client fields are discarded without IR retention under the [semantic boundary](../architecture/adaptation-and-dialects.md#field-handling). Public Chat does not activate private `enable_thinking`/`thinking_budget`/`top_k`; malformed known fields still reject.

Return standard Responses reasoning and Messages thinking/redacted-thinking carriers unchanged. Chat has no private reasoning-history transport. Provider-specific fields are handled only at the trusted upstream boundary.

When changing protocols or Providers, reasoning history may be omitted even if the model name stays the same; ordinary text and tool/result relationships remain intact. Changing only models within one trusted Chat/Responses service source preserves carriable native OpenAI reasoning for upstream validation. This does not promise cross-model reuse, widen compaction compatibility or trigger an automatic retry on rejection. Do not convert discarded reasoning into assistant answer text. See the [cross-source history contract](../architecture/history-and-replay.md#cross-protocol-and-cross-provider-history), including handling of older model-scoped reasoning envelopes.

Assessed Bailian Responses candidates keep summary-only history and apply `responses.transfer-visible-history.v1` to reasoning owners without a target carrier. This candidate-side projection preserves visible history and tool links without turning reasoning into assistant text. Explicit historical read intent cannot be fulfilled by omission; unassessed bindings retain their prior behavior.

Bailian Responses request controls use the selected [presentation/continuity policy](../architecture/adaptation-and-dialects.md#clientir-projection-principles): summary `auto` resolves to the native default; explicit `concise`/`detailed` and encrypted include take named omissions where no carrier exists. An admitted explicit thinking-off request needs no encrypted-include loss receipt.

DashScope and TokenPlan share the [verified-only function-strictness policy](../architecture/adaptation-and-dialects.md#shared-aliyun-billing-dialect) on Chat and Responses. Unverified model/API pairs accept `strict:true` as best-effort under `bailian.relax-function-strictness.v1`, retaining the supplied schema unchanged. The current verified set is empty, so neither plan promises strict-function enforcement. Validate arguments before tool execution. Structured-text `strict`, required selection, seriality and thinking controls remain independent. Other Providers retain ordinary strict admission and conditional-default handling.

Standard Chat tool requests with active/default thinking reject on declared `required_with_tools` bindings before dispatch. The gateway never disables thinking or clears effort/budget to make them work. If this blocks route planning, HTTP 400 returns `reasoning_history_required`; the runtime may explicitly submit `reasoning_effort:"none"` or choose a compatible model/API. Targets without a thinking-off carrier still reject an explicit off request. Unassessed history contracts delegate available standard history to upstream validation. See the [binding-history contract](../architecture/history-and-replay.md#binding-reasoning-history-contract).

Responses Client-history `output_text` may omit an empty `annotations` collection under the selected [L2 compatibility rule](../architecture/adaptation-and-dialects.md#semantic-equivalence-compatibility); explicit arrays are preserved and malformed values, including null, reject. Complete output-message history still needs its `type`, `id`, `role`, `content` and `status`; those fields are not inferred. Upstream response/event snapshots require the official annotations array. Official assistant string shorthand remains valid. SDK-derived `parsed`/`parsed_arguments` and Chat response-only annotations are unregistered Client fields: ignored, never authoritative over raw text/arguments. Chat audio history is an `id` reference, not a generated-audio response object. Standard input rejects nonstandard video content discriminators; declaring Provider dialects retain their separate carriers.

Chat accepts the deprecated `max_tokens` as the output cap, but not together with `max_completion_tokens`. An absent or null cap remains unspecified at Chat intake; bootstrap imposes no global default or token ceiling. Messages targets fill absence only from their configured `default_output_tokens`, without a loss receipt; without one they reject. Native Messages intake still requires a positive `max_tokens`. Targets without ceiling carriers omit caps under named rules, while known model maxima clamp excessive caps. Chat and Responses codecs carry `verbosity`; native Messages and Google projections use selected, recorded omission rules. A codec carrier does not establish model-effective support. Chat `stop` and Messages `stop_sequences` decode into ordered stop controls; an explicit stop still rejects on a target without its carrier.

### End-user attribution

OpenAI Chat/Responses accept independent `safety_identifier` (up to 128 characters) and deprecated `user` (up to 256); native Messages accepts `metadata.user_id` (up to 512). Native OpenAI paths, including Chat↔Responses, retain both fields independently. They never automatically modernize `user` into safety/cache fields.

Cross-protocol targets with one attribution carrier prefer an explicit safety identifier, otherwise general user attribution. Messages uses `metadata.user_id`; OpenAI uses `safety_identifier`, not legacy `user`; generateContent uses only `labels.safety_identifier`. Google values allow at most 63 Unicode codepoints of lowercase/uncased letters, numeric characters, underscores and dashes. No generic label/metadata map or Interactions carrier is selected.

Documented null defaults normalize to absence; an explicit empty string remains explicit and prevents fallback. Google omits an identifier that its label grammar cannot carry, with a named receipt and without selecting another ID. Other native field bounds remain strict; for example, a 129-character native Messages ID cannot project to OpenAI's 128-character safety carrier. No projection truncates, lowercases or hashes attribution.

`prompt_cache_key` remains independent. A projection can lose identity under `generation.omit-identity-hints.v1` and separately lose OpenAI legacy `user` cache bucketing under `cache.omit-legacy-user-bucketing.v1`; neither receipt includes identifier values. No identity is derived from credentials, prompts or conversation IDs. See the [attribution contract](../architecture/adaptation-and-dialects.md#end-user-attribution).

### Messages profile and cache markers

In root and by-value ordinary tool definitions, `eager_input_streaming: null` or omission keeps the target default; explicit `false` disables eager input and remains distinct from omission. Tool and supported content-wrapper `cache_control: null` or omission creates no breakpoint. These defaults do not enable new beta headers or relax strict validation of other known fields: supplied `tool_result.is_error`, `tool_choice.disable_parallel_tool_use` and `output_config.effort`/`format` nulls reject.

On native Messages candidates, tool definitions, system blocks and supported conversation blocks retain their own supported cache markers, including a `tool_result` wrapper. `{"type":"ephemeral"}` and explicit `ttl:"5m"` mean five minutes; `ttl:"1h"` means one hour. At most four explicit breakpoints are allowed, ordered with longer TTLs before shorter ones. Native projection does not move markers, shorten TTLs or add an automatic policy beside explicit markers. Marker-free Go/Haiku requests retain the existing automatic five-minute default. Other candidates omit cache controls without carriers under named rules at their original owners; see [owner-local omission](../architecture/adaptation-and-dialects.md#owner-local-cache-admission) and [automatic policy](../architecture/adaptation-and-dialects.md#caller-selected-automatic-cache-admission). Cache hits and deployed proxy TTL behavior are not certified by admission.

Public Messages intake accepts ordered `tool_addition`/`tool_removal` blocks only in valid mid-conversation system positions. Additions carry a declared reference or an inline function definition; removals carry a reference. Candidate dispositions retain native changes, collapse them under the existing named rule where admitted, or reject history whose model-visible examples cannot survive collapse. Known tool names, schemas, cache conflicts, call/result links and resource bounds remain strict.

System messages may set `"output_config":{"effort":"medium"}` with empty or omitted content. Keep root effort unchanged and replay historical markers in their original positions. A marker after new user input selects that reply; a marker after tool-result-only feedback waits for the next new-input turn. Native candidates preserve system boundaries; other candidates use the [ordered effort projection contract](../architecture/adaptation-and-dialects.md#ordered-effort-projection). Local controls on user/assistant messages, null local configuration/effort, local format/task budgets and `clear_at` remain unsupported.

Adaptive `thinking.block_binding` accepts `null`/`{}` exactly and omits `prefix_mismatch_behavior:"drop_block"` under `messages.omit-block-binding.v1`; this never bypasses gateway signature/origin or historical-prefix checks. Explicit `"error"` rejects because public intake cannot promise that enforcement.

### Chat custom tools

For custom-capable targets, Chat JSON uses `{"type":"custom","custom":{"name":"sql","format":{"type":"text"}}}` definitions and `{"type":"custom","custom":{"name":"sql"}}` choices. Calls retain raw `custom.input`; return their ID through an ordinary `role:"tool"` message. Preserve the preceding typed call in history so the result cannot be mistaken for a function result. Grammar definitions use the nested carrier described in [protocol sources](../refs/2026-10-16-chat-custom-tools.md).

Official Chat SSE sources disagree: Node defines custom-call deltas, while the inspected Python types and public streaming reference remain function-only. MorphieCore therefore keeps Chat upstream streaming that enables custom selection and downstream custom-call deltas rejected; use an admitted Responses streaming path instead. Custom history and declarations remain usable for a later stream when tool choice excludes custom output. The [source-qualified finding](../refs/2026-10-27-client-ir-boundaries.md#custom-tools-source-disagreement-versus-a-real-carrier-gap) is not a claim that a carrier is intrinsically impossible. Codec support does not activate models or execute tools.

### Unsupported controls and delivery

Following the [Client semantic boundary](../architecture/adaptation-and-dialects.md#field-handling):
- **Unknown extension fields** in client requests are silently ignored without entering semantic IR or forwarding to Providers.
- **Known controls** follow candidate-local [conversion rules](../architecture/adaptation-and-dialects.md#conversion-ladder); unscheduled capabilities still reject at intake with HTTP 400. Key examples:
  - Chat: `store: true`, `moderation`, `function_call`, `web_search_options`.
  - Messages: non-null `clear_at`, `container`, non-empty `mcp_servers` (`[]` is admitted as absent), `context_management`, `compaction`, `inference_geo`, `speed`, `fallbacks`, `fallback_credit_token`, `diagnostics`, and present `output_config.task_budget`.
- **Malformed fields** (e.g. invalid types or null values where non-null is required) reject immediately.
- **Tool results:** Results associate with their calls by call ID; grouped calls followed by their results are accepted without reordering ([association contract](../architecture/adaptation-and-dialects.md#tool-result-association)). Messages keeps its official rule that results appear in the user message immediately after the calls. In manual thinking mode, forced tool choice is rejected.

### Image input

Select an activated public model that admits image input. A tool-result image also needs a representable tool-media carrier on the selected upstream.

| Client protocol | User image | Tool-result image |
|---|---|---|
| Chat Completions | `image_url` | No tool-message carrier; clients may send a separate user image themselves |
| Responses | `input_image` | `function_call_output.output` array |
| Messages | `image` with a base64 or URL source | `tool_result.content` array |

Responses tool images require a Responses or native upstream carrier. All Generation client protocols use the model's configured candidates, so a tool image still fails on a Chat upstream. The gateway does not move tool media to another role, because standard Chat has no tool-media carrier.

For an inline Responses image, replace `<base64-image>` with valid base64 for the declared image type:

```json
{
  "model": "configured-public-model",
  "input": [{
    "role": "user",
    "content": [
      {"type": "input_text", "text": "Describe this image briefly."},
      {"type": "input_image", "image_url": "data:image/png;base64,<base64-image>", "detail": "auto"}
    ]
  }],
  "max_output_tokens": 256
}
```

For a tool image, append a `function_call_output` with the returned `call_id` and an `output` array containing `input_image`. Return the complete earlier history, including reported thinking and opaque replay values. Do not fabricate a tool call to substitute for model-issued history.

Image sources stay inert: the gateway does not fetch URLs, decode pixels, resize or transcode. Public URL availability and actual image validity remain upstream concerns. Client Provider file references are rejected, including Google Files API URLs disguised as ordinary image URLs. File services remain deferred.

Formats, encoded sizes and counts are target constraints, not a promise that every IR image fits every upstream. Preserve explicit per-use `detail` through an exact carrier; omission requires proven target-default equivalence, and other approximations require an evidence-backed selected rule. Without one, an unrepresentable detail rejects rather than disappearing. The example's explicit `auto` is not equivalent to absence and is not universally portable to native targets. Any approved projection changes only the candidate copy, not the source request. See [Media](../architecture/adaptation-and-dialects.md#media), [IR resource budgets](../../src/semantic/task/generation/resources/mod.rs) and [Messages media limits](../../src/protocol/anthropic/messages/media.rs).

## Stored Responses

Storage defaults to true, matching OpenAI Responses. Omitted or null `store` retains a Responses exchange in gateway memory; set `store: false` to opt out. Upstream requests always carry complete history and disable Provider storage where supported.

1. Create a response with `store` omitted or set to `true` and consume the full JSON body or SSE stream. A gateway-owned response ID becomes readable only after validated upstream EOF and final body handoff.
2. Send that ID as `previous_response_id` with only the new input. The gateway prepends the stored request input and response output. Send current instructions, tools and controls explicitly: top-level settings are not inherited, while instruction items in history remain.
3. The successor is also stored by default. Set `store: false` if it need not be retrievable or usable as another predecessor; it may still read an existing predecessor.

```json
{"model":"configured-public-model","input":"Remember the label cedar.","store":true,"max_output_tokens":64}
```

Use the returned `id` in the next request:

```json
{"model":"configured-public-model","previous_response_id":"<returned-id>","input":"What was the label?","store":true,"max_output_tokens":64}
```

For tool continuation, send only the new `function_call_output` or other admitted result. The stored call and reasoning remain in history. Existing carrier and replay checks still apply; a response reference does not bypass model admission or grant cross-Provider replay.

Signed Google ordinary text/calls require explicit managed Responses continuation. Their hidden signatures stay in typed stored history; use `previous_response_id`. Copying creation/retrieval/input-list JSON cannot export those hidden attachments. `store:false` can read a predecessor, but a newly returned ordinary signature rejects because no lawful stateless carrier exists. Native reasoning and compaction keep their separate standard replay rules.

The existing Google bindings accept the [qualified strict function subset](../architecture/adaptation-and-dialects.md#google-strict-function-arguments). Repeat the tool declaration and controls on each managed request. For example, create with `store:true`, `tool_choice:"required"` and a strict function whose parameters are `{"type":"object","properties":{"label":{"type":"string","enum":["cedar"]}},"required":["label"],"additionalProperties":false}`. Continue with the returned `id`, a `function_call_output` for every delivered call, and `tool_choice:"none"` when no further calls are wanted. Do not add `parallel_tool_calls:false` or an OpenAI named forced choice: those request a single-call guarantee Google cannot carry. Unsupported strict constraints still reject; this is not unrestricted JSON Schema support.

| Operation | Behavior |
|---|---|
| `GET /v1/responses/{response_id}` | Retrieve the final JSON snapshot, including responses created with SSE |
| `DELETE /v1/responses/{response_id}` | Return `{"id":"…","object":"response.deleted","deleted":true}` |
| `GET /v1/responses/{response_id}/input_items` | List the complete resolved input, excluding this response's output; `limit` is 1–100 (default 20), `order` is `asc` or `desc` (default `desc`), and `after` is a returned item ID |

All operations require a configured Agent key and retain shared access among admitted Agents. Unknown, expired, deleted and unstored IDs return `404` with `response_not_found`. Concurrent requests may branch from one predecessor; they do not mutate it. Deleting an ancestor leaves already stored descendants usable.

Snapshots use a fixed TTL from final delivery, not a sliding TTL. Defaults are 30 minutes, 1,024 entries and a 256 MiB accounted payload/owner budget; transient request and body limits still apply. See [Limits](../../src/gateway/startup/limits.rs) for embedded configuration and bounds. Capacity exhaustion rejects storage with `history_capacity_exceeded` before a successful terminal; an already started SSE body aborts. Unexpired snapshots are not evicted to make room.

History is process-local and is lost at shutdown or restart. A declared conversation header still controls routing/cache affinity independently; response IDs do not select credentials. There is no disk persistence, Conversations API, background generation or event log. Retrieval is JSON only: `stream=true` and `starting_after` are rejected. Retrieval and input listing accept only `include[]=reasoning.encrypted_content` as an optional include; other query fields are rejected.

## Conversation and cache affinity

Use `X-MorphieCore-Conversation-Id` for one logical conversation. Keep it stable across turns and model changes. Use a new ID for a new conversation.

The ID must be 1–256 visible ASCII characters, without commas, in one header. If it is absent, ingress checks these agent headers in order:

1. opencode: `X-Session-Id`
2. `session_id`
3. pi Responses / Codex: `session-id`
4. pi Chat/Messages: `x-session-affinity`
5. OpenCode native affinity: `x-opencode-session`
6. OpenCode native identity: `x-opencode-session-id`

For pi's ordinary API-key Chat and Messages integrations, enable `compat.sendSessionAffinityHeaders: true`; automatic session headers also require `cacheRetention` other than `none`. Responses normally sends `session-id` without that compatibility flag. Native OpenCode affinity takes precedence over its session identity because child/forked sessions can intentionally share an affinity. An invalid selected header rejects rather than falling through. `x-client-request-id` is not a conversation source.

This ID groups routing and cache affinity; it does not store history. Body `session_id` is not a standard downstream conversation carrier and is discarded as an unknown extension. Standard `prompt_cache_key` is a separate cache-grouping intent. Without a declared conversation, the optional prefix index offers best-effort cache reuse, not executor affinity or replay permission. [Upstream session projection](../architecture/runtime-and-routing.md#upstream-session-projection) owns SIWC, Go and OpenRouter carrier selection; clients need not send Provider-specific headers.

