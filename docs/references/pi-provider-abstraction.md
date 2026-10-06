# pi 的 Provider、Model 与协议抽象

本页是 pi 模型接入机制的固定版本技术参考，解释服务商身份、wire protocol、模型目录、统一 transcript、流事件和 Agent 执行的责任边界。它不是 MorphieCore 的设计决策、Provider 支持清单、运行结果或迁移实施计划；采用边界仍由 [Semantic Model](../architecture/semantic-ir.md)、[protocol/lowering](../architecture/protocol-and-lowering.md)和 [execution model](../architecture/execution-model.md)拥有。

## 1. 来源、版本与阅读边界

- 固定上游：`earendil-works/pi` 的 `v1.0.2`，提交 [`cd32f7725fdbddbaecdff5b1e68491563394e0ca`][pi-revision]，采用 [MIT License][pi-license]。
- 对象：`@earendil-works/pi-ai`、`@earendil-works/pi-agent-core` 和 `@earendil-works/pi-coding-agent` 的模型接入边界。实现依据为该版发布包的 JavaScript、TypeScript declarations 与随包文档；下方链接固定到对应源码提交，不跟随 `main`。
- 核心入口：[pi-ai README][ai-readme]、[Provider / Models][ai-models]、[共享类型][ai-types]、[自定义 Provider][custom-provider-doc]、[Coding Agent ModelRuntime][model-runtime]。
- 现有 [扩展与上下文](extensions-and-context.md)的 pi `0.99.2` 投影和 [SIWC 登录](siwc-login.md)的专项来源保持原用途。本页不隐式升级它们，也不替代官方 API、认证条款或具体模型准入证据。

源码描述客户端怎样构造和消费请求，不证明服务端接受所有字段、某账户可用、目录元数据准确或真实推理成功。以下示例只说明组装方式，不授权登录、目录刷新、联网、安装扩展或调用模型。

## 2. 核心结论

pi 将 **Provider、API implementation 和 Model 分开**，让 Agent 依赖统一的消息与流事件，而不是服务商 SDK DTO。它不把所有请求先转换成 OpenAI wire，也不为每个服务商维护独立 Agent loop。

- **Provider 是运行时行为单元**：拥有服务商身份、认证策略、最后已知模型目录、可用性过滤与模型操作。
- **API implementation 是可复用的 wire 实现**：把统一 transcript 转成目标协议，发送请求，并将响应归一化。
- **Model 是可序列化描述**：指明 Provider、API、端点及能力/限制，不携带执行函数。
- **Models 是 Provider 集合**：查询与注册 Provider，驱动认证、刷新和请求分派。
- **ModelRuntime 是 Coding Agent 的配置运行时**：组合内置 Provider、目录、`models.json`、扩展和虚拟模型。
- **Agent loop 拥有工具执行与续轮**：Provider 产生工具调用，不实际执行用户工具。

因此，多个 Provider 可以共享一个 API implementation；同一个 Provider 也可以按模型使用多种 API。统一接口带来的是调用与会话管理的一致性，**不是任意 Provider 语义的无损中间表示**。

## 3. 分层与调用链

```text
Coding Agent / Agent loop
  │ StreamFn：统一 transcript、通用 options、统一事件
  ▼
Models 或 Coding Agent ModelRuntime
  │ 查找 model.provider 对应 Provider
  │ 解析认证，合并 headers / env，归一化 Context
  ▼
Provider
  │ 自定义行为，或 createProvider 组合既有 API implementation
  │ 单 API：直接复用；多 API：按 model.api 分派
  ▼
API implementation
  │ transcript → 目标 request / replay
  │ provider response / events → AssistantMessage / events
  ▼
SDK / HTTP / SSE / WebSocket
```

这里的 `Models` 与 `ModelRuntime` 是两种上层调用入口，不意味着 Coding Agent 每次请求都再经过 `Models.stream()`。固定实现的 `ModelRuntime.prepareRequest()` 取得注册的 Provider、解析请求认证，然后直接调用其 `stream` / `streamSimple`。

| Owner | 负责 | 不应由这个名称推定 |
|---|---|---|
| `Model` | 模型和操作元数据、API 选择、目标 URL、能力/限制、定价及兼容配置 | 已认证、已激活、一定可执行 |
| `Provider` | 认证策略、目录、过滤、发现和请求行为 | 每个 Provider 都有独有 wire protocol |
| `ProviderStreams` | 一种 chat API 的 `stream` / `streamSimple` 实现 | 工具执行或 Agent orchestration |
| `Models` | 集合、认证驱动、目录刷新、请求便利方法 | 公共 HTTP 网关或独立语义准入体系 |
| `ModelRuntime` | Coding Agent 配置组合、可用性快照、注册与路由接线 | 纯 codec 或协议标准 |
| Agent loop | 消息累积、工具参数验证与执行、续轮和取消接线 | 各家 SDK 的请求编码 |

实现导航：[Models][ai-models]、[ModelRuntime][model-runtime]、[Agent 类型与 StreamFn][agent-types]。

## 4. Provider：身份、目录、认证与行为的组合

### 4.1 原生 Provider 合同

`Provider` 的必需部分是 `id`、`name`、`auth`、同步 `getModels()` 以及 `stream` / `streamSimple`。可选部分包括全操作目录、动态刷新、按凭据过滤、图片生成、分类和 deferred response 操作。精确类型由 [models.ts][ai-models]拥有，本页不复制完整 schema。

每个 Provider 都声明认证语义。无密钥的本地服务也可以用 `apiKey.resolve()` 返回 `{ auth: {} }` 表示已配置；这不同于没有认证配置、解析返回 `undefined`。接口名称中的 `apiKey` 不要求所有请求最终都有一个字符串 key。

`createProvider()` 把这些部件组装为 Provider。内置工厂通常只声明服务商元数据、认证、目录和惰性 API wrapper。例如固定 [Groq 工厂][groq-provider]复用 `openAICompletionsApi()`，而 [Anthropic 工厂][anthropic-provider]复用 `anthropicMessagesApi()`。

### 4.2 一对多与多对一的协议关系

`createProvider({ api })` 的 chat 实现有两种形式：

1. 一个 `ProviderStreams`：该 Provider 的 chat 请求使用同一实现。
2. 按 `model.api` 索引的实现表：每个模型使用对应实现；表内缺少相应实现时返回流错误，而不是尝试猜协议。

固定 [GitHub Copilot 工厂][copilot-provider]同时组装 Anthropic Messages、OpenAI Chat Completions 和 Responses，是单个 Provider 拥有多种 wire 的直接例子。它还根据 OAuth credential 的账号模型集合过滤可用目录。这个例子说明架构关系，不构成服务端或当前账号的支持矩阵。

图片和 classifier 实现分别通过 `images`、`classifiers` 按 API 索引；只提供这些操作的 Provider 不必有实际 chat 实现，但 `createProvider()` 至少需要一种非空操作实现。

### 4.3 可复用不等于没有服务商分支

共享 API implementation 内部仍可能检查 `model.provider` 或 `baseUrl`。例如固定 [OpenAI Chat 实现][chat-api]包含兼容检测和 Copilot 动态 headers；[Responses 实现][responses-api]也包含特定认证路径和端点行为。

所以 pi 的分层是**降低 wire 重复、保留客户端特例**，不是声明底层实现完全不认识服务商。API implementation 还执行网络 I/O；不能把它直接等同于 MorphieCore 的纯 codec/lowering。

## 5. Model：纯描述与操作身份

Model 是普通可序列化数据，没有 stream 方法或 SDK client。公共基础值包括身份、`provider`、`api`、`baseUrl`、输入种类、成本与可选 headers；chat 另有 reasoning、上下文和输出限制、thinking 映射及兼容配置。精确字段与条件类型归 [types.ts][ai-types]。

需要区分三个事实：

- `provider` 选择拥有认证和行为的运行时对象。
- `api` 选择 wire implementation，不是模型厂商或任务名称。
- `id` 是该 Provider、该操作类型下的模型 ID，不是独立全局模型身份。

固定目录包含 chat、image、classifier 三种操作类型。chat 的 `type` 可以省略，表示默认 chat；image/classifier 需要显式 discriminant。同一 Provider 下相同 ID 的不同操作可以分别存在，查询时须包含类型，而不是仅用 ID 合并。

`getModels()` / `getModel()` 返回 chat 模型；`*OfType()` 查询指定操作；`getAllModels()` 返回混合目录。目录存在、认证已配置、账号允许、wire 可表示和一次执行成功是不同层次，不由单个元数据标志互推。

## 6. 输入：Context 归一化为 Transcript

### 6.1 只在入口处理便捷字段

公开调用接受 `Context`，允许用 `systemPrompt` 和 `tools` 作为便捷字段。`normalizeContext()` 将其合入开头的 `SystemMessage`，Provider 和 API implementation 收到的是只有 `messages` 的 `TranscriptContext`。

因此自定义实现不能继续从 `context.systemPrompt` / `context.tools` 取值，应使用 `getCurrentSystemPrompt(context.messages)` 与 `getCurrentTools(context.messages)`。直接调用 API module 则绕过 Provider 认证与入口归一化，需要自己准备认证和 `normalizeContext()` 的结果。

固定 owner：[transcript.ts][transcript]、[自定义 Provider 合同][custom-provider-doc]。

### 6.2 System message 是状态变化载体

开头的 system message 建立初始提示词与工具集合；后续 system message 可以追加提示、修改命名 sections、增加或移除工具。辅助函数按序重建当前状态。

目标能接受中途 system message 时，适配层可保留变化位置；否则 `collapseSystemMessages()` 将当前提示与工具折叠为开头的 baseline，并去掉后续 system message。不同协议还有各自的工具变化 carrier，须查具体 implementation 和 compat，不能从 transcript 能表达推定目标能原样编码。

这个设计让 Agent 记录提示与工具变化而不必改写全部历史。但折叠后的有效前缀可能变化，不能由相同 session/cache key 推定缓存仍命中。

## 7. 输出：统一消息、事件和终态

### 7.1 AssistantMessage 与内容块

Chat 输出统一为 `AssistantMessage`，内容按序包含 `text`、`thinking` 和 `toolCall`。文本、reasoning 和工具调用可携带 Provider 特定的 opaque replay metadata；工具结果与用户消息还可包含图片。

消息记录请求的 `provider`、`api`、`model`，并可记录实际返回模型、response ID、Provider thinking level、诊断、原始停止原因等。这些字段用于归因或回放，不使 opaque 值变成任意目标可解释的共享语义。

`Usage` 区分 input/output、cache read/write、总量和成本。成本依赖目录价格与报告口径，不能把客户端计算当服务商账单；初始化的零值也不证明上游确实报告为零。精确输出类型归 [types.ts][ai-types]和 [消息文档][message-doc]。

### 7.2 统一流事件

```text
start
  ├─ text_start / text_delta / text_end
  ├─ thinking_start / thinking_delta / thinking_end
  └─ toolcall_start / toolcall_delta / toolcall_end
done 或 error
```

关键约束：

- setup 失败可以直接发 `error` 而没有 `start`；成功内容更新和 `done` 不能早于 `start`。
- 开始生成后恰有一个终态，取消通过 aborted 结果表示。
- `contentIndex` 关联各 block；不同 block 的事件可以交错，不能假设一个 block 的所有事件连续出现。
- `partial` 是共享的可变 response-so-far，不是事件发生时的不可变快照。保留事件引用不能得到历史状态。
- 工具参数在增量过程中是 best-effort parsed JSON；`toolcall_end` 才提供完整解析结果，但仍未完成工具 schema 验证。
- `pending` 属于流中 partial，不能当持久完成消息；普通终态还区分 stop、length、toolUse、error、aborted，具备相应实现时另有 deferred handle。

`complete()` / `completeSimple()` 使用对应 stream 的 `.result()`，不用另一套非流式转换。事件队列与结果接口归 [event-stream.ts][event-stream]；流中断不是成功终态。

### 7.3 Provider 与 Agent 的工具边界

Provider 负责把声明映射到目标 wire、解析调用和关联 ID。Agent loop 接收统一 `toolCall`，执行参数验证、允许的工具调用与结果回传，然后决定下一轮。

`StreamFn` 的合同是返回统一流；模型、请求或 runtime 失败通过错误事件和完成消息表达，不向 Agent loop 抛出未处理的请求失败。直接 API 调用的 setup 可能同步抛错，使用这种入口时不能照搬 collection 的失败假设。

## 8. 通用 options 与协议专有 options

pi 不用一份万能 request options 表达所有协议控制，而是保留两个入口：

| 入口 | 含义 |
|---|---|
| `streamSimple` / `completeSimple` | 通用 options，包括统一 reasoning level、预算、取消和请求 hooks |
| `stream` / `complete` | 具体 API 的完整 options；已知 API 由 `ApiOptionsMap` 提供类型关联 |

动态目录查询通常得到 `Model<Api>`。需要完整 API options 时，通过 `hasApi(model, apiId)` 在运行时确认 API 并收窄类型，而不是把未知模型强制断言成某协议。

统一 `reasoning: "medium"` 由 implementation 映射到 effort、thinking budget 或目标格式。`thinkingLevelMap` 可声明 Provider/model 值和不支持的档位，通用入口会约束有效档位。统一名称不证明不同模型投入相同计算或生成同等 reasoning；reasoning 关闭、显示方式、summary 和 replay signature 也不是一个概念。

[Simple options helper][simple-options]还处理输出上限与上下文估算，以及模型默认采样、有效 thinking 档位采样和请求采样的合并。它是客户端策略，不是目标硬限制的证明。部分 free-form sampling 参数只适用于特定 API，不能因 options 类型接受就推定所有 implementation 生效。

## 9. Compat：共享 wire 下的有限差异

`compat` 描述目标在已知 API 家族中的差异，而不是另建整个 Provider DTO。典型关注点包括 role、token 上限字段、流式 usage、严格工具 schema、reasoning 字段、工具结果形态、中途 system message 和工具变化。

固定 implementation 对一部分已知 Provider/URL 有默认检测；显式 compat 可覆盖，未设置部分继续使用检测的默认值。因此，仅根据端点宣称 OpenAI/Anthropic-compatible 设置所有 flags 是不充分的，需要查具体 wire 约束。

受约束工具生成也不只有一个 tools 标志。JSON schema 的 `strict: "prefer"` 在不支持时允许退回普通工具调用，`strict: "require"` 则必须拒绝无法保证的目标；grammar 工具按支持的变体与模型 metadata 选择或退回 function 工具。它们说明需求、表示能力和降级政策须分别看待；Provider 能调用工具不证明能满足每一种约束。

精确兼容类型归 [types.ts][ai-types]，执行选择归 [constrained-sampling.ts][constrained-sampling]和受影响 API，不在本页维护 flags 全集或动态能力矩阵。

## 10. 跨模型回放：可继续不等于无损

固定 [transformMessages()][transform-messages]的判断依据是源 assistant message 与目标的 **provider、api、model ID 三者同时相同**，不只是 Provider 或协议相同。回放变换作用于送往目标的历史，不代表存储中原始语义天然等价。

| 输入情况 | 固定实现的处理 | 语义后果 |
|---|---|---|
| 目标不支持图片 | 用户或工具结果中的图片替换成文本占位提示 | 丢失图片内容；不是媒体编码别名 |
| 跨模型的可见 thinking | 转成普通 `text` block | 丢失 reasoning 的类型/authority 区分 |
| 跨模型的 redacted thinking | 删除该 block | 不再保留目标不适用的 opaque 内容 |
| 相同模型且有 thinking signature | 保留，包括空可见文本的签名块 | 支持该模型的 replay，不证明跨 scope 可移植 |
| 跨模型的文本签名、工具 thought signature | 删除相关签名 | 不再承诺原签名回放依赖 |
| 工具 ID 不符合目标约束 | 调用目标提供的 ID normalizer，并同步工具结果关联 | 关联须整体变化，原 wire identity 不保持 |
| 有工具调用但没有结果 | 插入 `isError: true`、正文为 `No result provided` 的合成结果 | 这是客户端修复，不是一次真实执行报告 |
| error / aborted assistant | 回放时跳过 | 不将不完整生成当合法成功历史 |

变换还延后落在工具调用与结果之间的 system message，使其不错误关闭尚待匹配的工具结果。ID normalizer 由具体 API 提供，不能从公共函数的存在推定所有目标采用相同 ID 规则。

固定 README 对 handoff 使用了较宽泛的描述，包括 tagged thinking 和保留 partial 历史；实际公共变换先将可见 thinking 转为普通文本，并跳过 error/aborted assistant。精确目标 wire 仍须追踪后续 codec。不能把 README 的“seamless”当无损合同，也不能据其文字保证取消后的 partial 会原样回放。

## 11. 认证：Provider 策略，集合驱动与可注入存储

### 11.1 认证输出不是只有 key

Provider 可以提供 API-key/ambient 和 OAuth 方法。认证解析结果的 `auth` 可包含 `apiKey`、headers 和 base URL，另有 Provider-scoped `env` 与来源标签。因此账号认证可以改变请求 endpoint；把 access token 放进内部 `apiKey` 字段也不使它成为静态 API key。

`Models` 驱动解析并合并请求 options。通用顺序为 Provider auth headers → Model headers → 显式 request headers → `transformHeaders`；header 名称按大小写不敏感合并。`transformHeaders` 属于集合级入口，会在 Provider 分派前消费，不下传给 Provider。

显式请求值优先，认证返回的 base URL 可以形成请求时 Model 副本。env 是 Provider-scoped 配置输入，不进入 transcript。固定 Coding Agent 的凭据优先级与 `models.json` 解析归 [ModelRuntime][model-runtime]、[Provider composer][provider-composer]和 [模型文档][models-doc]，不把它当所有调用入口的统一配置格式。

### 11.2 存储、刷新与登录交互

`CredentialStore` 以 Provider ID 存储带类型的 credential，提供 read、仅非秘密 metadata 的 list、串行 read-modify-write 的 modify，以及 delete。默认 collection 使用内存 store；应用可以注入持久 store。OAuth 刷新在 modify 内进行，跨进程互斥取决于 backing store 的正确实现。

已存储 credential 拥有该 Provider 的认证路径；刷新失败不应悄悄退回环境 key。配置可用性检查、实际 auth resolution、登录、refresh 与注销是不同操作；目录 picker 的可用性不是服务端账号验证或推理成功。

OAuth 通过 UI-neutral prompt/notify 交互实现浏览器、设备码和手工输入；阻塞认证工作接收 signal。认证与登录合同归 [auth/types.ts][auth-types]及 [auth/resolve.ts][auth-resolve]，不在参考文档复制 token、私有 auth 文件或完整 Provider 响应。

## 12. 目录发现、刷新与配置组合

### 12.1 同步读取与显式异步刷新

`getModels()` 同步返回最后已知目录；网络更新由显式 `Models.refresh()` / `Provider.refreshModels()` 驱动。动态 Provider 可以从持久 snapshot 恢复，然后在允许联网时获取更新。静态目录读取不应被理解为实时上游查询。

原生 `refreshModels()` 返回 void，通过 `context.publish({ persist?, update? })` 发布 Provider-owned 状态；可选 persist 写入或删除 `ModelsStore`，同步 update 才更新私有内存目录。集合通过 generation 和串行 publication 防止旧刷新覆盖新的 Provider 注册或刷新。网络和阻塞工作仍须 honor signal；丢弃陈旧 publication 不等于已经停止底层 I/O。

`createProvider({ fetchModels })` 提供较简便的动态 overlay：同类型、同 ID 的动态 entry 替代 baseline entry，其他 entry 追加。账号过滤与目录刷新独立，刷新失败不能伪装为新目录成功。

### 12.2 Coding Agent 的组合层

[ModelRuntime][model-runtime]组合内置 Provider、可恢复/远端目录、原生扩展 Provider、legacy 扩展配置及 `models.json`。原生扩展可成为 base，配置层在其上应用 override；具体优先级和保留行为归 [provider-composer.ts][provider-composer]。

`ModelRegistry` 是暴露给扩展的同步兼容 facade，Coding Agent 内部使用 ModelRuntime。`ctx.modelRegistry.streamSimple()` 是扩展内进行 Provider-neutral nested call 的入口，不要求扩展另写认证或 wire 编码。[ModelRegistry 源码][model-registry]维护精确方法。

配置的任意 headers、credential commands、endpoint override 和 payload hooks 都处于受信客户端边界。它们不是可直接暴露给未受信下游请求的透传接口。

## 13. 新服务接入与版本化接口

### 13.1 选择最小接入方式

| 需求 | 固定版接入方式 |
|---|---|
| 现有 API 下增加模型或 endpoint | `models.json` |
| 自定义认证、账号过滤、目录发现 | 原生 Provider，复用已有 API implementation |
| 不支持的 wire protocol | 实现 stream / streamSimple 及统一事件合同 |

Coding Agent 支持两种注册形式：完整 Provider，或 `registerProvider(name, ProviderConfig)` 的 legacy 形式。新集成拥有较多行为时应优先原生 Provider。仅覆盖 endpoint/headers 可保留原目录；legacy 中提供 models 会替换该 Provider 的跨操作目录。原生与 legacy 的 refresh 返回合同不同，不能混用。

[GitLab Duo 示例][gitlab-example]展示了通过 legacy 配置注册自定义行为、再委托既有 Anthropic/Responses 实现的方式；它说明可组合性，不是所有新 Provider 都应复制其认证或 wrapper 细节。

### 13.2 原生组装示例

下面用 `.invalid` 保留域名和自定义 Provider ID 展示静态组装，不运行生成请求，也不要求真实 credential。实际值不得从业务输入选择。

```typescript
import { createModels, createProvider, envApiKeyAuth } from "@earendil-works/pi-ai";
import { openAICompletionsApi } from "@earendil-works/pi-ai/api/openai-completions.lazy";

const models = createModels();
const model = {
  id: "example-chat",
  name: "Example Chat",
  provider: "example-service",
  api: "openai-completions" as const,
  baseUrl: "https://models.example.invalid/v1",
  input: ["text"] as ("text" | "image")[],
  reasoning: false,
  contextWindow: 32768,
  maxTokens: 2048,
  cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
};

models.setProvider(createProvider({
  id: model.provider,
  name: "Example Service",
  baseUrl: model.baseUrl,
  auth: { apiKey: envApiKeyAuth("Example API key", ["EXAMPLE_API_KEY"]) },
  models: [model],
  api: openAICompletionsApi(),
}));

const selected = models.getModel("example-service", "example-chat");
// Lookup does not resolve authentication or issue an inference request.
```

若多个模型采用不同 API，`api` 可以改成按 API ID 索引的实现表，同时给各模型正确的 `api`。不要只改 Model 的字符串而不注册相应实现，也不要以同一个服务商名字推定只有一个协议。

### 13.3 新旧接口分辨

固定版的主接口是 `createModels()`、Provider factories、`createProvider()` 和 collection methods。旧教程中的全局 `stream()` / `complete()`、`registerApiProvider()` 与按 `model.api` 的全局 registry 位于 `/compat` 入口，不是本页描述的主路径。

API 实现位于 `packages/ai/src/api/`，工厂位于 `src/providers/`。lazy wrapper 在第一次使用时加载实现和 SDK；特定 Provider import 只带入其目录与 wrapper，`providers/all` 显式引入全部内置目录/工厂。能否拆成实际 lazy chunks 取决于 bundler/code splitting，不能从 dynamic import 语法推定最终单文件包没有 SDK。

## 14. 独立操作、路由、hooks 与信任边界

### 14.1 操作分离

chat、image、classifier 共享集合与 Provider 认证，但入口不同：chat 使用 stream/complete，image 使用 `generateImages()`，classifier 使用 `classify()`。图片和 classifier 返回各自结果，不伪造 assistant tool loop 来表示独立操作。

它们的具体输入、输出和失败形态由各自类型拥有。固定实现的 one-shot 操作将失败编码为结果，不意味着可以把任意 task 塞进 chat Context。

### 14.2 虚拟模型位于物理 Provider 之上

[Virtual Models][virtual-doc]将用户选择的虚拟 model/thinking level 路由到物理 model/thinking level。Provider 只接收物理模型；assistant message 记录实际 dispatch 身份，selection 则由会话的 model/thinking change 记录。

路由在每次请求前进行，可以区分 user、continuation、retry、direct，并使用 previous/failed 或 branch state。固定合同不允许路由到另一个 virtual model；物理目标仍需有效认证。路由可以换模型，但缓存和 replay scope 随目标重新判断，不由虚拟名称保证稳定。

ModelRuntime 的 direct virtual call 在跨 Provider 时不沿用原 Provider 的 apiKey/headers/env，改由目标解析认证。这是身份切换的重要边界，不等于已证明任意扩展 wrapper 都正确隔离凭据。

### 14.3 观测与转换 hooks

- `onPayload` 在发送前观察或替换目标请求 payload；它能改变请求，不是纯通知。
- `onResponse` 在取得响应后、消费 body 前观察状态与 headers。
- `onProviderStreamEvent` 在归一化前观察已解析的 Provider event，不保证是原 HTTP bytes 或 SSE frame，SDK 也可能已经丢弃字段。

这些 hooks 按流顺序 await，慢 handler 会阻塞消费。API-level observer 抛错与 Coding Agent 扩展 event handler 的错误隔离不能混为一谈；精确语义见 [API README][ai-readme]和 [扩展文档][extension-doc]。

Provider 扩展在 Pi 进程内拥有文件、提示词、工具和凭据访问权限，应当视为受信代码，而不是安全沙箱。取消、request deadline、SDK retry、Agent recovery、连接清理和 replay 是不同生命周期；统一接口不会自动替所有自定义实现建立资源预算或取消闭环。

## 15. MorphieCore 的采用边界

本页提供外部实现方法，不增加 ADR 或修改本项目合同。适用的结构线索是：分离服务商身份与 wire；让消费者使用统一交互表示；保持任务操作独立；将认证、目录和路由放在执行边界；在目标表示层明确兼容差异。

不能直接移植为本项目默认行为的部分包括：

- **语义表示**：pi 的 transcript、text/thinking/toolCall 并不是 MorphieCore Generation 有序 items、资源、引用、presence 和独立 task 的完整替代。缺失概念必须按 [IR 缺口规则](../architecture/semantic-ir.md#4-ir-不足与标准载体缺口)处理，不能藏在 compat 或 raw JSON。
- **有损回放**：图片占位、thinking 转 text、signature 删除、ID 重写和合成工具结果属于有具体后果的变换，不自动进入 MorphieCore 的 [损失许可](../architecture/protocol-and-lowering.md#semantic-loss)。尤其合成结果不能被报告为真实工具执行。
- **I/O 所有权**：pi API implementation 将转换与网络封装在一起；MorphieCore 纯 codec/lowering 不得查询目录、解析 credential 或联网。
- **目标与认证**：Model 的 endpoint、Provider env、auth-derived base URL、任意 headers 和 payload replacement 是受信客户端配置。本项目业务数据不得选择可信 origin、credential、auth/proxy headers 或脚本。
- **回放与计量**：Provider/model 字符串相同不替代 issuer/principal、owner dependency 和 finality 校验；初始化 usage、客户端成本估算与目录标志不替代真实报告。
- **运行时策略**：自动 OAuth refresh、retry、compaction、虚拟路由、WebSocket continuation 和 cache warming 均不因参考存在而成为 Gateway 默认行为或本阶段前置。

cache/session/turn 与登录细节分别归[扩展与上下文](extensions-and-context.md)、[OAuth/OIDC 来源](README.md#oauth-standards)及 [Grok](grok-login.md)/[SIWC](siwc-login.md) 专项 owner，本页不建立第二份合同。

## 16. 核对与验收层级

使用本参考时按证据层次核对，不以文档或接口存在代替执行：

1. **固定源码**：Provider/API 对应、目录身份、options 类型、归一化、变换、失败与 ownership。
2. **离线合成边界**：使用自主构造的 Provider/stream 与独立预期，检查多 API 分派、transcript、headers、签名失效、ID 关联、缺失结果和错误/取消回放；不载入真实认证。
3. **协议 fixture**：分别核对 wire→消息与消息→wire、流/静态一致性及畸形/partial 输入，不只看 round trip。
4. **隔离消费者**：Agent 工具参数验证、工具结果续轮、扩展事件、取消与 cleanup 分别验收。
5. **真实 Provider**：账号/endpoint/wire 接受、TLS/network、推理、usage 与 cache 收益需要明确目标、预算和脱敏的独立授权。

这些是方法与未覆盖边界，不是本页记录的成功结果。文档整理不证明运行时改进、一般 Agent 兼容性、真实 Provider 能力或生产就绪。

[pi-revision]: https://github.com/earendil-works/pi/tree/cd32f7725fdbddbaecdff5b1e68491563394e0ca
[pi-license]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/LICENSE
[ai-readme]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/README.md
[ai-models]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/models.ts
[ai-types]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/types.ts
[groq-provider]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/providers/groq.ts
[anthropic-provider]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/providers/anthropic.ts
[copilot-provider]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/providers/github-copilot.ts
[chat-api]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/api/openai-completions.ts
[responses-api]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/api/openai-responses.ts
[transcript]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/utils/transcript.ts
[event-stream]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/utils/event-stream.ts
[transform-messages]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/api/transform-messages.ts
[simple-options]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/api/simple-options.ts
[constrained-sampling]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/api/constrained-sampling.ts
[auth-types]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/auth/types.ts
[auth-resolve]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/auth/resolve.ts
[agent-types]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/agent/src/types.ts
[model-runtime]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/coding-agent/src/core/model-runtime.ts
[model-registry]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/coding-agent/src/core/model-registry.ts
[provider-composer]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/coding-agent/src/core/provider-composer.ts
[gitlab-example]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/coding-agent/examples/extensions/custom-provider-gitlab-duo/index.ts
[custom-provider-doc]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/coding-agent/docs/custom-provider.md
[models-doc]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/coding-agent/docs/models.md
[message-doc]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/coding-agent/docs/message-types.md
[extension-doc]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/coding-agent/docs/extensions.md
[virtual-doc]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/coding-agent/docs/virtual-models.md
