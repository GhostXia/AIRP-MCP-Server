//! MCP Prompt handlers

use serde_json::Value;
use rmcp::model::{PromptMessage, PromptMessageRole};
use crate::error::Result;
use crate::models::*;
use crate::storage::*;
use super::AirpMcpServer;

impl AirpMcpServer {
    pub async fn build_system_prompt_messages(&self, args: Value) -> Result<Vec<PromptMessage>> {
        let character_id = args["character_id"].as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing character_id".to_string()))?;
        let preset_id = args["preset_id"].as_str();

        let id = CharacterId::new(character_id)?;
        let char_store = CharacterStore::new(&self.storage);
        let character = char_store.get(&id).await?;

        let char_prompt = character.card.build_system_prompt();

        let final_prompt = if let Some(preset_id) = preset_id {
            let preset_store = PresetStore::new(&self.storage);
            let preset_id = PresetId::new(preset_id)?;
            let preset = preset_store.get(&preset_id).await?;
            preset.build_system_prompt(&char_prompt)
        } else {
            char_prompt
        };

        let state = char_store.get_live_state(&id).await?;
        let with_state = if !state.values.is_empty() {
            let state_prompt = state.format_for_prompt(None);
            format!("{}\n\n{}", final_prompt, state_prompt)
        } else {
            final_prompt
        };

        Ok(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "You are roleplaying as the following character:\n\n{}\n\n\
                Stay in character at all times. Respond naturally as this character would.",
                with_state
            ),
        )])
    }

    pub async fn filter_text_messages(&self, args: Value) -> Result<Vec<PromptMessage>> {
        let text = args["text"].as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing text".to_string()))?;
        let preset_id = args["preset_id"].as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing preset_id".to_string()))?;

        let id = PresetId::new(preset_id)?;
        let store = PresetStore::new(&self.storage);
        let preset = store.get(&id).await?;

        let filtered = preset.apply_filters(text);

        Ok(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!("Original text:\n{}\n\nFiltered text:\n{}", text, filtered),
        )])
    }

    pub async fn state_update_instruction_messages(&self) -> Result<Vec<PromptMessage>> {
        let instruction = r#"When your character's state changes (HP, MP, location, etc.), 
please output the updated state in the following format:

<state>
{
  "hp": {"value": 75, "max": 100},
  "mp": {"value": 30, "max": 50},
  "location": "Town Square",
  "quest_progress": 3
}
</state>

Only include fields that have actually changed. The system will automatically update your character's live state."#;

        Ok(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            instruction.to_string(),
        )])
    }

    // Decompose prompts

    pub async fn prompt_decompose_character_messages(&self, args: Value) -> Result<Vec<PromptMessage>> {
        let character_id = args["character_id"].as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing character_id".to_string()))?;
        let target_dir = args["target_dir"].as_str().unwrap_or("./decomposed");

        let content = format!(r#"You are helping the user decompose a character card into Agent-friendly Markdown documents.

## Task Goal

Decompose character card "{character_id}" and store in "{target_dir}" directory.

## Execution Steps

### Step 1: Get Character Data
Use MCP tool `get_character` to retrieve the full character data:
```
get_character(character_id="{character_id}")
```

### Step 2: Create Directory Structure
Create the following structure in {target_dir}/characters/{character_id}/:
```
characters/{character_id}/
├── README.md
├── basic_info.md
├── personality.md
├── world_setting.md
├── speech_style.md
├── greetings.md
├── state_schema.md
└── world_book/
    ├── index.md
    └── entry_XXX.md
```

### Step 3: Write Base Files
Follow these specifications for each file:

**basic_info.md**:
```markdown
# Basic Info

## Name
{{{{character_name}}}}

## Description
{{{{description field}}}}

## Creator
{{{{creator}}}}

## Tags
- {{{{tag1}}}}
- {{{{tag2}}}}
```

**personality.md**:
```markdown
# Personality

{{{{personality field}}}}

## Personality Keywords
<!-- To be filled by analysis -->

## Behavior Patterns
<!-- To be filled by analysis -->
```

**world_setting.md**:
```markdown
# World Setting

## Scenario
{{{{scenario field}}}}

## World Elements
<!-- To be filled by analysis -->
```

**speech_style.md**:
```markdown
# Speech Style

## Example Messages
{{{{mes_example field}}}}

## Language Features
<!-- To be filled by analysis -->
```

**greetings.md**:
```markdown
# Greetings

## Default Greeting
{{{{first_mes field}}}}

## Alternate Greetings
<!-- If alternate_greetings exist -->
```

### Step 4: Decompose Lorebook
Use MCP tool to get lorebook and create individual md files for each entry.

### Step 5: Generate Index
Create README.md with quick reference links to all files.

## Notes
- Use UTF-8 encoding for all files
- Use relative paths for inter-file links
- Preserve `<!-- -->` comment markers for later enhancement analysis

## After Completion
Return the decomposition result including:
- List of created files
- Target directory path
- Whether enhancement analysis is needed"#);

        Ok(vec![PromptMessage::new_text(PromptMessageRole::User, content)])
    }

    pub async fn prompt_enhance_analysis_messages(&self, args: Value) -> Result<Vec<PromptMessage>> {
        let character_id = args["character_id"].as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing character_id".to_string()))?;
        let target_dir = args["target_dir"].as_str().unwrap_or("./decomposed");

        let content = format!(r#"You are performing enhanced analysis on decomposed character files.

## Task Goal

Analyze files in {target_dir}/characters/{character_id}/ and fill analysis results into comment areas.

## Execution Steps

### Step 1: Analyze Personality
Read `personality.md`, analyze and fill:

1. **Personality Keywords**:
   - Extract 5-10 core personality keywords from description
   - Format: `- Keyword: brief explanation`

2. **Behavior Patterns**:
   - Infer typical behavior patterns based on personality
   - Include: social style, decision-making, emotional expression

### Step 2: Analyze World Setting
Read `world_setting.md`, analyze and fill:

1. **World Elements**:
   - Time period: Ancient/Modern/Future/Fantasy
   - Location settings: main activity venues
   - Social environment: social structure, cultural characteristics

2. **Relationship Network**:
   - If mentioned in card, organize character relationships

### Step 3: Analyze Speech Style
Read `speech_style.md`, analyze and fill:

1. **Language Features**:
   - Tone characteristics: formal/casual/arrogant/gentle etc.
   - Common expressions: catchphrases, special terms
   - Taboo topics: content to avoid

2. **Dialogue Notes**:
   - Key points when talking to this character

### Step 4: Infer State Tracking
Read `state_schema.md`, analyze and fill:

1. Based on character type, infer states to track:
   - Combat characters: HP, MP, equipment status
   - Social characters: relationship values, favorability
   - Exploration characters: location, inventory, quest progress

2. Fill state field table

### Step 5: Analyze Lorebook Entries
For each `world_book/entry_XXX.md`:

1. Analyze entry content
2. Fill "Usage Scenarios" explanation
3. Describe when this info should be triggered in dialogue

## Output Format
Edit each md file directly, replace `<!-- -->` comments with actual analysis content.

## Notes
- Analysis should be based on card text, don't fabricate
- Keep objective, mark "to be confirmed" if uncertain
- Results should be concise and practical for later reference"#);

        Ok(vec![PromptMessage::new_text(PromptMessageRole::User, content)])
    }

    pub async fn prompt_build_session_context_messages(&self, args: Value) -> Result<Vec<PromptMessage>> {
        let character_id = args["character_id"].as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing character_id".to_string()))?;
        let _session_id = args["session_id"].as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing session_id".to_string()))?;
        let decomposed_dir = args["decomposed_dir"].as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing decomposed_dir".to_string()))?;

        let content = format!(r#"You are building initial context for a roleplay session.

## Task Goal

Build system prompt and initial context for session based on decomposed character files.

## Execution Steps

### Step 1: Read Character Modules
Read these files in order:
1. `{decomposed_dir}/characters/{character_id}/basic_info.md`
2. `{decomposed_dir}/characters/{character_id}/personality.md`
3. `{decomposed_dir}/characters/{character_id}/world_setting.md`
4. `{decomposed_dir}/characters/{character_id}/speech_style.md`
5. `{decomposed_dir}/characters/{character_id}/state_schema.md`

### Step 2: Read Preset (if specified)
If preset specified, read:
- `{decomposed_dir}/presets/{{{{preset_id}}}}/system_prompt.md`

### Step 3: Assemble System Prompt
Assemble in this order:

```
[Preset Prefix (if any)]

# Character Setting

## Basic Info
{{{{basic_info content}}}}

## Personality
{{{{personality content}}}}

## World Setting
{{{{world_setting content}}}}

## Speech Style
{{{{speech_style content}}}}

## Current State
{{{{state content (if any)}}}}

[Preset Suffix (if any)]
```

### Step 4: Load Lorebook
Based on current dialogue content, check if lorebook entries need loading:
- Read `world_book/index.md`
- Match trigger keywords
- Load relevant entries

### Step 5: Build Initial Messages
Use MCP tool `append_message` to write:
1. System message (assembled system prompt)
2. Character greeting (read from greetings.md)

## Output
Return summary of completed session context."#);

        Ok(vec![PromptMessage::new_text(PromptMessageRole::User, content)])
    }

    pub async fn seal_volume_messages(&self, args: Value) -> Result<Vec<PromptMessage>> {
        let character_id = args["character_id"].as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing character_id".to_string()))?;
        let session_id = args["session_id"].as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing session_id".to_string()))?;

        let content = format!(r#"You are sealing/archiving a session volume.

## Task Goal

Archive all messages from session "{session_id}" of character "{character_id}" into a volume file.

## Execution Steps

### Step 1: Get Session Data
Use MCP tool `get_recent_context` to retrieve all messages:
```
get_recent_context(character_id="{character_id}", session_id="{session_id}", n=1000)
```

### Step 2: Create Volume Archive
1. Create volume directory if not exists: `memory/volumes/`
2. Generate volume filename: `vol_{{{{timestamp}}}}.md`
3. Format messages into markdown:

```markdown
# Volume Archive: {{{{timestamp}}}}

## Session: {session_id}
## Character: {character_id}
## Message Count: {{{{count}}}}

---

### Message 1
**Role**: {{{{role}}}}
**Time**: {{{{timestamp}}}}

{{{{content}}}}

---

### Message 2
...
```

### Step 3: Update Index
Update `memory/index.md` to include new volume reference.

### Step 4: Clear Current Session (Optional)
If user wants to start fresh, use `rollback_messages` to clear current session.

## Notes
- Preserve all message metadata
- Maintain chronological order
- Include state changes if recorded in messages"#);

        Ok(vec![PromptMessage::new_text(PromptMessageRole::User, content)])
    }

    pub async fn analyze_preset_messages(&self, args: Value) -> Result<Vec<PromptMessage>> {
        let preset_id = args["preset_id"].as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing preset_id".to_string()))?;

        let content = format!(r#"You are a preset analysis Agent. Analyze preset `{pid}` by following these steps:

**Step 1: Read the Preset**
Read `airp://presets/{pid}/raw` to get the full SillyTavern Preset JSON.

**Step 2: Generate Analysis Artifacts** (call `write_preset_artifact` for each):
- `analysis/summary.md` — Prompt list, order, and purpose of each segment
- `analysis/regex_scripts.json` — Regex filter script array (each with name/pattern/flags/purpose)
- `style/instructions.md` — Extracted writing instructions (style, format requirements, taboos)

**Step 3: Verify**
Read `airp://presets/{pid}/artifacts` to confirm all artifact paths appear."#,
            pid = preset_id
        );

        Ok(vec![PromptMessage::new_text(PromptMessageRole::User, content)])
    }

    pub async fn build_scene_messages(&self, args: Value) -> Result<Vec<PromptMessage>> {
        let scene_id = args["scene_id"].as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing scene_id".to_string()))?;

        let content = format!(r#"You are running a multi-character roleplay scene `{sid}`.

**Step 1: Load Scene Configuration**
Read `airp://scenes/{sid}` for the full scene config.

**Step 2: Load All Character Cards In Parallel**
For each `character_id` in the scene config, read these resources **simultaneously** (no dependencies between them):
- `airp://characters/{{character_id}}/card`
- `airp://characters/{{character_id}}/world/lorebook`
- `airp://characters/{{character_id}}/state/live`

**Step 3: Merge Lorebooks**
If `lorebook_merge` is "union" (default), deduplicate all lorebook entries by content.
If "primary_only", only use the primary character's lorebook.

**Step 4: Assemble Multi-Character System Prompt**

Format:
```
[场景设定]
{{scene.description}}

[在场角色]

## {{primary.name}}（主视角）
[性格]: {{personality}}
[描述]: {{description}}
[场景设定]: {{scenario}}

## {{npc.name}}（NPC）
{{intro}}
[描述]: {{description}}

[世界书信息]
{{merged lorebook}}

[格式规则]
{{scene.format_hint}}
用户扮演 {{user_name}}，AI 不代写用户台词。
```

**Step 5: Narrative Execution**
- AI plays ALL characters simultaneously in a single response
- Before each character's dialogue, prefix with "Name: " format
- The primary character gets more detailed personality + deeper perspective
- NPCs are played with lighter touch based on their intros/descriptions
- Apply the scene's `format_hint` for dialogue formatting"#,
            sid = scene_id
        );

        Ok(vec![PromptMessage::new_text(PromptMessageRole::User, content)])
    }

    pub async fn validate_card_messages(&self, args: Value) -> Result<Vec<PromptMessage>> {
        let character_id = args["character_id"].as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing character_id".to_string()))?;

        let content = format!("Validate the character card `{0}` for unknown or suspicious content.\n\
\n\
Read `airp://characters/{0}/card` and scan for:\n\
\n\
**1. Unknown macros/templates** — patterns with double-brace like {{{{...}}}} or [[[...]]]\n\
that don't match standard SillyTavern variables ({{{{char}}}}, {{{{user}}}}, {{{{original}}}}, {{{{time}}}}).\n\
Flag them: suspected origin and suggestion.\n\
\n\
**2. Orphan code fragments** — any text that looks like code (regex, JSON fragments,\n\
XML snippets, SQL, shell commands) embedded in description/personality/scenario\n\
without clear roleplay purpose. Report: which field, the fragment, your best guess.\n\
\n\
**3. Broken markup** — angle-bracket tags that are not paired, not well-formed, or\n\
don't match known patterns (state, think, action, thought, status).\n\
Report: location, malformed content, fix suggestion.\n\
\n\
**4. Format mismatches** — if `description` is actually a full JSON dump, or\n\
`personality` is raw HTML, or any field clearly misformatted for its expected use.\n\
Report: which field, what's wrong, what it should be.\n\
\n\
**5. Empty/missing critical fields** — name, first_mes. Report if empty.\n\
\n\
Output format:\n\
```\n\
## Validation Report: {{character_name}}\n\
### Issues Found: {{N}}\n\
(if 0: \"No issues found — card appears clean.\")\n\
### Field-by-field\n\
- **{{{{field}}}}**: [OK / WARNING / ERROR]\n\
  {{{{description of issue and suggested fix}}}}\n\
```\n\
\n\
If you find fragments you cannot identify, do NOT delete them. Instead:\n\
- Mark as [UNKNOWN_ORIGIN] with your best guess in your response\n\
- Recommend the user review manually\n\
- Save your validation report to analysis/validation.md if write_character_artifact tool exists",
            character_id
        );

        Ok(vec![PromptMessage::new_text(PromptMessageRole::User, content)])
    }

    pub async fn tune_preset_messages(&self, args: Value) -> Result<Vec<PromptMessage>> {
        let preset_id = args["preset_id"].as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing preset_id".to_string()))?;
        let feedback = args["feedback"].as_str().unwrap_or(
            "The user finds the current output style unsatisfactory.");

        let content = format!(r#"You are hot-tuning preset `{pid}` based on the user's style feedback.

## User feedback
{feedback}

## Why this works
The preset was successfully injected — the model already writes in the preset's
style. So the fix is the preset CONTENT itself, not regeneration. Editing the
source preset is permanent and reused every turn (cheap, high-leverage). Do NOT
post-process the generated text.

## Steps
1. Read the current preset: `airp://presets/{pid}/raw`
2. Diagnose what in the preset causes the complaint. Common culprits:
   - **Stiff / lifeless prose** on a strong model: the preset may carry
     model-specific suppression (anti-verbosity, anti-divergence, anti-refusal
     scaffolding) tuned for a DIFFERENT model. On a model that is already
     controlled, that over-suppresses → flat. Relax or remove those parts.
   - Missing positive style direction → add an explicit vivid-prose / sensory /
     pacing instruction in `system_prompt_prefix` or `system_prompt_suffix`.
   - No voice anchors → ensure the character card's dialogue examples are used
     (build_scene_system_prompt supports `style_enhance: true`).
3. Apply the minimal change. Write it back with `import_preset` (full JSON) —
   keep every field you are not changing.
4. Report exactly what you changed and why, so the user can judge / revert.

## Boundaries
- Change only what the feedback implies. Do not rewrite the whole preset.
- This is a best-effort enhancement: it improves the odds, it does NOT guarantee
  the resulting style. If the first tune misses, ask the user to refine the
  feedback and iterate."#,
            pid = preset_id, feedback = feedback);

        Ok(vec![PromptMessage::new_text(PromptMessageRole::User, content)])
    }

    pub async fn validate_preset_messages(&self, args: Value) -> Result<Vec<PromptMessage>> {
        let preset_id = args["preset_id"].as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing preset_id".to_string()))?;

        let content = format!("Validate the preset `{0}` for unknown or suspicious content.\n\
\n\
Read `airp://presets/{0}/raw` and scan for:\n\
\n\
**1. Unknown prompt identifiers** — `prompts[].identifier` values that don't\n\
match known SillyTavern conventions (main, system, nsfl, jailbreak, enhance, etc.).\n\
List them and guess their likely intent.\n\
\n\
**2. Broken regex scripts** — check `airp://presets/{0}/regex` for any\n\
`findRegex` that is not wrapped in /pattern/flags format, contains\n\
unmatched parentheses/brackets, or references nonexistent capture groups.\n\
Report each broken script with filename and specific issue.\n\
\n\
**3. Orphan code in prompt content** — prompt `content` fields containing\n\
code-like fragments (function calls, object literals, HTML), template macros\n\
not in ST standard (e.g. percent-enclosed or unusual patterns),\n\
or apparent copy-paste artifacts (duplicated blocks, encoding mojibake).\n\
Report: which prompt (by name/identifier), the fragment, your assessment.\n\
\n\
**4. Missing structural fields** — if preset.json lacks `prompts` array,\n\
or has empty `prompts`, or all prompts are disabled. Report severity.\n\
\n\
**5. Parameter anomalies** — temperature greater than 2.0, max_tokens less than 50\n\
or greater than 200000, top_p less than 0 or greater than 1, top_k less than 0.\n\
These may be intentional for specific LLMs but worth flagging.\n\
\n\
Output format:\n\
```\n\
## Validation Report: Preset `{0}`\n\
### Issues Found: {{N}}\n\
(if 0: \"No issues found — preset appears clean.\")\n\
### By Category\n\
- **Prompts**: {{{{findings about identifiers and content}}}}\n\
- **Regex Scripts**: {{{{findings about broken patterns}}}}\n\
- **Parameters**: {{{{anomalous values}}}}\n\
```\n\
\n\
For fragments you cannot identify:\n\
- Mark as [NEEDS REVIEW] with your speculation\n\
- Recommend user check against the SillyTavern preset spec\n\
- Save report to presets/{0}/analysis/validation.md via write_preset_artifact",
            preset_id
        );

        Ok(vec![PromptMessage::new_text(PromptMessageRole::User, content)])
    }

}
