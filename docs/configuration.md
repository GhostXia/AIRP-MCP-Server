# 可定制项（配置参考）

> **性质**：参考。列出**无需改源码**就能调的项（CLI / 环境变量），以及**需改源码**的硬编点（给开发者）。
> **真理顺序**：源码 > 本文档。冲突以源码为准。

---

## 1. CLI 命令与参数

二进制 = `airp-mcp`，两个子命令（`src/main.rs`）：

| 命令 | 参数 | 默认 | 作用 |
|:--|:--|:--|:--|
| `mcp` | `-d` / `--data-dir <dir>` | `./data` | stdio MCP 服务（Claude Code / Cursor / pi 等）；数据根目录 |
| `serve` | `-b` / `--bind <addr>` | `127.0.0.1:3000` | HTTP（Streamable）监听地址；`/mcp/v1` 端点 |
| `serve` | `-d` / `--data-dir <dir>` | `./data` | 数据根目录 |

例：`airp-mcp mcp --data-dir ./data` · `AIRP_HTTP_TOKEN=secret airp-mcp serve --bind 0.0.0.0:3000`

---

## 2. 环境变量（无需改码）

| 变量 | 默认 | 作用 | 约束 / 备注 |
|:--|:--|:--|:--|
| `AIRP_HTTP_TOKEN` | 未设 | 设且非空 → `/mcp/v1` 所有请求须带 `Authorization: Bearer <值>`；未设 → 无鉴权（绑非 loopback 时会告警） | 仅 `serve`（HTTP）生效。`src/transport/http.rs:42` |
| `AIRP_MAX_READ_BYTES` | `32768`（32 KiB） | 单次读取（工具 / 资源）字节上限，防巨型 blob / JSON 灌爆 token | **下限 1024**（更小回退默认）；非法值回默认；进程内读一次缓存。`src/mcp/mod.rs:28` |
| `RUST_LOG` | 未设（≈静默） | tracing 日志过滤；**日志只走 stderr**（不污染 stdout 帧） | 标准 `EnvFilter` 语法，如 `RUST_LOG=debug` / `airp_mcp_server=info`。`src/main.rs:44` |

---

## 3. 硬编点（要改得动源码 —— 给开发者）

目前尚无 env / CLI 开关，需改源码：

| 项 | 当前值 | 位置 | 说明 |
|:--|:--|:--|:--|
| 角色卡 PNG 上限 | 10 MiB | `src/mcp/tools.rs:15`（`MAX_PNG_BYTES`） | `import_card` 入口尺寸 / 防解压炸弹。env 化是 [ROADMAP](ROADMAP.md) §3「入口尺寸 cap」候选 |
| HTTP Host 校验 | 关闭 | `src/transport/http.rs`（`disable_allowed_hosts`） | 为 LAN 部署放开 rebind 守卫；安全模型 = bearer + 信任 LAN，勿暴露公网 |
| CORS | 允许任意来源 | `build_router`（`src/transport/http.rs`） | 暴露 `mcp-session-id` / `mcp-protocol-version` 响应头给浏览器 |
| 协议版本 | rmcp `LATEST`（自动） | `src/mcp/mod.rs`（`get_info`） | **不固化**；initialize 时 `min(client, server)` 协商向下兼容（见 §1.2） |

---

## 4. RP 数据 = 用户的主定制面（是数据，不是配置）

**角色卡 / 预设 / 世界书 / 场景 / 状态 / 记忆**都是 `data-dir` 下的**用户数据**，经 MCP 工具增删改（`import_card` / `import_preset` / `create_scene` / `update_state` …），**不是配置项**。这才是 RP 体验的主要定制面。用法见 [README](../README.md) / [SKILL.md](../SKILL.md)。

---

## 5. 待定（计划中，尚未可配 —— 见 ROADMAP）

- **入口尺寸 cap env 化**（`MAX_PNG_BYTES` / preset / blob / stdio 帧）—— §3 候选。
- **`--read-only` 启动开关**（不可信 Agent 下只读）—— §3 候选。
- **软删除 `.trash`**（删除可逆）—— §2.D 候选。
