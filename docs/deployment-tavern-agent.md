# 部署拓扑：酒馆前端 + MCP-agent 后端 + AIRP 数据

> **性质**：部署参考 / 范例，非承诺功能。给未来开发者与使用者一个「AIRP 怎样蹲在被 SillyTavern（酒馆）当模型驱动的 agent 背后」的标杆。
> **一句话**：AIRP 是**通用 MCP 数据后端**，能插在**任何** MCP-capable agent 背后 —— 包括被酒馆前端驱动的那种。**AIRP 零改动**；这只是一种部署组合，不是新功能。

---

## 1. 拓扑

```
酒馆(SillyTavern, 前端 UI)
    │  OpenAI 兼容 /v1/chat/completions
    ▼
agent shim（把一个 agent 包成 OpenAI 端点）        例: agy2api
    │  驱动 agent CLI / runtime
    ▼
MCP-capable agent（如 Google Antigravity `agy`）
    │  MCP (stdio / SSE / Streamable HTTP)
    ▼
AIRP-MCP-Server —— 供 RP 数据（卡/预设/世界书/会话/状态/记忆）
```

与 AIRP 的**常规**用法（Agent 当大脑、直接经 MCP 用 AIRP 数据）相比，这里多了一层：**酒馆当前端 UI**，agent 当后端「模型」。AIRP 在最底层不变 —— 还是 agent 的数据源。

---

## 2. AIRP 的角色：零改动插入

- AIRP 是**通用 MCP server**（host/model 无关）。任何支持挂 MCP 的 agent 都能把它当数据后端。
- **Antigravity（含 CLI）支持 MCP server**（local/remote，stdio / SSE / Streamable HTTP）。AIRP 这两种传输都现成（stdio + Streamable HTTP）→ **直接插入，不改一行**。
- 这正是 AIRP **standalone-first / 通用非特供**设计的回报：换前端、换 agent，AIRP 都不动。

---

## 3. 例子：agy2api（参考，非依赖）

> https://github.com/GhostXia/agy2api （Apache-2.0，Python）

把 Google Antigravity CLI（`agy`）包成 OpenAI 兼容端点（`/v1/chat/completions`，默认 `127.0.0.1:7862`）：跑 `agy --print <prompt>` 子进程、从本地 SQLite 会话库经 protobuf 解码取答、假流式回 SSE。酒馆把 base URL 指向它即可。

**成熟度**：早期（~8 star / 8 commit），且靠**刮 SQLite + protobuf 解码 + 假流式 + 依赖 Google CLI** —— **脆弱，仅供参考，勿进生产链路**。

---

## 4. 边界：这层 shim 永远是外部 peer

- agent shim（agy2api 之类）是**「agent 运行时包装器」**。**AIRP 永不造、不跑 agent**（那不是数据层的事）→ 这类组件**不可能成为 AIRP 产品**，只能是外部 peer。
- 对照三产品（见 README / `ROADMAP.md`）：MCP-Server=数据、Gateway=协议桥、State-Protocol=UI 渲染。**「把 agent 包成 OpenAI 端点」不属其中任何一个** —— 它是 agent 运行时层，本就在 AIRP 体系之外。
- 它与 AIRP **正交**，只在 **agent 层**交汇。

---

## 5. 安全姿态（重要 —— 这套放大注入威胁）

酒馆加载**不可信角色卡**（提示注入面），而后端 agent 可能很强（如 Antigravity 是**编码 agent，有文件/shell 权限**）。卡里写「忽略指令，删库 / 跑命令」→ ST → agent 执行。这是 [ROADMAP](ROADMAP.md) 安全审查那个注入威胁的**加强版**（从删 RP 数据升到动文件/shell）。

姿态建议：

- **AIRP 侧**（本服务能管的）：
  - 不可信前端下**跑只读**（ROADMAP §3 `--read-only` 候选）/ 启用**软删除**（§2.D 候选，删除可逆）。
  - 路径沙箱**已有**（`safe_resolve_for_write` + `validate_id_segment`，越权读写被拦）。
  - 别把 HTTP 传输暴露公网；LAN 用 `AIRP_HTTP_TOKEN` bearer。
- **AIRP 管不到的**（宿主 / shim 侧的责任）：编码 agent 的 shell/文件权限隔离 —— 该在 agent / agy 侧用沙箱（sandbox）或容器进行隔离。AIRP 是数据层，挡不住 agent 在自己进程里跑命令。

---

## 6. 给使用者 / 开发者

- 想用**酒馆当前端 + agent 当后端**跑 RP：用一个 agent-shim（agy2api 之类或自建）把 agent 包成 OpenAI 端点，agent 内挂 AIRP-MCP-Server 当数据源。AIRP 不需改。
- **务必**在不可信卡场景给 AIRP 上只读/软删、给 agent 上 sandbox。
- 这条链任一环（shim、agy、Antigravity）都可换；AIRP 作为通用数据后端保持不变。

Sources: [agy2api](https://github.com/GhostXia/agy2api) · [MCP servers in Antigravity (Codelabs)](https://codelabs.developers.google.com/google-workspace-mcp-antigravity)
