# Codec 与扩展验收基线

本页定义验证方法，不记录执行结果。语义不变量依据[Agent-first 设计](../architecture/semantic-ir.md)，协议预期分别依据所选官方合同；现有 Responses 依据[标准基线](responses-standard.md)和[固定来源](upstream-sync.md)，其他协议查[官方入口](providers/README.md)后另行固定版本。外部测试与模型 benchmark 不自动成为本地 oracle。引入资产或升级 SDK/profile 时重核版本、许可与独立预期。

## 1. 准入表必须按语义分支

每个已实现标准字段、item、event 或扩展要回答：

| 问题 | 验收内容 |
|---|---|
| 来源 | 官方 schema/guide、固定 SDK 或具体 extension profile；不能仅引用本地 encoder |
| Owner | task、request/response context、delivery、fidelity 或 scoped extension；只有一个值权威 |
| Presence | required/optional、absent/null/empty/default 的等价或区别；依赖其他字段的合法组合 |
| 双向映射 | 独立 wire→IR 和 IR→wire 预期；必要的 profile 差异 |
| Transform | 插入/替换/删除/重排；requirements 更新；owner/group/prefix/resource 依赖的保留或失效，悬空关系的显式修复或拒绝 |
| State | item/call/part/response、产物完整性、逻辑 turn、continuation、scope、finality 与 terminal；合法组合与未知值 |
| 拒绝 | unknown、错类型、身份/状态冲突、不可表示目标、跨 issuer 或错误后恢复 |
| 资源 | bytes、items、深度/nodes、schema references、padding、partial payload 与总状态预算 |

设计全景不等于每轮实现全部，但每轮完成的范围必须真正闭合。媒体和状态服务缺实现不是删掉语义目标的理由；一个协议能接收请求不证明它能交付并回传所需 continuation。标准规范性、能力覆盖度、保真度与自定义 carrier 分别验收：标准场景不能靠 `_openbridge`、本地事件名或 SDK 宽松保留额外字段才成立。Chat 兼容规则需独立断言实际损失、必要语义保留、依赖影响及静态/事件一致性；规范 wire 不等于无损，round trip 也不能要求恢复已声明丢失的信息。独立 `_openbridge` attachment 需作为拒绝输入验证，不构造新的 SDK unknown-field 回传承诺。扩展测试不能替代标准消费者检查。

多模态切片须固定 task 与标准 operation，覆盖来源/编码、输出类型与完整性、适用事件、资源引用及失败预算；vector 不能作为文本通过检查，语音不能用 transcript 代替媒体。发现 typed 承载不足时，先按 [IR 缺口规则](../architecture/semantic-ir.md#4-ir-不足与标准载体缺口)报告结构选择，再确定对应 oracle，不用当前 encoder 的输出反推目标设计。

## 2. 四个独立验证层

1. **语义与 codec**：typed invariant、正反映射、删除不复活、扩展 scope；无需网络。
2. **字节与生命周期**：严格 JSON、SSE framing、UTF-8/CRLF、分片独立预算、EOF、背压、cancel、pre/post-commit。
3. **固定消费者**：SDK create/stream/parse 及 structured replay、Agent tool loop；版本固定，synthetic loopback。
4. **外部实际执行**：固定 Provider/account/model/payload 的能力和错误；授权归 [AGENTS.md](../../AGENTS.md#standing-authorization-for-live-provider-verification)，不能由前三层替代。

模型输出质量和长期负载再单独评价，不要求 codec 引入大型真实会话或 benchmark 数据。Open Responses compliance 也必须与 OpenAI 标准版本分别标记。

## 3. 必须保护的闭合反例

- 已验证 IR 的外层 presence 与有效子字段不能矛盾，更不能在 encode 时静默省略约束。
- low-level input/snapshot 简写不能放宽完整 response 的必填 id/status；显式合成与上游缺字段分别处理。
- event 缺 required payload、跨 kind 字段、非法 status 不得被忽略后继续成功。
- JSON 重复键必须在失去原始键序列前按严格边界处理。
- Schema 不能只检查 object/bytes；嵌套类型、strict/profile、引用与顺序分别验证。
- 每种 SDK derived view 都需要独立的回放准入与一致性规则；一种派生 view 通过不证明另一种也支持。
- 标准 phase、configuration update、媒体、工具与 state 必须分支验收，不靠一个两轮 fixture 声明完整。

涉及 Generation 续轮的切片还须独立审查以下相关边界，未实现前不能视作现有覆盖；这些不是全部媒体/Embedding 的前置，更不要求先细化 Realtime：

- 正常 response 终止但仍等待工具结果或 continuation；后继 response 不复活旧 reducer，工具失败不冒充生成失败。
- Opaque-only 内容、非 reasoning attachment、成组回放与跨响应关联；缺失载体、变换失效或 scope 不匹配时拒绝，不自动丢失历史。
- 流停止但参数未完整/无效；只收到 signature 的部分值；静态与事件最终性一致，不能修补成成功。
- 工具媒体结果及引用的双端依赖；源文档重排只改变 wire 坐标，删除源后不得继续输出旧引用。
- 缓存前缀变化与标记 owner 生存分别检查；reported usage、累计快照和合法派生分开，不重复计数或猜测缺失值。

独立反例由 [semantic](../../tests/semantic.rs) 和 [transport](../../tests/transport.rs) 测试维护；剩余审计范围见[实施缺口](../implementation-status/generation.md#验收缺口)。新发现的问题须区分违反合同、未准入与缺少验收，不能从测试存在推断已经通过。

## 4. 属性顺序、JSON 与 Schema

JSON object 通常无语义顺序，但 Structured Outputs 官方明确承诺按 schema key 顺序生成输出。实现应明确 schema owner 的顺序表示，不能用普通 map 的排序规范化覆盖该合同；是否启用保序解析必须以实际实现验证。

重复 JSON key 在进入 Value 前解决；资源验证不能在已经无界 parse/clone 后才发生。递归 schema 的 `$ref` 是引用图，不等于无界输入 nesting；两种预算需要分开，不能为阻止栈爆而拒绝所有合法递归 schema。

## 5. SDK 升级门槛

当前消费者与传递依赖在 `tests/sdk/pyproject.toml` 和 `uv.lock` 固定；版本与执行入口见[开发指南](../development.md)。升级应比较 required/null/default、derived views、items/events 和输出 schema，再更新锁文件并执行严格多轮 JSON/SSE gate；其通过不消除其他未覆盖分支的验收缺口。

SDK 宽松解析成功不证明完整 wire 正确；严格模型验证也不证明 gateway state machine。新版本 type union 可包含尚无完整 endpoint 参数/资源语义闭环的分支，需要记录冲突而非补猜。

## 6. 接受扩展的专项反例

- 合法 namespace + 错误 attachment/schema/version 拒绝；
- final token 与 owner、origin、principal scope 不匹配拒绝；
- 同 turn 重放允许，跨 turn/auth owner 不沿用 Codex sticky state；
- 扩展不得覆盖已有共享字段或传入上游地址/认证；
- 稳定 local identity 不因排序变化被重建；删除 owner 后不回填旧 signature/annotation；
- 提升为共享语义后不再双写共享 owner 和 extension owner；协议同名不构成等价证明；
- 不可迁移的 media/resource 引用不在 fallback 中自动跨目标转移。

## 7. 资产与证据卫生

优先自主编写小型 synthetic fixture，并保留源测试路径、commit、许可与抽象行为。GPL/AGPL、限制商业使用的数据、enterprise 目录或混合来源 benchmark 不能未经审查复制。原始源码/Provider transcript 不是默认测试数据；不保存密钥、账户、真实会话、媒体或签名 URL。
