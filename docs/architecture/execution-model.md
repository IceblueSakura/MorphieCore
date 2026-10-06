# Execution Model

Execution owns trusted I/O, attempts and delivery, not model semantics or Agent orchestration. [Protocol/lowering](protocol-and-lowering.md) prepares a valid target representation, including any declared compatibility loss; execution cannot invent a new downgrade after encoding. Current owners are linked from [architecture](../architecture.md).

## Fixed plan and attempt

```text
validated immutable IR → public requirements → fixed Route
 → independent candidate projection → encoded request + trusted auth/target
 → transport → selected response/event codec → IR validation
 → fixed downstream projection → body publication / handoff / completion
```

Startup compiles Public Model/task → Route → ordered Endpoints, with explicit representation/execution contracts and credential bindings. Business input cannot create, reorder or expand these relations. Candidate policy is pure; no executable closures or dynamic discovery are part of a request plan.

Each candidate starts from the same final IR. A projection may produce a separate validated view under its declared policy, but cannot mutate the input for another candidate. Endpoint equality does not imply credential/replay-scope equality. Fallback is fixed, opt-in, bounded and limited to eligible failures; current execution never repeats or races a candidate. See [ADR 0010](decisions/0010-canonical-model-fixed-fallback.md).

## Publication, commit and completion

Encoding and queueing do not commit. First downstream-frame publication conservatively freezes fallback before the HTTP response escapes; actual body handoff acknowledges commit, not peer receipt. Completion requires strict upstream closure and final downstream handoff. Once published/committed, errors can only abort the current body, never splice another attempt or fabricate success.

`Attempt::push` consumes at most one frame; the I/O owner retains unconsumed bytes and controls readiness. Terminal output is withheld until strict EOF. Delivery remains incremental and bounded, without a retained replay log; backpressure does not suspend the absolute deadline.

Response/item completion, semantic incomplete/failure/refusal, transport error and cancellation are separate observations. A compatibility policy cannot promote one to successful completion. `StrictComplete` may reject late after partial output; callers needing pre-delivery completeness use bounded static delivery.

One chain shares its permit, attempt budget and deadline. Tighter endpoint limits still apply. Drop, shutdown, timeout and consumers that stop polling must release upstream resources. No sleep-based race masking, unbounded queues or post-commit replay is permitted.

## Runtime authority

Transport receives only prepared trusted origin/path/method, safe headers, sensitive auth, encoded bytes/stream and resource policy. It does not receive Task IR, forward inbound headers, inherit ambient proxies or follow redirects implicitly. Response task/profile remains the selected endpoint's contract, never inferred from body content.

Credential material is acquired only at execution; ordinary requests do not log in, refresh or discover accounts. Secret ownership and recovery belong to [credentials](../credentials.md), not context/fidelity. Uncertain upstream completion may already have incurred work or billing; cancellation is not proof of remote termination.

A continuation describes later-operation requirements, not retry permission. Agent callers separately decide tool execution, subsequent requests and budgets. A terminal reducer cannot reopen. Necessary state for future Realtime is deferred rather than approximated by the current request lifecycle.

## Verification boundary

Request/profile, byte framing, body lifecycle and actual Router checks have separate owners. Isolation, bounded timeouts, cancellation and non-secret diagnostics are required. Synthetic behavior does not establish Provider/TLS/network acceptance, long-running load, cache benefit or future Agent support. Methods and commands belong to [development](../development.md), not execution diaries.
