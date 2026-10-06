# OpenAI Embeddings Create 协议参考

## 来源、范围与快照

本文是 `POST /v1/embeddings` JSON request/response 的唯一协议 owner。Embeddings 是独立向量 operation，不属于文本或媒体生成。

- 官方来源：[Create embeddings](https://developers.openai.com/api/reference/resources/embeddings/methods/create)、[Embeddings guide](https://developers.openai.com/api/docs/guides/embeddings)、[Python SDK request type](https://github.com/openai/openai-python/blob/main/src/openai/types/embedding_create_params.py)
- 原始资料复核日期：2026-08-04；使用前须重新核对动态 model、limit 或 SDK surface。

## 1. Request

| 字段              | 协议语义                                                                              |
|-------------------|---------------------------------------------------------------------------------------|
| `model`           | 选择 embedding model；目录分类不能替代 endpoint/profile 证明                         |
| `input`           | 单文本、文本批次、token input 或 token-input 批次                                     |
| `encoding_format` | 常见为 `float` 或 `base64`；default 与可显式选择值必须分别确认                        |
| `dimensions`      | 只适用于明确支持缩短维度的 model/profile                                             |
| `user`            | 可选终端用户标识，属于可能敏感的 request 数据                                         |

单项/批量形状、batch size、单项 token 与总 token limit 都依赖目标 profile。字符数或 UTF-8 bytes 不能冒充 tokenizer token 数。

## 2. Success response

response 使用 `object: "list"` 和有序 `data[]`。每项包含 `object: "embedding"`、`embedding`、`index`；顶层还包含 `model` 与
input/total token `usage`。

`data[].index` 关联 input 顺序，不能仅依赖数组当前位置。Embeddings 没有 assistant role、completion token、tool item 或 SSE terminal
event。

## 3. Encoding、dimension 与 budget

`float` 与 `base64` 产生不同 JSON size、decode 路径和内存预算。response budget 至少受 batch item 数、dimension、最坏序列化大小、
envelope 和 parser/decode buffer 共同约束。

没有明确 contract 时，本地 encoding 转换或降维不能宣称为上游原生兼容。若 fixed Provider adapter 显式将 float32 array 重编码为标准
Base64，必须固定使用 IEEE-754 little-endian float32 bytes、保持维度、拒绝非有限或超出 float32 的值，并将其描述为网关 wire
translation，而不是上游原生能力；JSON number 超出 float32 的额外精度不属于 Base64 wire 可保留的合同。

## 4. Vector identity 与重放

vector identity 至少依赖 immutable model/checkpoint、tokenizer/input encoding、dimension、归一化/距离语义及输出 encoding。endpoint
或 model 名称相似、维度相同都不足以证明跨 Provider 等价。

一次无会话 operation 不自动证明任意 retry/fallback 安全；response 开始提交后也不能拼接第二次 attempt 的向量列表。

## 5. 数据与证据边界

- 原始文本、token array、向量、Base64 和 `user` 不应进入普通日志或 metric label；
- 一个成功 sample 只证明该 model/input/encoding/dimension 组合；
- SDK helper 不能替代 HTTP/JSON wire；
- fixture/mock 不证明真实 Provider、当前 SDK、负载或长期兼容。
