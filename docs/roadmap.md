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

1. **Upstream stream tolerance.** Responses intake rejects any unregistered event `type`; decide which unknown events may be skipped. Ignoring unknown fields must not widen into skipping unknown content or terminals. Known vendor shapes in Chat streams fail today: a continuation `function.name: ""` (rejected as a changed identity), `finish_reason: ""` (unsupported), `tool_calls: []` (opens the message owner early), and `finish_reason: "stop"` beside tool calls from relays (unsupported; Magpie reads it as a tool turn). Each becomes a named dialect rule, mirroring `empty_continuation_call_id`, only when a bound upstream emits it. Native Messages accepts only `end_turn`, `tool_use` and `max_tokens`; map the remaining documented stop reasons, with `refusal` as an honest refusal outcome that Phase 2 relies on. Frozen media decoders are not extended.
2. **Activate the native Messages binding.** `MESSAGES_BINDINGS` compiles only an explicit topology, and directory bootstrap rejects the `AnthropicMessages` protocol. Activate it only when listed in `models`, never by default. Verify model release metadata first; never invent dates.
3. **Chat reasoning carriers.** Model the `signature` of Chat `reasoning.text` details (currently rejected) through the envelope. Carry multiple thinking blocks and interleaved thinking/text/tool order instead of rejecting them where the Chat shape allows it. Stop emitting the empty `reasoning_content: ""` opener downstream; consumers treat each reasoning delta as a block boundary.
4. **Tool errors.** Neither OpenAI downstream protocol has an error flag on tool results, so `is_error` is rejected there. Decide whether a Chat/Responses extension carrier is needed (open question below) before implementing; a failed tool reported as plain text already round-trips.
5. **Messages cache breakpoints.** Native Messages requests carry no `cache_control`, so OpenAI clients get no prompt caching on that upstream. Project breakpoints in the Anthropic module: the stable prefix (system, or the last tool definition when there is no system) and the last non-thinking block of the final message, never on thinking, within the breakpoint limit. Recheck the current Anthropic caching docs first.

Further trimming of diagnostics/billing switches and fidelity records is assessed with the affected modules, not as a prerequisite. `morphiecore-auth` stays independent.

Exit: `Responses / Chat × Anthropic`, JSON and SSE, covers text, tool round trips, tool failures, thinking replay, truncation, upstream errors and breakpoint placement, with synthetic tests and one bounded real call (opencode-go `claude-haiku-5-5` or another available target) through the CLI-started gateway. A reported cache read is an observation, not an exit condition.

## Later phases

### Phase 2 — held streams and scenario fixtures

Follow Magpie's held-stream design ([references](references.md)); its timings come from client reports and are starting points, not tuned values. Today the first upstream lifecycle event (`response.created`, the Chat role chunk, `message_start`) is published at once and freezes the candidate, so an upstream that opens a stream and then reports overload cannot fall back.

**Hold until content.** Lifecycle-only frames (creation, pings, an empty message start) are held. Content is text, a tool call, or reasoning; a target that may refuse after reasoning declares so in its trusted binding, and its reasoning is held too. While held, an upstream error or a safety refusal (Anthropic `refusal`, OpenAI `content_filter`) is a failure the next candidate may answer; the first content releases the held frames and freezes the candidate. A refusal after content is the answer, not a candidate failure.

**Bounds.** A hold ends by releasing, never by dropping: at the first content, at a byte cap, or at a time cap. The time cap is short when the client cannot be kept alive and longer when it can, and reasoning or a first-content wait extends it. Responses clients such as Codex count only events toward their idle timeout, so the total hold across all candidates stays below it.

**Keep-alive.** Once a hold passes its threshold, send the response headers and SSE `: keepalive` comments at a fixed interval. Comments carry no semantics: the next candidate's stream continues in the same response. Once headers are out, a final failure is the downstream protocol's stream error, never a status code or a success terminal. A stream that goes quiet after content also gets comments between complete events, up to a limit after which the stream is left to its timeout. Any wait before sending, such as a cooldown pause, is kept alive the same way.

**Non-API 2xx.** An HTML body, an empty body, or a body under a JSON type that does not start as JSON is a failure classified like a 5xx, decided from the first bytes before anything is published. Diagnostics name the reason without copying the page.

**First-content timeout, opt-in per route.** Cancel a candidate with no content after the limit and advance only if another candidate remains and no content was published. Sequential, not hedged. Diagnostics record time to first content and to first text separately.

**Scenario fixtures.** One hand-written wire fixture per scenario per protocol, checked wire→IR and IR→wire against a synthetic upstream. Pair-level tests exist only where a projection rule is specific to that downstream × upstream pair; no full pair matrix (see the test rules in [AGENTS.md](../AGENTS.md#tests)). Each SSE fixture is also fed in small fixed-size chunks to prove framing is independent of chunk boundaries.

Scenarios:

- Text, single and parallel tools, tool errors, reasoning replay, usage, truncation, upstream errors.
- Tool arguments: deltas equal to done; done with no deltas; done extending the deltas (emit the missing suffix); conflicting done (reject).
- Held delivery, after Magpie's tests: creation then overload advances, and the client sees one stream with one creation (`TestSilentHeldStreamStillFailsOver`); a held stream is kept alive and still answered (`TestSilentHeldStreamKeptAlive`); comments stop after the quiet limit (`TestQuietKeptAliveOnlySoLong`); a slow first content advances (`TestGroupFirstTokenFailsOver`); a non-API 2xx advances (`TestNotAnAPIReplyFailsOver`); an error after content aborts with no splice; a refusal after content is delivered as the answer, with no cooldown; post-commit transport loss.
- Consumer shapes: a tool name appears only on the first fragment; no empty reasoning or tool-call deltas; null, empty and absent stay distinct.

Exit: every listed scenario has an independent expectation per protocol, the held-delivery scenarios pass with synthetic upstreams, and the SDK loopback gate still passes with keep-alive comments.

### Phase 3 — routing: failure scope, cooldown and target affinity

The compiled route stays the authority on which targets are legal; this phase adds trusted runtime ordering inside that set (architecture §5).

- **Execution identity.** A candidate is Endpoint × credential identity × upstream model. Cooldown, affinity and diagnostics key on that, never on candidate ordinals or display names.
- **Failure scope and cooldown.** Separate short rate limits, quota windows, missing credit and authentication, including errors reported inside a held stream; scope each to the credential or to credential × model. Cooldown demotes within the legal set and expires (Retry-After or reset, capped); hard exclusions (disabled, unauthorized, unsupported by the request) are never revived by cooldown or affinity. A pre-content refusal advances without a cooldown. Unknown-scope 429s still do not rotate credentials.
- **Target affinity, opt-in per route.** Separate from the existing prefix cache index, which keeps choosing nothing. Keyed by ingress principal, public model and conversation ID; the binding names the exact executor, so a different credential is a different cache domain and never inherits it. Ordered checks: executor no longer legal → drop; cooling down → skip; tool round trip (last user turn carries tool results, read from the IR) → keep the actual executor, even after a fallback; new turn → keep only with a recent reported cache read, otherwise base order. Feedback comes only from honest completed terminals; failure or abort clears the binding. State is in memory, bounded and disposable.
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

Reuse the Anthropic module from Phase 1; add request decoding and response/SSE encoding. Native `is_error`, thinking signatures and block order map directly.

### After that

Stateful Responses (gateway-managed, time-limited history), file services, resumed media work, Realtime.

## Open questions

- Tool errors and refusal details in Chat/Responses: add extension carriers (following existing OpenRouter-style shapes where one exists), or keep failures as plain tool output? Gates Phase 1 item 4.
- Unknown fields in downstream requests are currently rejected (closed request shapes). Keep that, or ignore and record standard fields that are not implemented, so newer SDK parameters do not fail?
- When switching Provider or model, should visible thinking be passed to the new target as assistant text?
- Phase 3 target affinity: which turn policy applies when the upstream reports no cache reads (keep for the session, per turn, or a time window)?
