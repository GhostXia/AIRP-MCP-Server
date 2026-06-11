# AIRP MCP Server

> **AIRP = AI Roleplay Data Manager**

一个纯 MCP 协议角色扮演数据管理服务器。不调用 AI API，不自造 Agent runtime。
所有推理、叙事推进、角色演绎由 MCP Client（Claude Code / Cursor / Pi / Codex）通过 AIRP 提供的 Tools / Resources / Prompts 完成。

> **AIRP-MCP-Server** 是 AIRP 的轻量级 MCP 版本。如果你需要完整的流式 SSE 网关、
> 实时角色状态面板、守护进程部署、OpenAI 兼容 API 代理等高级功能，
> 请使用 **[AIRP-Core](https://github.com/GhostXia/AIRP-Core)** ——
> AIRP 项目的高级和全面版本。

## 快速开始

```bash
# 编译
cargo build --release

# Stdio 模式（推荐：Claude Code / Cursor / Pi）
./target/release/airp-mcp mcp --data-dir ./data

# HTTP 模式
./target/release/airp-mcp serve --bind 127.0.0.1:3000 --data-dir ./data
```

### MCP Client 配置

**Claude Code / Cursor** (`mcp.json` 或 `cursor.json`):

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

项目根目录的 [SKILL.md](SKILL.md) 是给 **AI Agent 阅读** 的操作手册，包含：
- 完整的 RP 工作流（6 阶段标准轮次 + 三幕叙事弧）
- 并行调用策略（3x-4x 加速）
- 多角色场景管理
- 预设文风一键移植
- 38 个工具 / 19 个资源 / 12 个提示词的速查表

---

## 功能总览

### 38 个 MCP Tools

| 工具 | 用途 |
|:--|:--|
| `import_card` | 从 base64 PNG 导入 SillyTavern 角色卡 |
| `list_characters` | 列出所有角色 |
| `get_character` | 查看角色详情 |
| `delete_character` | 删除角色及所有数据 |
| `start_session` | 创建新会话（自动加载 preset + lorebook + state） |
| `list_sessions` | 列出角色的所有会话 |
| `append_message` | 向会话追加消息（JSONL） |
| `get_recent_context` | 获取最近 N 条消息 |
| `rollback_messages` | 回滚最后 N 条消息 |
| `seal_volume` | 封存当前会话为归档卷（支持清空） |
| `apply_lorebook` | 关键词扫描 → 返回匹配的世界书条目 |
| `update_lorebook` | 更新世界书 |
| `update_state` | 更新实时状态（HP / MP / 位置 / 关系值） |
| `get_live_state` | 获取当前状态 |
| `list_presets` | 列出所有 AI 预设 |
| `get_preset` | 查看预设详情 |
| `analyze_card` | 4 档分级角色卡分析（Tier 0-3） |
| `decompose_character` | 拆解角色卡为 7 个 Markdown 文件 |
| `decompose_preset` | 拆解预设为结构化文档 |
| `get_gating_status` | 查看检查点进度 |
| `import_preset` | 导入 SillyTavern 预设 JSON |
| `write_preset_artifact` | Agent 写入预设分析产物 |
| `list_preset_regex_scripts` | 列出预设正则脚本（含元数据） |
| `remove_preset_regex_script` | 删除预设正则脚本 |
| `set_preset_regex_enabled` | 启用/禁用预设正则脚本 |
| `create_scene` | 创建多角色场景 |
| `list_scenes` | 列出所有场景 |
| `get_scene` | 查看场景配置 |
| `add_character_to_scene` | 向场景添加角色 |
| `merge_lorebooks` | 合并多角色世界书（去重排序，纯算法） |
| `build_scene_system_prompt` | 自动装配多角色场景系统提示词（可选 `style_enhance` 注入对话范例+suffix 文风锚） |
| `export_context_bundle` | 导出自包含成品上下文包（context.md + raw sidecar），交接给隔离 subagent；可选 `thinking_mode_text` 置于正文最前；未知捆绑内容原样旁路不解析 |
| `plugin_kv_get` | 读插件 KV（plugins/{name}/{key}.json，零 schema） |
| `plugin_kv_set` | 写插件 KV（任意 JSON 值） |
| `plugin_jsonl_append` | 插件 JSONL 追加（O(1) append） |
| `plugin_jsonl_read` | 插件 JSONL 分页读取 |
| `plugin_blob_write` | 插件任意文件写入（base64 / UTF-8 文本） |
| `plugin_blob_read` | 插件任意文件读取（单次上限 256 KiB，护住 token 预算） |

> **M_PLUGIN_DATA（戒律 4 开放接入）**：任何第三方插件、任何语言，取一个 `plugin_name` 命名空间即可在 `data/plugins/{plugin_name}/` 存取自己的数据 —— 无 manifest、无注册、无 schema 强制。AIRP 不解析、不校验、不索引其语义。

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
| `airp://presets/{id}/raw` | 预设原始 JSON（100KB 截断） |
| `airp://presets/{id}/artifacts` | 预设分析产物文件树 |
| `airp://presets/{id}/regex` | 预设正则脚本列表 |
| `airp://scenes` | 场景列表 |
| `airp://scenes/{id}` | 场景完整配置 |
| `airp://plugins` | 插件命名空间列表 |
| `airp://plugins/{name}/files` | 插件文件相对路径列表（递归） |
| `airp://plugins/{name}/data/{path}` | 插件文件内容（UTF-8；二进制用 plugin_blob_read） |

### 12 个 MCP Prompts

| Prompt | 用途 |
|:--|:--|
| `build_system_prompt` | 组装角色系统提示词（支持 preset 注入） |
| `filter_text` | 应用预设正则过滤 |
| `state_update_instruction` | 状态更新 `<state>` 格式说明 |
| `seal_volume` | 卷封存 Agent 指导 |
| `build_scene` | 多角色场景装配（含并行加载引导） |
| `analyze_preset` | 预设分析 3 步 workflow |
| `tune_preset` | 按用户反馈热调预设文风（改预设源头，非洗输出；best-effort 不保证） |
| `prompt_decompose_character` | 角色卡拆解的 Agent 指导（6 步） |
| `prompt_enhance_analysis` | 增强分析的 Agent 指导（5 步） |
| `prompt_build_session_context` | 会话上下文构建的 Agent 指导 |
| `validate_card` | 角色卡内容验证（未知宏/孤儿代码/破损 markup） |
| `validate_preset` | 预设验证（破损正则/未知 identifier/参数异常） |

---

## 架构

```
┌──────────────────────────────────────────┐
│  MCP Client (Claude / Cursor / Pi ...)     │  ← 做推理、叙事、状态更新
│  ─ read SKILL.md → call AIRP tools ──────│
│                                            │
│  最终决策权在 Agent。AIRP 的 Tools/       │
│  Resources/Prompts/SKILL.md 均为建议，    │
│  Agent 自行选择使用哪些、如何使用。       │
└─────────────┬────────────────────────────┘
              │ MCP 协议 (stdio / HTTP)
┌─────────────▼────────────────────────────┐
│  AIRP MCP Server (本项目)                  │  ← 只做数据管理
│                                            │
│  src/models/    数据模型                   │
│  src/storage/   文件存储 + 迁移 + 安全     │
│  src/mcp/       MCP 协议实现              │
│  src/transport/ stdio / HTTP              │
└──────────────────────────────────────────┘
```

- **AIRP 不调用任何 AI LLM API** — `PresetConfig` 中的 `temperature`/`top_p` 等参数仅供 Agent 参考
- **AIRP 不做推理** — 叙事推演、角色扮演、状态判断全部由 Agent 完成
- **AIRP 不维护 UI** — 所有界面由 MCP Client 提供
- **AIRP 不强制工作流** — 所有 Tools/Resources/Prompts/SKILL.md 均为建议，Agent 自行抉择使用

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
└── scenes/
    └── {id}/
        └── scene.json             # 多角色场景配置
```

---

## 开发

```bash
cargo check         # 0 errors, 0 warnings ✅
cargo build --release
cargo test
cargo fmt
```

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
