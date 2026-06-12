# AIRP-MCP-Server — 对外接口需求：HTTP Streamable Transport

> **来源**：AIRP-Gateway — https://github.com/GhostXia/AIRP-Gateway
> **收录**：2026-06-12 · **状态**：✅ 已实现（编译/CI 绿；活体 HTTP 握手待 Gateway 实测）
> **链路**：`前端 → AIRP-Gateway → AIRP-MCP-Server → Agent`

本文记录 AIRP-Gateway 对本服务 HTTP 传输的对接需求，供追踪完成情况。需求正文逐字收录于下方，未改写。

## ⚠️ 优先级：HTTP 是「按需」，不是硬阻塞

审计了 AIRP-Gateway（其 `docs/DESIGN.md` 的 ADR/看板/R6 实测）后的结论：

- **stdio 路径现在就能对接，MCP-Server 零改动。** Gateway 拉起 `airp-mcp mcp --data-dir` 子进程、行分隔 JSON-RPC，对接本服务的真实 MCP。前端↔后端跑通**不需要本服务做任何改动**——瓶颈在 Gateway 自己的端到端验证（其 Stage 1），是 Gateway 侧的活。
- **下方 R1–R8（补完 HTTP `/mcp/v1`）只在一种拓扑下才需要**：把 MCP-Server 当作**独立远程 HTTP 服务**（分布式 / 非子进程 / 多网关共享）。本地「Gateway 拉子进程」拓扑**用不到 HTTP**。
- 因此：**先别为此改 stdio 或急着补 HTTP。** 确认要「远程 HTTP 部署」时再实施 R1–R8；推荐路径（rmcp `transport-streamable-http-server` feature）已审好。
- **不要**让 MCP-Server 自己长出 REST/限流/前端鉴权——边缘关切属于 Gateway 层，本服务保持纯 MCP 数据层。

## 代码核实（截至 `main` @ 25581a9）

Gateway 所述桩状态**属实**：

- `Cargo.toml`：`rmcp = { features = ["server", "transport-io", "macros"] }` —— 缺 `transport-streamable-http-server`。
- [`src/transport/http.rs`](../src/transport/http.rs)：`handle_mcp_post(State(_state) ...)` 返回 `"result": {}`，从未派发给 rmcp；SSE 为 `broadcast` 全局通道，无 `Mcp-Session-Id`。
- stdio 模式（`serve_server(Router, rmcp::transport::io::stdio())`）为真实 MCP，**保持不动**。

## 状态追踪

| 项 | 内容 | 状态 |
|:--|:--|:--|
| R1 | `POST /mcp/v1` 真实派发 rmcp，返回真实 `result`/`error` | ✅ rmcp `StreamableHttpService` |
| R2 | 生命周期 `initialize` → `notifications/initialized` → `tools/list`/`tools/call`/`resources/read` | ✅ rmcp |
| R3 | 会话 `Mcp-Session-Id`（响应头 + 校验 + SSE 按会话隔离） | ✅ rmcp（`LocalSessionManager`，`stateful_mode`） |
| R4 | 校验 `MCP-Protocol-Version` 头 | ✅ rmcp（`validate_protocol_version_header`） |
| R5 | 内容协商 `application/json` vs `text/event-stream` | ✅ rmcp |
| R6 | 鉴权 `AIRP_HTTP_TOKEN` bearer + 常数时间校验，统一作用 `/mcp/v1` | ✅ 本服务 `require_bearer_auth` route_layer |
| R7 | CORS 允许/暴露 `Authorization, Mcp-Session-Id, MCP-Protocol-Version` | ✅ 本服务 `CorsLayer`（暴露 `mcp-session-id`/`mcp-protocol-version`） |
| R8 | 规范 JSON-RPC error code（协议不符 `-32602` 等） | ✅ rmcp |

### 实现（`feat/http-streamable-transport`）

- 开 rmcp feature `transport-streamable-http-server`，用 `StreamableHttpService` 挂 `/mcp/v1`，替换手写桩。R1–R5、R8 由 rmcp 提供；R6/R7 本服务在外层包裹。
- `allowed_hosts` 关闭（`disable_allowed_hosts`）以支持局域网部署（电脑后端 + 手机同 wifi）；安全模型 = bearer + 信任局域网。**勿公网暴露。**
- **CI 验证**：编译 + 单元测试 + clippy + fmt 全绿。
- **待实测（CI 验不了活体 HTTP）**：Gateway 的 3 条验收 —— `initialize` 返 `Mcp-Session-Id`、带会话头 `tools/list` 返 38 工具、`tools/call` 返真实内容。需起服务用 curl/Gateway 实跑（本机无 MSVC linker，未本地起服务）。

---

## 需求正文（逐字收录，AIRP-Gateway 提供）

**致 AIRP-MCP-Server 维护方：来自 AIRP-Gateway（https://github.com/GhostXia/AIRP-Gateway） 的对接需求**

**背景**：AIRP-Gateway 是一个纯协议桥，作为 MCP 客户端连接你方的 MCP 服务。链路为 `前端 → AIRP-Gateway → AIRP-MCP-Server → Agent`。Gateway 支持两种上游传输：stdio 与 HTTP。

**现状结论（我已审过你方 `src/transport/`）**：
- **stdio 模式（`airp-mcp mcp --data-dir`）完全可用**，是真实 MCP（`serve_server(Router, rmcp::transport::io::stdio())`）。**这部分请勿改动**，Gateway 现在就能对接。
- **HTTP 模式（`airp-mcp serve --bind`，`/mcp/v1`）是未完成的桩**，需要你方修复，Gateway 才能走 HTTP 接入。

**HTTP 模式当前的问题**：
1. `POST /mcp/v1` 的 `handle_mcp_post` 返回空的 `{"jsonrpc":"2.0","id":...,"result":{}}`，`State(_state)` 未使用，**从未把请求转发给 rmcp 服务**。
2. `GET /mcp/v1` 的 SSE 是单一全局广播通道，**无会话隔离**，无 `Mcp-Session-Id`。
3. `Cargo.toml` 里 `rmcp` 只启用了 `server, transport-io, macros`，**缺少** `transport-streamable-http-server`（AIRP-Core 已启用该 feature）。

**需求（按 MCP 规范 2025-06-18 的 Streamable HTTP transport）**：

- **R1（核心）** `POST /mcp/v1` 必须把收到的 JSON-RPC 请求真实派发给 rmcp 服务并返回真实 `result`/`error`，而非空对象。
- **R2 生命周期** 完整支持 `initialize`（返回真实 `protocolVersion` + `capabilities` + `serverInfo`）→ 接受 `notifications/initialized` → 之后正常处理 `tools/list`、`tools/call`、`resources/read` 等。
- **R3 会话** `initialize` 响应须返回 `Mcp-Session-Id` 响应头；后续请求校验该头；SSE 流按会话隔离（不要全局广播）。
- **R4 协议头** 初始化后的所有请求要求并校验 `MCP-Protocol-Version` 头。
- **R5 内容协商** 依据客户端 `Accept: application/json, text/event-stream`：单次响应用 `application/json`，流式响应用 `text/event-stream`(SSE)。
- **R6 鉴权** 保留 `AIRP_HTTP_TOKEN` 的 bearer + 常数时间校验，统一作用于 `/mcp/v1`。
- **R7 CORS** 允许请求头 `Authorization, Mcp-Session-Id, MCP-Protocol-Version`，并 `expose` `Mcp-Session-Id`。
- **R8 错误** 用规范的 JSON-RPC error code（如协议版本不符返回 `-32602`）。

**强烈建议的实现路径（省事且与 AIRP-Core 一致）**：
给 `rmcp` 开启 `transport-streamable-http-server`（及其 session 相关 feature，参照 AIRP-Core 的 `Cargo.toml`），用 rmcp 自带的 streamable-HTTP router 挂到 `/mcp/v1`，替换掉现在手写的桩。这样 R1–R5、R8 基本由 rmcp 直接满足，你只需保留外层鉴权(R6)与 CORS(R7)。

**验收标准（Gateway 会这样验证）**：
1. `POST /mcp/v1` 发 `initialize` → 返回真实 `protocolVersion`（与请求一致或服务端支持版本）+ 响应头含 `Mcp-Session-Id`。
2. 带会话头发 `tools/list` → 返回全部 38 个工具。
3. 发 `tools/call`（如 `list_characters`）→ 返回真实内容，而非空 `{}`。

**不需要改的**：stdio 模式、数据模型、工具实现，全部维持现状。
