# Protocol Codec、目标投影与语义损失

[Semantic Model](semantic-ir.md)是唯一语义权威。本页定义 wire 映射、能力检查、损失与 fidelity；具体已实现字段归 owning codec/profile，不在这里复制 schema。执行与提交归[execution model](execution-model.md)。

## Codec 与 lowering

```text
外部 wire → codec/profile → typed IR + bounded source records
                                ↓ validation / requirements
                    fixed target + projection policy
                                ↓ lowering
                     validated representation → codec → wire
```

- Codec 拥有语法、envelope、presence、事件 grammar 与已声明 carrier；adapter 组合受信规则，不成为第二份语义模型。完整响应、请求简写和 SDK 派生视图分别验证。
- Lowering 判断最终 typed 值是否能按指定目标和投影策略表达。它不选择 Provider、查 registry、取 credential 或联网，不在 encode 后修改 JSON。
- 等价别名、已验证派生视图、精确数值推导与字段级兼容默认必须具名、有限且有前提。默认不覆盖实际报告或 malformed 值，来源记录在语义外；编码不能重做 intake 默认来恢复删除值。
- 未知报告优先使用所选标准的缺省或 nullable 载体；Generation usage 未报告时输出 `null`，不补造零计量。格式错误或计数矛盾仍失败，不能替换为未知以掩盖错误。仅在现有标准语义无法表达时，才按定稿的具名规则考虑兼容回落值，并与实际报告区分；这不是通用补零或吞错许可。
- [共享响应 metadata](../../src/protocol/decoded.rs)的创建时间分开表达未报告与精确 Number；独立校验允许缺省，不把非法报告改成未知。Chat/Responses 的静态与事件目标仍要求有效 created/created_at，Chat 保持整数限制；缺少必要值时拒绝，不填本地时间、零或 null。流已绑定的时间不能删除或替换，失败后不恢复成功。
- 同协议与跨协议使用同一链路。对原始字节必须在丢失键序列前拒绝重复 JSON key，并限制解析深度/节点/bytes；预解析 Value 不能证明原字节合法。共享 parser 归 [JSON owner](../../src/semantic/value/json.rs)。

## 能力与固定目标

四层合同回答不同问题：**semantic** 描述何种任务含义及模型能力，**representation** 能用何种 wire 表达，**execution** 具有什么 I/O/资源属性，**public** 向下游承诺什么。宽泛 tools/media 标志不证明每个值都可用，公共能力不能取全部候选的并集。

Canonical Model 拥有模型语义身份；Public Model 绑定 task 和固定 Route；Route 按顺序列出 Endpoint；Endpoint 绑定 Provider、可信 origin/path、upstream model、协议和表示/执行合同。Provider/auth/selected endpoint 不是 task 内容。编译关系归 [topology](../../src/topology/mod.rs)，现场激活按 [AGENTS](../../AGENTS.md#current-provider-model-and-compatibility-information)查询。

Requirements 从最终值和 delivery 推导，不含路由选择。每个固定候选独立从同一不可变 IR 投影，不能让前一候选的降级污染后一候选。投影若改变值，需重验剩余结构、依赖、大小与要求；它不能扩张已批准公共请求或目标能力，也不能靠换目标绕过授权。

<a id="cache-affinity-projection"></a>
## 缓存亲和与会话分组载体

下游只表达 Provider 无关的逻辑 conversation；HTTP carrier 归[网关指南](../http-gateway.md#conversation-context)，typed owner 为共享 `ConversationContext`。它区分稳定 conversation 与独立请求范围，不存历史，不证明认证、issuer 或 replay 兼容。上游 session/cache 载体由目标内部选择，消费者不承担 Provider 分组规则。

显式分组与下述有界隐式优化共用纯目标投影；缺省请求仍具有独立的请求 identity，推断亲和不把它改成真实 conversation。

- 有显式 conversation 时，同一逻辑对话保持 ID，切换模型/Provider 不要求更换。缺省 HTTP 请求在候选预检前分配一次性 identity；优化层只能为固定兼容候选选择内部亲和提示，不改写该 identity，不按用户、cache key 或凭据猜 conversation。
- `prompt_cache_key` 仍是独立可选缓存分组，可以跨 conversation 共用。显式 value/null 按目标准入与原 presence 处理；absent 时可从显式 conversation 或候选局部亲和提示派生默认 key。禁用或不适用隐式优化的独立请求不自动填充标准 key，未发送 hint 不等于关闭自动缓存。
- Go Chat 从显式中性上下文、候选局部亲和提示或请求本地分组派生 `x-opencode-session`；native Messages 保持其显式上下文路径。不从 cache key 反推 session，原生 body 不增加 cache/session 字段。派生编码使用有版本、用途隔离的确定性 SHA-256，生成有界 ASCII，算法归 [cache projection](../../src/protocol/cache.rs)，不是认证凭据或必要 replay 证明。
- HTTP Chat/Responses 均拒绝 Provider `session_id` body。纯库低层的显式 Provider 分组仍可用于独立原生 wire 准备；它与中性上下文不能同时成为权威。纯 codec 不生成随机 ID，也不解析 HTTP headers。
- 未映射的 advisory key/retention 仅按既有 cache projection 从目标副本省略；非法 typed 值先拒绝，活动 cache options 仍需目标准入。新上下文只进入可选缓存前缀检查，不增加 reasoning replay 的 conversation/key 绑定。

投影不写回共享请求、伪造 response echo/usage、保存会话、选择凭据或改变候选顺序。预检与发送使用同一规则；不同 conversation、用途和一次性范围不混为同一分组。OAuth 不授权复制产品 backend 的 session/thread headers。目标载体依据：[xAI Responses 缓存](https://docs.x.ai/developers/advanced-api-usage/prompt-caching/maximizing-cache-hits)、[Go session 要求](https://opencode.ai/docs/go/)。

<a id="implicit-cache-affinity"></a>
### 尽力隐式亲和

目标是在 ClientManaged 完整请求上改善同 Provider/Model 的原生缓存机会，不是回答缓存或历史服务。显式会话标识保持稳定方案；隐式优化失败不改变原请求、必要 replay 或执行结果的合法性。

- 亲和来源依次采用显式中性会话标识、未来有状态父记录的已存关系、短 TTL 前缀索引、独立新分组或目标允许的无提示路径。标准 `prompt_cache_key` 是另一维度的显式缓存意图，按原 presence/目标准入处理，不以它反推 session。
- 索引按认证主体、Provider/Model、已知上游授权/缓存域及投影版本隔离，不跨 Provider 复用。目标和凭据仍由固定执行规则选择；不能为了命中索引而换目标、换凭据或改序。无法界定兼容域时跳过隐式复用。
- 只对有限数量的完整 item 边界做稳定内容编码与精确前缀摘要匹配，保留影响请求的顺序、参数与必要签名。不使用 Rust Debug、易变本地 ID、语义相似度或编辑猜测；不将每轮变化的整个请求 hash 直接作为上游 key。
- 摘要范围由所选 profile 的 prompt 相关表示决定，排除本次派生的亲和提示和非 prompt 执行 metadata；不能借此省略实际送模型的 call ID 或其他有效内容。
- 从已验证的完整请求/输出建立可复用前缀记录，只保存摘要、亲和标识、作用域与期限，不保存正文。索引只说明可能的缓存亲和，不创建真实 conversation identity，也不允许补齐历史或重新签发 replay 证明。
- 编辑、压缩、格式变化、歧义、过期、禁用、容量不足或索引故障均可视为未命中；按原合同发送完整请求。该降级不吞原请求的验证错误，不添加 retry/fallback。TTL、条目/累计内存和每请求检查点均有硬界限，索引可随时丢弃。
- 稳定前缀优先于更复杂的匹配。是否派生 key、发送 session header 或使用原生缓存控制归具体 profile；不为所有目标强制同一策略。索引命中与实际 cache-read/usage 分别观察，未报告不补零，不以高命中率作为正确性门槛。

当前 Chat/Responses 的完整文本、普通 function 与可读 reasoning 历史使用有版本的[最终 wire 前缀编码](../../src/protocol/cache_affinity.rs)；媒体、opaque、custom/program/Provider tool history 等未声明组合跳过推断。完整目标值编码后才取摘要，保留 system/tools、实际 call/item ID、参数字符串、顺序和精确数值；派生 hint、delivery、计量及非 prompt envelope 不参与匹配。只查询有限完整 item 边界，登记完整请求及可表示的后继输出前缀，不截断、拼补或存储正文。

Gateway 的[索引](../../src/gateway/affinity.rs)以固定候选的受信 identity/scope 和 profile 修订隔离，默认启用且可关闭；TTL、记录/累计内存、编码和检查点硬界限归索引与编码 owner。非阻塞锁竞争、poison、禁用、过期、淘汰与同摘要不同分组的歧义均按未命中处理。凭据正常刷新不改变已绑定身份；材料/身份/epoch 失效仍由现有 loader 拒绝。

[候选接线](../../src/gateway/exchange.rs)验证固定凭据后选择提示，从原请求构造私有副本并重新预检；hint 超出预算时回到原投影，不把优化失败变成请求错误。严格上游 EOF、完整语义终态、最终目标投影及下游 handoff 成立后才发布记录，取消、失败或不可表示的后继输出不登记。纯 codec/lowering 不访问索引。启停方式归[启动配置](../credentials.md#gateway-access-绑定)，与权威历史存储的失败边界归[交互合同](interaction-contract.md#context-authority)。

## Semantic loss

目标是**低损而不是任意无损**，不构造通用保真百分比。投影分为：

| 类别 | 合同 |
|---|---|
| 精确映射 | 值、行为、关系及必要依赖保持，仅改变 wire 形式 |
| 等价归一化 | 在已声明前提下含义不变，具名规则和独立反例保护 |
| 有损兼容 | 明确哪些语义被省略/合并/降级，保证剩余结果仍满足目标合同 |
| 不可表示 | 无合法映射或超出允许损失时明确失败，不伪装成成功 |

**公开 Chat Completions 是兼容投影，允许部分语义损失。** 采用逐条定稿、独立验证的有限白名单规则，并在相应 Chat 兼容路径默认应用，不要求每个请求增加开关或重新批准；未定稿的规则仍按当前 strict lowering 拒绝，不是全局 best-effort。Responses 的主要接口和 Embedding 的标准接口不因此继承 Chat 的损失策略。

Responses 完善优先，不为 Chat 扩大标准核心以外的执行行为。每条有损规则至少固定：方向（request/history/response/event）、目标 profile、受影响 owner、具体损失、前提、保留的约束、对续轮/依赖的影响及独立预期。非必要展示信息的省略、多个文本单元的目标排列、附加报告的降级可作为分析对象，**不是本页已经批准的字段白名单**。精确取舍由实施切片定稿，不靠碰到错误时临时丢字段。`phase` 等可影响后继模型行为的 optional 信息不属于默认安全损失；schema 可缺省不证明已有报告可删除。

以下不属于普通 Chat 兼容损失：

- 弱化指令 authority、安全/权限、工具选择或行为约束来让请求通过；
- 改写工具 call identity、参数或结果关联，丢弃必要 opaque/replay 依赖却继续承诺同等续轮；
- 将音频、图片或向量冒充普通文本；未经明确内容变换合同用 transcript/caption 代替媒体；
- 把 refusal、失败、取消、截断或未闭合流改为完整成功；
- 补造 usage、timestamp、resource access、issuer 真实性，或绕过资源/提交边界。

损失应可由类型化投影结果或有界非敏感观察判别，不保存原始正文来说明损失；不能把“被省略”混同于“上游未报告”。观察的具体 API/存储在相应切片定稿，不为通知损失而默认添加私有 wire 字段。静态与流式须使用同一策略，不能在已经发布内容后改换策略或撤回事实。

**标准 wire 正确性、功能覆盖和保真度分别验收。** 有损输出必须仍是规范 Chat，而不是借兼容名义增加任意字段；往返不能被要求恢复已经声明丢失的信息。核心 IR 保留原始权威值，不为 Chat 的限制缩减设计。

<a id="message-envelope-projection"></a>
### 消息容器与目标重组

共享消息容器归[交互合同](interaction-contract.md#身份分组与依赖)。以下是独立批准的规则，不使 Responses 继承一般 Chat 损失许可；来源 IR 始终保留原分组，投影仅改变候选私有副本：

- `responses.omit-message-envelopes.v1`：request/history、response/event → 标准 Responses。省略目标没有位置的来源容器边界，保留有序 items、内容、call identity、原始参数、结果关联和终态。容器本身不是工具执行或逻辑 turn 边界。不承诺跨标准客户端保存后恢复原始分组，不编码私有字段或特殊 wire ID。携带 opaque 或其他必要 replay/依赖而无法保持其前提时仍拒绝。
- `chat.regroup-message-items.v1`：request/history、response/event → Chat。没有来源归属的 assistant message 与其后连续独立 function calls 可以组装成目标 assistant message；不跨 user、指令、结果或其他不支持的 item 合并，不把独立调用加入已有来源容器。空文本 owner、调用参数和 call-result ID 原样保留，不按名称重关联，不把空参数补成 `{}`。更广异构容器、多个文本单元或不能保持顺序的组合仍按所选 profile 拒绝。
- 目标分组是编码安排，不是源语义推断；解析 Responses 时不写入伪造 membership，编码后也不污染原观察、其他候选或后续重投影。静态和事件使用同一规则，事件失败不能发成功终态；typed projection 记录区分省略/重组与原本未报告。

方法依据：[OpenAI 迁移指南](https://developers.openai.com/api/docs/guides/migrate-to-responses#2-map-messages-to-items)明确将 Messages 拆成独立 Items，并以 call ID 连接结果；[OpenAI Agents SDK converter](https://github.com/openai/openai-agents-python/blob/38636a5c04d54717030878a133fd21970a0e1dec/src/agents/models/chatcmpl_converter.py)提供目标 assistant 累积/工具结果切断的实现参考。后者不是 API 的无损保证；不采用其参数补值、文本拼接、私有 metadata 或 opaque 重建策略。源码与独立预期拥有精确准入。

### Anthropic 原生工具投影

所选 request/history → Anthropic Messages 的 `anthropic.function-json-object.v1` 表示归一化和 `anthropic.omit-tool-execution-detail.v1` 有限损失由[原生合同](anthropic-messages-profile.md#参数身份与工具结果)维护。前者仅允许完整 JSON object 参数的编码表示变化；后者仅省略目标没有 carrier 的、调用者已报告的有限执行细节，保留错误位、正文与 call 关联。两者尚待实现，不放宽 Responses/Chat 或 Provider 工具准入，也不授权 native source 容器之外的自动重组。

### Chat 图片 token 明细投影

`chat.omit-input-image-tokens.v1` 只适用于 Generation response/event → Chat：目标 profile 未声明图片 token 明细载体时，默认从投影副本省略 `Usage.input_image_tokens`。Owner 是 [Usage](../../src/semantic/task/generation/usage.rs)，规则与有界诊断归 [projection](../../src/lowering/projection.rs)。显式零同样是已报告值，省略必须可检查；没有报告则不记损失，有载体则保持精确值。

- 先验证完整报告，再投影和重新验证最终值、依赖、预算与 requirements。只移除图片 token 明细；总量、其他明细、scope/basis/计数关系不变，不从图片数或文本推算计量，不将非法 usage 改成未知。
- 原始 IR 保持权威；`ResponseRepresentation` 以不可变借用或私有副本提供最终值与逐段规则、owner、方向、目标和修订。`reproject` 从上一段最终值重验并累积损失，不能从来源记录恢复省略值；独立候选仍从同一原观察开始。借用和私有字段防止修改已验证值/目标，输入、配置、资源或目标变化需要新的 lowering。
- 表示证明仅覆盖本地合同，不代表资源权限、issuer 接受或执行准入；未满足的必要本地前提仍返回错误，不用损失标记放行。事件目标从首次 encode 起固定；静态与事件共用同一规则，只有验证成功的终态提供完整投影诊断。
- 不修改请求/history、Responses 或独立媒体策略；不省略其他报告、不合并消息/part、不改变正文、分组、phase、工具关联、必要 replay、硬控制或失败/终态。新的损失继续单独定稿；未知规则拒绝。

<a id="responses-usage-projection"></a>
### 标准 Responses 的附属 token 明细

Generation response/event → Responses 允许独立、逐字段的 `responses.omit-*-tokens.v1` 规则；具体字段与 typed loss 枚举归 [projection](../../src/lowering/projection.rs)。它仅覆盖目标无载体的输入图像/文本/音频、输出文本/音频与预测接受/拒绝细分；目标有具名 carrier 时保留，不扩大入站字段准入。

先验证完整原报告，再投影副本并重验。Input/output/total、cache-read/cache-write、reasoning、scope/basis 与计数关系不变；不从细分推算总量、不把未知补零、不将 malformed usage 改成 absent。原 IR 保留全部报告；显式零的省略同样记录损失。静态与事件共用规则，重投影只从上一段最终值开始，fidelity 不能恢复删除值；各字段的记录共同受投影阶段预算限制。

本规则不涉及请求控制、正文/模态转换、工具行为、必要 replay 或终态。`StrictComplete` 对未报告的必填事实仍拒绝，不因为省略附属计量就获得完整报告。标准差异以固定 SDK 的 [ResponseUsage](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_usage.py) 与 [CompletionUsage](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/completion_usage.py)核对。

<a id="responses-fingerprint-projection"></a>
### Responses 的 system_fingerprint 投影

`responses.omit-system-fingerprint.v1` 仅适用于 Generation response/event → Responses：目标没有 Chat `ResponseContext.system_fingerprint` 的载体时，从私有 metadata 副本省略该字段。Absent 不产生损失记录；null 或有效 value 的存在均记录，原 metadata 保持原 presence/value，不把它改写成 user metadata、请求字段或私有 attachment。

完整 metadata 先验证；非法空值、超限值不能借省略通过。响应 ID/model/timestamps、其他 metadata、正文、usage、工具关联与必要 replay 不变。静态/事件使用同一规则，重投影只消费最终 metadata，不从原来源恢复字段；不把这个字段当作 opaque replay 或凭据，也不授权其他附属字段的省略。Owning code 与逐项诊断归 [lowering](../../src/lowering/generation.rs) / [projection](../../src/lowering/projection.rs)。

### 独立 Images 的计量投影

标准 Images 静态响应另有明确限定的 `OmitUnrepresentableAccounting` 策略，由受信 Route 选择，不继承或扩张 Chat 规则。方向仅为 ImageGeneration response → 标准 Images response；owner 为 [图片计量](../../src/semantic/task/image_generation/accounting.rs)，实现归 [Images lowering](../../src/lowering/images.rs)。

- IR 保留实际 token 报告、稀疏明细与精确 USD 费用；缺少 input text/image 明细时不得从纯文本请求、总量或默认值推算。
- 标准结构能完整承载 token 报告时输出 usage；否则省略整个 usage，不输出残缺的标准对象。费用及其附加报告没有标准位置，按该策略不输出。
- 严格策略仍拒绝不可表示的报告。投影只修改副本，并通过 typed loss flags 区分“上游未报告”和“投影省略”；不添加私有 wire 字段。
- 图片 bytes、格式与其他产物报告、真实完成、错误、资源预算和取消/提交边界不受损失许可影响。非法计量、非法或不完整产物必须先拒绝，不能靠删除 usage 获得成功；本规则不授权流式、请求控制或其他 task 的损失。

### 独立音频的附属报告投影

独立 Speech 的二进制输出与 Transcription 的简洁 JSON 分别采用具名、限定的附属报告投影，不继承 Chat 或 Images 的损失许可：

- Speech result → 标准 audio：只省略请求/产物引用身份、URL/expiry、字符计量及分句/对齐报告；音频 bytes 与实际报告的编码保持不变。不得从请求的 MP3 意图或 URL 后缀补造输出格式。Owner 为 [Speech result](../../src/semantic/task/speech_synthesis/result.rs)，投影和 typed omission flags 归 [Speech lowering](../../src/lowering/speech.rs)。
- Recognition result → 标准 JSON：完整转录正文和可准确映射的处理时长计量仍输出；请求身份、末句/词时间戳、标点细分及 channel 等无标准位置的附属报告只在该投影省略。Owner 为 [SpeechRecognition](../../src/semantic/task/speech_recognition.rs)，投影和 omission flags 归 [Transcription codec](../../src/protocol/openai/transcription.rs)。不得把末句报告变成全文时序，或从 language hint、词尾时间推算语言和文件总时长。
- IR 保留实际报告、单位、scope 和最终性；累计快照不相加。结果编辑使相关报告失效，纯库消费者可检查未投影的完整结果。不能把“投影省略”标作“上游未报告”，也不为通知损失新增私有字段。
- 两种投影都先验证完整结果；请求控制、媒体/转录正文、真实失败/终态、预算、取消和 handoff 不得受损。上游 SSE 聚合仅用于明确的 Speech profile，不授权下游流式交付或其他音频 operation 的损失。

### 独立 Embedding 的计量归一化与请求标识投影

Embedding 不继承 Generation、Chat 或媒体的损失许可。以下规则仅用于所选静态文本/float 分支：

- `dashscope.embedding-input-only-total.v1`：只有受信 DashScope Embedding profile 可以将其 `total_tokens` 解释为已报告的 input-only token aggregate。[同步 API](https://help.aliyun.com/en/model-studio/text-embedding-synchronous-api)明确该计数来自输入分词。IR 保存该 basis 和唯一 aggregate，未独立报告的 input counter 保持可判别；标准 `prompt_tokens` 是该已知 input-only aggregate 的派生 view，不是补零或从未知报告猜值。实际报告了 `prompt_tokens` 时逐值验证，不忽略 null、非法值或矛盾关系；缺少 total 仍拒绝。Standard/OpenRouter 的缺失 prompt 报告不能使用此规则。Owner 为 [Embedding usage](../../src/semantic/task/embedding.rs)，具名 intake 归 [codec](../../src/protocol/openai/embeddings.rs)。
- `embeddings.omit-response-identifier.v1`：Embedding response → 标准 Embeddings response，只省略目标没有 carrier 的 ancillary request/response identifier。原 [envelope](../../src/adapter/embeddings.rs)保留 ID 和 presence；[投影](../../src/lowering/embeddings.rs)在验证后的私有副本移除，并以 typed flag 区分未报告与已省略。Model、向量顺序/精度、索引、维度、usage/basis 不变；非法 ID 不得借省略通过。该 ID 不承载资源权限、续轮或 replay，不增加私有 wire 字段。

两条规则都不改变请求准入、标准 parser、错误、完整批次、EOF、预算或 handoff，也不授权 Base64 量化、其他报告省略或一般 best-effort。独立反例归 [Embedding tests](../../tests/semantic/embeddings.rs)，当前范围归 [profile](embedding-profile.md)。

## Source records

Source/fidelity records 只保存有界的表示形式、wire identity、来源与依赖证明，不保存能覆盖 typed 值的第二正文。复用要求 owner 仍存在、目标/profile/scope 兼容、依赖未失效，且不能恢复删除值。请求、静态响应与事件分别检查。

Opaque 值归 typed owner；fidelity 只绑定格式、来源/依赖。Replay 同时要求 value 和 owner 满足其格式的最终性及目标 scope；partial intake 不等于 history 可重用。绑定不能通过重新 hash 已修改历史伪造原始完整性，普通 scope 标签也不是 issuer 认证。具体依赖归[交互合同](interaction-contract.md)，实现归 [fidelity](../../src/protocol/fidelity.rs)。

SDK parsed/output-text 等派生视图只在所选合同下验证后丢弃，不覆盖 raw/structured authority。Classified extras 不承载未建模行为；需要 declared profile、预算、owner 和最终语义依赖。它们的省略条件与业务语义损失不能混同。

## 静态、事件与 Provider 边界

- Response requirements 从实际结果推导，不伪造 request 做检查。Public Model 的输入准入不充当响应 reported facts 白名单。
- Event lowering 与静态投影保持一致；terminal snapshot 不得补缺失事件、改写已交付值或完成不合法 partial。`StrictComplete` 可在终态前失败；要求先验证完整结果的消费者使用有界静态交付。
- Provider 只声明可信 origin/path/auth、安全 headers、错误分类与具体 profile；不能在 encode 后手术式改写 JSON，不能通过业务字段切换规则。
- 产品 profile 的事件终态摘要、可读 reasoning 或计量别名只有具名规则允许时成立，不扩张公共标准，也不成为新协议的通用开关集合。

Owners：[Adapter](../../src/adapter/mod.rs)、[WireRules](../../src/protocol/adaptation.rs)、[request/response lowering](../../src/lowering/generation.rs)、[event lowering](../../src/lowering/events.rs)、[shared protocol types](../../src/protocol/mod.rs)。当前 Responses/Chat 拒绝边界归相应 profiles；缺少 IR 概念按[缺口决策](semantic-ir.md#4-ir-不足与标准载体缺口)先报告结构方案，而不是添加兼容特例。
