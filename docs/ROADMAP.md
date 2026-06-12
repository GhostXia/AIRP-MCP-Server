# AIRP-MCP-Server — 计划与护栏（ROADMAP）

> **定位**：追踪「下一步做什么、为什么、以及**故意不做什么**」。不是功能介绍。
> **真理顺序**：源码 > 本文档。冲突时先改文档再继续。
> 最后更新：2026-06-12

## 0. 判据（动手前先过这条）

> **「一类协议 / 传输 / 纪律都受益」→ 通用基建，可提前做。**
> **「只有某个具体消费者 / 业务形态用得上」→ 特供，不做（或下沉到 Gateway 配置）。**

铁律不变：纯 MCP 数据层 · 不调 LLM · 不做推理 · 通用优先于特供 · 决策下放 Agent。
边缘关切（限流 / REST 映射 / OpenAI 兼容 / 前端鉴权）属于 **Gateway**，不进本服务。
传输级安全（bearer / CORS）可进，因为它服务于传输本身、对所有该传输的客户端通用。

---

## 1. 进行中（Active）

### A · HTTP 传输集成测试 ← **下一步**

**背景**：R1–R8 的 Streamable HTTP（`feat/http-streamable-transport`，已合并）目前**只编译过、未实测活体握手**。CI 有 linker，可在进程内跑真实 HTTP 往返，把「编译过」升到「验证过」并永久防回归。

**目标**：不起网络、不依赖外部客户端，进程内验证 `/mcp/v1`。

**做法**（新 `tests/http_transport_test.rs`，独立 target）：
- 构造与 `run_http_server` 同款的 axum app（`StreamableHttpService` 挂 `/mcp/v1` + 鉴权 + CORS）；为可测，把 app 装配抽成一个 `pub(crate)` 或测试可达的函数（如 `build_router(server, auth_token)`），`run_http_server` 复用它。
- 用 `tower::ServiceExt::oneshot` 对 app 打请求，无需 `TcpListener`。
- 用例：
  1. `POST /mcp/v1` 发 `initialize`（带 `Accept: application/json, text/event-stream`）→ 断言 200 + 响应头含 `Mcp-Session-Id` + body 含真实 `protocolVersion`/`serverInfo`。
  2. 带该 session 头发 `notifications/initialized` → 202。
  3. 带 session 头发 `tools/list` → 解析（SSE 帧或 json）→ 断言**38 个工具**。
  4. `tools/call` `list_characters` → 断言真实内容（非空 `{}`）。
  5. 鉴权：设 token 后无 `Authorization` → 401；正确 bearer → 放行。
  6. 缺/错 `MCP-Protocol-Version`（已初始化会话）→ 400。

**退出标准**：上述用例在 CI 全绿；CI 的 `cargo test` 自动覆盖（无需改 workflow，新 target 自动跑）。

**风险/注意**：
- 响应可能是 SSE（`text/event-stream`）—— 测试需从 SSE 帧里抽 JSON。或给该会话/测试用 `json_response` 模式简化断言（评估 rmcp 是否允许按请求切）。
- session-id 从响应头取，后续请求回带。
- 本机无 MSVC linker → **本测试只能靠 CI 验**（与现状一致）。

---

## 2. Backlog（通用基建，按需触发，不预建）

| 项 | 触发条件 | 说明 |
|:--|:--|:--|
| **优雅关停** | 真要长跑 HTTP 部署时 | SIGTERM → rmcp `StreamableHttpServerConfig.cancellation_token`，干净终止会话；stdio 走关闭序列 |
| **工具/Schema 稳定策略** | 现在就该立规约 | **加法-only**：不改工具名、不删字段、破坏性变更才升版本。客户端靠契约，契约稳=永不为某客户端返工。写进本文件即生效（见 §4） |
| **可观测性** | 生产部署 / 排障需要时 | 结构化日志已具；按需加请求级 tracing / 指标。**不为加而加** |
| **健康/就绪探针** | 容器/编排部署时 | 已有 `/health`；需要时加 `/ready` |

---

## 3. 故意不做（护栏 · YAGNI / 特供陷阱）

- ❌ **新传输预建**（WebSocket / gRPC）—— 无需求。要时 rmcp 给，几行挂上。
- ❌ **OpenAI 兼容 / REST 映射 / 限流** —— 永远 Gateway 的活，进本服务即违初衷。
- ❌ **插件沙箱 / ACL** —— 违戒律 4（零 schema 开放接入）。
- ❌ **多租户鉴权 / 聚合多上游** —— 前者投机；后者是 Gateway 的 fan-out。
- ❌ **为配置而配置** —— 只有实需才加 env 开关（如 `AIRP_MAX_READ_BYTES`）。
- ❌ **为某个具体消费者预留接口** —— 见 §0 判据。Gateway 也只是普通 MCP 客户端，不享特殊适配。

---

## 4. 工具/资源契约稳定规约（立即生效）

客户端（Claude Code / Cursor / pi / Gateway / 任意 MCP 客户端）依赖工具与资源的**契约**。为「永不为某客户端返工」：

1. **不改名**：已发布的 tool / resource / prompt 名不变。
2. **不删/不改义**：已有参数字段保留语义；废弃只标注、不立即移除。
3. **只加法**：新增工具 / 可选参数随意；破坏性变更**必须**升 `Cargo.toml` 版本并在变更日志记。
4. **资源 URI 稳定**：`airp://...` 形态不破坏。

违反此规约 = 让下游客户端崩 = 制造「针对性重写」，正是本项目要避免的。

---

## 5. 变更日志

- **2026-06-12** 建档。确立 §0 判据 + §3 护栏 + §4 契约规约。下一步 = §1.A HTTP 集成测试。
