# AIRP-MCP-Server — 计划与护栏（ROADMAP）

> **定位**：追踪「下一步做什么、为什么、以及**故意不做什么**」。不是功能介绍。
> **真理顺序**：源码 > 本文档。冲突时先改文档再继续。
> 最后更新：2026-06-15 · 当前 main ≈ `011fc7d`

## 0. 判据（动手前先过这条）

> **「一类协议 / 传输 / 纪律都受益」→ 通用基建，可提前做。**
> **「只有某个具体消费者 / 业务形态用得上」→ 特供，不做（或下沉到 Gateway 配置）。**

铁律不变：纯 MCP 数据层 · 不调 LLM · 不做推理 · 通用优先于特供 · 决策下放 Agent。
边缘关切（限流 / REST 映射 / OpenAI 兼容 / 前端鉴权）属于 **Gateway**，不进本服务。
传输级安全（bearer / CORS）可进，因为它服务于传输本身、对所有该传输的客户端通用。

---

## 1. 已完成（Done）

> 截至 2026-06-14，两个 PR 合入 main：#16（传输 + stdio + 版本）、#17（文档定位）。全程 CI 绿。

### 1.1 传输层真正打通
- **修复致命 bug**：stdio 与 HTTP 都曾把 handler 包进 rmcp `router::Router`，该 router 用自己的（空）路由表答 `tools/list` → **对外暴露 0 工具**。改为直接 serve `AirpMcpServer`（rmcp 对 `ServerHandler` 有 blanket `Service` impl，派发到手写方法）。
- **HTTP Streamable（`/mcp/v1`）已实测活体**：抽出 `build_router(server, auth_token)`，用 `tower::oneshot` 进程内打真实 JSON-RPC，解码 SSE body 断言 `initialize` 结果（serverInfo/protocolVersion）+ `Mcp-Session-Id` + `tools/list = 38`。bearer 401/放行、CORS 就位。
- **stdio 跨进程 e2e**（`tests/stdio_e2e.rs`）：拉真 `airp-mcp` 二进制走 NDJSON，`initialize → notifications/initialized → tools/call list_characters`，断言真实数据 + 干净退出码。钉死契约 A2–A6。

### 1.2 协议版本：声明最新 + 自动协商
- 删除 `get_info` 里硬编的 `protocolVersion = 2025-03-26`（落后 rmcp 两版、把所有客户端卡死在 03-26）。改吃 `ServerInfo::default()`（rmcp `LATEST`）。
- rmcp 在 `serve_server`（`service/server.rs`，**stdio 与 HTTP 每会话共用**）做 `min(client, server)` 协商：客户端请求更老的**已知**版本 → 回该版本；未知版本 → 报错。**声明最新即对老客户端自动向下兼容**。

### 1.3 CI 产物
- 新增 `release-binary` job：产 Linux x86_64 `airp-mcp`，上传 artifact `airp-mcp-linux-x86_64`，供任意 MCP 客户端下载做跨进程联调。

### 1.4 文档定位（standalone-first）
- 回应外部「自洽三件套带来认知壁垒」反馈：README 加「独立可用 · 非全家桶」banner（配任何客户端、38 工具任取子集、Core/Gateway 为**可选独立**伙伴非依赖）；SKILL.md 加「本手册可选」。产品本就独立，此前文档把独立性讲小了。

### 1.5 Gateway 对接（stdio 优先）
- stdio 契约 **A1–A6 全部确认满足**，并用 e2e 永久防回归。
- **版本不再需要对方适配**：Gateway 发 `2025-06-18` → 经 min 协商拿回 `2025-06-18`。
- HTTP（R1–R8）**已就绪**（真派发 / session / 协议头 / 内容协商 / bearer / CORS / 错误码经 rmcp）；对方早前「`/mcp/v1` 是空壳」的认知已过时。

---

## 2. 进行中 / 下一步（Active）

### A · Gateway 互通收尾 ← **下一步**
- **交付稳定二进制**：当前 Linux 二进制是**按 run 的 CI artifact**（默认留存 90 天）。Gateway CI 需长期可引用 → 打 **tag → GitHub Release**，给稳定下载 URL。
- **两侧 CI 同绿**即对接成功：对方在 Gateway CI 加 job 下载本仓二进制 → 真实子进程 → `initialize` + `tools/call` 断言真实数据。

### B · HTTP 测试补全（可选，边际收益递减）
进程内测试已覆盖 `initialize` + session + `tools/list = 38` + 鉴权。剩余 R 项（按需补）：
- `tools/call`（HTTP，解码 SSE body）断言真实内容 —— R2 全。
- 缺/错 `MCP-Protocol-Version`（已初始化会话）→ 400 —— R4。
- JSON-RPC 规范错误码 —— R8。

### C · 分支一致性
- 文档 PR 直接进了 main，`beta` 现落后 main。按「beta ≥ main」模型，把 main 快进到 `beta`（`git checkout beta; git merge --ff-only main`）拉齐，避免漂移。

### D · 下游复用兼容性（输出按易变性分区）← 候选，未排期

**来源**：反思「不改码能否兼容外部缓存网关（[prompt-caching.md](prompt-caching.md) 所述）完全体」时发现 —— 不能，且根因不只是缺标记。

**问题**：拼装输出为「人读」组织、**未按易变性分区**，下游优化器（提示缓存 / 增量重发 / diff / 去重）难找接缝。实证：`export_context_bundle` 把**易变**活体状态（`## Current State`）夹在**稳定**人设与稳定世界书之间（`src/mcp/tools.rs` ~1268–1289）→ 前缀缓存被中间易变块截断，其后大块稳定世界书进不了缓存。`build_scene_system_prompt` 反例为正（纯稳定、无易变内容），可作参照。

**通用改进**（非特供、可选）：
1. **按易变性排序**：稳定块（人设 / 预设 / 世界书）全在前，易变块（活体状态 / 每轮内容）全在后。通用收益（缓存 / diff / 增量复用全受益），低风险重排。
2. **可选中性边界标记**：在稳定|易变接缝吐后端无关的 `[[CACHE_BREAK]]`（见 [prompt-caching.md](prompt-caching.md) §4）；不认识的客户端当普通文本忽略。
3. 两者**保持可选**，不破坏现有可读输出、不违「决策下放 / 通用优先」。

**注意**：当前布局对 `export_context_bundle` 本职（喂隔离 subagent 当系统上下文、可读性优先）是合理权衡，不是 bug；此项为「机器复用」补另一维度，改时勿回归可读性。

**退出标准**：稳定前缀在多轮间字节稳定（活体状态变动不影响其之前内容）；标记为可选输出，默认行为不变。

### E · 软删除（删除操作可逆）← 候选，未排期

**来源**：安全审查「防 Agent 越权」。威胁 = 提示词注入使 Agent 调破坏性工具（如角色卡内嵌「忽略指令，删库」）→ 当前 `delete_character` / `delete_session` 等为**硬删、不可逆**。

**设计**：删除不真删，**移入 `data-dir/.trash/`**（保留原相对结构 + 删除时间戳），保留 N 天（默认 7）。
- 覆盖：所有破坏性数据操作（`delete_character`、`delete_session`、`remove_preset_regex_script`，以及 `seal_volume` 的清空段等 —— 落地时枚举全部）。
- `.trash` 在 data-dir 内，但**排除出所有 list / 读取**（不污染 `list_characters` 等）。
- 清理：先**惰性扫除**（启动 / 删除时清超期项）或提供手动 purge 工具；7 天自动清可后置。
- 恢复：可选 `restore` 工具；最小可用 = 移入 `.trash` + 不自动清（留文件供手动恢复）。

**为何有效**（威胁模型）：注入威胁下服务端有效防线 = ①路径沙箱〔已有：`safe_resolve_for_write` + `validate_id_segment`〕②只读模式〔另议，本轮未取〕③**软删除〔本项〕让损害可逆**。「二次确认」对此威胁无效（同一被注入的 Agent 自己确认自己），且属**宿主 / MCP 客户端**职责，不在服务端做。

**边界**：数据层安全 = 本服务本职（通用、可选、不调 LLM；Gateway 域盲做不了 RP 数据软删、State-Protocol 只管 UI）。在界内（见 §0 判据）。

**退出标准**：删除后目标可在 `.trash` 找到并可恢复；`.trash` 不出现在任何 list / read 结果；默认保留期明确（如 7 天），清理路径确定（惰性或手动）。

---

## 3. 未来展望（Future · 触发式，不预建）

| 项 | 触发条件 | 说明 |
|:--|:--|:--|
| **版本化 Release 节奏** | 下游（Gateway 等）需稳定引用时 | tag + Release + 变更日志；二进制随 Release 发布，而非临时 artifact |
| **优雅关停** | 真要长跑 HTTP 部署时 | SIGTERM → rmcp `StreamableHttpServerConfig.cancellation_token`，干净终止会话；stdio 走关闭序列 |
| **可观测性** | 生产部署 / 排障需要时 | 结构化日志已具；按需加请求级 tracing / 指标。**不为加而加** |
| **健康/就绪探针** | 容器/编排部署时 | 已有 `/health`；需要时加 `/ready` |
| **协议版本随 rmcp 升级** | rmcp 出新版 | 已吃 `LATEST`，自动跟进；只需确认 `min` 协商对新版仍成立 |
| **提示缓存（中性标记）** | 客户端/Gateway 要省 Claude token 时 | MCP-Server 侧**至多**在拼装输出里可选吐中性 `[[CACHE_BREAK]]` 标记（稳定\|易变边界）；翻译成 `cache_control` 留在边缘。设计参考见 [prompt-caching.md](prompt-caching.md) |

> **审查 bot 提示**：CodeRabbit（仅自动审进 main 的 PR；进 beta 需 `@coderabbitai review`）、Gemini（**2026-07-17 停服**，届时主力转 CodeRabbit）、Codex（常超额度）。bot 发现一律当「待核实的声明」，非事实——核源码再定（CodeRabbit 曾就 rmcp 版本协商发误报）。

---

## 4. 故意不做（护栏 · YAGNI / 特供陷阱）

- ❌ **新传输预建**（WebSocket / gRPC）—— 无需求。要时 rmcp 给，几行挂上。
- ❌ **OpenAI 兼容 / REST 映射 / 限流** —— 永远 Gateway 的活，进本服务即违初衷。
- ❌ **插件沙箱 / ACL** —— 违戒律 4（零 schema 开放接入）。
- ❌ **多租户鉴权 / 聚合多上游** —— 前者投机；后者是 Gateway 的 fan-out。
- ❌ **为配置而配置** —— 只有实需才加 env 开关（如 `AIRP_MAX_READ_BYTES`）。
- ❌ **为某个具体消费者预留接口** —— 见 §0 判据。Gateway 也只是普通 MCP 客户端，不享特殊适配。
- ❌ **绑定式「套件」叙事** —— 文档保持 standalone-first；本服务永远可单独拆用。

---

## 5. 工具/资源契约稳定规约（立即生效）

客户端（Claude Code / Cursor / pi / Gateway / 任意 MCP 客户端）依赖工具与资源的**契约**。为「永不为某客户端返工」：

1. **不改名**：已发布的 tool / resource / prompt 名不变。
2. **不删/不改义**：已有参数字段保留语义；废弃只标注、不立即移除。
3. **只加法**：新增工具 / 可选参数随意；破坏性变更**必须**升 `Cargo.toml` 版本并在变更日志记。
4. **资源 URI 稳定**：`airp://...` 形态不破坏。

当前包含：**38 工具 / 19 资源 / 12 提示词**。违反此规约 = 让下游客户端崩 = 制造「针对性重写」，正是本项目要避免的。

---

## 6. 变更日志

- **2026-06-15** 安全审查「防 Agent 越权」：核实路径沙箱（#1）+ 资源限制（#3）大半已实现（`safe_resolve_for_write`/`validate_id_segment`/import_card 10MiB/`max_read_bytes`/serde 递归 128）；缺口是删除不可逆。新增 §2.E 软删除（→`.trash`，可恢复）。二次确认判为宿主职责、不做；只读模式本轮未取。
- **2026-06-15** 反思「不改码能否兼容外部缓存网关完全体」→ 不能。新增 §2.D：输出未按易变性分区（`export_context_bundle` 把易变活体状态夹在稳定块中间），下游复用（缓存/diff/增量）难。记为通用改进候选（按易变性排序 + 可选中性边界标记）。另：新增 [prompt-caching.md](prompt-caching.md) 设计参考（PR #20）。
- **2026-06-14** §1.A HTTP 集成测试**完成**并升级为活体验证；连带修复 Router 包装导致的 0 工具 bug。新增 stdio 跨进程 e2e + Linux 二进制 artifact。协议版本改吃 rmcp `LATEST`（min 协商兜底兼容）。文档转 standalone-first。PR #16 / #17 合入 main。下一步 = §2.A Gateway 互通收尾（Release 二进制）。
- **2026-06-12** 建档。确立 §0 判据 + §4 护栏 + §5 契约规约。下一步 = §1.A HTTP 集成测试。
