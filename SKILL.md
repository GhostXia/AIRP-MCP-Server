# AIRP MCP Server — Agent Skill Guide

> **用途**：这是给 **AI Agent (Claude/Cursor/Pi/Codex)** 阅读的操作指南。
> 它教你如何通过 MCP 协议使用 AIRP 来管理和推进角色扮演(RP)会话。
> **AIRP 只做数据管理；推理、叙事推进、角色演绎都由你（Agent）完成。**

---

## 0. 哲学约束 — 请先阅读

> **AIRP MCP 的全部内容 (Tools / Resources / Prompts / 本文档) 均作为参考建议，
> 不具备强制性。最终决定权和使用权完全归 Agent。**

### 为什么？

不同的 MCP Client 背后是不同的 LLM——Claude、GPT、Gemini、DeepSeek——
它们的推理能力、上下文窗口、思考风格、遵循指令的倾向各不相同。
一套固定的 "标准工作流" 不可能适合所有 Agent。

### 这意味着什么？

```
✅ 你可以做的:
  - 只使用你觉得有用的工具，忽略其余
  - 改写、简化、扩展本文档中的工作流
  - 用你自己的方式推进叙事，不必遵循 6 阶段 / 三幕结构
  - 跳过你认为不必要的步骤（如每轮都读 state）
  - 根据你的上下文窗口大小调整 parallel/sequential 策略

❌ 你不必做的:
  - 照搬文档中的每一个步骤
  - 在不需要验证的时候调 validate_card
  - 每轮都调 apply_lorebook（如果已知世界书不变）
  - 严格遵守 20-30 轮封档的建议

💡 AIRP 的立场:
  AIRP 提供的是「工具箱」，不是「操作手册」。
  你选择哪些工具、按什么顺序、以什么节奏使用——全部由你决定。
  本文档中的一切示例、策略、工作流都是建议，不是规则。
```

---

## 0.5 使用前·优先评估清单（决策提示，非强制）

> 下面不是规则，是**决策提示**。AIRP 无法、也不会强制你执行任何动作。
> 但请在动手前**先权衡得失**——这里给出「为何要先想」和「不先想的代价」，
> 让你基于后果**知情地自行决断**，而非被命令。

### ① 沉浸式 RP 文笔质量
- **建议**：用 `export_context_bundle` 产出成品上下文，交给**隔离的 subagent** 书写；不要在编排主上下文里直接写。
- **为何要先想**：主 Agent（你）的上下文塞满编程身份、工具 schema、harness 提醒；直接写 RP 时这层编程腔会压扁文笔。
- **不先想的代价**：即使预设文风已成功注入，输出仍**僵硬、出戏**（"死人化"）。这是文笔质量问题，不是指令遵循问题——靠改提示词救不回来。
- 你仍自行决断：单模型客户端、轻量短场景可跳过，直接写。

### ② 用户嫌文风僵硬 / 出戏
- **建议**：用 `tune_preset` 改**预设源头**；不要靠重生成或对输出做后处理。
- **为何要先想**：风格其实**已经成功迁移**（特供预设会盖过宿主模型本味）。问题在预设**内容**，常是跨模型的压制脚手架在当前模型上过度压制。
- **不先想的代价**：重生成/后处理只是**洗稿**（治标），且每轮**多烧一遍 token**；源头不改，下轮照样僵。
- 你仍自行决断：若预设本就适配当前模型，可不动。

### ③ 上下文 / Token 纪律
- **建议**：分块、按需读（`get_recent_context` 的 n 取小；用 `apply_lorebook` 关键词扫描，别整文件倒进上下文）；长会话用 `seal_volume` 归档并清空。
- **为何要先想**：AIRP 的数据（卡/预设/历史/世界书）可能很大；插件 blob/KV 也可能巨大。
- **不先想的代价**：把整文件/全历史灌进上下文 → **token 预算瞬间烧光**。读取已有 256 KiB/100KB 上限兜底，但**调用方的读取策略**才是第一防线。
- 你仍自行决断：短会话、小数据可直接全读。

### ④ 世界书：关键词触发，不整本预载
- **建议**：每轮用 `apply_lorebook(text)` 按关键词取相关条目；不要把整本 lorebook 塞进 system prompt。
- **为何要先想**：世界书天生是**关键词门控**的背景库。
- **不先想的代价**：双重损失——**token 浪费** + 角色**「知道太多」**（一次倒出全部设定）→ 破坏「逐步揭示」的沉浸感。
- 你仍自行决断：世界书极小或剧情需要全量背景时可整载。

> 另：导入**不可信第三方卡/预设**前，建议先 `validate_card` / `validate_preset`（详见 §15.5）——孤儿代码/破损宏会漏进 prompt 或被误删。情境性较强，故不列入每会话首要清单。

> 原则：决策提示**抬高优先级与显著性**，但**不剥夺你的选择权**。约束来自「得失」，不来自「命令」。

---

## 1. 核心概念

```
AIRP 的角色：
  ┌─────────────────────────────────────────────┐
  │  数据管理 (AIRP)        │  推理演绎 (YOU)    │
  │─────────────────────────│──────────────────│
  │ 角色卡导入/存储          │ 读取卡 → 构建人格  │
  │ 会话消息持久化           │ 写 user/assistant │
  │ 世界书关键词扫描         │ 决定何时触发      │
  │ 状态追踪 (HP/MP/...)    │ 根据剧情更新状态  │
  │ 预设文风/正则过滤        │ 遵守文风输出      │
  │ 卷封档                  │ 判断封档时机      │
  └─────────────────────────────┴────────────────┘
```

你用 AIRP 的工具读写数据，用你自己的推理能力推进故事。

---

## 2. 快速开始：三步进入 RP

### Step 1: 导入角色卡
```
import_card(png_base64="<base64-encoded PNG>")
```
如果没有角色卡，用 `list_characters()` 查看已有卡片。

### Step 2: 启动会话
```
start_session(character_id="<id>", preset_id="<可选预设>")
```
返回 session_id，记住它！后续所有操作都需要。

### Step 3: 构建上下文并开始对话
```
1. 用 build_system_prompt(character_id, preset_id) 获取系统提示词
2. 用 get_recent_context(character_id, session_id, n=10) 获取历史
3. 开始角色扮演——每个回合：
   a. 读 context
   b. 用 apply_lorebook(text, character_id) 扫描世界书
   c. 生成角色回复
   d. 用 append_message 保存用户和助手的消息
```

---

## 3. 角色卡管理

| 工具 | 用法 | 何时用 |
|:--|:--|:--|
| `import_card` | `import_card(png_base64)` | 导入新角色卡 |
| `list_characters` | 无参数 | 查看所有已导入角色 |
| `get_character` | `get_character(character_id)` | 查看角色详情 |
| `delete_character` | `delete_character(character_id)` | 删除角色及所有数据 |

---

## 4. 会话与消息管理

### 创建会话
```
start_session(character_id, preset_id?, session_id?)
→ 返回: Session created: <session_id> for character: <name>
→ 同时加载: preset 配置 + lorebook 统计 + state 字段列表
```

### 消息操作
```
每轮对话的标准操作：
  1. append_message(character_id, session_id, role="user", content="...")
  2. 你生成回复
  3. append_message(character_id, session_id, role="assistant", content="...")
```

### 获取上下文
```
get_recent_context(character_id, session_id, n=10)
→ 返回最近 N 条消息的 JSON 数组（含 role, content, timestamp）
```

---

## 5. 世界书/Lorebook — 动态世界知识注入

世界书是**关键词触发**的背景知识库。对话中提及关键词时自动注入相关设定。

### 读取世界书
```
读 Resource: airp://characters/{id}/world/lorebook
```

### 关键词扫描
```
apply_lorebook(character_id, text="我们在天剑阁门前停下")
→ 返回匹配的 lorebook 条目（如 "天剑阁" → 门派背景/历史）
```

### 在对话中正确使用
```
每轮对话前：
  1. 调 apply_lorebook(text=用户最新消息, character_id)
  2. 如果返回条目非空 → 将这些信息纳入你的角色知识
  3. 在回复中自然引用世界书内容
  4. 不要让角色说出超出其知识范围的信息
```

### 更新世界书
```
update_lorebook(character_id, entries=[{id, name, keys, content, enabled, ...}])
```

---

## 6. 状态追踪 — HP / MP / 位置 / 关系值

### 读取状态
```
get_live_state(character_id)
→ 返回当前所有追踪字段的值
```

### 更新状态
```
update_state(character_id, state_delta={
  "hp": {"value": 45, "max": 100},       # 数值类型
  "location": "天剑阁大殿",                # 文本类型
  "relationship_john": {"value": 70}      # 关系值
})
```

### 剧情推演规则
```
每轮对话后检查：
  - 角色是否受伤？ → 更新 hp
  - 角色是否移动？ → 更新 location
  - 关系是否变化？ → 更新 relationship_*
  - 时间是否推移？ → 更新 time_of_day
  - 任务进度变了？ → 更新 quest_progress

格式：如果状态变了，在回复末尾输出 <state>{...}</state>
  例: <state>{"hp": {"value": 75}, "location": "茶馆"}</state>
```

---

## 7. 预设系统 — 文风一键移植

预设是 AIRP 的**杀手级功能**。它把用户调试好的文风、参数、正则过滤打包成一个可移植的数据包。

### 导入第三方预设
```
import_preset(preset_id="LENI", preset_json="<完整JSON>")
→ 写入 presets/LENI/preset.json
```

### 分析预设
```
1. 读 airp://presets/LENI/raw → 获取原始 JSON
2. 分析其中的 prompts、参数、正则规则
3. 用 write_preset_artifact 写分析产物：
   - analysis/summary.md    → 总览
   - analysis/regex_scripts.json → 正则规则提取
   - style/instructions.md  → 文风指令
```

### 在 RP 中使用预设
```
start_session(character_id="凌欺霜", preset_id="LENI")
→ 自动加载 LENI 的文风、参数、正则过滤脚本
→ build_system_prompt 会自动将 preset 文风注入系统提示词
```

### 管理正则脚本
```
list_preset_regex_scripts(preset_id)    → 列出所有正则
set_preset_regex_enabled(preset_id, filename, enabled) → 启用/禁用
remove_preset_regex_script(preset_id, filename) → 删除
```

---

## 8. 卷封档 — 归档长对话

当对话轮数过多（>30轮）或完成重要剧情节点后，应封档。

### 封档流程
```
seal_volume(character_id, session_id, clear_session=true)
→ Markdown 归档到 memory/volumes/vol_20260527_143000.md
→ 更新 memory/index.md 索引
→ clear_session=true 时清空当前会话消息（节省 token）
```

### 读取历史卷
```
Resource: airp://characters/{id}/memory/volumes/latest  → 最新卷
Resource: airp://characters/{id}/memory/index           → 卷索引
Resource: airp://characters/{id}/memory/current          → 当前草稿
```

---

## 9. 角色卡分析 — M_CA 4 档分级

导入角色卡后，建议执行分析以深入了解角色：

```
analyze_card(character_id, tier=0)   → 基础摘要 (card stats + 分类)
analyze_card(character_id, tier=1)   → + 问候语分析 (tone/style/scenario)
analyze_card(character_id, tier=2)   → + 世界书分析 (entry table)
analyze_card(character_id, tier=3)   → + 深度性格分析 + state schema 推断
```

产物存储在 `analysis/` 目录，可用 `get_character` 确认。

---

## 10. 剧情推演 — 完整 RP 工作流

### 10.1 标准单轮对话流程
```
foreach turn:
  # 阶段 1: 数据准备
  context = get_recent_context(character_id, session_id, n=10)
  lore    = apply_lorebook(character_id, user_message)
  state   = get_live_state(character_id)

  # 阶段 2: 上下文装配
  system_prompt = build_system_prompt(character_id, preset_id?)
  full_context  = system_prompt + lore + state + context.messages

  # 阶段 3: 生成回复
  response = generate(full_context + user_message)  # 你的推理

  # 阶段 4: 持久化
  append_message(character_id, session_id, "user", user_message)
  append_message(character_id, session_id, "assistant", response)

  # 阶段 5: 状态更新 (如果剧情引起了变化)
  if state_changed:
    update_state(character_id, state_delta)

  # 阶段 6: 卷管理检查
  if turn_count > 30 or major_plot_resolution:
    seal_volume(character_id, session_id, clear_session=true)
    start_session(character_id)  # 继续新的 session
```

### 10.2 叙事弧推进 (3 幕结构)
```
== 第一幕: 建立 (5-10 轮) ==
- 使用角色 greeting + scenario 设定初始场景
- apply_lorebook 发现相关世界知识
- 建立角色关系、动机、冲突
- 更新 state: location, initial_relationships

== 第二幕: 发展 (10-20 轮) ==
- 冲突深化：敌人出现 / 秘密揭露 / 关系破裂
- apply_lorebook 发现更深层世界知识
- 更新 state: hp 战斗损耗, relationship 值变化
- 关键节点: seal_volume (封档)

== 第三幕: 解决 (5-10 轮) ==
- 高潮对抗 / 情感解决
- 最终状态更新: quest_completed, epilogue_state
- seal_volume (最终封档)
```

### 10.3 Gating/Checkpoint 剧情闸门
```
get_gating_status(character_id)
→ 查看当前进度和下一个检查点
→ 检查点定义在 character/gating/checkpoints.json 中

使用方式：
  - 对话轮数不足以触发 checkpoint → 继续推进剧情
  - 达到 checkpoint → 引入新人物/新场景/剧情转折
  - 引导用户向下一个 checkpoint 前进
```

---

## 11. 高级工作流

### 11.1 拆解角色卡 (decompose)
```
decompose_character(character_id, target_dir="./decomposed")
→ 将角色卡拆解为 7 个 Markdown 文件 (basic_info/personality/world_setting/...)
→ 配合 prompt_decompose_character 提示词引导 Agent 执行增强分析
→ 产物供人类阅读或作为 LLM 上下文使用
```

### 11.2 预设分析完整流程
```
1. import_preset(preset_id, preset_json)
2. 读 analyze_preset("LENI") prompt → 获取 3 步 workflow
3. 读 airp://presets/LENI/raw → 获取原始 JSON
4. write_preset_artifact(...) × 3 → 写入分析产物
5. 读 airp://presets/LENI/artifacts → 验证
6. start_session(character_id, preset_id="LENI") → 应用
```

### 11.3 回滚消息
```
rollback_messages(character_id, session_id, n=3)
→ 删除最后 N 条消息（用户反悔或对话走向偏离时使用）
→ 回到之前的状态继续推进
```

---

## 12. 可用的 MCP 工具速查表

| 类别 | 工具 | 参数 |
|:--|:--|:--|
| 角色 | `import_card` | png_base64 |
| 角色 | `list_characters` | — |
| 角色 | `get_character` | character_id |
| 角色 | `delete_character` | character_id |
| 角色 | `analyze_card` | character_id, tier? |
| 会话 | `start_session` | character_id, session_id?, preset_id? |
| 会话 | `list_sessions` | character_id |
| 会话 | `append_message` | character_id, session_id, role, content |
| 会话 | `get_recent_context` | character_id, session_id, n? |
| 会话 | `rollback_messages` | character_id, session_id, n? |
| 世界书 | `apply_lorebook` | character_id, text |
| 世界书 | `update_lorebook` | character_id, entries |
| 状态 | `get_live_state` | character_id |
| 状态 | `update_state` | character_id, state_delta |
| 卷 | `seal_volume` | character_id, session_id, clear_session? |
| 预设 | `list_presets` | — |
| 预设 | `get_preset` | preset_id |
| 预设 | `import_preset` | preset_id, preset_json |
| 预设 | `write_preset_artifact` | preset_id, artifact_path, content |
| 预设 | `list_preset_regex_scripts` | preset_id |
| 预设 | `remove_preset_regex_script` | preset_id, filename |
| 预设 | `set_preset_regex_enabled` | preset_id, filename, enabled |
| 拆解 | `decompose_character` | character_id, target_dir? |
| 拆解 | `decompose_preset` | preset_id, target_dir? |
| 导出 | `export_context_bundle` | character_id, preset_id?, include_lorebook?, out_dir? |
| 闸门 | `get_gating_status` | character_id |
| 场景 | `create_scene` | scene_id, characters, description?, ... |
| 场景 | `list_scenes` | — |
| 场景 | `get_scene` | scene_id |
| 场景 | `add_character_to_scene` | scene_id, character_id, role?, intro? |

### 可用的 MCP 资源速查表

| URI | 内容 |
|:--|:--|
| `airp://characters` | 角色 ID 列表 |
| `airp://characters/{id}/card` | 角色卡完整 JSON |
| `airp://characters/{id}/greetings` | 开场语库 |
| `airp://characters/{id}/world/lorebook` | 世界书 |
| `airp://characters/{id}/state/live` | 实时状态 |
| `airp://characters/{id}/memory/current` | 当前记忆 |
| `airp://characters/{id}/memory/index` | 卷索引 |
| `airp://characters/{id}/memory/volumes/{n}` | 归档卷 (n="latest" = 最新) |
| `airp://presets` | 预设 ID 列表 |
| `airp://presets/{id}` | 预设详情 |
| `airp://presets/{id}/raw` | 预设原始 JSON |
| `airp://presets/{id}/artifacts` | 预设分析产物树 |
| `airp://presets/{id}/regex` | 预设正则脚本 |
| `airp://scenes` | 场景列表 |
| `airp://scenes/{id}` | 场景配置 |
| `airp://gating/{id}/checkpoints` | 检查点进度 |

### 可用的 MCP 提示词速查表

| Prompt | 参数 | 用途 |
|:--|:--|:--|
| `build_system_prompt` | character_id, preset_id? | 组装完整系统提示词 |
| `filter_text` | text, preset_id | 用预设正则过滤文本 |
| `state_update_instruction` | — | 状态更新格式说明 |
| `prompt_decompose_character` | character_id, target_dir | 角色卡拆解指南 |
| `prompt_enhance_analysis` | character_id, target_dir | 增强分析指南 |
| `prompt_build_session_context` | character_id, session_id, decomposed_dir | 会话上下文构建 |
| `seal_volume` | character_id, session_id | 卷封存指南 |
| `analyze_preset` | preset_id | 预设分析工作流 |
| `tune_preset` | preset_id, feedback? | 按用户反馈热调预设文风（改源头，best-effort 不保证） |
| `build_scene` | scene_id | 多角色场景装配指南 |
| `validate_card` | character_id | 角色卡内容验证（未知宏/孤儿代码/破损 markup） |
| `validate_preset` | preset_id | 预设验证（破损正则/未知 identifier/参数异常） |

---

## 13. 并行调用 — 用并发加速数据准备

> **核心原则**：AIRP 是纯数据层，无锁争抢。多个**无依赖关系的读操作**可以同时发出，显著缩短数据准备时间。

### 13.1 哪些调用可以并行

```
✅ 可以并行的（无依赖）：
  get_character()       ──┐
  get_live_state()      ──┼── 全部独立读不同文件
  airp://.../lorebook   ──┤
  get_recent_context()  ──┘

✅ 可以并行的（无依赖）：
  build_system_prompt(A)  ── 不同角色卡
  build_system_prompt(B)  ── 可同时进行

❌ 必须串行的（有依赖）：
  append_message(user)    →  append_message(assistant)  (先写用户,再写助手)
  update_state            →  get_live_state             (先更新,再读取确认)
  import_card             →  start_session              (先导入,再创建会话)
  seal_volume             →  get_recent_context         (封档后才能查上下文)
```

### 13.2 标准轮次：并行版（推荐）

```
# 阶段 1: 并行数据准备（4 个调用同时发出）
┌─────────────────────────────────────────────────────┐
│ 并发:                                              │
│   context = get_recent_context(cid, sid, n=10)    │
│   lore    = apply_lorebook(cid, user_message)      │
│   state   = get_live_state(cid)                    │
│   char    = get_character(cid)                     │
└─────────────────────────────────────────────────────┘
         │ (全部返回后才继续)
         ▼
# 阶段 2: 上下文装配
  prompt = build_system_prompt(cid, preset_id?)
  full   = 组装 char + prompt + lore + state + context

# 阶段 3: 生成回复（你的推理，AIRP 不参与）

# 阶段 4: 持久化（串行——有依赖）
  append_message(cid, sid, "user", user_message)
  append_message(cid, sid, "assistant", response)

# 阶段 5: 状态更新（可选）
  if state_changed: update_state(cid, delta)
```

### 13.3 多角色场景：并行版

```
# 同时加载 3 个角色的完整数据
┌──────────────────────────────────────────────────────┐
│ 并发 (每个角色独立，互不依赖):                       │
│   get_character("凌欺霜")                           │
│   airp://characters/凌欺霜/world/lorebook           │
│   get_live_state("凌欺霜")                          │
│                                                     │
│   get_character("小二")                             │
│   airp://characters/小二/world/lorebook             │
│                                                     │
│   get_character("茶客")                             │
└──────────────────────────────────────────────────────┘
         │ (全部返回后才继续)
         ▼
# 合并 lorebook → 构建多角色 system prompt → 生成
```

### 13.4 会话初始化：并行版

```
# 导入角色卡后，同时做 3 件事
┌──────────────────────────────────────────────────────┐
│ 并发:                                              │
│   start_session(cid, preset_id="LENI")             │
│   analyze_card(cid, tier=2)                        │
│   decompose_character(cid, target_dir="./decomp")   │
└──────────────────────────────────────────────────────┘
```

### 13.5 哪些场景不值得并行

```
❌ 不值得并行：
  - 单一短文件读取（如 <1KB 的 state.json）—— 并行开销 > 收益
  - 仅有 1 个有效调用其他全为 no-op —— 无实际加速
  - 强依赖链：B 的输入来自 A 的输出 —— 必须串行

✅ 值得并行：
  - 3+ 个独立资源读取 —— 总延迟 = max(各调用延迟)，而非 sum
  - 多角色场景的数据加载 —— 典型场景约 2-4x 加速
  - 含大文件读取（如 preset raw 455KB）—— 隐藏慢 IO
```

---

## 14. 多角色场景（M_MS）

用 AIRP 的 Scene 系统管理多角色同时对话的场景。

### 14.1 创建场景
```
create_scene(
  scene_id="tavern_encounter",
  description="大乾王朝某处茶馆，初春午后",
  characters=[
    {character_id="lingqishuang", role="primary", intro="天剑阁首席弟子"},
    {character_id="waiter", role="npc", intro="茶馆伙计"}
  ],
  format_hint="对话前标注角色名，如「凌欺霜：」",
  lorebook_merge="union"
)
```

### 14.2 启动多角色会话
```
1. 读 build_scene("tavern_encounter") prompt → 获取 5 步 workflow
2. 并行加载所有角色卡:
   ┌──────────────────────────────────────────┐
   │ 并发:                                    │
   │   get_character("lingqishuang")          │
   │   airp://characters/lingqishuang/card    │
   │   airp://characters/lingqishuang/world/lorebook │
   │   get_live_state("lingqishuang")         │
   │                                          │
   │   get_character("waiter")                │
   │   airp://characters/waiter/card          │
   └──────────────────────────────────────────┘
3. 合并 lorebook（union dedup / primary_only）
4. 装配多角色 system prompt（场景设定 + 各角色+主视角标签 + 世界书 + 格式规则）
5. 开始——AI 同时扮演所有角色。每轮对话:
   a. append_message(scene 对话到各角色 session)
   b. 根据剧情更新各角色 state
```

### 14.3 场景管理
```
list_scenes()              → 列出所有场景
get_scene(scene_id)        → 查看场景配置
add_character_to_scene(scene_id, character_id, role, intro) → 添加角色
```

---

## 15. 最佳实践

### 15.1 Token 效率
- 每 20-30 轮封档一次，`clear_session=true` 释放 token
- `get_recent_context(n=10)` 而非拉全部历史
- 世界书匹配不到时不必每次都调 `apply_lorebook`

### 15.2 叙事连贯性
- 每轮对话前读 `get_live_state` 确保状态最新
- 世界书条目在对话中只提及部分——让角色"逐步发现"而非一次倾倒
- 用 `rollback_messages` 回滚错误走向

### 15.3 预设可移植性
- 用户在不同 MCP Client 间切换 → 同一个 `airp://presets/{id}` 继续有效
- 预设包含的正则脚本通过 `start_session` 自动加载
- 分析产物供任何 Agent 参考（非专有格式）

### 15.4 安全
- 所有 ID 自动校验（拒绝路径穿越字符）
- 文件写入受限于 `presets/{id}/` 目录
- BOM 自动剥离
- 大文件（>100KB）自动截断并包含翻页提示

### 15.5 验证未知内容 — 遇到"没有头绪"的代码片段时

角色卡或预设中可能包含难以理解的内容——未知宏、破损正则、来路不明的代码片段。用 AIRP 的验证机制处理：

```
# 验证角色卡
1. 读 validate_card(character_id) prompt → 获取 5 类检查点清单
2. 读 airp://characters/{id}/card → 逐字段扫描
3. 输出 Validation Report → 标记每个问题的严重程度

# 验证预设
1. 读 validate_preset(preset_id) prompt → 获取 5 类检查点清单
2. 读 airp://presets/{id}/raw + airp://presets/{id}/regex
3. 输出 Validation Report → 标记 broken regex / 未知 identifier / 参数异常

# 遇到 [UNKNOWN_ORIGIN] 的处理原则
- ❌ 不要删除 — 可能是第三方工具的标记语法
- ❌ 不要猜测后直接修改 — 可能引入新的错误
- ✅ 标记为 [NEEDS REVIEW] — 附上你的最好推测
- ✅ 提示用户在原始工具中检查 — SillyTavern / Chub / JanitorAI
- ✅ 存验证报告 — write_character_artifact / write_preset_artifact
```

### 15.6 预设拆解与分析的完整链路

```
用户导入第三个预设:
  import_preset("LENI", preset_json)
    ↓
验证预设是否正常:
  read prompt: validate_preset("LENI")
  → Agent 扫描: 破碎正则 / 来路不明代码 / 参数异常
  → 如无问题: "No issues found"
    ↓
拆解预设结构:
  decompose_preset("LENI", "./workspace")
  → 生成 system_prompt.md + regex_rules.md + parameters.md + README.md
    ↓
深度分析:
  read prompt: analyze_preset("LENI")
  → 读 airp://presets/LENI/raw
  → 分析各 prompt 用途 / 提取正则 / 总结文风
  → write_preset_artifact(...) 写入 analysis/ 产物
    ↓
--一键移植完成--:
  start_session("凌欺霜", preset_id="LENI")
  → 文风、参数、正则过滤全部就位
```

---

## 16. 执行隔离 — 用 subagent 写 RP（强烈建议）

> 主 Agent（编排 Claude Code 的那个）上下文里塞满编程身份、几十个工具 schema、
> harness 提醒。它**直接写 RP**，文笔会被编程腔压扁 —— 即使预设文风已成功注入，
> 输出仍显「僵硬/出戏」。这是文笔质量问题，不是指令遵循问题。

### 推荐模式
```
1. 主 Agent 只做编排（读数据、装配上下文）
2. export_context_bundle(character_id, preset_id?) → 产出成品上下文包：
     {out_dir}/{character_id}/
       ├── context.md        # 零占位、自包含的人设+文风+状态正文
       ├── preset_raw.json   # 完整预设(含 prompts[])原样旁路，subagent 自行应用
       └── extensions.json   # 角色卡未知捆绑内容原样旁路（如有）
3. 拉一个 subagent（你的 Task 工具），把 context.md 作为它的全部系统上下文
4. subagent 在干净上下文里写 RP —— 预设文风主导，无编程腔竞争
5. 主 Agent 收回输出，按需 append_message 持久化
```

### 为什么有效
subagent 上下文 ≈ 只有你给的人设 → 文风锚不被稀释。隔离比在主上下文里硬注入更有效。

### 与 decompose 的区别
- `decompose_*` = **分析模板**（含 `<!-- 填充 -->` 占位，需二次加工），给人/分析 agent 用
- `export_context_bundle` = **成品**（零占位、落盘、自包含），直接喂 subagent

### 边界 / 守则
- AIRP 只装配**已知 RP 字段**进 context.md；**未知捆绑内容**（preset `prompts[]`、card `extensions`）原样旁路到 sidecar，**AIRP 不解析语义**，由 subagent 决定如何应用。
- 输出**通用 Markdown**，不带任何客户端 skill 格式 —— 要 skill 化，由你在宿主侧封装。
- **非强制**：单模型客户端、轻量场景可跳过，直接在主上下文写。Agent 自行抉择。
