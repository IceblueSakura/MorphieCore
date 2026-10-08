# Responses：当前无状态 Generation profile

本页描述当前库级合同，不是完整标准符合性声明。标准目标归 [Semantic Model](semantic-ir.md#3-客户端-api-目标与扩展边界)；本地兼容形式仍需单独识别；[客户端边界](client-generation-profile.md)不提供独立私有 attachment。精确字段/预算/拒绝分支归 [OpenAI codecs](../../src/protocol/openai/mod.rs)、[lowering](../../src/lowering/generation.rs)和独立测试，HTTP 接线归[网关指南](../http-gateway.md)。

当前范围是有序文本/选定图片/inline 或 URL 文件 history、文本/工具/reasoning 输出及适用事件。单个 profile 能解析不等于 Public Model/Endpoint 准入；未实现媒体、资源或 task 不由标准名称激活。标准来源和本地选择分开，见[固定基线](../references/responses-standard.md)。

## Response outcome and continuation

`GenerationResponse::outcome()` 表达一次 response 的 completed/incomplete/failed/cancelled，不表达逻辑 turn 结束或工具执行成功。合法 incomplete 保留部分产物及原因，不作为 fallback 理由。完成 response 仍可有待回应 call。

当前 `continuation()` 是从最终 items 借用派生的 pending-result view：call ID 与种类必须匹配后续 result，位置/名称不能替代关联；部分结果只解决其对应 call。历史编辑后重新计算，不保存另一份 pending 权威。无已知 pending 返回 `Unreported`，不是 end-of-turn 或发送许可；partial call/ongoing result 也不能证明就绪。

本地 `ResponseContinuation` 可检查调用方声明的后继关系、缺失/进行中结果与有限依赖；不验证真实 upstream turn identity、issuer、目标可表示性或执行权限。Program history 按当前 profile 要求配套 reported output 后才可 replay，不能由通用 pending view 放宽。

标准 codec 不从 finish label 或 call 数量补 `InteractionProgress`。显式 progress 保留在 typed IR，但当前公开目标没有 carrier，投影拒绝；不推断报告或授权自动 Agent loop。Owners：[continuation](../../src/semantic/task/generation/continuation.rs)、[turn](../../src/semantic/task/generation/turn.rs)、[progress](../../src/semantic/task/generation/progress.rs)。

## Message owners and cross-protocol grouping

当前标准 Responses 没有 `ToolCall.message` 的 carrier。Request/static lowering 拒绝显式 message-call membership，event 投影在 attached call 的 opening 拒绝；不能仅保留两个独立 item 就声称关系仍在。空的 tool-only message owner 也适用。当前 MorphieCore adapter 也不提供私有 wire-ID 关联。

独立 assistant message（包括空 owner）与独立 function call 按其身份/状态保留，不从邻接推断归属。Standalone-call run 的具名 Chat 投影不使反向 membership 自动获得标准位置。重排依赖 final typed identity，删除 owner 不能附到相同位置的新项。

有损 Chat 是[设计方向](protocol-and-lowering.md#semantic-loss)，不是此处现行拒绝的自动撤销；标准 Responses 也不继承 Chat 的损失许可。Owning checks：[group projection tests](../../tests/semantic/group_projection.rs)、[group view](../../src/semantic/task/generation/group.rs)。

## Reasoning replay authority

Reasoning 控制、可读内容与格式绑定的 opaque 值分别拥有权威，基础语义归[交互合同](interaction-contract.md#typed-replay-与信任)。[标准流事件](https://developers.openai.com/api/reference/resources/responses/streaming-events)说明 reasoning 的 `output_item.added.encrypted_content` 可能尚不完整，续轮应使用对应 `output_item.done` 的 item。当前 profile 将 item-start 值视为 partial，item-done 确定最终值，可替换或移除此前 partial；这不是对已闭合 item 的更新许可。

Value 和 owner 必须满足该格式 finality，且 scope/依赖与目标兼容，才能作为 history 回放。当前事件累积结果与非空终态 output snapshot 的 replay 新增、替换或删除不一致仍拒绝；具名空终态摘要规则不授权覆盖已有值。该行为是当前 profile 边界，不是所有 opaque 格式在所有阶段都必须不变的普遍规则。闭合后的变化暂缓调整，未决点与恢复条件归[待决状态](../implementation-status/open-questions.md#reasoning-opaque-的闭合后权威)。

删除 typed 值不能从 fidelity 恢复，单删证明也不授权丢值继续。可读内容不是必要 signature 的替代品，某个 item 已完成不代表整个 response 成功。来源记录与编辑失效归 [source records](protocol-and-lowering.md#source-records)及 [ADR 0006](decisions/0006-reasoning-ownership.md)，不复制另一份 token。

## Tool namespaces

标准 `namespace` 分组拥有有序 function/custom definitions、名称与描述，不嵌套。限定工具 identity 由 namespace、kind、name 共同组成；组内同 kind/name 唯一，跨组同名不合并。调用保留 namespace 与原 name，结果仍通过 call ID 关联，不增加另一份 namespace 权威；历史工具不必仍存在于当前可选定义中。

纯库可构造、检查和编辑分组及限定引用，选择引用在删除/更名后必须重新验证。固定标准的具名 `tool_choice` 没有 namespace 字段，因此限定选择没有已定稿 wire carrier，当前投影拒绝，不创造字段或点号拼接规则；分组请求可以使用 `auto/required/none`。Chat 无分组/限定调用载体，明确拒绝，不静默扁平化。

Owners：[tool IR](../../src/semantic/task/generation/tool.rs)、[tool codec](../../src/protocol/openai/function_tools.rs)、[independent oracles](../../tests/semantic/tool_namespaces.rs)。固定来源：[namespace definition](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/namespace_tool_param.py)、[named function choice](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/tool_choice_function_param.py)。SIWC 的额外限制由独立 [adapter admission](../../src/adapter/siwc.rs)拥有，不改变共享语义。

## Tool image results

Responses function/custom result history 的标准 carrier 接受有序 `input_text` 与 URL/inline `input_image` parts。Text string、parts、空数组及单 part 不隐式互换。Call identity、kind、结果状态独立校验；structured result authority 与 execution report 仍无公开 carrier，目标投影拒绝，不能 stringify 后声称保留。

`tool_result_images` 准入独立于 user `image_input`；共用目标图片预算，不因此启用 Chat 工具图片、file ID、cache breakpoint 或输出图片事件。Owners：[tool results](../../src/semantic/task/generation/tool_result.rs)、[Responses codec](../../src/protocol/openai/responses.rs)、[independent oracles](../../tests/semantic/tool_results.rs)。固定 union 来源：[function result](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_function_call_output_item_list_param.py)、[custom result](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_custom_tool_call_output_param.py)。

## User image input

当前 user text/image 输入由 typed `Resource` 拥有顺序、URL/inline source 和 detail；编辑不恢复旧媒体。仅投影 HTTP(S) URL 与 Base64 image data URL。File ID 需要 issuer/resource 生命周期，instruction/assistant images 与生成图片不因此准入。Missing detail 与 explicit auto 区分，null 拒绝，目标不支持的 detail 不暗改。

Codec 不下载、重定向、OCR、解析像素、转码或上传；URL 语法合法不证明 Provider 获取安全或可用。库级、HTTP body 与 Endpoint 预算独立。Owners：[resource](../../src/semantic/task/generation/resource.rs)、[image codec](../../src/protocol/openai/image.rs)、[image tests](../../tests/semantic/images.rs)。

Reported image/text/audio token counts 不是从正文或图像大小估计的值。MorphieCore 对 image/text usage 的具名 carrier 属于非标准位置，普通 Responses 目标无对应位置时当前拒绝，包括显式零。计量别名/视图从最终 typed 报告再投影，不保存第二个 total；来源不足不补猜。精确规则归 [adaptation](../../src/protocol/adaptation.rs)、[image usage](../../tests/semantic/image_usage.rs)和[billing modality tests](../../tests/semantic/billing_modal_usage.rs)。

## User inline file input

标准 `input_file.file_data` 仅接受有界 Base64 data URL；文件 MIME/source 与可选 filename、file detail 由 [Resource](../../src/semantic/task/generation/resource.rs) 的类型化描述分别拥有。filename 是描述，不是本地路径；缺省与显式空字符串分开。File detail 缺省与 explicit auto 分开，不能承载 image original；null、多个来源和未准入字段拒绝。

纯 codec 不打开文件、下载、上传、解析文档、OCR 或推定 MIME 与文件内容相符。File ID、cache breakpoint、工具文件结果、assistant/instruction 文件、文件输出与 Chat 文件投影仍拒绝。URL 来源按下节独立准入；共享 opaque source 不代表 issuer-bound file ID 已准入。

文件 requirements 与图片独立；模型语义、目标 MIME/detail/name/count、单资源及总 decoded bytes、总 encoded 状态与 HTTP body 分层检查。已注册的文本/图片绑定不自动获得文件能力；文件目标可以只准入 PDF，不能从单个文件 carrier 推定一般格式或来源都可用。其他 Provider dialect 未声明文件 carrier 时保持拒绝。编辑后按最终值重验，文件描述及私有来源进入进程内依赖证明，但该证明不成为 issuer 认证或跨请求持久化格式。

Owners：[file codec](../../src/protocol/openai/file.rs)、[file constraints](../../src/protocol/file_constraints.rs)、[independent oracles](../../tests/semantic/files.rs)。标准来源为固定 [input file union](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_input_file_param.py)；适用 guide 导航归[媒体基线](../references/multimodal-and-resources.md)。

## User file URL input

标准 `input_file.file_url` 映射既有 `ResourceLocation::Url`，必须与 `file_data` 恰一来源；包含另一来源的 null 也不接受。filename/detail presence 与 inline 文件相同。HTTP(S) URL 沿用共享资源校验，禁止 userinfo、控制/空白字符和反斜杠；原始 URL（含 query/fragment/转义）精确保留，不将 parser 规范化结果写回。URL/签名 query 只在 typed source 与必要请求正文保留，Debug/诊断不回显。

目标的 URL 开关与 inline MIME allowlist 独立，交集只能收窄。URL 和 inline 文件共享 max_files 与请求 encoded bytes 预算，URL 自身还受资源 URL 字节限制；inline decoded bytes 限制不声称覆盖远端下载量。不能根据文件名或扩展名证明 PDF/MIME，也不猜远端 bytes。已有文件模型能力不自动开启目标 URL 开关；Chat、file ID 与工具文件仍拒绝。

URL 是交给明确选定上游的内容来源，不改变网关 upstream origin、认证头、凭据或协议。网关不解析 DNS、不连接 URL、不跟随 redirect；地址可达性、远端重定向、访问许可及资源下载安全由消费上游负责，此语法检查不是远端 SSRF 安全证明。签名 URL 可能授予访问能力，客户端必须有权将其提供给所选上游；不转发额外下载 headers 或 cookies。

URL 值/filename/detail 编辑会使本地依赖证明失效，删除/切换来源不能由 fidelity 恢复。相同 URL 只证明相同来源字符串，不证明相同远端内容或永久可用；本片不建立不可变资源身份、expiry 或 issuer 证明。静态/流式 history 均保留实际 source，不能自动下载后替换为 inline。

## Raw JSON admission

原始 JSON/SSE 字节入口使用共享严格 parser，拒绝重复键（包括 escape-equivalent keys）、非法 UTF-8/JSON 和尾随非空白，并在分配前限制 bytes、depth/nodes。Value API 不能证明原字节的这些性质；HTTP 收集另行有界。Raw tool argument string 不递归当 envelope 解析。

Unknown、错类型、跨 kind 字段与不合法 presence 不能静默省略。错误不回显输入；stream failure poisons state。所有精确配额以 [JSON parser](../../src/semantic/value/json.rs)、[protocol JSON boundary](../../src/protocol/openai/json.rs)和对应 tests 为准，而不是文档副本。

## Complete-stream required fields

完整 envelope 与 events 使用各自固定 schema，不能套用 request 简写或低层 snapshot 的宽松形状。身份、timestamp、required output/annotations/probability arrays、sequence 与 owner 必须逐分支满足；显式空值不等于缺字段。Named reported-fact normalization 与结构 requiredness 分开，不能补造整份 usage、身份或终态。

Added/delta/done 与最终 snapshot 必须一致；终态不能补缺失的必要事件或改写已经交付的值。Selected product profile 可有具名事件权威规则，但不能改变标准 grammar 或把 EOF 当作成功。Applicable padding 有独立预算，不能因耗尽预算静默关闭所请求的保护。

Framing、event 数/bytes、aggregate state、strict EOF 和真实 terminal 分别验证。终态前或终态后非法数据、缺分隔符、截断和取消均失败，late projection error 只能中止，不伪造成功或前移。Owners：[event codec](../../src/protocol/openai/events/mod.rs)、[SSE](../../src/protocol/openai/sse.rs)、[byte tests](../../tests/transport/responses_sse.rs)。

## Derived replay views

SDK `parsed`、`parsed_arguments` 和 `output_text` 是派生视图，不是正文权威。当前 history 仅按已声明规则验证 parsed views 与原 raw string 的一致性后丢弃；SDK 的 coercion、alias 或 filled defaults 不能冒充相同原值。替换正文不复活旧 parsed view。

上游 response/events 不因 SDK 有便利属性就允许同名未知字段；具名 Provider 规则仍独立。精确 replay 规则及 Chat 对应位置归 [parsed replay tests](../../tests/semantic/parsed_replay.rs)和 owning codec。

## Assistant phase labels

Assistant `phase` 是标准标签，独立于 item lifecycle；missing/null 表示未报告标签，不补默认。History/static/event 对同一 owner 保持一致，不能从 phase 推断终态或语义执行。当前 Chat 无 phase 投影，commentary 与 SDK parsed view 的组合按 owning validation 拒绝。Owners：[phase tests](../../tests/semantic/phase.rs)、[message type](../../src/semantic/task/generation/request.rs)。

## Schema property order

Schema 与工具参数/输出约束保留 authoritative JSON 的属性顺序，变换不能排序、填 required、删除约束或恢复 source。Schema 的结构、strict 默认、局部引用与 quota 归 [Schema profile](schema-profile.md)，request constraint 不补成 response fact。

Value 调用方负责入站前未丢顺序；普通 Value equality/round trip 不证明顺序保持。Shared text-format 意图与 Chat shell 可映射，但不是字节同构；function strict 默认不等价时当前拒绝，而不是归一到同一值。独立证据归 [schema tests](../../tests/semantic/schema.rs)。

Token Plan 的受信 Responses profile 仅在 reported `text.format.type=json_schema` 中将 `schema_` 映射为 `schema`。两字段同时出现（含 null）拒绝；映射后的 Schema 沿用相同验证，属性顺序、精度与 strict 保持。静态响应与事件 snapshot 共用此规则；请求、标准下游及普通 Bailian/Chat 不继承别名准入，编码只使用最终 typed `schema`，编辑不恢复旧 alias。Owners：[具名规则](../../src/protocol/adaptation.rs)、[映射](../../src/protocol/openai/adapter_shapes.rs)、[独立预期](../../tests/semantic/provider_profiles.rs)。

## Control, message and annotation admission details

当前有几个不能与标准目标混同的边界：

- `reasoning.summary` 与 deprecated `generate_summary` 仅接受 `auto/concise/detailed` 或 null；缺省、空对象和 null 分别保留，冲突 alias 拒绝。布尔值在请求、响应回显与事件中均拒绝。Typed `ReasoningSummary::Disabled` 仍是合法语义，但当前 Responses 目标没有载体，投影/编码失败，不改写为缺省或 null。
- `cancelled` 可以是静态 response 状态，但固定 SDK 没有 `response.cancelled` SSE 事件；事件接收与编码均拒绝，不改写为 failed/completed。取消的 typed 产物仍保留，transport 取消不会合成事件或成功终态。
- HTTP Responses 不接受 `session_id`；该 carrier 仅在显式库级 MorphieCore/Provider profile 或 Chat 兼容入口适用。标准 identity/cache hints 有各自 owner，不从 key/user/token 派生 session，不透传 session headers，也不提供服务端会话。
- Function/custom definition 的 `async` 与 `defer_loading` 是非 nullable 布尔字段；省略/false 均为 inactive，null 拒绝。Call item 的同名字段使用其独立 schema，不能用 definition 的缺省推断调用事实。
- 仅 inactive state forms 准入，request `store` 投影为 false；活动 conversation、previous response、background、模板、moderation 和 compaction 尚未形成主链。
- Configuration/program/custom 分支按 owning codecs 明确准入，只表示有限语义；不是任意设置 patch、工具执行或脚本授权。Program history 需要匹配的 reported output，Chat 不自动支持它。
- `CustomSections`/`CodexHeaders` 是低层 carrier；非空 body sections 在 adapter 主链拒绝，headers 未因此接入 HTTP。未知字段不能通过它们旁路。
- 逐字段 required/null/empty、annotation/probability owner、引用范围与数量、non-complete instruction、reported context 的接受/拒绝由 owning codec 与[独立 admission tests](../../tests/semantic/text_admission.rs)维护，不把低层 convenience 放宽到完整 wire。

这些有效约束仍需在实现新切片时同步迁移。尚未完成的 union 审计、标准路径隔离、资源与媒体主链只归[实现缺口](../implementation-status/generation.md)，不在 profile 保存运行结果或完成度表。
