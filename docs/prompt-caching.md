# 提示缓存与 `[[CACHE_BREAK]]` 标记 — 设计参考

> **性质**：参考 / 范例，非承诺功能。给未来开发者与使用者一个「如何在 AIRP 体系里看待提示缓存」的标杆。
> **一句话**：缓存能省 token，但它属于**调 LLM 的边缘层**（客户端 / Gateway），**不进 AIRP-MCP-Server**（铁律：不调 LLM）。MCP-Server 至多吐一个**中性标记**，翻译留给边缘。

---

## 1. 背景：为什么关心缓存

长 RP 会话里，每轮都把**稳定内容**（角色卡、预设正文、世界书、人设）连同**易变内容**（近期对话）一起发给模型。稳定部分逐轮重复 → token 成本与延迟随会话线性膨胀。这与 AIRP 一贯的 token 纪律（`seal_volume` 归档、分页读、`apply_lorebook` 关键词触发）是**同一主题的另一根杠杆**：

> 让模型**复用**已计算过的稳定前缀，而不是每轮重算。

Anthropic 的 **prompt caching** 正是干这个：把稳定前缀标记为 `cache_control`，命中缓存的部分按折扣计费。

---

## 2. 参考范例：ST-ClaudeCacheGateway

> https://github.com/shanye5593/ST-ClaudeCacheGateway （MIT，Node.js 零依赖；引用时为早期项目，宜参考其**思路**而非作为依赖）

一个**本地代理**，做的事很聚焦：

- 把提示词里的 `[[CACHE_BREAK]]` 标记，在转发给上游（Anthropic / 兼容端）**之前**，转成 Claude 原生 `cache_control` 块（Claude 每请求最多 4 个缓存断点）。
  - 注：Anthropic 缓存的服务端 TTL **默认 5 分钟**（每次命中自动续期），另有 **1 小时**扩展档（beta，需对应 header）；不是任意时长。该网关默认请求 1h 档，并非 Anthropic 的默认值。
- 对外收 `POST /v1/chat/completions`（OpenAI 式）与 `POST /v1/messages`（Anthropic 式），SillyTavern 把 base URL 指向它（如 `http://127.0.0.1:8788`）即可，**不打断 ST 原有的记忆/世界书/正则/预设插件链**——它们先跑完，最终请求才到这个网关。

**值得学的点**：用一个**人类可写、后端无关的标记**（`[[CACHE_BREAK]]`）划出「稳定 | 易变」边界，把**模型特定的翻译**（→ `cache_control`）下沉到一个**薄边缘代理**。关注点分离得很干净。

---

## 3. 分层原则（本项目的核心教益）

缓存**不该进 AIRP-MCP-Server**，原因是铁律，不是偏好：

| 关切 | 该在哪层 | 为什么 |
|:--|:--|:--|
| 调 Claude / 注入 `cache_control` | **客户端 / Gateway（边缘）** | MCP-Server **不调 LLM**；缓存是 LLM-API 关切 |
| `cache_control` 是 Anthropic 专有 | **边缘可特供** | 边缘允许针对后端优化；**核心必须通用**（通用优先于特供） |
| 划「稳定 \| 易变」边界 | **可在 MCP-Server，但只作中性标记** | 见 §4 |

> 判据（同 [ROADMAP](ROADMAP.md) §0）：「只有某后端 / 某消费者用得上」= 特供 → 下沉边缘。`cache_control` 绑 Anthropic，是典型边缘活。

---

## 4. AIRP 体系里**合法**的接入点

MCP-Server 的提示拼装工具（如 `build_scene_system_prompt`、`export_context_bundle`）**可选地**在「稳定块（卡 / 预设 / 世界书）」与「易变块（近期对话）」之间，输出一个**中性标记** `[[CACHE_BREAK]]`：

```
<角色卡 + 预设正文 + 世界书 + 人设>      ← 稳定，适合缓存
[[CACHE_BREAK]]
<近期对话 / 本轮输入>                     ← 易变
```

为何**合律**：

- 吐一个**字符串标记** ≠ 调 LLM、≠ 解析/校验语义 → 不违「不调 LLM」「戒律 4（零 schema 旁路）」。
- 标记**后端无关**（不是 `cache_control`，不绑 Anthropic）→ **不是特供**。任何缓存网关（上面那个项目、或 AIRP-Gateway、或自研客户端）按需把它翻译成各自后端的缓存指令；不认识它的客户端**当普通文本忽略**即可。
- 与 AIRP 的 token 纪律同源，纯增量、可选、不破坏既有契约。

翻译那一步（marker → `cache_control`）始终在**边缘**完成。各司其层。

> **实现注意**：Anthropic 的 `cache_control` 挂在**结构化内容块**上（`system` / `messages` 的 `[{type:"text", text, cache_control}]` 数组），不是扁平字符串里的内联标记。所以边缘翻译时要：按 `[[CACHE_BREAK]]` 把扁平提示**切成结构块**，给标记**之前**的稳定前缀块附 `cache_control`，标记本身从文本里去掉。MCP-Server 只管吐标记，这层重组是边缘的活。

---

## 5. 给开发者的建议

1. **参考，别依赖**：转换逻辑极小（零依赖、本质是 marker→`cache_control`）。早期单作者项目不宜进生产链路——要用就**重写**进 AIRP-Gateway 的 Anthropic 模式，做成**可选开关**。
2. **安全**：任何此类代理都**看得见 API key 与全量提示词**。路真流量 / 真 key 前**必须审码**（该项目零依赖，可审）。
3. **保持通用**：边缘可针对 Claude 优化，但别把 Anthropic 假设回灌进 MCP-Server 或通用客户端逻辑。其他后端（无缓存或机制不同）必须仍能跑。
4. **MCP-Server 侧**：唯一候选是 §4 那个**可选的中性标记**；要不要做，进 [ROADMAP](ROADMAP.md) §2/§3 评估。

## 6. 给使用者的建议

- 用 **Claude** 跑长 RP 又想**省 token / 降延迟** → 在**客户端或 Gateway 层**加提示缓存（参考上面的项目或自建薄代理），把稳定前缀缓存住。
- **别期待 AIRP-MCP-Server 自己做缓存**——它是纯数据层、不调模型。它能帮的是**把稳定内容拼装好**（并可选标出缓存边界），省钱那一步在边缘。
- 换非 Claude 后端时，缓存策略随后端走；AIRP 的数据与标记保持不变。
