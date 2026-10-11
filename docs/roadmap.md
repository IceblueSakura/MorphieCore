# Roadmap

The single source for direction and the current slice. Delete finished items (Git keeps history). Principles live in [architecture](architecture.md).

## Frozen scope

Maintenance only, as refactors require:

- Standalone media and vector tasks: Images, Speech, Transcription, Embedding.
- New Providers or model bindings, except during the Google phases. Activating an existing binding is not a new binding.
- New features for subscription credentials (SIWC, Grok).
- Stateful Responses, file services, Realtime.

Not planned: raw wire passthrough or post-encoding JSON patches, weakening schemas (`oneOf` → `anyOf`), synthesizing missing tool results or re-roling orphan results, first-message hashes as conversation identity, LLM intent classifiers for routing, plugin or middleware hosts, capabilities learned automatically from upstream errors.

## Current focus: Phase 1 — protocol boundaries and product bindings

Goal: OpenAI clients reach a native Messages upstream through the normal CLI startup, with no silent loss of reasoning, tool or terminal semantics.

1. **Upstream stream tolerance.** Responses intake rejects any unregistered event `type`. Skip and record unknown event types instead; Codex ignores them (`codex-api/src/sse/responses.rs`) and the OpenAI SDK passes them through without failing; terminals and content events stay registered and strict, so a skipped event can never stand in for a terminal. Known vendor shapes in Chat streams fail today: a continuation `function.name: ""` (rejected as a changed identity), `finish_reason: ""` (unsupported), `tool_calls: []` (opens the message owner early), and `finish_reason: "stop"` beside tool calls from relays (unsupported; Magpie reads it as a tool turn). Each becomes a named dialect rule, mirroring `empty_continuation_call_id`, only when a bound upstream emits it. Native Messages accepts only `end_turn`, `tool_use` and `max_tokens`. Map the rest: `stop_sequence` completes; `model_context_window_exceeded` is truncation; `refusal` (HTTP 200 with `stop_details.category`) is an incomplete answer with reason `content_filter`, and partial output before it is not complete; `pause_turn` needs server tools and stays unsupported. Frozen media decoders are not extended.
2. **Activate the native Messages binding.** `MESSAGES_BINDINGS` compiles only an explicit topology, and directory bootstrap rejects the `AnthropicMessages` protocol. Activate it only when listed in `models`, never by default. Verify model release metadata first; never invent dates.
3. **Native request mapping.** OpenAI clients fail on the native binding today: any reasoning control is rejected as unverified on the OpenCode Go dialect, and Codex sends `reasoning.effort` (configured or the model default), `reasoning.summary` when its model metadata allows, and `reasoning.context` for Responses Lite (`core/src/client.rs`, `build_reasoning`). `reasoning.context` has no native meaning and is dropped by a named rule. Map effort to `output_config.effort` (Haiku 5.5 takes `low` through `max`, default `medium`; `minimal` and `none` need named rules), a requested summary to `display: "summarized"` and otherwise send `display: "omitted"` explicitly, since 5.x models default to omitted and a stable configuration keeps the cache. Drop `temperature`/`top_p` by a named degradable rule on targets that reject non-default values (all 5.x models). Project tool call IDs into `^[a-zA-Z0-9_-]{1,64}$` deterministically and injectively; conforming IDs pass unchanged (Kimi-style `functions.bash:0` does not conform).
4. **Chat reasoning carriers.** Model the `signature` of Chat `reasoning.text` details (currently rejected) through the envelope. Carry multiple thinking blocks and interleaved thinking/text/tool order instead of rejecting them where the Chat shape allows it. Stop emitting the empty `reasoning_content: ""` opener downstream; consumers treat each reasoning delta as a block boundary.
5. **Downstream unknown fields.** Ignore unregistered request fields at any depth and record their names in diagnostics; keep rejecting unknown `type`/`role` discriminators and keep known fields strict. Update `docs/openapi.json`, which closes request objects today.
6. **Cross-target thinking.** A mismatched target rejects readable reasoning today. Pass readable reasoning as plain assistant text in every turn, with no delimiters, as pi's `transform-messages.ts` does; drop opaque-only reasoning; record both as losses. Every turn, not only the current one: dropping a converted block later would edit the prefix and invalidate the target's own later thinking (architecture §4, append-only projection). Adaptive thinking (all 4.7+ models) does not require a turn to start with a thinking block, and a mid-turn mismatch disables thinking for that request instead of failing.
7. **Messages prompt caching.** Native Messages requests carry no `cache_control`, so OpenAI clients get no prompt caching on that upstream. Send the top-level `cache_control: {"type": "ephemeral"}` (automatic caching: the API places and advances the breakpoint on the last cacheable block); placing breakpoints by hand, as Magpie does, is the fallback only if a relay rejects the top-level field. The minimum cacheable prompt is 512 tokens on 5.x models, and changing thinking or top-level effort restarts the cache.

Further trimming of diagnostics/billing switches and fidelity records is assessed with the affected modules, not as a prerequisite. `morphiecore-auth` stays independent.

Exit: `Responses / Chat × Anthropic`, JSON and SSE, covers text, tool round trips, tool failures, thinking replay, a mid-loop Provider switch, ignored request fields, truncation, refusals, upstream errors and the cache field, with synthetic tests and bounded real calls (opencode-go `claude-haiku-5-5` or another available target) through the CLI-started gateway: one Codex-shaped request with reasoning controls, then a second turn on the same prefix. Whether the relay accepts the reasoning and cache fields is an exit check; a reported cache read is an observation.

## Later phases

### Phase 2 — held streams and scenario fixtures

Follow Magpie's held-stream design ([references](references.md)); its timings come from client reports and are starting points, not tuned values. Today the first upstream lifecycle event (`response.created`, the Chat role chunk, `message_start`) is published at once and freezes the candidate, so an upstream that opens a stream and then reports overload cannot fall back.

**Hold until content.** Hold only while another candidate could still answer; otherwise frames pass through at once. Lifecycle-only frames (creation, pings, an empty message start) are held. Content is text, a tool call, reasoning, or a terminal. While held, an upstream error is a failure the next candidate may answer; the first content releases the held frames and freezes the candidate. A safety refusal (Anthropic `refusal`, OpenAI `content_filter`) is delivered by default and never retried on the same model; reasoning is never held for it. An opt-in fallback to a different model is an open question below.

**Bounds.** A hold ends by releasing, never by dropping: at the first content, at a byte cap, or at a time cap. The time cap is short when the client cannot be kept alive and longer when it can, and a first-content wait extends it. SDK and Codex SSE readers drop comment lines, so Codex counts only events toward its idle timeout (300 s by default); the total hold across all candidates stays below it.

**Keep-alive.** Once a hold passes its threshold, send the response headers and SSE `: keepalive` comments at a fixed interval. Comments carry no semantics: the next candidate's stream continues in the same response. Once headers are out, a final failure is the downstream protocol's stream error, never a status code or a success terminal: Responses sends a gateway-created `response.created` if none went out (the SDK stream helper raises without it) and then `response.failed`, whose `error.code` uses names Codex classifies (`server_is_overloaded`, `rate_limit_exceeded` with "try again in Ns", `insufficient_quota`, `context_length_exceeded`); Codex ignores a bare `error` event. Chat sends one `data: {"error": {...}}` chunk, which the SDK raises as `APIError`, and closes without `[DONE]`. A stream that goes quiet after content also gets comments between complete events, up to a limit after which the stream is left to its timeout. Any wait before sending, such as a cooldown pause, is kept alive the same way.

**Non-API 2xx.** An HTML body, an empty body, or a body under a JSON type that does not start as JSON is a failure classified like a 5xx, decided from the first bytes before anything is published. It points at the origin or network path rather than the credential, so any cooldown is scoped to the Endpoint. Diagnostics name the reason without copying the page.

**First-content timeout, opt-in per route.** Cancel a candidate with no content after the limit and advance only if another candidate remains and no content was published. Sequential, not hedged. Diagnostics record time to first content and to first text separately.

**Scenario fixtures.** One hand-written wire fixture per scenario per protocol, checked wire→IR and IR→wire against a synthetic upstream. Pair-level tests exist only where a projection rule is specific to that downstream × upstream pair; no full pair matrix (see the test rules in [AGENTS.md](../AGENTS.md#tests)). Each SSE fixture is also fed in small fixed-size chunks to prove framing is independent of chunk boundaries.

Scenarios:

- Text, single and parallel tools, tool errors, reasoning replay, usage, truncation, upstream errors.
- Tool arguments: deltas equal to done; done with no deltas; done extending the deltas (emit the missing suffix); conflicting done (reject).
- Held delivery, after Magpie's tests: creation then overload advances, and the client sees one stream with one creation (`TestSilentHeldStreamStillFailsOver`); a held stream is kept alive and still answered (`TestSilentHeldStreamKeptAlive`); comments stop after the quiet limit (`TestQuietKeptAliveOnlySoLong`); a slow first content advances (`TestGroupFirstTokenFailsOver`); a non-API 2xx advances (`TestNotAnAPIReplyFailsOver`); an error after content aborts with no splice; a refusal before or after content is delivered as the answer, with no fallback or cooldown; post-commit transport loss.
- Consumer shapes: a tool name appears only on the first fragment; no empty reasoning or tool-call deltas; null, empty and absent stay distinct.

Exit: every listed scenario has an independent expectation per protocol, the held-delivery scenarios pass with synthetic upstreams, and the SDK loopback gate still passes with keep-alive comments.

### Phase 3 — routing: failure scope, cooldown and target affinity

The compiled route stays the authority on which targets are legal; this phase adds trusted runtime ordering inside that set (architecture §5).

- **Execution identity.** A candidate is Endpoint × credential identity × upstream model. Credential identity is the binding plus an in-memory fingerprint of an API key (never logged), or the verified OAuth principal. Cooldown, affinity and diagnostics key on that, never on candidate ordinals or display names.
- **Failure scope and cooldown.** Separate short rate limits, quota windows, missing credit and authentication, including errors reported inside a held stream; scope each to the credential or to credential × model. Cooldown demotes within the legal set and expires (Retry-After or reset, capped); hard exclusions (disabled, unauthorized, unsupported by the request) are never revived by cooldown or affinity. Refusals are answers: no fallback, no cooldown. Unknown-scope 429s still do not rotate credentials.
- **Target affinity, opt-in per route.** Separate from the existing prefix cache index, which keeps choosing nothing. Keyed by ingress principal, public model and conversation ID; the binding names the exact executor, so a different credential is a different cache domain and never inherits it. Ordered checks: executor no longer legal → drop; cooling down → skip; tool round trip (last user turn carries tool results, read from the IR) → keep the actual executor, even after a fallback; new turn → keep only with a recent reported cache read, otherwise base order (an upstream that never reports cache reads gets base order every new turn, which is deterministic and usually the same first choice). Feedback comes only from honest completed terminals; failure or abort clears the binding. State is in memory, bounded and disposable.
- **Diagnostics.** Record whether a cache hint was inferred or explicit, the legal candidates and why others were excluded, the affinity decision, and each physical attempt with its executor. Missing cache reports stay missing.

Exit: synthetic multi-candidate routes show: the executor is kept across a tool round trip after fallback; a removed or disabled target is not revived; a credential change does not reuse the old binding; the same conversation ID under different ingress keys shares nothing; concurrent first requests and cancellations release state; routes without affinity keep compiled order.

Deferred until a real need: turn-start routing rules (token, image, effort conditions held across tool round trips), per-credential concurrency and RPM lanes counting physical attempts, and budget reservations.

### Phase 4 — Google generateContent upstream

IR questions to settle first:

- `thoughtSignature` attaches to parts (including `functionCall`); confirm `Part.replay` and `CallContext.replay` carry it directly.
- `functionCall.id` may be missing: the gateway must mint call IDs and map back by name and order. Check whether `NativeCallAlias` suffices; if not, report the IR gap with options first.
- Streaming returns `GenerateContentResponse` fragments, close to Chat chunks.
- More `finishReason` values (SAFETY, MALFORMED_FUNCTION_CALL, …) and usage details (`thoughtsTokenCount`, `cachedContentTokenCount`).
- Schemas use the OpenAPI subset or `parametersJsonSchema`.

### Phase 5 — Google Interactions upstream

Submit full history statelessly, without server-side storage; reuse the opaque envelope. Pin the official API version before starting.

### Phase 6 — downstream Anthropic Messages

Reuse the Anthropic module from Phase 1; add request decoding and response/SSE encoding. Native `is_error`, thinking signatures and block order map directly. OpenAI targets have no tool error flag: keep the result text and drop the flag by a named lossy rule. Chat/Responses get no tool error extension until a client needs one.

### After that

Stateful Responses (gateway-managed, time-limited history), file services, resumed media work, Realtime.

## Open questions

- Pre-content refusals: add an opt-in route policy that retries on a different model? Anthropic documents this as the intended recovery (`refusals-and-fallback`: refusals are HTTP 200, retrying the same model usually refuses again, conversations then stay on the fallback model), which fits Phase 2 holding and Phase 3 affinity. Recommendation: yes, pre-content only, different model only, sticky for the conversation, never another credential of the same model.
