//! 角色卡拆解工具和提示词
//!
//! 设计思路：通过MCP提示词引导Agent执行拆解，而非AIRP自造runtime

use crate::error::Result;
use crate::models::*;
use tokio::fs;

/// 拆解配置
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DecomposeConfig {
    /// 目标目录（用户指定）
    pub target_dir: String,
    /// 是否执行Agent增强分析
    pub enhance_analysis: bool,
    /// 是否拆解世界书
    pub decompose_lorebook: bool,
}

impl Default for DecomposeConfig {
    fn default() -> Self {
        Self {
            target_dir: "./decomposed".to_string(),
            enhance_analysis: true,
            decompose_lorebook: true,
        }
    }
}

/// 角色卡拆解器
#[derive(Default)]
pub struct CharacterDecomposer;

impl CharacterDecomposer {
    pub fn new() -> Self {
        Self
    }

    /// 执行拆解（基础部分，不含Agent增强）
    pub async fn decompose(
        &self,
        character: &Character,
        config: &DecomposeConfig,
    ) -> Result<DecomposeResult> {
        let target_dir = std::path::Path::new(&config.target_dir)
            .join("characters")
            .join(character.id.as_ref());

        // 创建目录结构
        fs::create_dir_all(&target_dir).await?;
        fs::create_dir_all(target_dir.join("world_book")).await?;

        let mut files_written = vec![];

        // 1. 写入 basic_info.md
        let basic_info = self.generate_basic_info(character);
        let path = target_dir.join("basic_info.md");
        fs::write(&path, basic_info).await?;
        files_written.push(path.display().to_string());

        // 2. 写入 personality.md
        let personality = self.generate_personality(character);
        let path = target_dir.join("personality.md");
        fs::write(&path, personality).await?;
        files_written.push(path.display().to_string());

        // 3. 写入 world_setting.md
        let world_setting = self.generate_world_setting(character);
        let path = target_dir.join("world_setting.md");
        fs::write(&path, world_setting).await?;
        files_written.push(path.display().to_string());

        // 4. 写入 speech_style.md
        let speech_style = self.generate_speech_style(character);
        let path = target_dir.join("speech_style.md");
        fs::write(&path, speech_style).await?;
        files_written.push(path.display().to_string());

        // 5. 写入 greetings.md
        let greetings = self.generate_greetings(character);
        let path = target_dir.join("greetings.md");
        fs::write(&path, greetings).await?;
        files_written.push(path.display().to_string());

        // 6. 写入 state_schema.md
        let state_schema = self.generate_state_schema(character);
        let path = target_dir.join("state_schema.md");
        fs::write(&path, state_schema).await?;
        files_written.push(path.display().to_string());

        // 7. 生成 README.md（最后，包含索引）
        let readme = self.generate_readme(character, &files_written);
        let path = target_dir.join("README.md");
        fs::write(&path, readme).await?;
        files_written.push(path.display().to_string());

        Ok(DecomposeResult {
            character_id: character.id.clone(),
            target_dir: target_dir.display().to_string(),
            files_written,
            needs_enhancement: config.enhance_analysis,
        })
    }

    /// 拆解世界书
    pub async fn decompose_lorebook(
        &self,
        character_id: &CharacterId,
        lorebook: &Lorebook,
        config: &DecomposeConfig,
    ) -> Result<Vec<String>> {
        let target_dir = std::path::Path::new(&config.target_dir)
            .join("characters")
            .join(character_id.as_ref())
            .join("world_book");

        fs::create_dir_all(&target_dir).await?;

        let mut files_written = vec![];

        // 写入每个条目
        for (idx, entry) in lorebook.entries.iter().enumerate() {
            let entry_md = self.generate_lorebook_entry(entry, idx);
            let filename = format!("entry_{:03}_{}.md", idx + 1, sanitize_filename(&entry.id));
            let path = target_dir.join(&filename);
            fs::write(&path, entry_md).await?;
            files_written.push(path.display().to_string());
        }

        // 写入索引
        let index = self.generate_lorebook_index(&lorebook.entries);
        let path = target_dir.join("index.md");
        fs::write(&path, index).await?;
        files_written.push(path.display().to_string());

        Ok(files_written)
    }

    // === 生成各模块的Markdown内容 ===

    fn generate_basic_info(&self, character: &Character) -> String {
        format!(
            r#"# 基础信息

## 名称
{name}

## 完整描述
{description}

## 创作者
{creator}

## 版本
{version}

## 标签
{tags}

## 元数据
- 导入来源: {source}
- 创建时间: {created}
- 更新时间: {updated}
"#,
            name = character.card.name,
            description = character.card.description,
            creator = character.card.creator.as_deref().unwrap_or("未知"),
            version = character.card.character_version.as_deref().unwrap_or("1.0"),
            tags = character
                .card
                .tags
                .iter()
                .map(|t| format!("- {}", t))
                .collect::<Vec<_>>()
                .join("\n"),
            source = character.data.import_source.as_deref().unwrap_or("未知"),
            created = character.data.created_at.format("%Y-%m-%d %H:%M:%S"),
            updated = character.data.updated_at.format("%Y-%m-%d %H:%M:%S"),
        )
    }

    fn generate_personality(&self, character: &Character) -> String {
        format!(
            r#"# 性格特征

{personality}

## 性格关键词提取
<!-- Agent分析后填充 -->
<!-- 请分析上述性格描述，提取关键性格特征词 -->

## 行为模式
<!-- Agent分析后填充 -->
<!-- 请基于性格描述，推断角色的典型行为模式 -->
"#,
            personality = if character.card.personality.is_empty() {
                "（未定义）".to_string()
            } else {
                character.card.personality.clone()
            },
        )
    }

    fn generate_world_setting(&self, character: &Character) -> String {
        format!(
            r#"# 世界观设定

## 场景背景
{scenario}

## 世界观要素
<!-- Agent分析后填充 -->
<!-- 请分析场景背景，提取以下要素： -->
<!-- - 时代背景 -->
<!-- - 地点设定 -->
<!-- - 社会环境 -->

## 关系网络
<!-- 如有定义，请在此描述角色与其他人物的关系 -->
"#,
            scenario = if character.card.scenario.is_empty() {
                "（未定义）".to_string()
            } else {
                character.card.scenario.clone()
            },
        )
    }

    fn generate_speech_style(&self, character: &Character) -> String {
        format!(
            r#"# 说话风格

## 示例对话
{examples}

## 语言特征
<!-- Agent分析后填充 -->
<!-- 请分析示例对话，提取以下特征： -->
<!-- - 语气特点 -->
<!-- - 常用表达 -->
<!-- - 禁忌话题 -->

## 对话注意事项
<!-- Agent分析后填充 -->
<!-- 请总结与该角色对话时需要注意的事项 -->
"#,
            examples = if character.card.mes_example.is_empty() {
                "（未定义）".to_string()
            } else {
                character.card.mes_example.clone()
            },
        )
    }

    fn generate_greetings(&self, character: &Character) -> String {
        let mut content = format!(
            r#"# 开场白

## 默认开场白
{first_mes}
"#,
            first_mes = if character.card.first_mes.is_empty() {
                "（未定义）".to_string()
            } else {
                character.card.first_mes.clone()
            },
        );

        // 添加备选开场白
        if let Some(ext) = &character.card.extensions {
            if let Some(alts) = ext.get("alternate_greetings").and_then(|v| v.as_array()) {
                if !alts.is_empty() {
                    content.push_str("\n## 备选开场白\n");
                    for (idx, alt) in alts.iter().enumerate() {
                        if let Some(text) = alt.as_str() {
                            content.push_str(&format!("\n### 开场白 {}\n{}\n", idx + 1, text));
                        }
                    }
                }
            }
        }

        content.push_str(
            r#"
## 开场白选择建议
<!-- Agent分析后填充 -->
<!-- 请根据角色特点，给出不同场景下的开场白选择建议 -->
"#,
        );

        content
    }

    fn generate_state_schema(&self, character: &Character) -> String {
        format!(
            r#"# 状态追踪定义

> 该角色是否支持状态追踪: {has_tracking}

## 状态字段

<!-- 如果角色支持状态追踪，请在此定义字段 -->
<!-- 格式：| 字段名 | 类型 | 当前值 | 最大值 | 说明 | -->

| 字段名 | 类型 | 当前值 | 最大值 | 说明 |
|--------|------|--------|--------|------|
<!-- 示例：-->
<!-- | hp | number | - | 100 | 生命值 | -->
<!-- | mp | number | - | 50 | 魔法值 | -->
<!-- | location | text | - | - | 当前位置 | -->

## 状态更新格式
在回复中使用以下格式更新状态：

```xml
<state>
{{
  "hp": {{"value": 75, "max": 100}},
  "location": "城镇广场"
}}
</state>
```

## 状态推断建议
<!-- Agent分析后填充 -->
<!-- 请根据角色卡内容，推断可能需要追踪的状态字段 -->
"#,
            has_tracking = if character.data.has_state_tracking {
                "是"
            } else {
                "否"
            },
        )
    }

    fn generate_readme(&self, character: &Character, files: &[String]) -> String {
        format!(
            r#"# {name}

> 导入时间: {timestamp}
> 来源: {source}
> 分析等级: {tier}

## 快速引用

- [基础信息](./basic_info.md)
- [性格特征](./personality.md)
- [世界观设定](./world_setting.md)
- [说话风格](./speech_style.md)
- [开场白](./greetings.md)
- [世界书](./world_book/index.md)
- [状态定义](./state_schema.md)

## 一句话描述
{desc_short}

## 标签
{tags}

## 文件列表
共 {file_count} 个文件：
{file_list}
"#,
            name = character.card.name,
            timestamp = character.data.created_at.format("%Y-%m-%d %H:%M:%S"),
            source = character.data.import_source.as_deref().unwrap_or("未知"),
            tier = match &character.data.analysis_tier {
                Some(AnalysisTier::Tier0Basic) => "Tier 0 (基础)",
                Some(AnalysisTier::Tier1Greeting) => "Tier 1 (问候语)",
                Some(AnalysisTier::Tier2Lorebook) => "Tier 2 (世界书)",
                Some(AnalysisTier::Tier3Advanced) => "Tier 3 (高级)",
                None => "未分析",
            },
            desc_short = character
                .card
                .description
                .chars()
                .take(100)
                .collect::<String>(),
            tags = character.card.tags.join(", "),
            file_count = files.len(),
            file_list = files
                .iter()
                .map(|f| format!("- {}", f))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    fn generate_lorebook_entry(&self, entry: &LorebookEntry, _idx: usize) -> String {
        format!(
            r#"# {name}

> ID: {id}
> 触发关键词: {keys}
> 插入顺序: {order}

## 内容

{content}

## 使用场景
<!-- Agent分析后填充 -->
<!-- 请分析该条目内容，说明应在什么场景下触发 -->
"#,
            name = entry.name.as_deref().unwrap_or(&entry.id),
            id = entry.id,
            keys = entry.keys.join(", "),
            order = entry.insertion_order,
            content = entry.content,
        )
    }

    fn generate_lorebook_index(&self, entries: &[LorebookEntry]) -> String {
        let mut content = format!(
            r#"# 世界书索引

> 共 {} 条条目

## 条目列表

| 编号 | 名称 | 触发关键词 | 文件 |
|------|------|------------|------|
"#,
            entries.len()
        );

        for (idx, entry) in entries.iter().enumerate() {
            let filename = format!("entry_{:03}_{}.md", idx + 1, sanitize_filename(&entry.id));
            content.push_str(&format!(
                "| {:03} | {} | {} | [查看](./{}) |\n",
                idx + 1,
                entry.name.as_deref().unwrap_or(&entry.id),
                entry.keys.join(", "),
                filename,
            ));
        }

        content.push_str(
            r#"
## 使用说明
当对话中出现触发关键词时，Agent应查阅对应条目获取背景信息。
"#,
        );

        content
    }
}

/// 预设拆解器
#[derive(Default)]
pub struct PresetDecomposer;

impl PresetDecomposer {
    pub fn new() -> Self {
        Self
    }

    pub async fn decompose(
        &self,
        preset: &Preset,
        config: &DecomposeConfig,
    ) -> Result<DecomposeResult> {
        let target_dir = std::path::Path::new(&config.target_dir)
            .join("presets")
            .join(preset.id.as_ref());

        fs::create_dir_all(&target_dir).await?;

        let mut files_written = vec![];

        // 1. system_prompt.md
        let system_prompt = self.generate_system_prompt(preset);
        let path = target_dir.join("system_prompt.md");
        fs::write(&path, system_prompt).await?;
        files_written.push(path.display().to_string());

        // 2. regex_rules.md
        let regex_rules = self.generate_regex_rules(preset);
        let path = target_dir.join("regex_rules.md");
        fs::write(&path, regex_rules).await?;
        files_written.push(path.display().to_string());

        // 3. parameters.md
        let parameters = self.generate_parameters(preset);
        let path = target_dir.join("parameters.md");
        fs::write(&path, parameters).await?;
        files_written.push(path.display().to_string());

        // 4. README.md
        let readme = self.generate_readme(preset);
        let path = target_dir.join("README.md");
        fs::write(&path, readme).await?;
        files_written.push(path.display().to_string());

        Ok(DecomposeResult {
            character_id: CharacterId::new(preset.id.as_ref())?,
            target_dir: target_dir.display().to_string(),
            files_written,
            needs_enhancement: false,
        })
    }

    fn generate_system_prompt(&self, preset: &Preset) -> String {
        format!(
            r#"# 系统提示词

## 前缀
```
{prefix}
```

## 主体
<!-- 由角色卡的各模块组合而成 -->
<!-- 组装顺序： -->
<!-- 1. 前缀 -->
<!-- 2. 角色基础信息 -->
<!-- 3. 性格特征 -->
<!-- 4. 世界观设定 -->
<!-- 5. 当前状态（如有） -->
<!-- 6. 后缀 -->

## 后缀
```
{suffix}
```
"#,
            prefix = if preset.config.system_prompt_prefix.is_empty() {
                "（无）"
            } else {
                &preset.config.system_prompt_prefix
            },
            suffix = if preset.config.system_prompt_suffix.is_empty() {
                "（无）"
            } else {
                &preset.config.system_prompt_suffix
            },
        )
    }

    fn generate_regex_rules(&self, preset: &Preset) -> String {
        let mut content = format!(
            r#"# 正则过滤规则

> 共 {} 条规则

## 规则列表

"#,
            preset.config.regex_scripts.len()
        );

        for (idx, script) in preset.config.regex_scripts.iter().enumerate() {
            content.push_str(&format!(
                r#"### 规则 {}: {}

- **查找**: `{}`
- **替换**: `{}`
- **状态**: {}

"#,
                idx + 1,
                script.name,
                script.find,
                script.replace,
                if script.enabled { "启用" } else { "禁用" },
            ));
        }

        content
    }

    fn generate_parameters(&self, preset: &Preset) -> String {
        format!(
            r#"# 模型参数

| 参数 | 值 | 说明 |
|------|-----|------|
| temperature | {} | 生成随机性 |
| top_p | {} | 核采样 |
| top_k | {} | 候选词数量 |
| repetition_penalty | {} | 重复惩罚 |
| max_tokens | {} | 最大生成长度 |

## 停止序列
{}
"#,
            preset.config.temperature,
            preset.config.top_p,
            preset.config.top_k,
            preset.config.repetition_penalty,
            preset.config.max_tokens,
            if preset.config.stop_sequences.is_empty() {
                "（无）".to_string()
            } else {
                preset
                    .config
                    .stop_sequences
                    .iter()
                    .map(|s| format!("```\n{}\n```", s))
                    .collect::<Vec<_>>()
                    .join("\n")
            },
        )
    }

    fn generate_readme(&self, preset: &Preset) -> String {
        format!(
            r#"# 预设: {name}

> ID: {id}

## 快速引用

- [系统提示词](./system_prompt.md)
- [正则规则](./regex_rules.md)
- [模型参数](./parameters.md)

## 说明
该预设定义了AI生成回复时的行为规范和参数设置。
"#,
            name = preset.name,
            id = preset.id.as_ref(),
        )
    }
}

/// 拆解结果
#[derive(Debug, Clone)]
pub struct DecomposeResult {
    pub character_id: CharacterId,
    pub target_dir: String,
    pub files_written: Vec<String>,
    pub needs_enhancement: bool,
}

// 辅助函数
fn sanitize_filename(name: &str) -> String {
    name.to_lowercase()
        .replace(" ", "_")
        .replace(|c: char| !c.is_alphanumeric() && c != '_' && c != '-', "")
}
