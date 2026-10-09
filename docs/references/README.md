# 协议来源入口

这里只维护仍用于设计的标准出处、固定版本和必要许可。接受的决策归 [ADRs](../architecture/README.md#架构决策)，实现细节归代码和邻近注释；不保留历史分析、项目比较或测试报告。

| 入口 | 用途 |
|---|---|
| [固定上游来源](upstream-sync.md) | OpenAI SDK、Codex 的固定提交、许可和官方页面入口 |
| [推理控制与 Schema 来源](upstream-sync.md#reasoning-schema-sources) | Interactions v1、Messages 控制区别及 JSON Schema 2020-12 的有限设计证据；不代表原生接入 |
| [Responses 标准基线](responses-standard.md) | Responses codec 的固定公开语义；不是共享 IR 的上限，也不等于本地准入 |
| [扩展与上下文](extensions-and-context.md) | session/cache/turn、存储与连接状态；固定 Codex/pi 投影，与认证 owner 分开 |
| [pi Provider、上下文与回放](pi-provider-abstraction.md) | 固定 `1.0.2` 的 Provider/API/Model 与 replay、`1.0.4` 的 ClientManaged 投影入口 |
| [OAuth/OIDC 标准](#oauth-standards) | 共用协议来源；实现、存储与操作授权归[凭据指南](../credentials.md) |
| [Grok Build / xAI 登录](grok-login.md) | 官方浏览器/标准设备授权、pi 内置与补充参考、credential/backend 边界 |
| [Codex 产品协议识别](chatgpt-login.md) | 产品认证的固定出处与协议隔离；不是本项目登录入口 |
| [公开 SIWC 与 ChatGPT plan usage](siwc-login.md) | 官方条款、动态 registration、身份/权限、renewal 与推理限制 |
| [多模态与资源](multimodal-and-resources.md) | task、wire、资源与媒体的语义边界 |
| [Codec 验收方法](conformance-baseline.md) | 独立 oracle、变换和失败/资源边界；不是执行记录 |
| [OpenAI operation 导航](openai/README.md) | 集中定位标准资料与既有日期；Embedding 有独立任务来源，不复制多份字段/事件快照 |
| [Provider 官方入口](providers/README.md) | 包括 Google Gemini、Anthropic Messages 的一手设计参照；是查询导航，不是兼容清单 |

IR 的共享语义由[设计基线](../architecture/semantic-ir.md)定义，不由任一参考协议独占。现有 OpenAI SDK/Codex 固定版本保持原有用途；新增协议前另行固定所选 operation、API/schema/SDK 版本与许可，不从动态网页推定稳定合同。

新增或修改协议行为时，按需核对一手来源，将必要 URL 和非显然理由留在 owning code；只有跨模块决策才更新 ADR。固定标准、SDK consumer 和产品私有协议不能混为同一合同。不要为每次调查新增分析页，也不要把本地实现缺口写成标准限制。

使用外部资产前核对具体版本、许可、敏感性和独立预期；默认自主编写最小 synthetic fixture，不复制真实会话。实际 Provider/模型准入按 [AGENTS 查询流程](../../AGENTS.md#current-provider-model-and-compatibility-information)确认，历史成功和源码类型均不能替代现场验证。

<a id="oauth-standards"></a>

## OAuth/OIDC 标准

- [RFC 6749](https://www.rfc-editor.org/rfc/rfc6749.html)：authorization-code 与 refresh grant。
- [RFC 7636](https://www.rfc-editor.org/rfc/rfc7636.html)：PKCE。
- [RFC 8252](https://www.rfc-editor.org/rfc/rfc8252.html)：native app、系统浏览器与 loopback callback。
- [RFC 8628](https://www.rfc-editor.org/rfc/rfc8628.html)：标准设备授权及 pending、slow-down、拒绝与过期。
- [RFC 9700](https://www.rfc-editor.org/rfc/rfc9700.html)：OAuth 安全最佳实践、refresh rotation 与重放风险。
- [OpenID Connect Core](https://openid.net/specs/openid-connect-core-1_0.html)、[Discovery](https://openid.net/specs/openid-connect-discovery-1_0.html)：issuer/metadata、ID-token 验证及 nonce；JWT payload decode 不是签名验证。
- [RFC 7009](https://www.rfc-editor.org/rfc/rfc7009.html)：token revocation；本地清理、远端撤销与删除 registration 分开。
- [RFC 8707](https://www.rfc-editor.org/rfc/rfc8707.html)：resource indicators；scope、resource/audience 与推理 backend 分别绑定。

标准不决定具体产品的 registration、订阅资格、scope、账户/workspace、redirect URI 或 endpoint。公开 client ID 不是 secret，也不授予第三方复用资格。具体 authority 来源归 [Grok](grok-login.md)、[SIWC](siwc-login.md)；项目决定归 [ADR 0012](../architecture/decisions/0012-grok-personal-credential-pool.md)，操作授权与自有存储归[凭据指南](../credentials.md#授权与安全边界)。
