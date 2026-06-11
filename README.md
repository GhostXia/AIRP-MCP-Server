# AIRP MCP Server

> 致谢：本项目的部分理念与实践，参考、借鉴自 Discord 社区 **「类脑ΟΔΥΣΣΕΙΑ」** 内的相关讨论与教程，并与社区成果相互参照。

> **AIRP = AI Roleplay Data Manager**

[![CI](https://github.com/GhostXia/AIRP-MCP-Server/actions/workflows/ci.yml/badge.svg)](https://github.com/GhostXia/AIRP-MCP-Server/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#许可)

一个纯 MCP 协议的角色扮演**数据管理**服务器。**不调用任何 AI API，不自造 Agent runtime，不做推理。**
所有推理、叙事推进、角色演绎，都由 MCP Client（Claude Code / Cursor / pi / Codex）通过 AIRP 暴露的 **Tools / Resources / Prompts** 完成。AIRP 只负责把角色卡、世界书、预设、会话、状态、记忆**存好、取好、装配好**。

> **AIRP-MCP-Server** 是 AIRP 的轻量级 MCP 版本。若需完整的流式 SSE 网关、实时角色状态面板、
> 守护进程部署、OpenAI 兼容 API 代理等高级功能，请用 **[AIRP-Core](https://github.com/GhostXia/AIRP-Core)**
> —— AIRP 项目的全面版本。

---

## 快速开始

```bash
# 编译
cargo build --release

# Stdio 模式（推荐：Claude Code / Cursor / pi）
./target/release/airp-mcp mcp --data-dir ./data

# HTTP 模式
./target/release/airp-mcp serve --bind 127.0.0.1:3000 --data-dir ./data
```

### MCP Client 配置

**Claude Code / Cursor**（`mcp.json` 或 `cursor.json`）：

```json
{
  "mcpServers": {
    "airp": {
      "command": "airp-mcp",
      "args": ["mcp", "--data-dir", "./data"]
    }
  }
}
```

### Agent 用法指南

项目根目录的 [SKILL.md](SKILL.md) 是给 **AI Agent 阅读**的操作手册：
- §0.5 **使用前优先评估清单**（决策提示，见下方「RP 文风质量」）
- 完整 RP 工作流（6 阶段标准轮次 + 三幕叙事弧）
- 并行调用策略（2–4x 加速）、多角色场景管理、预设文风一键移植
- §16 **执行隔离**：用隔离 subagent 书写 RP
- **38 个工具 / 19 个资源 / 12 个提示词**的速查表

---

## 功能总览

### 38 个 MCP Tools

| 类别 | 工具 | 用途 |
|:--|:--|:--|
| 角色卡 | `import_card` | 导入 SillyTavern 角色卡。**推荐 `png_path`**（服务端读盘解析，base64 不进上下文、不烧 token）或 `png_base64`；≤ 10 MiB |
| 角色卡 | `list_characters` | 列出所有角色 |
| 角色卡 | `get_character` | 查看角色详情 |
| 角色卡 | `delete_character` | 删除角色及所有数据 |
| 会话 | `start_session` | 创建新会话（自动加载 preset + lorebook + state） |
| 会话 | `list_sessions` | 列出角色的所有会话 |
| 会话 | `append_message` | 向会话追加消息（JSONL） |
| 会话 | `get_recent_context` | 获取最近 N 条消息 |
| 会话 | `rollback_messages` | 回滚最后 N 条消息 |
| 记忆 | `seal_volume` | 封存当前会话为归档卷（支持清空，省 token） |
| 世界书 | `apply_lorebook` | 关键词扫描 → 返回匹配的世界书条目 |
| 世界书 | `update_lorebook` | 更新世界书 |
| 状态 | `update_state` | 更新实时状态（HP / MP / 位置 / 关系值） |
| 状态 | `get_live_state` | 获取当前状态 |
| 分析 | `analyze_card` | 4 档分级角色卡分析（Tier 0–3） |
| 分析 | `get_gating_status` | 查看检查点进度 |
| 预设 | `list_presets` | 列出所有 AI 预设 |
| 预设 | `get_preset` | 查看预设详情 |
| 预设 | `import_preset` | 导入 SillyTavern 预设 JSON |
| 预设 | `write_preset_artifact` | Agent 写入预设分析产物 |
| 预设 | `list_preset_regex_scripts` | 列出预设正则脚本（含元数据） |
| 预设 | `remove_preset_regex_script` | 删除预设正则脚本 |
| 预设 | `set_preset_regex_enabled` | 启用/禁用预设正则脚本 |
| 拆解/导出 | `decompose_character` | 拆解角色卡为 7 个 Markdown 文件（分析模板，含 TODO 占位） |
| 拆解/导出 | `decompose_preset` | 拆解预设为结构化文档 |
| 拆解/导出 | `export_context_bundle` | 导出**自包含成品**上下文包（`context.md` + raw sidecar），交接给隔离 subagent；可选 `thinking_mode_text` 置于正文最前；未知捆绑内容原样旁路不解析 |
| 场景 | `create_scene` | 创建多角色场景 |
| 场景 | `list_scenes` | 列出所有场景 |
| 场景 | `get_scene` | 查看场景配置 |
| 场景 | `add_character_to_scene` | 向场景添加角色 |
| 场景 | `merge_lorebooks` | 合并多角色世界书（去重排序，纯算法） |
| 场景 | `build_scene_system_prompt` | 装配多角色场景系统提示词（前载 union 世界书；可选 `style_enhance` 注入对话范例+suffix 文风锚） |
| 插件 | `plugin_kv_get` / `plugin_kv_set` | 插件 KV（`plugins/{name}/{key}.json`，任意 JSON，零 schema） |
| 插件 | `plugin_jsonl_append` / `plugin_jsonl_read` | 插件 JSONL（O(1) 追加 / 分页读，带字节上限） |
| 插件 | `plugin_blob_write` / `plugin_blob_read` | 插件任意文件（base64 / UTF-8；单次读上限 256 KiB，护住 token 预算） |

> **M_PLUGIN_DATA（戒律 4 · 开放接入）**：任何第三方插件、任何语言，取一个 `plugin_name` 命名空间即可在 `data/plugins/{plugin_name}/` 存取自己的数据 —— 无 manifest、无注册、无 schema 强制。AIRP 不解析、不校验、不索引其语义。

### 19 个 MCP Resource URIs

| URI | 内容 |
|:--|:--|
| `airp://characters` | 角色 ID 列表 |
| `airp://characters/{id}/card` | 角色卡完整 JSON |
| `airp://characters/{id}/greetings` | 开场语库（含备选问候语） |
| `airp://characters/{id}/world/lorebook` | 世界书 |
| `airp://characters/{id}/state/live` | 实时状态 |
| `airp://characters/{id}/memory/current` | 当前会话摘要 |
| `airp://characters/{id}/memory/index` | 卷索引 |
| `airp://characters/{id}/memory/volumes/{n}` | 归档卷（n="latest"=最新） |
| `airp://characters/{id}/gating/checkpoints` | 检查点进度 |
| `airp://presets` | 预设 ID 列表 |
| `airp://presets/{id}` | 预设详情 |
| `airp://presets/{id}/raw` | 预设原始 JSON（>100KB 截断 + 翻页提示） |
| `airp://presets/{id}/artifacts` | 预设分析产物文件树 |
| `airp://presets/{id}/regex` | 预设正则脚本列表 |
| `airp://scenes` | 场景列表 |
| `airp://scenes/{id}` | 场景完整配置 |
| `airp://plugins` | 插件命名空间列表 |
| `airp://plugins/{name}/files` | 插件文件相对路径列表（递归） |
| `airp://plugins/{name}/data/{path}` | 插件文件内容（UTF-8；二进制用 `plugin_blob_read`；超限截断） |

### 12 个 MCP Prompts

| Prompt | 用途 |
|:--|:--|
| `build_system_prompt` | 组装角色系统提示词（支持 preset 注入，含 mes_example） |
| `filter_text` | 应用预设正则过滤（八股后处理） |
| `state_update_instruction` | 状态更新 `<state>` 格式说明 |
| `seal_volume` | 卷封存 Agent 指导 |
| `build_scene` | 多角色场景装配（含并行加载引导） |
| `analyze_preset` | 预设分析 3 步 workflow |
| `tune_preset` | 按用户反馈热调预设文风（改预设源头，非洗输出；best-effort 不保证） |
| `prompt_decompose_character` | 角色卡拆解的 Agent 指导（6 步） |
| `prompt_enhance_analysis` | 增强分析的 Agent 指导（5 步） |
| `prompt_build_session_context` | 会话上下文构建的 Agent 指导 |
| `validate_card` | 角色卡内容验证（未知宏 / 孤儿代码 / 破损 markup） |
| `validate_preset` | 预设验证（破损正则 / 未知 identifier / 参数异常） |

### RP 文风质量与 Agent 决策提示

AIRP 不调 LLM、不强制工作流，但会**主动给 Agent 决策提示**：在 MCP `instructions` 与 [SKILL.md](SKILL.md) §0.5 写明「使用前优先评估」的得失清单（**为何该做 + 不做的代价**），让 Agent **知情自选**，而非被命令。社区实践中「文风死人化 / 出戏」的常见成因都有对应解：

| 决策提示 | 治什么 | 相关能力 |
|:--|:--|:--|
| **隔离 subagent 书写 RP** | 主 Agent 编程上下文压扁文笔（「死人化」） | `export_context_bundle` 产成品上下文包 |
| **首轮注入思考模式** | 思考层没入戏（比文风更上游，社区公认第一杠杆） | `export_context_bundle` 的 `thinking_mode_text` 槽 |
| **文风僵 → 改预设源头** | 跨模型预设错配；洗输出只治标 | `tune_preset` |
| **Token 纪律 / 世界书关键词触发** | 整文件 / 全历史灌爆预算 | 分页读 + `seal_volume` + `apply_lorebook` |
| **八股语病 → 正则后处理** | prompt 硬禁反伤模型思考 | 预设 `regex_scripts` + `filter_text` |

> 全部**非强制**：内容（思考模式块、文风、预设等）是用户数据，AIRP **不代写、不解析**；Agent 自行决断。详见 [SKILL.md](SKILL.md) §0.5（决策清单）/ §16（执行隔离）。

---

## 架构

```
┌──────────────────────────────────────────────┐
│  MCP Client (Claude Code / Cursor / pi ...)    │  ← 推理、叙事、状态判断
│  ─ read SKILL.md → call AIRP tools ──────────  │
│                                                │
│  最终决策权在 Agent。AIRP 的 Tools / Resources │
│  / Prompts / SKILL.md 均为建议（含决策提示），  │
│  Agent 自行选择用哪些、怎么用。                 │
└─────────────┬──────────────────────────────────┘
              │ MCP 协议 (stdio / HTTP)
┌─────────────▼──────────────────────────────────┐
│  AIRP MCP Server (本项目)                        │  ← 只做数据管理
│  src/models/    数据模型                         │
│  src/storage/   文件存储 + 迁移 + 路径安全        │
│  src/mcp/       MCP 协议实现（tools/resources/   │
│                 prompts）                         │
│  src/transport/ stdio / HTTP                     │
└──────────────────────────────────────────────────┘
```

四条边界（贯穿设计的「铁律」）：

- **不调任何 AI LLM API** —— `PresetConfig` 里的 `temperature`/`top_p` 等仅供 Agent 参考
- **不做推理** —— 叙事推演、角色扮演、状态判断全由 Agent 完成
- **不维护 UI** —— 界面由 MCP Client 提供
- **不强制工作流，但给决策提示** —— 一切是建议；用得失提示抬高优先级、引导而非命令（AI 自行决断）。改法**通用优先于特供**，不为单一客户端/模型特判

---

## 数据目录结构

```
data/
├── characters/
│   └── {id}/
│       ├── card.json              # 角色卡
│       ├── card.png               # 原始 PNG
│       ├── data.json              # 元数据（分析 tier 等）
│       ├── world/lorebook.json    # 世界书
│       ├── state/live.json        # 实时状态
│       ├── state/history.jsonl    # 状态变更历史
│       ├── gating/checkpoints.json
│       ├── analysis/              # analyze_card 产物
│       ├── memory/
│       │   ├── index.md           # 卷索引
│       │   └── volumes/vol_*.md   # 归档卷
│       └── sessions/{id}/
│           ├── meta.json
│           └── chat.jsonl
├── presets/
│   └── {name}/
│       ├── preset.json            # SillyTavern 预设
│       ├── regex/*.json           # 正则脚本
│       └── analysis/              # Agent 分析产物
├── scenes/
│   └── {id}/
│       └── scene.json             # 多角色场景配置
└── plugins/
    └── {plugin_name}/             # 第三方插件自管数据（零 schema）
```

---

## 开发

```bash
cargo build --release                  # 构建 lib + bin
cargo test --test plugin_data_test     # 独立测试目标（全绿）
cargo fmt
```

- **CI**：GitHub Actions 在每次 push / PR 跑 `cargo build` + `cargo test --test plugin_data_test`（见徽章）。
- 全量 `cargo test` 暂缓：`tests/integration_test.rs` 在 main 上预存损坏（`crate::` 路径 + PNG zTXt/base64 测试辅助），属独立 follow-up。

## 安全与部署

AIRP 的威胁模型假设 **本地、单用户、stdio / loopback** 运行。基于此：

- **路径安全**：所有插件/预设的读写经**组件式**校验 —— 拒 `..` 逃逸、拒绝对路径、拒符号链接，结果锁在 `data/` 根内（`Storage::safe_resolve_for_write`）。
- **输入限制**：`import_card` 的 PNG ≤ 10 MiB（`png_path` 走 metadata 预检，炸弹文件不读即拒），PNG 解码器设分配上限（挡 zlib 压缩炸弹）；工具单次读 ≤ 256 KiB；预设 raw / JSONL 超限截断或分页。
- **PNG 导入用 `png_path` 而非 `png_base64`**：让 AIRP **服务端直接读盘解析**，base64 **永不进模型上下文** —— 否则 Agent 为产生 base64 得先把 PNG 读进上下文（10 MiB 卡 ≈ 13 MiB 文本），**烧光 token**。
- **插件信任模型**：`data/plugins/` 是**零 schema、开放接入**（戒律 4）—— AIRP 不解析、不校验、不沙箱化插件数据语义。插件写入被限制在自己的 `plugins/{name}/` 命名空间内（拒 `..`/绝对路径/符号链接），**但内容本身不受信任**。⚠️ **只安装可信来源的插件。**
- **HTTP 暴露：局域网 OK，公网 NO**。`serve --bind` 支持同 wifi 下「电脑跑后端 + 手机对话」这类用法。但 **AIRP 的 HTTP 无内置鉴权** —— 同网段任何设备都能调读写接口（含 `png_path` 服务端读文件）。**把你的局域网当可信网络**；**切勿绑定公网 IP 或做公网端口转发**。需要远程访问请走 VPN / SSH 隧道，别裸暴露。
- **备份**：数据是本地文件，建议定期备份 `data/` 目录。

## 隐私

AIRP MCP Server 是 **本地优先** 的二进制工具：

- 所有数据存储在用户本地磁盘
- 通信仅在本机进程间（stdio / loopback HTTP）
- 项目方不收集、不接收、不存储任何用户数据

## 许可

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
