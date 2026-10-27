# Instruction authority and Provider dialects

Researched **2026-10-25** against official API references, SDK implementations, and Provider specifications. [Architecture](../architecture.md#instruction-authority-and-canonical-roles) owns accepted canonical instruction roles and conversion rules.

## Contents

- [Instruction conversion review](#instruction-conversion-review-2026-10-25)
- [Instruction authority research: current, legacy and Provider dialects](#instruction-authority-research-current-legacy-and-provider-dialects)
  - [OpenAI: API system versus Model Spec authority](#openai-api-system-is-not-automatically-model-spec-system-authority)
  - [Messages: operator system and placement](#messages-operator-system-placement-and-version-are-independent)
  - [Provider dialects](#provider-dialects-accepted-spelling-versus-effective-role)
  - [Directional conversion recommendation](#directional-conversion-recommendation)
- [Vercel Provider instruction cross-check](#vercel-provider-instruction-cross-check-2026-10-25)
  - [Source boundary](#source-boundary)
  - [Findings by operation](#findings-by-operation)
  - [Counterexamples and disposition](#minimal-counterexamples-and-disposition)

## Instruction conversion review, 2026-10-25

Review of Messages, Chat, and Responses instruction intake, target encoding, and typed continuation.

| Source | IR / Target behavior | Assessment |
|---|---|---|
| Explicit Chat/Responses system/developer | Ordered instruction owner; authority and position survive Chat/Responses. Messages uses native placement subset | Preserve order; do not hoist later instructions |
| Responses root `instructions` | Request setting, leading developer for default Chat, or root system for Messages | Target-specific projection; current request scope |
| Messages `system:"P"` vs `system:[{type:text,text:P}]` | String maps to root settings; blocks map to leading System instruction | Representation-dependent authority discrepancy resolved by canonical Developer normalization |
| Managed Responses continuation | New settings replace old root instructions; explicit instruction history persists | Ownership follows continuation contract; does not flatten history |
| Named or cached instructions | Unsupported participant or cache carriers reject at target boundary | Intentional boundary; text compatibility does not imply carrier support |
| Later instructions in Messages | Native system-message placement checked; invalid positions reject | Preserves turn boundaries without silent reordering |

## Instruction authority research: current, legacy and Provider dialects

Separates wire role spelling, instruction author authority, temporal placement, and target model capabilities.

### OpenAI: API system is not automatically Model Spec system authority

- Pinned OpenAI Chat `system` parameter is defined as "Developer-provided instructions" and directs developers to use developer messages with o1 and newer.
- Pinned Chat `developer` parameter explicitly "replaces the previous `system` messages". Starting with `o1-2024-12-17`, reasoning models support developer messages.
- OpenAI Model Spec (2026-08-18) distinguishes vendor-reserved System instructions (OpenAI-only) from Developer instructions. API `system` represents caller-supplied application instructions, not vendor-level authority.
- Migration from legacy API `system` to `developer` is an application-instruction modernization, not a privilege change.

| Boundary | Evidence | Significance |
|---|---|---|
| Historical Chat | Python v1.54.0 (2024-11-04): `system` only | Pre-developer baseline |
| o1-preview launch | Announcement (2024-09-12): no system message | Target model capability constraint |
| Production o1 | Python v1.58.0 (2024-12-17): `developer` and `system` | Model-specific developer migration |
| Current Chat | Verified pin v3.37.0: both exist | `developer` is preferred application role |

Responses root `instructions` are defined as "a system (or developer) message inserted into the model's context", scoped to the current generation request.

### Messages: operator system, placement and version are independent

- Anthropic Messages accepts root `system` (string or text blocks) and mid-conversation `system` messages. There is no native `developer` role.
- Documentation defines system instructions as originating from "you, the application operator", winning conflicts over user messages. Mid-conversation and root system carry identical authority.
- Mapping caller-controlled Messages `system` to canonical `Developer` in MorphieCore accurately reflects application-operator semantics.
- Content-bearing system messages cannot begin the message array (initial instructions belong at root) and cannot interrupt tool-use/result pairs.

### Provider dialects: accepted spelling versus effective role

| Provider / Operation | Documented carriers | Effective semantics |
|---|---|---|
| DeepSeek Chat | `system`, `user`, `assistant`, `tool` | Developer role undocumented; target `system` preserves operator channel |
| DeepSeek Responses | `developer` treated as `user`; root `instructions` inserted as `system` | Developer role loses application authority; target must use `system` |
| DashScope Chat | `system`, `user`, `assistant`, `tool` | `system` sets role and constraints; string content required |
| DashScope Responses | `system`, `developer`; root `instructions` inserted as `system` | Both roles outrank user; root instructions do not inherit across continuation |
| Xiaomi Chat / Responses | `developer` and `system` supported | Developer instructions supported; root instructions separate in Responses |
| OpenRouter Chat / Responses | `developer` and `system` admitted in schema | Route-dependent; downstream models normalize independently |

### Directional conversion recommendation

| Source → Target | Evidence-backed relation | Boundary rule |
|---|---|---|
| Legacy OpenAI system → current developer | Documented application-channel modernization | Dependent on target model support |
| Current developer → provider operator system | Preserves application-operator role | Requires target system support; declare loss |
| Messages root system → developer / instructions | Canonical application authority | String and block forms normalize identically |
| Developer → Messages system | Operator-channel mapping | Leading to root; mid-turn to supported positions |
| Privileged instruction → user text | Prohibited fallback | Authority loss; reject rather than silently demote |
| Mixed caller system + developer | Order-preserving retention | Preserve source order; do not invent arbitrary precedence |

## Vercel Provider instruction cross-check, 2026-10-25

Inspection of pinned Vercel repository revision [`594495a5af70547d9d7a13d6df334ae30db49622`](https://github.com/vercel/ai/tree/594495a5af70547d9d7a13d6df334ae30db49622) across provider converters and capability selectors.

### Source boundary

- Pinned packages: `ai` 7.0.150, `@ai-sdk/openai` 4.0.101. Other provider packages inspected at matching repository revision.
- Generic compatible Chat converter emits `system` in encounter order. Official OpenAI selector uses `developer` for recognized reasoning models, falling back to `system` for unknown/third-party IDs.

### Findings by operation

| Target operation | Vercel comparator | Primary provider contract | Disposition |
|---|---|---|---|
| Zhipu Chat (GLM) | `@ai-sdk/zai` emits `system` | Schema lists `system`, string only | Use `system` carrier; string content only |
| Zhipu Responses | No native Vercel factory | Schema admits `developer`, requires `type:message` | Preserve `developer`; emit required type discriminator |
| NVIDIA hosted Chat | Generic compatible emits `system` | Nemotron schema requires `system`, string only, first turn only | Restrict to initial turn, string content |
| NVIDIA hosted Responses | No verified hosted contract | Self-hosted NIM differs from hosted API | Unverified; require explicit target contract |
| OpenCode Go Chat | Generic compatible emits `system` | Go forwarder changes model ID, not role | Match client-adapter default or verify upstream |
| DashScope / Token Plan Chat | Native Alibaba emits `system` | Schema requires `system`, string content | Map to `system`; multipart arrays reject |
| DashScope Responses | No native Vercel factory | Admits `developer` and `system` | Preserve `developer` |
| Xiaomi Chat / Responses | Generic compatible emits `system` | Schema documents `developer` | Retain `developer` |
| DeepSeek Chat / Responses | Native provider emits `system` | Responses treats developer as user | Map to `system` dialect |

### Minimal counterexamples and disposition

1. **Zhipu Chat**: Use `system` application carrier; enforce string representation.
2. **Zhipu Responses**: Preserve `developer`; include `type:message` discriminator.
3. **NVIDIA Chat**: Enforce single initial instruction and string content; reject multipart arrays.
4. **Bailian / DashScope Chat**: Enforce string system content; reject multipart arrays without lossy flattening.
5. **Canonical IR**: MorphieCore normalizes all application instructions to canonical `Developer` in IR, while dialects handle target wire spelling (`developer` vs `system`) and content shape (string vs array).
