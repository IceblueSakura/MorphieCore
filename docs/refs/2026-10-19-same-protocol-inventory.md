# Same-protocol Generation inventory

Observed on 2026-10-19 against the [selected baselines](../references.md#pinned-sources): OpenAI Python **3.35.1**, [`547ecb1e4fc233f19030736c29fee4374dfbaca1`](https://github.com/openai/openai-python/tree/547ecb1e4fc233f19030736c29fee4374dfbaca1/src/openai/types), and Anthropic Python **1.15.0**, [`bcd55fe1d3f0f1240765f8cef04295b67337f55a`](https://github.com/anthropics/anthropic-sdk-python/tree/bcd55fe1d3f0f1240765f8cef04295b67337f55a/src/anthropic/types). Both public source checkouts matched these revisions and had clean working trees. The client baseline is pi **1.3.0**, `71023ae`; its selected beta profile is documented in the [Messages profile note](2026-10-17-generation-contracts.md#pi-messages-request-profile-2026-10-19).

This is a source/codec inventory, not design approval, activation, exhaustive test evidence or live Provider acceptance. [Architecture](../architecture.md) owns decisions and [Roadmap](../roadmap.md#stage-1-work-plan) owns remaining work. The protocol pairs are Chat/DashScope–TokenPlan, Responses/OpenRouter plus Grok/public OpenAI SIWC, and Messages/OpenCode Go. Google generateContent and Interactions have upstream request encoders and SSE intake, **not client endpoints or static intake**, so there is no Google same-protocol pair.

## Reading the inventory

- **E:** an inspected typed/existing protocol-record carrier, within the stated subset. Independently test each direction before making a lossless claim.
- **L:** a named loss; never lossless evidence, even when approved.
- **R:** explicit rejection, including unsupported whole variants and their nested fields.
- **I:** ignored/dropped; names below classify known official omissions. This is a gap, not permission to keep dropping protected meaning.
- **G:** incomplete directional evidence or unresolved semantics. Absence/null/empty/default normalization is equivalent only where the source contract establishes equivalence.

Request rows describe client wire → IR → native upstream wire. Result rows describe upstream JSON/events → IR → client delivery; history rows also cover the next request. Model labels resolve through trusted bindings rather than preserving the client's string verbatim. Unknown extensions are ignored at client/upstream boundaries; closed gateway-built request readers remain stricter. SDK header arguments and derived parsing conveniences are not JSON body fields.

Code paths below are relative to `src/protocol/`; test paths are relative to `tests/`. Test owners identify where existing assertions live, **not proof of exhaustive field coverage**. Each table groups related fields by name; a rejected whole variant includes its children rather than implying partial support.

## Shared Aliyun dialect

The user selected identical DashScope/TokenPlan dialect behavior on 2026-10-19. Their official [TokenPlan quickstart](https://help.aliyun.com/zh/model-studio/token-plan-team-quickstart) and [tool configuration](https://help.aliyun.com/zh/model-studio/more-tools), rechecked that day, distinguish compatible protocols from billing credentials: TokenPlan, Coding Plan and pay-as-you-go keys/base URLs are not interchangeable. TokenPlan's native Responses entry is independently documented by the existing Codex/base-URL sources in `provider/catalog.rs`; it is not inferred from Chat or the older Coding Plan's limitations.

One `Bailian` profile now owns readable reasoning/history reading, Chat created-time/empty-ID normalization, response-only `schema_` aliases and existing accounting mappings. Current Qwen3.8, GLM5.3/Prime and Kimi-K3 Chat total-cap encoding is shared by model on both plans. Native decoder conflict/identity checks remain strict. Binding-level expectations check shared wire rules and control bodies while separately asserting origin, credential and replay-issuer isolation. Shared syntax does not establish a live service's effective parameter support or identical model availability.

The subsequent [official compatible-request review](2026-10-19-bailian-compatible-requests.md) owns the stricter Bailian subset, native controls, model/protocol effort aliases and refusal boundaries. The standard codec vocabulary below is not a declaration that every standard/OpenRouter field is a Bailian request carrier. In particular, native Responses does not inherit strict/default-normalization tools, result media, ordered configuration updates or unlisted control guarantees.

## Chat and the DashScope dialect

Sources: SDK `chat/completion_create_params.py`, `chat/chat_completion.py`, `chat/chat_completion_chunk.py`, `chat/chat_completion_message.py`, all `chat/chat_completion_*_param.py` content/tool types, and `completion_usage.py`. DashScope and TokenPlan select one `Dialect::Bailian`; total-cap overrides are model-specific, not billing-plan-specific.

### Request fields

| Official field(s) | Classification and owner |
|---|---|
| `model`, `messages` | E trusted model selection and ordered history; `openai/chat/envelope.rs`, `openai/chat/mod.rs` |
| `temperature`, `top_p`, `frequency_penalty`, `presence_penalty`, `seed`, `stop` | E admitted values; null normalizes absent and scalar stop to a list; `openai/chat/mod.rs`, `generation_controls.rs` |
| `max_completion_tokens`, `max_tokens` | E one typed total cap; simultaneous spellings R. Current Bailian bindings emit total `max_completion_tokens`; native legacy reply-only intake R unless thinking is explicitly off, with GLM5.3's documented legacy-total exception. No independent answer/thought conversion; `openai/bailian.rs`, `openai/chat/envelope.rs`, `openai/chat/mod.rs` |
| Bailian `enable_thinking`, Chat `thinking_budget`, `top_k` | E selected existing-IR subset with strict types and model guards, no inferred defaults. Conflicts, unsupported/ignored carriers and allocations outside the IR relation R. Positive top-k codec carriage does not expand binding admission; `openai/bailian.rs` |
| `logprobs`, `top_logprobs` | E boolean and 0–20 count; a count requires `logprobs:true`; `openai/chat/mod.rs` |
| `reasoning_effort`, `verbosity` | E admitted levels/presence; effective target support separate; `openai/reasoning.rs`, `openai/chat/mod.rs` |
| `response_format` | E `text`, `json_object`, `json_schema.{name,description,schema,strict}` subset; SDK-optional missing schema R; `openai/chat/mod.rs`, `openai/settings.rs` |
| `tools`, `tool_choice`, `parallel_tool_calls` | E function/custom definitions, auto/none/required/named and allowed-tools choices; strict schema constraints can R; `openai/function_tools.rs` |
| `audio`, `modalities` | E existing string-voice/format/modality subset. Custom voice `{id}` R; streamed audio requires PCM16; `openai/chat/audio.rs`, `openai/adapter/request.rs` |
| `n` | E absent/null/1; other values R; `openai/chat/envelope.rs` |
| `stream`, `stream_options` | E `include_usage`, `include_obfuscation`; active options require streaming and non-null booleans; `openai/chat/envelope.rs`, `openai/adapter/request.rs` |
| `metadata`, `service_tier` | E codec carriers, R on configured DashScope standard-context restrictions; `openai/chat/envelope.rs`, `openai/service.rs`, `../adapter/request.rs` |
| `safety_identifier`, `user` | E codec identity fields, L on identity-incapable configured targets; `openai/adapter/request.rs`, `cache.rs` |
| `prompt_cache_key` | L on Bailian under `cache.omit-key.v1`; `cache.rs` |
| `prompt_cache_options` | Empty object L; active `mode`/`ttl` R on Bailian; null R; `cache.rs` |
| `prompt_cache_retention` | R without target carrier; extended retention L only under separately selected trusted opt-in; `cache.rs` |
| `store` | True R; false/null accepted and omitted as inactive/default normalization; no completion storage; `openai/chat/envelope.rs` |
| `prediction` | Validated shape then L `chat.omit-prediction.v1`; `openai/chat/mod.rs` |
| `functions`, `function_call`, `web_search_options` | Active values R; inactive null/empty forms accepted without native functionality; `openai/chat/mod.rs` |
| `logit_bias`, `moderation` | R on presence, including null; `openai/chat/mod.rs`, `openai/chat/envelope.rs` |

### Content, tools and returned history

| Carrier | Classification and owner |
|---|---|
| System/developer/user/assistant `role`, `content` | E admitted text and ordered text arrays; one-part arrays may normalize to strings; non-null participant `name` R; legacy function role R; `openai/chat/mod.rs`, `openai/chat/owners.rs` |
| Function `name,description,parameters,strict`; custom `name,description,format.{type,grammar.{syntax,definition}}`; allowed-tools `mode,tools` | E admitted typed definitions/references; no schema repair; `openai/function_tools.rs` |
| Calls `id,type,function.{name,arguments}`, custom `name,input`; results `tool_call_id,content` | E exact raw arguments/input, links and ordered string/text-array results, including empty result arrays. A preceding call determines function/custom result kind; `openai/chat/mod.rs`, `openai/common.rs` |
| User `image_url.{url,detail}`, `input_audio.{data,format}`, `file.{file_data,file_id,filename}` | E supported inline/URL subsets; credential-owned file IDs R; Bailian file admission remains narrower; `openai/content/{image,input_audio,file}.rs` |
| Content-part `prompt_cache_breakpoint.{mode}` | **R** until a Chat target carrier is selected. Previously ignored during permissive client intake; now rejected before filtering, on text and media arrays. Responses records and Chat lowering rejection do not constitute a native Chat carrier; `openai/chat/mod.rs`, `openai/lowering/mod.rs` |
| Assistant `refusal`, text/refusal array parts | E representable combinations; conflicting text/refusal/calls R; `openai/chat/mod.rs` |
| Assistant `audio.{id,data,expires_at,transcript}` | E existing static/replay carrier with origin/expiry checks; request uses ID, not regenerated audio; `openai/chat/audio.rs`, `fidelity.rs` |
| `annotations[].url_citation.{start_index,end_index,title,url}` | E typed static attribution; annotated request history R in lowering. No citation-loop completeness claim; `openai/chat/annotations.rs`, `openai/lowering/mod.rs` |
| SDK `parsed`, `parsed_arguments` | Validate equality with authoritative raw JSON, then discard derived view; `openai/common.rs` |
| DashScope `reasoning_content` | Registered E readable extension, not an official Chat field. Nonempty leading text retained; null/empty normalize away; late SSE reasoning R; `openai/chat/reasoning.rs`, `openai/events/chat.rs` |
| DashScope `preserve_thinking` | E strict boolean through independent `ReasoningRequest.read_history`; absent stays absent. The public gateway source registers it explicitly; the selected native Bailian target carries it. It controls historical reasoning consumption, not current-turn context. Other upstream dialects do not inherit this carrier; `openai/chat/mod.rs`, `openai/lowering/mod.rs` |
| `reasoning`, `reasoning_details` | **Not registered on Bailian**: client/upstream I, closed request R. OpenRouter's structured carrier is not automatically a DashScope carrier. Determine the selected model's real replay contract before widening; `openai/adapter.rs`, `openai/chat/reasoning.rs` |
| `video_url.video_url.url` | Registered dialect extension E within media admission; not an official Chat content variant; `openai/content/video.rs` |
| pi `enable_thinking`, `chat_template_kwargs` | Unregistered client I by name; selected clients must not rely on these fields reaching DashScope |

### Static results and chunks

| Official field(s)/event | Classification and owner |
|---|---|
| `id,object,created,model,choices,system_fingerprint,service_tier,usage`; static `metadata` | E admitted typed envelope; exactly one choice with index 0; identity/report drift rejects; `openai/static_response.rs`, `openai/events/decode.rs` |
| `moderation` with input/output results or errors | **R** non-null in JSON and every chunk, including late usage tails; null is inactive. Previously silently ignored outside registered accounting dialects; `openai/chat/mod.rs`, `openai/static_response.rs`, `openai/events/chat.rs` |
| Choice `index,message/delta,finish_reason,logprobs` | E admitted subset: `stop,tool_calls,length,content_filter`; deprecated `function_call` R. `stop` beside a forced call remains source evidence, not inferred tool-call termination |
| Message/delta `role,content,refusal,annotations,audio,function_call,tool_calls` | E applicable carriers above. Active legacy `function_call` R; custom-call delta has no selected carrier and R; `openai/events/chat.rs` |
| Logprobs `content,refusal`; token/alternative `token,logprob,bytes,top_logprobs` | E typed observations and empty arrays; `openai/chat/logprobs.rs` |
| Usage `prompt_tokens,completion_tokens,total_tokens` | E checked counters, not inferred missing totals; `openai/static_response.rs` |
| `prompt_tokens_details.{audio_tokens,cache_write_tokens,cached_tokens,image_tokens,text_tokens}` | E numeric details; null/unreported presence may normalize; same owner |
| `completion_tokens_details.{accepted_prediction_tokens,audio_tokens,reasoning_tokens,rejected_prediction_tokens,text_tokens}` | E numeric details; same owner |
| Chunk `obfuscation` | Validated/bounded padding, then I and freshly generated at delivery; not semantic content; `openai/chat/sse.rs` |
| SSE chunks, finish, usage tail, `[DONE]` | E explicit lifecycle; early/duplicate/late content R, EOF cannot replace DONE. Downstream usage depends on `include_usage`; `openai/events/chat.rs`, `openai/chat/sse.rs` |

Existing test owners: `semantic/{chat,providers,tool_calls,reasoning_replay,messages}.rs`, `transport/chat.rs`. Stage-1 additions independently cover native history-reading controls, readable reasoning, static usage/delivery and late reasoning after text/calls. Cache/moderation rejection remains rejection, not new support. Model-effective replay and real client loops remain separate acceptance.

## Responses and the OpenRouter dialect

Sources: SDK `responses/response_create_params.py`, `response.py`, `response_input_item_param.py`, `response_input_item.py`, `response_output_item.py`, `response_stream_event.py`, and their referenced item/content/event definitions.

### Request and result fields

| Official request field(s) | Classification and owner |
|---|---|
| `model,input,instructions` | E supported item subset and string/null instructions; input string normalizes to user message; `openai/responses/{envelope,mod}.rs` |
| `temperature,top_p,max_output_tokens,top_logprobs,parallel_tool_calls,truncation` | E admitted values; null generally collapses to absent, requiring field-specific default evidence; `openai/settings.rs`, `openai/responses/envelope.rs` |
| `reasoning` | E supported `effort,summary,context,mode`; alias `generate_summary` normalizes with conflict checks; arbitrary mode strings R; `openai/reasoning.rs` |
| `include` | E `reasoning.encrypted_content`, `message.output_text.logprobs`; other official members R. Null/empty/absent and duplicate/order normalize; `openai/reasoning.rs`, `openai/settings.rs` |
| `text` | E `format,verbosity`; null request object R; format/tool subset restrictions apply; `openai/settings.rs` |
| `tools,tool_choice` | E function/custom/namespace subset and ordinary choices; hosted/discovery R; `openai/function_tools.rs` |
| `max_tool_calls,metadata` | E presence-aware budget and bounded map (16 entries, key 64/value 512 characters); `openai/responses/envelope.rs`, `../semantic/context.rs` |
| `stream,stream_options` | E `include_obfuscation` and delivery constraints; `openai/responses/envelope.rs` |
| `service_tier,safety_identifier,user,prompt_cache_key` | E supported context; identity/cache L if independently restricted target caps require it; `openai/service.rs`, `openai/adapter/request.rs`, `cache.rs` |
| `prompt_cache_options` | Empty object L on OpenRouter; active `mode,ttl,comparison_response_id,prewarm` R; null R; `cache.rs` |
| `prompt_cache_retention` | R on OpenRouter by default; extended-retention omission only under trusted opt-in; `cache.rs` |
| `store,previous_response_id` | Gateway-owned state, not native upstream state. Expand local history, emit upstream `store:false` without previous ID; raw stateless codec rejects active upstream state; `openai/responses/managed.rs` |
| `background` | True R; false/null/absent inactive normalization; `openai/responses/envelope.rs` |
| `conversation,prompt,moderation,access_programs,context_management` | Active values R; some inactive null/empty shapes accepted, not feature support; same owner |

| Official result field(s) | Classification and owner |
|---|---|
| `id,object,created_at,model,output` | E supported identity/content; public gateway identity may differ; `openai/static_response.rs` |
| `status` | E static completed/incomplete/failed/cancelled; static queued/in_progress R, handled as SSE lifecycle instead; null R |
| `usage` | E totals and `input_tokens_details.{cached_tokens,cache_write_tokens}`, `output_tokens_details.reasoning_tokens`; sparse/null reports normalize selectively; same owner |
| `error` | E `code,message,param,type`; original type has a bounded diagnostic owner, distinct from standalone SSE event type. Non-null `misalignment` R; `openai/terminal.rs` |
| `incomplete_details` | E `reason`; unlabelled cause remains unspecified rather than inferred; `openai/terminal.rs` |
| `instructions,reasoning,parallel_tool_calls,temperature,top_p,tool_choice,tools,max_output_tokens,top_logprobs,truncation,text` | E shared supported settings subset. Reported text absent/null/object states have a bounded carrier; request null still rejects. The [2026-10-20 presence review](2026-10-17-generation-contracts.md#responses-text-container-presence-2026-10-20) resolves the former container-null gap; `openai/settings.rs` |
| `metadata,max_tool_calls,background,prompt_cache_key,prompt_cache_retention,prompt_cache_options,safety_identifier,user,service_tier` | E supported independent reported context within shared bounds; active background R; `openai/responses/envelope.rs`, `../semantic/context.rs` |
| `completed_at` | E completed results; writer omits on other statuses; `openai/static_response.rs` |
| `prompt_cache_diagnostics` | E `cache_hit,comparison_response_not_found,unavailable,cache_miss`, with miss reason/count/comparison count; same owner |
| `previous_response_id` | Active upstream state R; gateway supplies local lineage; `openai/responses/managed.rs` |
| `conversation,prompt,moderation,access_programs` | Active values R, not arbitrary report carriage; `openai/responses/envelope.rs` |
| SDK `output_text` | Derived convenience property, not a Response wire-model field |

### Items and history

| Variant and critical children | Classification and owner |
|---|---|
| Easy/input/output `message`, `input_text`, `output_text`, `refusal` | E role/order/ID/lifecycle and applicable content; assistant `phase:commentary/final_answer` E; defaults normalize selectively; `openai/responses/mod.rs` |
| `input_image,input_file` | E admitted URL/inline/detail/filename, including `prompt_cache_breakpoint`; non-null credential-owned `file_id` R even beside a usable URL; `openai/content/{image,file}.rs` |
| `function_call` | E `id,call_id,name,arguments,status,namespace,async,caller`; raw arguments retained and direct context normalizes; `openai/common.rs`, `openai/responses/mod.rs` |
| `function_call_output,custom_tool_call_output` | E association/ID/status/name/namespace/caller, string or text/image/file output and bounded output-only `created_by`. Actor records bind the owner/resources and check snapshot/publication drift; request history has no actor field. Request history requires a call ID despite optional official field; `openai/responses/mod.rs`, `fidelity.rs` |
| `custom_tool_call` | E ID/call ID/name/raw input/context, text/lark/regex grammar; same owner and `openai/function_tools.rs` |
| `reasoning` | E ordered `summary_text,reasoning_text`, encrypted content, ID/status and origin-bound replay; `openai/reasoning.rs`, `fidelity.rs` |
| `configuration_update` | E ordered reasoning effort; null/default normalization; `openai/responses/mod.rs` |
| `compaction` | E ID/encrypted content/order/replay and bounded output-only `created_by`. Not the explicit compact operation; same owner |
| `program,program_output` | E ID/call ID/code/fingerprint/result/status; request program requires matching output. Carrier does not activate hosted execution; same owner |
| `file_search_call,computer_call,computer_call_output,web_search_call,tool_search_call,tool_search_output,additional_tools,image_generation_call,code_interpreter_call,local_shell_call,local_shell_call_output,shell_call,shell_call_output,apply_patch_call,apply_patch_call_output,mcp_list_tools,mcp_approval_request,mcp_approval_response,mcp_call` | R whole item, including action/security/result children; `openai/responses/mod.rs` |
| Input `compaction_trigger,item_reference` | R whole variant; no hidden state/resource authority |
| Tool definition `name,description,parameters,strict,output_schema,defer_loading,allowed_callers`; custom `format`; namespace leaves | E supported typed declarations; `openai/function_tools.rs` |
| Output annotations `url_citation,file_citation,container_file_citation,file_path`; logprob `token,bytes,logprob,top_logprobs` | E existing static subset; citation revision/point-anchor gap remains deferred and `file_path` annotation-added SSE R; `openai/content/{text,citations}.rs` |
| SDK `parsed,parsed_arguments` | Validate then discard derived view; `openai/common.rs` |

Gateway continuation expands **old typed input + old typed output + new input**, with current instructions/settings; it does not forward native previous-response authority. Input listing may synthesize IDs/status. Basic continuation/pagination tests in `openai/responses/managed.rs` and storage lifecycle tests in `src/gateway/generation/history.rs` do not prove full stored/stateless parity for encrypted reasoning, phase and complete call/result history.

### Every pinned SSE event family

Brace notation enumerates exact event names with the displayed prefix. Supported families inherit the item/result subset and ignored-field gaps above.

| Event(s) | Classification |
|---|---|
| `response.{queued,created,in_progress,completed,incomplete,failed}`, `error` | E supported lifecycle/error/terminal facts |
| `response.output_item.{added,done}`, `response.content_part.{added,done}` | E supported item/part subset; unsupported item kinds R |
| `response.output_text.{delta,done}`, `response.refusal.{delta,done}`, `response.function_call_arguments.{delta,done}`, `response.custom_tool_call_input.{delta,done}` | E exact values/fragments, ordering and owner checks |
| `response.reasoning_summary_part.{added,done}`, `response.reasoning_summary_text.{delta,done}`, `response.reasoning_text.{delta,done}` | E supported reasoning, including event-only summary-part done `status:incomplete`; it does not invent static summary status or change the item/response outcome |
| `response.output_text.annotation.added` | E supported citation subset; `file_path` R |
| `response.audio.{delta,done}`, `response.audio.transcript.{delta,done}` | I; no selected audio event carrier |
| `response.code_interpreter_call_code.{delta,done}`, `response.code_interpreter_call.{completed,in_progress,interpreting}` | I; deferred hosted execution |
| `response.file_search_call.{completed,in_progress,searching}`, `response.web_search_call.{completed,in_progress,searching}` | I; deferred hosted search |
| `response.shell_call_command.{added,delta,done}`, `response.shell_call_output_content.{delta,done}` | I; deferred hosted execution |
| `response.image_generation_call.{completed,generating,in_progress,partial_image}` | I; deferred hosted image generation |
| `response.mcp_call_arguments.{delta,done}`, `response.mcp_call.{completed,failed,in_progress}`, `response.mcp_list_tools.{completed,failed,in_progress}` | I; deferred MCP |
| `response.compaction.compacting` | E repeatable item-local sampling observation, with owner/index validation and no content, finality or static event log |

Dispatch and delivery owners: `openai/events/{decode,encode}.rs`, `openai/responses/sse.rs`. Sequence numbers are validated then regenerated; obfuscation is bounded nonsemantic padding. Unknown events increment a count, not a name inventory. Skipping a deferred event does not establish its support or permit fabricated successful hosted execution; supported item/terminal consistency still applies. OpenRouter accepts its selected post-terminal DONE convention.

Existing test owners: `semantic/{text,messages,reasoning_replay,tool_calls,compaction,providers}.rs`, `transport/responses_sse.rs`, and managed-history unit tests. Stage-1 additions use independent event/report oracles and a handwritten stored/stateless native-input oracle covering reasoning, final encrypted values, phase, exact arguments, results, current settings and a second continuation. There is no separate `tests/semantic/responses.rs` suite. This is codec evidence, not storage-service or client acceptance.

## Grok and public SIWC Responses

Checked on 2026-10-19 against the pinned OpenAI schema plus the public [SIWC inference contract](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference), [preview limitations](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations.md) and [xAI Responses reference](https://docs.x.ai/developers/rest-api-reference/inference/responses). These web sources are unversioned. The configured bindings select `grok-4.7` and the three existing SIWC GPT bindings; this is not another Provider/model matrix.

| Direction/domain | Treatment and evidence |
|---|---|
| Native request | E within text, request-wide/ordered developer instructions, explicit non-strict function tools, raw call/result association, encrypted-output include and body cache key. Independent authored IR and wire expectations use actual subscription endpoints. SIWC groups definitions/calls in namespaces and forces upstream SSE; Grok preserves selected stream intent. Both force `store:false`; no product session headers |
| Unsupported controls | SIWC R output caps, sampling, native HTTP continuation, standard metadata and the other final-check controls; flat tools/system history R. Grok does not inherit those SIWC restrictions, but its existing binding still closes standard context/strict tools/files. No model-effective parameter claim follows from codec support |
| Native static/result | E selected reasoning summary/readable parts/final encrypted value, phase, exact raw arguments, namespace and standard cached/reasoning usage. Wire→IR and separately authored IR→wire expectations are independent. SIWC static fixtures validate representation, not nonstreaming upstream HTTP |
| SSE and client delivery | E selected native event values and JSON/SSE materialization; byte fragmentation, changed final ciphertext, missing item-done/terminal and `[DONE]` reject. SIWC alone permits the existing missing-media/event-owned-empty-output compatibility; Grok remains strict. Completed/incomplete/failed terminals stay distinct |
| Returned history | E selected native encrypted reasoning/phase/call/result history and a second continuation through both gateway-stored and stateless paths. Current controls replace old ones; SIWC namespaces persist and native `previous_response_id` is not forwarded |
| Identity/cache policy | L `generation.omit-identity-hints.v1`, independently checked with source immutability; extended retention R by default. A separately opted-in omission remains L, not a successful exactness case |
| Source-only reports | E only for same-dialect/issuer/response-dependency return: xAI cost ticks/context counters/zero hosted counts; SIWC access programs, attribution, inactive penalties and exact zero-tool reports. Unknown nested program/attribution/context fields are filtered; malformed known counters/penalties R. Billed usage does not inherit context counters |
| Client report exclusion | **I / non-lossless** when projecting those scoped reports to the MorphieCore client dialect, including Responses→Responses. No client carrier or new report owner is selected here. Foreign scope and edited semantic dependencies cannot restore them |

Test owners: `semantic/providers.rs::subscription_stage1`, `semantic/usage.rs::subscription_accounting`, `transport/responses_sse.rs`, gateway subscription tests and managed-history unit tests. Existing unsupported hosted/citation/tool-discovery domains remain excluded. Synthetic gates do not accept live OAuth plans, final-cipher replay against an account, cached-token hits or every real-client loop.

## Messages and the Go/Haiku profile

Sources: SDK stable `message_create_params.py`, `message.py`, `usage.py`, raw event/content unions and referenced types; beta `message_create_params.py`, `beta_message.py`, `beta_usage.py`, beta event/content unions and selected mid-conversation/tool-change/block-binding types. Stable/beta **create schemas at this pin omit `temperature,top_p,top_k`**; existing compatibility intake still recognizes them. Do not confuse current model policy with a protocol-wide prohibition on older sampling carriers.

### Request fields

| Official field(s) | Classification and owner |
|---|---|
| `model,max_tokens,messages,system` | E supported ordered input/explicit ceiling and trusted model selection; `anthropic/client.rs`, `anthropic/semantic_request.rs` |
| `tools,tool_choice` | E supported function declarations and auto/none/any/tool, name, disable-parallel flag; `anthropic/request.rs` |
| `thinking` | E disabled/adaptive/ordinary manual budget and display. Manual budgets require ≥1024 and < output ceiling at source and target; adaptive-only Go/Haiku R numeric budgets. Beta `between_tools` and wider manual-interleaved scope R; `anthropic/client.rs`, `anthropic/semantic_request.rs` |
| `output_config` | E effort and unnamed JSON-schema format; beta **I `task_budget`** during client filtering, not an ordinary thinking budget; `anthropic/client.rs`, `anthropic/request.rs` |
| `cache_control` | E explicit ephemeral default/5m/1h; native codec absence stays absent. The selected Go/Haiku target retains its marker-free default with named insertion, never beside explicit markers; `anthropic/semantic_request.rs`, `anthropic/adapter.rs` |
| `stop_sequences` | E protocol carrier; R selected Haiku admission; `anthropic/mod.rs`, `anthropic/request.rs` |
| `stream,service_tier` | E envelope delivery flag and auto/standard-only policy; direct semantic helper is not the transport envelope; `anthropic/adapter.rs`, `anthropic/service.rs` |
| `metadata.user_id` | E native absent/null/empty and ≤512 characters, independently bounded at source/target. Unknown metadata names are filtered/named. No automatic cross-protocol attribution; `anthropic/client.rs`, `anthropic/adapter.rs` |
| `diagnostics.previous_message_id` | I request diagnostic control; no cache-divergence request carrier; same owner |
| `container,inference_geo,speed`; beta `compaction,context_management,fallbacks,fallback_credit_token,mcp_servers` | R active values; inactive null and empty MCP-server list ignored; same owner |
| `user_profile_id,workspace_id`; beta `betas` | SDK header arguments, not JSON body controls. Body spellings I; arbitrary header forwarding not implied. Trusted adapter selects upstream headers; `anthropic/adapter.rs` |
| Compatibility `temperature,top_p,top_k` | E native codec carriers; trusted fixed-sampling Go/Haiku policy L temperature/top-p under `messages.omit-sampling.v1` and R top-k. Generic codecs no longer assume that model policy |

### Blocks, tools and history

| Carrier | Classification and owner |
|---|---|
| Text `type,text,cache_control,citations` | E plain text/cache; nonempty citations R; null/empty citations normalize; `anthropic/shape.rs`, `anthropic/response.rs` |
| Thinking `thinking,signature`; redacted `data` | E ordered readable/opaque owners, including signature-only thinking; client values are sealed/restored with origin-bound manifests, not copied raw; `anthropic/semantic_intake.rs`, `anthropic/semantic_request.rs` |
| Thinking `display` and adaptive `block_binding.prefix_mismatch_behavior` | E summarized/omitted display and error/drop-block subset; beta display `updates` R. Selected carrier-capable targets retain binding; missing-carrier fallback is L; `anthropic/request.rs` |
| Enabled `budget_tokens`, beta enabled block binding | E ordinary native budget with strict source/target relation; enabled block binding R. Shared allocation rules remain broader, but cannot bypass native strict inequality |
| Function `name,description,input_schema,strict,cache_control,defer_loading,eager_input_streaming` | E admitted values; false defer-loading normalizes; strict schema constraints reject, not rewrite; `anthropic/request.rs`, `anthropic/strict_schema.rs` |
| Tool `type:custom`, `allowed_callers,input_examples` | Custom discriminator normalizes at intake; explicit callers/examples R. Hosted/versioned tool declarations R; `anthropic/client.rs` |
| `tool_use.{id,name,input,caller}` | E complete object input and native call identity; direct caller normalizes, server caller R; `anthropic/response.rs`, `anthropic/semantic_intake.rs` |
| `tool_use.toolset_name`, `tool_result.toolset_name` | I dispatch-bearing fields, including history. Not an exact ordinary-tool extension; `anthropic/client.rs`, `anthropic/response.rs` |
| `tool_result.{tool_use_id,content,is_error,cache_control}` | E links/errors and string/text/image/document subset; missing content normalizes empty. Search-result/tool-reference/browser-state children R; `anthropic/media.rs` |
| Image `source`, `transformations.oversized_image` | E URL/base64 and explicit error policy; downsize/default normalize; file source R; same owner |
| Document `source,title,context,citations` | E supported PDF URL/base64/plain text/title; nested-content/file sources, active context/citations R; same owner |
| Beta message `role,content,output_config.effort` | E selected system placement/effort ordering; active `clear_at` R; `anthropic/client.rs` |
| Beta `tool_addition,tool_removal` with tool-reference name/by-value definition | E selected ordered function-tool changes/cache; MCP references/toolsets and removal by definition R; `anthropic/request.rs`, `anthropic/response.rs` |
| `search_result,server_tool_use,web_search_tool_result,web_fetch_tool_result,code_execution_tool_result,bash_code_execution_tool_result,text_editor_code_execution_tool_result,tool_search_tool_result,container_upload`; beta `advisor_tool_result,mcp_tool_use,mcp_tool_result,mcp_tool_listing,compaction,fallback` | R whole unsupported block, including nested execution/resource fields; `anthropic/response.rs`, `anthropic/media.rs` |

### Results, usage and events

| Official field/event | Classification and owner |
|---|---|
| Message `id,type,role,model,content,stop_reason,stop_sequence,usage` | E supported envelope/content. Stops E end-turn/tool-use/max-tokens/stop-sequence/refusal/context-window; pause-turn R; `anthropic/response.rs` |
| Message `container,diagnostics,stop_details`; beta `context_management` | I reports, including structured refusal category/explanation; no exact report owner or omission receipt |
| Beta `input_transformations` | L validated omission of known thinking-dropped/mismatch-allowed reports and path; unknown transformation types ignored; `anthropic/response.rs` |
| Usage `input_tokens,output_tokens,cache_creation_input_tokens,cache_read_input_tokens` | E disjoint buckets; no fabricated total; `anthropic/accounting.rs` |
| Usage `cache_creation.{ephemeral_5m_input_tokens,ephemeral_1h_input_tokens}`, `server_tool_use.{web_search_requests,web_fetch_requests}`, `output_tokens_details.thinking_tokens,service_tier` | E supported native observations; optional null means no observation, nested non-null object replaces its scope |
| Usage `inference_geo,speed`; beta `fallback_credit,iterations` | I separate execution/accounting detail, not complete usage support |
| `message_start,content_block_start,content_block_delta,content_block_stop,message_delta,message_stop` | E supported lifecycle/content; sequential indices, closure and strict EOF required; `anthropic/sse.rs`, `anthropic/response.rs` |
| `text_delta,thinking_delta,signature_delta,input_json_delta` | E supported fragments. Unfinished object JSON at `max_tokens` retains exact raw input and incomplete lifecycle for native SSE only. JSON/history, invalid successful terminals, malformed prefixes, duplicates and limits R; no repair |
| `citations_delta`; beta `compaction_delta` | R unsupported content deltas |
| Delta `container,stop_details`; beta `context_management` | I, as with static reports; supported stop/usage retained |
| `ping`, unknown event types, `error` | Ping/unknown event I; error is a failed attempt, not a successful Generation result |

Existing test owners: `semantic/anthropic.rs`, `transport/{anthropic_sse,native}.rs`. Stage-1 additions cover native sampling versus fixed target policy, ordinary budget boundaries, identity states/bounds, exact partial fragments, signed-thinking coexistence and strict closure. The pinned Messages SDK gate additionally checks JSON tool input, returned call/result history and raw partial SSE. Opening-only input-transformation receipts and unselected report carriers remain outside this slice's exactness claim.

## Named rules on native paths

These rules have to remain visible when they fire; a same protocol name does not guarantee a compatible replay issuer, native profile or target policy. Test owners below contain selected examples, not a universal reachability matrix.

| Rule(s) | Reachability and code/test owner |
|---|---|
| `chat.omit-prediction.v1` | Chat intake with prediction; `openai/chat/mod.rs`, `fidelity.rs`; `semantic/anthropic.rs` |
| `cache.omit-key.v1`, `cache.omit-inactive-options.v1`, `generation.omit-identity-hints.v1` | Selected target/context dependent; Bailian omits key/identity, OpenRouter retains supported keys, empty unsupported options omit; `cache.rs`, `openai/adapter/request.rs`, `anthropic/adapter.rs`; `semantic/providers.rs` |
| `cache.omit-day-retention.v1` | Trusted opt-in only, never default or inferred storage-policy acceptance; same code/test owners |
| `chat.transfer-visible-history.v1`, `responses.transfer-visible-history.v1`, `messages.transfer-visible-history.v1` | Same protocol/different replay issuer can still transfer foreign optional reasoning and forget affected aliases; `openai/adapter/request.rs`, `anthropic/semantic_request.rs`; `semantic/reasoning_replay.rs` |
| `generation.omit-unmatched-replay-manifest.v1` | Client returned manifest no longer matches content; `attachments.rs`; `semantic/reasoning_replay.rs`, attachment unit tests |
| `messages.omit-sampling.v1` | Explicit trusted fixed-sampling target policy, even when protocol names match; `anthropic/adapter.rs`; `semantic/anthropic.rs` |
| `messages.default-automatic-cache.v1` | Explicit trusted Go/Haiku marker-free default insertion, not native source absence preservation; `anthropic/adapter.rs`, `../topology/catalog/messages.rs`; `transport/native.rs` |
| `messages.omit-block-binding.v1`, `messages.flatten-message-effort.v1`, `messages.collapse-tool-changes.v1` | Same-protocol target lacking selected beta carrier. Selected Go carrier policy retains them, generic native profiles can degrade; `anthropic/carriers.rs`, `anthropic/semantic_request.rs`; `semantic/anthropic.rs` |
| `messages.omit-input-transformations.v1` | Native Messages input-transformation reports; `anthropic/response.rs`; `transport/anthropic_sse.rs` |

The following generic rules can also occur with synthetic/imported/edited IR on a same-protocol endpoint, but are **not losses generated by untouched, supported native input**. Do not use such tests as evidence that official native fields are preserved:

- `openai/lowering/projection.rs` and `openai/terminal.rs`: `generation.usage-report-view.v1`, `chat.omit-message-phase.v1`, `chat.regroup-message-items.v1`, `chat.omit-incidental-settings.v1`, `chat.coarsen-completion-cause.v1`, `chat.context-window-as-length.v1`, `openai.recitation-as-content-filter.v1`; `responses.omit-incidental-reasoning-controls.v1`, `responses.omit-system-fingerprint.v1`, `responses.omit-input-image-tokens.v1`, `responses.omit-input-text-tokens.v1`, `responses.omit-output-text-tokens.v1`, `responses.omit-input-audio-tokens.v1`, `responses.omit-output-audio-tokens.v1`, `responses.omit-accepted-prediction-tokens.v1`, `responses.omit-rejected-prediction-tokens.v1`, `responses.omit-message-envelopes.v1`, `responses.omit-completion-cause.v1`, `responses.omit-context-window-reason.v1`. Test owners: `semantic/{providers,messages,anthropic,reasoning_replay,usage}.rs`.
- `anthropic/semantic_request.rs`: `messages.omit-verbosity.v1`, `messages.omit-message-phase.v1`, `messages.summarize-reasoning.v1`, `messages.project-call-ids.v1`, `messages.reasoning-as-text.v1`, `messages.developer-as-system.v1`, `messages.omit-schema-name.v1`, `messages.default-max-tokens.v1`, plus explicitly selected reasoning-policy rule IDs. Test owner: `semantic/anthropic.rs`. The fixed default cap remains stage-3 implementation work.
- `anthropic/client.rs`: `messages.omit-incidental-settings.v1`, `messages.omit-system-fingerprint.v1`, `messages.refusal-as-text.v1`, `messages.usage-view.v1` and the shared usage-report projection. Test owners: `semantic/anthropic.rs`, `transport/anthropic_sse.rs`. They do not account for ignored official reports above.

Default materialization such as omitted non-strict → `strict:false` is not a loss rule. Marker-free automatic-cache insertion is now target-local and named; it is not exact source absence preservation. No effort mapping is selected for the Luna binding; generic reasoning-mapping policies are not evidence of native Luna loss or support.

## Gap disposition and minimal counterexamples

These are scoped findings, not new semantic decisions. Preserve unsupported/protected rejection and present structural choices before adding owners. Whole hosted-tool, citation and execution-policy expansions remain outside the selected stage-1 domains.

| Package/domain | Counterexample or missing evidence | Disposition |
|---|---|---|
| S1-1 Chat maintenance | Text part with `prompt_cache_breakpoint:{mode:explicit}` previously vanished; non-null static/chunk `moderation` previously vanished | Explicit rejection now covers both, including late reports. Exact cache/moderation support is not selected |
| S1-1 reasoning/history | Bailian does not register structured replay details | Native Qwen replay is readable reasoning plus exact history-reading control; no invented opaque carrier. See the dated source finding below |
| S1-1 request/result | Named prediction/key/identity omissions and narrower standard-context admission remain non-lossless | Keep explicit domain exclusions or deliberately select evidenced target policy; verify stream options, usage and finish facts independently |
| S1-2 reasoning | Summary-part interruption previously vanished | Implemented user-selected event-only observation; no static status invention |
| S1-2 history/error | Output creator/original error type previously vanished | Implemented bounded output-only records and original error classification; reported drift rejects |
| S1-2 progress | Compaction sampling progress previously skipped | Implemented repeatable owner-validated observation, distinct from final content |
| S1-2 parity/presence | Reasoning+phase+call/result continuation and current controls | Handwritten independent native-input oracle covers stored/stateless paths and a second continuation; SDK/real-service claims stay separate |
| S1-3a/b/c/d | Earlier generic sampling, numeric-budget, identity and partial-input gaps | Implemented selected native contracts; wider manual/interleaved, malformed-successful tool input and JSON/history partial carriage remain explicit exclusions |
| S1-3 inventory maintenance | Earlier implicit automatic-cache insertion lacked evidence | Default moved to trusted target policy with named insertion; generic source absence preserved |
| Deferred Messages controls | `output_config.task_budget`, `toolset_name`, `diagnostics` ignored; these are not ordinary thinking budget, function identity or generic metadata | Explicitly classified gaps outside selected pi carriers; no acceptance claim for them. Reopen only with the relevant owner/scope selection |
| Deferred reports | Messages container/context-management/refusal/geo/speed/fallback/iteration reports ignored; deferred Responses hosted events skipped | Keep named inventory gaps distinct from existing rejection; scheduling later work does not make silent report loss lossless |
| Deferred citation/execution/resource semantics | Request citations/context/clear-at, hosted item/block variants, file IDs, protected execution controls | Retain current rejection; select separate owners and gates when scheduled |

## Selected source findings and implementation boundaries

Rechecked on 2026-10-19: the official [DashScope Chat reference](https://www.alibabacloud.com/help/zh/model-studio/qwen-api-via-openai-chat-completions) documents `preserve_thinking` as consuming historical assistant `reasoning_content`, which is billed as input. Its general default is false; Max and Omni profiles have documented enabled defaults, so the codec must preserve omission rather than choose one. Flash supports the explicit switch. Max requires complete historical reasoning; the field must not be concatenated into assistant `content`. This is readable native replay, not evidence of encrypted reasoning details. Unregistered `thinking_budget`, `enable_thinking`, `chat_template_kwargs` and other vendor controls are not implicitly opened by this slice.

The [fine-grained streaming guide](https://platform.claude.com/docs/en/agents-and-tools/tool-use/fine-grained-tool-streaming) makes opening `input:{}` a placeholder and raw `partial_json` the real supplied input; `max_tokens` can interrupt a parameter. The user selected only native SSE unfinished-prefix carriage, not repair or a successful invalid call. The prefix checker uses serde syntax/duplicate checks with independent byte/depth/node bounds before accepting EOF; it never generates missing argument bytes.

The user also selected event-only summary interruption and compaction progress, output-only creator records, and original error classification. Native static/history schemas cannot reconstruct the event-only observations. Passing these scoped contracts does not implement every official field, close human design review or re-accept a real client loop.
