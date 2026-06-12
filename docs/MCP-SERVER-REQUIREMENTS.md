# AIRP-MCP-Server — 对外接口需求：HTTP Streamable Transport

> **来源**：AIRP-Gateway — https://github.com/GhostXia/AIRP-Gateway
> **收录**：2026-06-12 · **状态**：OPEN（stdio 已可用；HTTP `/mcp/v1` 待实现）
> **链路**：`前端 → AIRP-Gateway → AIRP-MCP-Server → Agent`

本文记录 AIRP-Gateway 对本服务 HTTP 传输的对接需求，供追踪完成情况。需求正文逐字收录于下方，未改写。

## 代码核实（截至 `main` @ 25581a9）

Gateway 所述桩状态**属实**：

- `Cargo.toml`：`rmcp = { features = ["server", "transport-io", "macros"] }` —— 缺 `transport-streamable-http-server`。
- [`src/transport/http.rs`](../src/transport/http.rs)：`handle_mcp_post(State(_state) ...)` 返回 `"result": {}`，从未派发给 rmcp；SSE 为 `broadcast` 全局通道，无 `Mcp-Session-Id`。
- stdio 模式（`serve_server(Router, rmcp::transport::io::stdio())`）为真实 MCP，**保持不动**。

## 状态追踪

| 项 | 内容 | 状态 |
|:--|:--|:--|
| R1 | `POST /mcp/v1` 真实派发 rmcp，返回真实 `result`/`error` | ☐ 待做 |
| R2 | 生命周期 `initialize` → `notifications/initialized` → `tools/list`/`tools/call`/`resources/read` | ☐ 待做 |
| R3 | 会话 `Mcp-Session-Id`（响应头 + 校验 + SSE 按会话隔离） | ☐ 待做 |
| R4 | 校验 `MCP-Protocol-Version` 头 | ☐ 待做 |
| R5 | 内容协商 `application/json` vs `text/event-stream` | ☐ 待做 |
| R6 | 鉴权 `AIRP_HTTP_TOKEN` bearer + 常数时间校验，统一作用 `/mcp/v1` | ✅ 已有（bearer + 常数时间 + route_layer；待与新 router 整合） |
| R7 | CORS 允许/暴露 `Authorization, Mcp-Session-Id, MCP-Protocol-Version` | ☐ 待做 |
| R8 | 规范 JSON-RPC error code（协议不符 `-32602` 等） | ☐ 待做 |

> 推荐路径：给 `rmcp` 开 `transport-streamable-http-server`（参照 AIRP-Core 的 `Cargo.toml`），用其自带 streamable-HTTP router 挂 `/mcp/v1`，替换手写桩 —— R1–R5、R8 基本由 rmcp 直接满足，本服务只保留外层鉴权（R6）与 CORS（R7）。

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
