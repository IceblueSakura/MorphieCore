# Roadmap

The single source for direction and the current slice. Delete finished items (Git keeps history). Principles and settled decisions live in [architecture](architecture.md); sources in [references](references.md).

## Frozen scope

Maintenance only, as refactors require:

- Standalone media and vector tasks: Images, Speech, Transcription, Embedding.
- New Providers or model bindings, except during the Google phases. Activating an existing binding is not a new binding.
- New features for subscription credentials (SIWC, Grok).
- Stateful Responses, file services, Realtime.

Not planned: raw wire passthrough or post-encoding JSON patches, weakening schemas (`oneOf` → `anyOf`), synthesizing missing tool results or re-roling orphan results, first-message hashes as conversation identity, LLM intent classifiers for routing, plugin or middleware hosts, capabilities learned automatically from upstream errors.

## Current focus: Phase 1 — protocol boundaries and product bindings

Goal: OpenAI clients, Codex first, use the native Messages binding (activated by listing it in `models`), and the OpenAI boundaries accept known client and vendor shapes, with no silent loss of reasoning, tool or terminal semantics.

Items in implementation order:

1. **Native request mapping.** Codex fails on the binding today: the OpenCode Go dialect rejects every reasoning control, and Codex sends `reasoning.effort`, `reasoning.summary` when its model metadata allows, and `reasoning.context`.
   - Effort → `output_config.effort` (Haiku 5.5: `low`–`max`, default `medium`); `minimal` and `none` need named rules.
   - A requested summary → `display: "summarized"`; otherwise send `display: "omitted"` explicitly (the 5.x default), keeping the configuration stable for the cache.
   - `reasoning.context`, and `temperature`/`top_p` on targets that reject non-default values (all 5.x models), are dropped by named degradable rules.
   - Tool call IDs project deterministically and injectively into `^[a-zA-Z0-9_-]{1,64}$`; conforming IDs pass unchanged.
2. **Native stop reasons.** Intake accepts only `end_turn`, `tool_use` and `max_tokens`. `stop_sequence` completes; `model_context_window_exceeded` is truncation; `refusal` (HTTP 200 with `stop_details`) is an incomplete answer with reason `content_filter`, and output before it is not complete; `pause_turn` needs server tools and stays unsupported.
3. **Cross-target thinking.** A mismatched target rejects readable reasoning today. Pass it as plain assistant text in every turn, without delimiters (as pi does); drop opaque-only reasoning; record both as losses. Converting every turn keeps the projection append-only (architecture §4). Adaptive thinking does not require a turn to start with a thinking block, and a mid-turn mismatch disables thinking for that request instead of failing.
4. **Messages prompt caching.** Native requests carry no `cache_control`. Send the top-level `cache_control: {"type": "ephemeral"}` so the API places and advances the breakpoint; place breakpoints by hand only if a relay rejects the field. 5.x models cache from 512 tokens; changing thinking or top-level effort restarts the cache.
5. **OpenAI stream tolerance.** Responses intake rejects unregistered event types; skip and record them instead, as Codex does, while terminals and content events stay strict. Chat streams fail on known vendor shapes: a continuation `function.name: ""`, `finish_reason: ""`, `tool_calls: []` (opens the message owner early), and `finish_reason: "stop"` beside tool calls. Each becomes a named dialect rule, like `empty_continuation_call_id`, only when a bound upstream emits it. Frozen media decoders are not extended.
6. **Chat reasoning carriers.** Model the `signature` of Chat `reasoning.text` details (rejected today) through the envelope. Carry multiple thinking blocks and interleaved thinking/text/tool order where the Chat shape allows it. Stop emitting the empty `reasoning_content: ""` opener; consumers treat each reasoning delta as a block boundary.
7. **Downstream unknown fields.** Ignore unregistered request fields at any depth and record their names; keep rejecting unknown `type`/`role` discriminators and keep known fields strict. Update `docs/openapi.json`, which closes request objects today.

Further trimming of diagnostics/billing switches and fidelity records is assessed with the affected modules, not as a prerequisite. `morphiecore-auth` stays independent.

Exit: `Responses / Chat × Anthropic`, JSON and SSE, covers text, tool round trips, tool failures, thinking replay, a mid-loop Provider switch, refusals, truncation, upstream errors, ignored request fields and the cache field, with synthetic tests. Bounded real calls through the CLI-started gateway (opencode-go `claude-haiku-5-5` or another available target): one Codex-shaped request with reasoning controls, then a second turn on the same prefix. The relay accepting the reasoning and cache fields is an exit check; a reported cache read is an observation.

## Later phases

### Phase 2 — held streams and scenario fixtures

Follow the structure of Magpie's held-stream design; its timings come from client reports and are starting points. Today the first upstream lifecycle event (`response.created`, the Chat role chunk, `message_start`) is published at once and freezes the candidate, so an upstream that opens a stream and then reports overload cannot fall back.

- **Hold until content**, only while another candidate could still answer. Lifecycle-only frames are held; content is text, a tool call, reasoning or a terminal. A pre-content upstream error advances; the first content releases the held frames and freezes the candidate.
- **Refusals** are delivered by default. A route may opt in to retrying a pre-content refusal on a different upstream model, never the same model on any credential. Reasoning is never held for this, so a refusal after content is an incomplete answer.
- **Bounds.** A hold ends by releasing, never dropping: at first content, a byte cap or a time cap. The time cap is longer when the client can be kept alive. SSE comments do not reset Codex's 300 s idle timeout, so the total hold across candidates stays below it.
- **Keep-alive.** Past a threshold, send headers and `: keepalive` comments; the next candidate continues in the same response. After headers, a final failure is the protocol's stream error: Responses sends a gateway `response.created` if none went out, then `response.failed` with codes Codex classifies (`server_is_overloaded`, `rate_limit_exceeded`, `insufficient_quota`, `context_length_exceeded`); Chat sends one `data: {"error": …}` chunk and closes without `[DONE]`. Quiet streams after content get comments up to a limit; waits before sending are kept alive the same way.
- **Non-API 2xx** (HTML, empty, or non-JSON under a JSON type) is a 5xx-class failure decided from the first bytes; cooldown, when Phase 3 lands, is scoped to the Endpoint. Diagnostics name the reason without copying the body.
- **First-content timeout**, opt-in per route: cancel and advance only if another candidate remains and nothing was published. Sequential, not hedged. Diagnostics record time to first content and to first text.
- **Scenario fixtures.** One hand-written wire fixture per scenario per protocol, checked wire→IR and IR→wire against a synthetic upstream; pair-level tests only for pair-specific projection rules ([test rules](../AGENTS.md#tests)). Each SSE fixture is also fed in small fixed-size chunks. Scenarios:
  - Text, single and parallel tools, tool errors, reasoning replay, usage, truncation, upstream errors.
  - Tool arguments: deltas equal to done; done without deltas; done extending the deltas (emit the suffix); conflicting done (reject).
  - Held delivery, after Magpie's tests: creation then overload advances with one visible creation (`TestSilentHeldStreamStillFailsOver`); a held stream is kept alive and answered (`TestSilentHeldStreamKeptAlive`); comments stop after the quiet limit (`TestQuietKeptAliveOnlySoLong`); a slow first content advances (`TestGroupFirstTokenFailsOver`); a non-API 2xx advances (`TestNotAnAPIReplyFailsOver`); an error after content aborts with no splice; refusals follow the rule above; post-commit transport loss.
  - Consumer shapes: a tool name only on the first fragment; no empty reasoning or tool-call deltas; null, empty and absent stay distinct.

Exit: every scenario has an independent expectation per protocol, held delivery passes with synthetic upstreams, and the SDK loopback gate passes with keep-alive comments.

### Phase 3 — routing: failure scope, cooldown and target affinity

The compiled route stays the authority on legal targets; this phase adds trusted runtime ordering inside that set (architecture §5).

- **Execution identity**: Endpoint × credential identity × upstream model. Credential identity is the binding plus an in-memory API key fingerprint (never logged), or the verified OAuth principal. Cooldown, affinity and diagnostics key on it, never on ordinals or display names.
- **Failure scope and cooldown.** Separate short rate limits, quota windows, missing credit and authentication, including errors inside a held stream; scope each to the credential or credential × model. Cooldown demotes within the legal set and expires (Retry-After or reset, capped). Hard exclusions (disabled, unauthorized, unsupported by the request) are never revived. Refusals cause no cooldown. Unknown-scope 429s still do not rotate credentials.
- **Target affinity**, opt-in per route, separate from the prefix cache index. Keyed by ingress principal, public model and conversation ID; bound to the exact executor. Ordered checks: executor no longer legal → drop; cooling down → skip; tool round trip (read from the IR) → keep the actual executor, even after a fallback; fell back after a refusal → keep the fallback model; new turn → keep only with a recent reported cache read, else base order. Feedback only from honest completed terminals; failure or abort clears the binding. In memory, bounded, disposable.
- **Diagnostics**: inferred or explicit cache hint, legal candidates and exclusion reasons, the affinity decision, each physical attempt with its executor. Missing cache reports stay missing.

Exit: synthetic multi-candidate routes show the executor kept across a tool round trip after fallback; a refusal fallback staying on its model; removed or disabled targets not revived; no binding reuse after a credential change; no sharing across ingress keys for one conversation ID; state released on concurrent first requests and cancellation; compiled order kept without affinity.

Deferred until a real need: turn-start routing rules (token, image, effort conditions held across tool round trips), per-credential concurrency and RPM lanes counting physical attempts, budget reservations, per-message effort to keep the cache when clients change effort mid-conversation.

### Phase 4 — Google generateContent upstream

IR questions to settle first:

- `thoughtSignature` attaches to parts (including `functionCall`); confirm `Part.replay` and `CallContext.replay` carry it directly.
- `functionCall.id` may be missing: the gateway must mint call IDs and map back by name and order. Check whether `NativeCallAlias` suffices; if not, report the IR gap with options first.
- Streaming returns `GenerateContentResponse` fragments, close to Chat chunks.
- More `finishReason` values (SAFETY, MALFORMED_FUNCTION_CALL, …) and usage details (`thoughtsTokenCount`, `cachedContentTokenCount`).
- Schemas use the OpenAPI subset or `parametersJsonSchema`.
- Keep-alive comments: Magpie found a Gemini client that rejects them; recheck before enabling them for a Gemini downstream.

### Phase 5 — Google Interactions upstream

Submit full history statelessly, without server-side storage; reuse the opaque envelope. Pin the official API version before starting.

### Phase 6 — downstream Anthropic Messages

Reuse the Anthropic module from Phase 1; add request decoding and response/SSE encoding. Native `is_error`, thinking signatures and block order map directly. Toward OpenAI targets, keep tool result text and drop `is_error` by a named lossy rule; Chat/Responses get no tool error extension until a client needs one.

### After that

Stateful Responses (gateway-managed, time-limited history), file services, resumed media work, Realtime.
