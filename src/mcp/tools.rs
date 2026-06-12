//! MCP Tool handlers

use super::AirpMcpServer;
use crate::error::{AirpError, Result};
use crate::models::gating::GatingConfig;
use crate::models::*;
use crate::storage::*;
use serde_json::Value;

impl AirpMcpServer {
    // Character tools

    pub async fn handle_import_card(&self, args: Value) -> Result<String> {
        // Bounds memory + limits decompression-bomb surface (zTXt/IDAT zlib).
        const MAX_PNG_BYTES: usize = 10 * 1024 * 1024;
        let png_base64 = args["png_base64"].as_str();
        let png_path = args["png_path"].as_str();

        let png_data = match (png_base64, png_path) {
            (Some(b64), None) => {
                let data = base64_decode(b64)?;
                if data.len() > MAX_PNG_BYTES {
                    return Err(crate::error::AirpError::Validation(format!(
                        "PNG too large: {} bytes exceeds {} byte cap",
                        data.len(),
                        MAX_PNG_BYTES
                    )));
                }
                data
            }
            (None, Some(path)) => {
                // Server-side read: the PNG bytes / base64 never enter the model
                // context, avoiding the base64 token-burn (a 10 MiB card = ~13 MiB
                // of base64 text if the agent encodes it). Size-check via metadata
                // BEFORE reading, so a bomb file is rejected without loading it.
                let meta = tokio::fs::metadata(path).await.map_err(|e| {
                    crate::error::AirpError::Validation(format!(
                        "cannot stat png_path {}: {}",
                        path, e
                    ))
                })?;
                if !meta.is_file() {
                    return Err(crate::error::AirpError::Validation(format!(
                        "png_path is not a file: {}",
                        path
                    )));
                }
                if meta.len() > MAX_PNG_BYTES as u64 {
                    return Err(crate::error::AirpError::Validation(format!(
                        "PNG too large: {} bytes exceeds {} byte cap",
                        meta.len(),
                        MAX_PNG_BYTES
                    )));
                }
                tokio::fs::read(path).await.map_err(|e| {
                    crate::error::AirpError::Validation(format!(
                        "cannot read png_path {}: {}",
                        path, e
                    ))
                })?
            }
            (Some(_), Some(_)) => {
                return Err(crate::error::AirpError::Validation(
                    "provide exactly one of png_base64 / png_path".to_string(),
                ));
            }
            (None, None) => {
                return Err(crate::error::AirpError::Validation(
                    "missing png_base64 or png_path".to_string(),
                ));
            }
        };

        let store = CharacterStore::new(&self.storage);
        let character = store.import_from_png(&png_data).await?;

        Ok(format!(
            "Successfully imported character: {} (ID: {})",
            character.card.name,
            character.id.as_ref()
        ))
    }

    pub async fn handle_list_characters(&self) -> Result<String> {
        let store = CharacterStore::new(&self.storage);
        let characters = store.list().await?;

        if characters.is_empty() {
            return Ok("No characters imported yet.".to_string());
        }

        let mut lines = vec!["Characters:".to_string()];
        for char in characters {
            lines.push(format!("- {} (ID: {})", char.card.name, char.id.as_ref()));
        }

        Ok(lines.join("\n"))
    }

    pub async fn handle_get_character(&self, args: Value) -> Result<String> {
        let character_id = args["character_id"].as_str().ok_or_else(|| {
            crate::error::AirpError::Validation("Missing character_id".to_string())
        })?;

        let id = CharacterId::new(character_id)?;
        let store = CharacterStore::new(&self.storage);
        let character = store.get(&id).await?;

        let json = serde_json::to_string_pretty(&character)?;
        Ok(json)
    }

    pub async fn handle_delete_character(&self, args: Value) -> Result<String> {
        let character_id = args["character_id"].as_str().ok_or_else(|| {
            crate::error::AirpError::Validation("Missing character_id".to_string())
        })?;

        let id = CharacterId::new(character_id)?;
        let store = CharacterStore::new(&self.storage);
        store.delete(&id).await?;

        Ok(format!("Character {} deleted successfully.", character_id))
    }

    // Session tools

    pub async fn handle_start_session(&self, args: Value) -> Result<String> {
        let character_id = args["character_id"].as_str().ok_or_else(|| {
            crate::error::AirpError::Validation("Missing character_id".to_string())
        })?;

        let session_id = args["session_id"]
            .as_str()
            .map(SessionId::new)
            .transpose()?;

        let preset_id_str = args["preset_id"].as_str();

        let char_id = CharacterId::new(character_id)?;
        let char_store = CharacterStore::new(&self.storage);
        let character = char_store.get(&char_id).await?;

        let store = SessionStore::new(&self.storage);
        let session = store.create(&char_id, session_id).await?;

        let mut info_lines = vec![format!(
            "Session created: {} for character: {} ({})",
            session.id.as_ref(),
            character.card.name,
            character_id
        )];

        // Integrate preset
        if let Some(pid) = preset_id_str {
            let preset_id = PresetId::new(pid)?;
            let preset_store = PresetStore::new(&self.storage);
            match preset_store.get(&preset_id).await {
                Ok(preset) => {
                    let session_dir = self
                        .storage
                        .character_dir(&char_id)
                        .join("sessions")
                        .join(&session.id.0);
                    let meta_path = session_dir.join("meta.json");
                    let meta_json = tokio::fs::read_to_string(&meta_path).await?;
                    let mut meta: SessionMeta = serde_json::from_str(&meta_json)?;
                    meta.preset_id = Some(preset_id.as_ref().to_string());
                    let updated_meta = serde_json::to_string_pretty(&meta)?;
                    tokio::fs::write(&meta_path, updated_meta).await?;
                    info_lines.push(format!("Preset applied: {} ({})", preset.name, pid));

                    // Load and report SillyTavern regex scripts
                    let regex_dir = self.storage.preset_regex_dir(pid);
                    let scripts = crate::mcp::preset_regex::load_preset_regex_scripts(&regex_dir);
                    if !scripts.is_empty() {
                        let active = scripts.iter().filter(|s| !s.disabled).count();
                        info_lines.push(format!(
                            "Regex scripts: {} total, {} active",
                            scripts.len(),
                            active
                        ));
                    }
                }
                Err(_) => info_lines.push(format!("Warning: preset '{}' not found", pid)),
            }
        }

        // Load lorebook summary
        let lorebook = char_store.get_lorebook(&char_id).await?;
        let entry_count = lorebook.entries.iter().filter(|e| e.enabled).count();
        info_lines.push(format!("Lorebook loaded: {} active entries", entry_count));

        // Load live state
        let state = char_store.get_live_state(&char_id).await?;
        if !state.values.is_empty() {
            let state_summary = state.values.keys().cloned().collect::<Vec<_>>().join(", ");
            info_lines.push(format!("Live state fields: [{}]", state_summary));
        } else {
            info_lines.push("Live state: empty (tracking not yet started)".to_string());
        }

        Ok(info_lines.join("\n"))
    }

    pub async fn handle_list_sessions(&self, args: Value) -> Result<String> {
        let character_id = args["character_id"].as_str().ok_or_else(|| {
            crate::error::AirpError::Validation("Missing character_id".to_string())
        })?;

        let id = CharacterId::new(character_id)?;
        let store = SessionStore::new(&self.storage);
        let sessions = store.list(&id).await?;

        if sessions.is_empty() {
            return Ok(format!("No sessions for character: {}", character_id));
        }

        let enriched: Vec<serde_json::Value> = sessions
            .iter()
            .map(|(sid, meta)| serde_json::json!({ "session_id": sid, "meta": meta }))
            .collect();
        let json = serde_json::to_string_pretty(&enriched)?;
        Ok(json)
    }

    pub async fn handle_append_message(&self, args: Value) -> Result<String> {
        let character_id = args["character_id"].as_str().ok_or_else(|| {
            crate::error::AirpError::Validation("Missing character_id".to_string())
        })?;
        let session_id = args["session_id"]
            .as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing session_id".to_string()))?;
        let role = args["role"]
            .as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing role".to_string()))?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing content".to_string()))?;

        let char_id = CharacterId::new(character_id)?;
        let sess_id = SessionId::new(session_id)?;

        let message_role = match role {
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "system" => MessageRole::System,
            _ => {
                return Err(crate::error::AirpError::Validation(format!(
                    "Invalid role: {}",
                    role
                )));
            }
        };

        let message = Message::new(message_role, content);

        let store = SessionStore::new(&self.storage);
        store.append_message(&char_id, &sess_id, &message).await?;

        Ok("Message appended successfully.".to_string())
    }

    pub async fn handle_get_recent_context(&self, args: Value) -> Result<String> {
        let character_id = args["character_id"].as_str().ok_or_else(|| {
            crate::error::AirpError::Validation("Missing character_id".to_string())
        })?;
        let session_id = args["session_id"]
            .as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing session_id".to_string()))?;
        let n = args["n"].as_u64().unwrap_or(10) as usize;

        let char_id = CharacterId::new(character_id)?;
        let sess_id = SessionId::new(session_id)?;

        let store = SessionStore::new(&self.storage);
        let messages = store.get_recent_context(&char_id, &sess_id, n).await?;

        let json = serde_json::to_string_pretty(&messages)?;
        Ok(json)
    }

    // Lorebook tools

    pub async fn handle_apply_lorebook(&self, args: Value) -> Result<String> {
        let character_id = args["character_id"].as_str().ok_or_else(|| {
            crate::error::AirpError::Validation("Missing character_id".to_string())
        })?;
        let text = args["text"]
            .as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing text".to_string()))?;

        let id = CharacterId::new(character_id)?;
        let store = CharacterStore::new(&self.storage);
        let lorebook = store.get_lorebook(&id).await?;

        let context = lorebook.build_context(text);

        if context.is_empty() {
            Ok("No lorebook entries matched.".to_string())
        } else {
            Ok(context)
        }
    }

    pub async fn handle_update_lorebook(&self, args: Value) -> Result<String> {
        let character_id = args["character_id"].as_str().ok_or_else(|| {
            crate::error::AirpError::Validation("Missing character_id".to_string())
        })?;
        let entries = args["entries"]
            .as_array()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing entries".to_string()))?;

        let id = CharacterId::new(character_id)?;

        let lorebook_entries: Vec<LorebookEntry> = entries
            .iter()
            .map(|e| serde_json::from_value(e.clone()).map_err(AirpError::Json))
            .collect::<Result<Vec<_>>>()?;

        let lorebook = Lorebook {
            entries: lorebook_entries,
        };

        let store = CharacterStore::new(&self.storage);
        store.save_lorebook(&id, &lorebook).await?;

        Ok(format!("Lorebook updated for character: {}", character_id))
    }

    // State tools

    pub async fn handle_update_state(&self, args: Value) -> Result<String> {
        let character_id = args["character_id"].as_str().ok_or_else(|| {
            crate::error::AirpError::Validation("Missing character_id".to_string())
        })?;
        let state_delta = args["state_delta"].clone();

        let id = CharacterId::new(character_id)?;
        let store = CharacterStore::new(&self.storage);

        let mut state = store.get_live_state(&id).await?;
        state.update(state_delta);
        store.save_live_state(&id, &state).await?;

        // Notify subscribers
        // In real implementation, send notification to subscribed clients

        Ok(format!("State updated for character: {}", character_id))
    }

    pub async fn handle_get_live_state(&self, args: Value) -> Result<String> {
        let character_id = args["character_id"].as_str().ok_or_else(|| {
            crate::error::AirpError::Validation("Missing character_id".to_string())
        })?;

        let id = CharacterId::new(character_id)?;
        let store = CharacterStore::new(&self.storage);
        let state = store.get_live_state(&id).await?;

        let json = serde_json::to_string_pretty(&state)?;
        Ok(json)
    }

    // Volume tools

    pub async fn handle_seal_volume(&self, args: Value) -> Result<String> {
        let character_id = args["character_id"].as_str().ok_or_else(|| {
            crate::error::AirpError::Validation("Missing character_id".to_string())
        })?;
        let session_id = args["session_id"]
            .as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing session_id".to_string()))?;
        let clear_session = args["clear_session"].as_bool().unwrap_or(false);

        let char_id = CharacterId::new(character_id)?;
        let sess_id = SessionId::new(session_id)?;

        // 1. Get all messages from the session
        let session_store = SessionStore::new(&self.storage);
        let session = session_store.get(&char_id, &sess_id).await?;

        if session.messages.is_empty() {
            return Ok(format!("Session {} has no messages to seal.", session_id));
        }

        // 2. Create volume directory
        let char_dir = self.storage.character_dir(&char_id);
        let volumes_dir = char_dir.join("memory").join("volumes");
        tokio::fs::create_dir_all(&volumes_dir).await?;

        // 3. Generate volume filename with timestamp
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let volume_filename = format!("vol_{}.md", timestamp);
        let volume_path = volumes_dir.join(&volume_filename);

        // 4. Get current volume number
        let current_volume = session.meta.current_volume;
        let new_volume = current_volume + 1;

        // 5. Format messages into markdown
        let mut volume_content = format!(
            r#"# Volume {}

## Metadata
- **Session**: {}
- **Character**: {}
- **Sealed At**: {}
- **Message Count**: {}
- **Volume Number**: {}

---

"#,
            new_volume,
            session_id,
            character_id,
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"),
            session.messages.len(),
            new_volume
        );

        // Add each message
        for (idx, msg) in session.messages.iter().enumerate() {
            let role_str = match msg.role {
                crate::models::MessageRole::System => "System",
                crate::models::MessageRole::User => "User",
                crate::models::MessageRole::Assistant => "Assistant",
            };

            volume_content.push_str(&format!(
                r#"### Message {}

**Role**: {}
**Time**: {}

{}

---

"#,
                idx + 1,
                role_str,
                msg.timestamp.format("%Y-%m-%d %H:%M:%S"),
                msg.content
            ));
        }

        // 6. Write volume file
        tokio::fs::write(&volume_path, volume_content).await?;

        // 7. Update memory index
        let memory_dir = char_dir.join("memory");
        tokio::fs::create_dir_all(&memory_dir).await?;
        let index_path = memory_dir.join("index.md");

        let index_entry = format!(
            "- [Volume {}](./volumes/{}) - {} messages - {}\n",
            new_volume,
            volume_filename,
            session.messages.len(),
            chrono::Utc::now().format("%Y-%m-%d")
        );

        // Append to index
        use tokio::io::AsyncWriteExt;
        let mut index_file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&index_path)
            .await?;
        index_file.write_all(index_entry.as_bytes()).await?;
        index_file.flush().await?;

        // 8. Update session metadata with new volume number
        let session_dir = char_dir.join("sessions").join(&sess_id.0);
        let meta_path = session_dir.join("meta.json");
        let meta_json = tokio::fs::read_to_string(&meta_path).await?;
        let mut meta: crate::models::SessionMeta = serde_json::from_str(&meta_json)?;
        meta.current_volume = new_volume;
        meta.updated_at = chrono::Utc::now();

        let updated_meta = serde_json::to_string_pretty(&meta)?;
        tokio::fs::write(&meta_path, updated_meta).await?;

        // 9. Clear session if requested
        let clear_info = if clear_session {
            // Clear chat.jsonl
            let chat_path = session_dir.join("chat.jsonl");
            tokio::fs::write(&chat_path, "").await?;

            // Update message count
            meta.message_count = 0;
            let updated_meta = serde_json::to_string_pretty(&meta)?;
            tokio::fs::write(&meta_path, updated_meta).await?;

            " Session cleared."
        } else {
            ""
        };

        Ok(format!(
            "Volume {} sealed successfully.\nFile: {}\nMessages archived: {}{}",
            new_volume,
            volume_path.display(),
            session.messages.len(),
            clear_info
        ))
    }

    // Preset tools

    pub async fn handle_list_presets(&self) -> Result<String> {
        let store = PresetStore::new(&self.storage);
        let presets = store.list().await?;

        if presets.is_empty() {
            return Ok("No presets configured.".to_string());
        }

        let mut lines = vec!["Presets:".to_string()];
        for preset in presets {
            lines.push(format!("- {} (ID: {})", preset.name, preset.id.as_ref()));
        }

        Ok(lines.join("\n"))
    }

    pub async fn handle_get_preset(&self, args: Value) -> Result<String> {
        let preset_id = args["preset_id"]
            .as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing preset_id".to_string()))?;

        let id = PresetId::new(preset_id)?;
        let store = PresetStore::new(&self.storage);
        let preset = store.get(&id).await?;

        let json = serde_json::to_string_pretty(&preset)?;
        Ok(json)
    }

    // Decompose tools

    pub async fn handle_decompose_character(&self, args: Value) -> Result<String> {
        let character_id = args["character_id"].as_str().ok_or_else(|| {
            crate::error::AirpError::Validation("Missing character_id".to_string())
        })?;
        let target_dir = args["target_dir"].as_str().unwrap_or("./decomposed");
        let enhance = args["enhance"].as_bool().unwrap_or(true);

        let id = CharacterId::new(character_id)?;

        // Get character
        let char_store = CharacterStore::new(&self.storage);
        let character = char_store.get(&id).await?;

        // Get lorebook
        let lorebook = char_store.get_lorebook(&id).await?;

        // Decompose
        let config = crate::mcp::DecomposeConfig {
            target_dir: target_dir.to_string(),
            enhance_analysis: enhance,
            decompose_lorebook: !lorebook.entries.is_empty(),
        };

        let decomposer = crate::mcp::CharacterDecomposer::new();
        let result = decomposer.decompose(&character, &config).await?;

        // Decompose lorebook if exists
        if config.decompose_lorebook {
            decomposer
                .decompose_lorebook(&id, &lorebook, &config)
                .await?;
        }

        Ok(format!(
            "Character '{}' decomposed successfully.\nTarget directory: {}\nFiles created: {}\nEnhancement needed: {}",
            character.card.name,
            result.target_dir,
            result.files_written.len(),
            if result.needs_enhancement {
                "Yes"
            } else {
                "No"
            }
        ))
    }

    pub async fn handle_decompose_preset(&self, args: Value) -> Result<String> {
        let preset_id = args["preset_id"]
            .as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing preset_id".to_string()))?;
        let target_dir = args["target_dir"].as_str().unwrap_or("./decomposed");

        let id = PresetId::new(preset_id)?;

        // Get preset
        let store = PresetStore::new(&self.storage);
        let preset = store.get(&id).await?;

        // Decompose
        let config = crate::mcp::DecomposeConfig {
            target_dir: target_dir.to_string(),
            enhance_analysis: false,
            decompose_lorebook: false,
        };

        let decomposer = crate::mcp::PresetDecomposer::new();
        let result = decomposer.decompose(&preset, &config).await?;

        Ok(format!(
            "Preset '{}' decomposed successfully.\nTarget directory: {}\nFiles created: {}",
            preset.name,
            result.target_dir,
            result.files_written.len()
        ))
    }

    // Gating tools

    pub async fn handle_gating_status(&self, args: Value) -> Result<String> {
        let character_id = args["character_id"].as_str().ok_or_else(|| {
            crate::error::AirpError::Validation("Missing character_id".to_string())
        })?;

        let id = CharacterId::new(character_id)?;
        let char_dir = self.storage.character_dir(&id);
        let gating_path = char_dir.join("gating").join("checkpoints.json");

        if !gating_path.exists() {
            return Ok(format!(
                "No gating configured for character '{}'. Create gating/checkpoints.json to enable.",
                character_id
            ));
        }

        let gating: GatingConfig =
            serde_json::from_str(&tokio::fs::read_to_string(&gating_path).await?)?;

        let mut status = vec![
            format!("Gating Status for {}", character_id),
            format!("Turn count: {}", gating.turn_count),
            format!(
                "Checkpoints: {}/{}",
                gating.checkpoints.iter().filter(|c| c.reached).count(),
                gating.checkpoints.len()
            ),
            String::new(),
            "Checkpoints:".to_string(),
        ];

        for cp in &gating.checkpoints {
            let marker = if cp.reached { "✓" } else { "○" };
            status.push(format!(
                "  {} {} (turn {}) {}",
                marker,
                cp.label,
                cp.turns_required,
                if !cp.reached {
                    format!(
                        "- {} turns remaining",
                        cp.turns_required.saturating_sub(gating.turn_count)
                    )
                } else {
                    String::new()
                }
            ));
        }

        Ok(status.join("\n"))
    }

    // Message management

    pub async fn handle_analyze_card(&self, args: Value) -> Result<String> {
        let character_id = args["character_id"].as_str().ok_or_else(|| {
            crate::error::AirpError::Validation("Missing character_id".to_string())
        })?;
        let tier = args["tier"].as_u64().unwrap_or(0) as u8;

        let char_id = CharacterId::new(character_id)?;
        let char_store = CharacterStore::new(&self.storage);
        let character = char_store.get(&char_id).await?;
        let lorebook = char_store.get_lorebook(&char_id).await?;

        let char_dir = self.storage.character_dir(&char_id);
        let analysis_dir = char_dir.join("analysis");
        tokio::fs::create_dir_all(&analysis_dir).await?;

        let mut files_created = vec![];

        // Tier 0: Basic summary (always)
        let summary_md = format!(
            r#"# Character Analysis: {}

## Tier Classification
**Tier**: {} - {}
**Reasoning**: {}

## Basic Info
- **Name**: {}
- **Creator**: {}
- **Version**: {}
- **Tags**: {}

## Card Fields Summary
| Field | Length |
|-------|--------|
| Description | {} chars |
| Personality | {} chars |
| Scenario | {} chars |
| Mes Example | {} chars |
| First Message | {} chars |

## Lorebook Stats
- Total entries: {}
- Active entries: {}
- Key categories: {}
"#,
            character.card.name,
            if character.data.has_state_tracking {
                2
            } else {
                if !lorebook.entries.is_empty() { 1 } else { 0 }
            },
            match (
                character.data.has_state_tracking,
                !lorebook.entries.is_empty()
            ) {
                (true, true) => "Key-Value State Card + Lorebook",
                (true, false) => "Key-Value State Card",
                (false, true) => "Pure Setting/Geographic Card",
                (false, false) => "Basic RP Card",
            },
            class_reason(
                character.data.has_state_tracking,
                !lorebook.entries.is_empty()
            ),
            character.card.name,
            character.card.creator.as_deref().unwrap_or("Unknown"),
            character.card.character_version.as_deref().unwrap_or("1.0"),
            character.card.tags.join(", "),
            character.card.description.len(),
            character.card.personality.len(),
            character.card.scenario.len(),
            character.card.mes_example.len(),
            character.card.first_mes.len(),
            lorebook.entries.len(),
            lorebook.entries.iter().filter(|e| e.enabled).count(),
            class_categories(&lorebook),
        );

        let summary_path = analysis_dir.join("summary.md");
        tokio::fs::write(&summary_path, summary_md).await?;
        files_created.push("analysis/summary.md".to_string());

        // Tier 1+: Greetings analysis
        if tier >= 1 {
            let greetings_md = format!(
                r#"# Greetings Analysis

## Default Greeting
{}

## Analysis
- **Tone**: {}
- **Style**: {}
- **Scenario hint**: {}
"#,
                character.card.first_mes,
                detect_tone(&character.card.first_mes),
                detect_style(&character.card.first_mes),
                if character.card.scenario.is_empty() {
                    "None"
                } else {
                    &character.card.scenario
                },
            );

            let greeting_path = analysis_dir.join("greetings.md");
            tokio::fs::write(&greeting_path, greetings_md).await?;
            files_created.push("analysis/greetings.md".to_string());
        }

        // Tier 2+: Lorebook analysis
        if tier >= 2 && !lorebook.entries.is_empty() {
            let mut lorebook_md = format!(
                r#"# Lorebook Analysis

## Entry Count: {}
## Active: {}

| # | Name | Keys | Content Length |
|---|------|------|----------------|
"#,
                lorebook.entries.len(),
                lorebook.entries.iter().filter(|e| e.enabled).count(),
            );

            for (idx, entry) in lorebook.entries.iter().enumerate() {
                lorebook_md.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    idx + 1,
                    entry.name.as_deref().unwrap_or(&entry.id),
                    entry.keys.join(", "),
                    entry.content.len(),
                ));
            }

            let lorebook_path = analysis_dir.join("lorebook.md");
            tokio::fs::write(&lorebook_path, lorebook_md).await?;
            files_created.push("analysis/lorebook.md".to_string());
        }

        // Tier 3: Advanced - state schema and personality deep dive
        if tier >= 3 || character.data.has_state_tracking {
            let state_schema = analysis_dir.join("state_schema.md");
            let has_schema = state_schema.exists();
            if has_schema {
                files_created.push("analysis/state_schema.md (existing)".to_string());
            } else {
                let schema_md = "# State Schema\n\n| Field | Type | Min | Max | Description |\n|-------|------|-----|-----|-------------|\n<!-- Auto-detected fields will be added here -->\n".to_string();
                tokio::fs::write(&state_schema, schema_md).await?;
                files_created.push("analysis/state_schema.md".to_string());
            }

            let personality_path = analysis_dir.join("personality_deep.md");
            let personality_md = format!(
                r#"# Personality Deep Dive

## Raw Personality Text
{}

## Keyword Extraction
```
<!-- Agent: Please extract 5-10 personality keywords from the above text -->
```

## Behavior Prediction
```
<!-- Agent: Based on personality, predict typical behavior patterns -->
```

## Voice & Tone Analysis
{}
"#,
                character.card.personality,
                detect_voice(&character.card.mes_example),
            );
            tokio::fs::write(&personality_path, personality_md).await?;
            files_created.push("analysis/personality_deep.md".to_string());
        }

        // Save tier to character data
        let data_path = char_dir.join("data.json");
        if data_path.exists() {
            let json = tokio::fs::read_to_string(&data_path).await?;
            let mut data: CharacterData = serde_json::from_str(&json)?;

            let tier_enum = match tier.min(3) {
                0 => AnalysisTier::Tier0Basic,
                1 => AnalysisTier::Tier1Greeting,
                2 => AnalysisTier::Tier2Lorebook,
                _ => AnalysisTier::Tier3Advanced,
            };

            data.analysis_tier = Some(tier_enum);
            data.has_state_tracking = character.data.has_state_tracking || tier >= 3;
            data.updated_at = chrono::Utc::now();

            let updated = serde_json::to_string_pretty(&data)?;
            tokio::fs::write(&data_path, updated).await?;
        }

        Ok(format!(
            "Analysis complete for '{}'. Tier: {}\nFiles created in analysis/:\n{}",
            character.card.name,
            tier,
            files_created
                .iter()
                .map(|f| format!("  - {}", f))
                .collect::<Vec<_>>()
                .join("\n"),
        ))
    }

    pub async fn handle_rollback_messages(&self, args: Value) -> Result<String> {
        let character_id = args["character_id"].as_str().ok_or_else(|| {
            crate::error::AirpError::Validation("Missing character_id".to_string())
        })?;
        let session_id = args["session_id"]
            .as_str()
            .ok_or_else(|| crate::error::AirpError::Validation("Missing session_id".to_string()))?;
        let n = args["n"].as_u64().unwrap_or(1) as usize;

        let char_id = CharacterId::new(character_id)?;
        let sess_id = SessionId::new(session_id)?;

        // Get session
        let store = SessionStore::new(&self.storage);
        let session = store.get(&char_id, &sess_id).await?;

        if session.messages.len() < n {
            return Err(crate::error::AirpError::Validation(format!(
                "Cannot rollback {} messages, session only has {}",
                n,
                session.messages.len()
            )));
        }

        // Calculate new message count
        let new_count = session.messages.len() - n;

        // Rewrite chat.jsonl with truncated messages
        let session_dir = self
            .storage
            .character_dir(&char_id)
            .join("sessions")
            .join(&sess_id.0);
        let chat_path = session_dir.join("chat.jsonl");

        // Keep only first new_count messages
        let messages_to_keep: Vec<Message> = session.messages.into_iter().take(new_count).collect();

        // Rewrite file
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::File::create(&chat_path).await?;
        for msg in &messages_to_keep {
            let line = serde_json::to_string(msg)?;
            file.write_all(line.as_bytes()).await?;
            file.write_all(b"\n").await?;
        }
        file.flush().await?;

        // Update metadata
        let meta_path = session_dir.join("meta.json");
        let meta_json = tokio::fs::read_to_string(&meta_path).await?;
        let mut meta: crate::models::SessionMeta = serde_json::from_str(&meta_json)?;
        meta.message_count = new_count;
        meta.updated_at = chrono::Utc::now();

        let updated_meta = serde_json::to_string_pretty(&meta)?;
        tokio::fs::write(&meta_path, updated_meta).await?;

        Ok(format!(
            "Rolled back {} message(s) from session {}.\nNew message count: {}",
            n, session_id, new_count
        ))
    }

    // ── M_MS Scene tools ─────────────────────────────────────────────────

    pub async fn handle_create_scene(&self, args: serde_json::Value) -> Result<String> {
        let scene_id = args["scene_id"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing scene_id".to_string()))?;
        let description = args["description"].as_str().unwrap_or("");
        let narrator_style = args["narrator_style"]
            .as_str()
            .unwrap_or("third_person_limited");
        let format_hint = args["format_hint"].as_str().unwrap_or("");
        let lorebook_merge = args["lorebook_merge"].as_str().unwrap_or("union");

        let characters: Vec<CharacterEntry> = if let Some(arr) = args["characters"].as_array() {
            arr.iter()
                .map(|v| CharacterEntry {
                    character_id: v["character_id"].as_str().unwrap_or("").to_string(),
                    role: match v["role"].as_str().unwrap_or("npc") {
                        "primary" => CharacterRole::Primary,
                        _ => CharacterRole::Npc,
                    },
                    intro: v["intro"].as_str().unwrap_or("").to_string(),
                })
                .collect()
        } else {
            return Err(AirpError::Validation(
                "characters must be an array".to_string(),
            ));
        };

        let merge = match lorebook_merge {
            "primary_only" => LorebookMerge::PrimaryOnly,
            _ => LorebookMerge::Union,
        };

        let config = SceneConfig {
            scene_id: scene_id.to_string(),
            description: description.to_string(),
            characters,
            narrator_style: narrator_style.to_string(),
            lorebook_merge: merge,
            format_hint: format_hint.to_string(),
        };

        self.storage.save_scene(&config).await?;

        Ok(serde_json::to_string_pretty(&config)?)
    }

    pub async fn handle_list_scenes(&self) -> Result<String> {
        let scenes = self.storage.list_scenes().await?;
        serde_json::to_string_pretty(&scenes).map_err(Into::into)
    }

    pub async fn handle_get_scene(&self, args: serde_json::Value) -> Result<String> {
        let scene_id = args["scene_id"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing scene_id".to_string()))?;

        let config = self.storage.load_scene(scene_id).await?;
        serde_json::to_string_pretty(&config).map_err(Into::into)
    }

    pub async fn handle_add_character_to_scene(&self, args: serde_json::Value) -> Result<String> {
        let scene_id = args["scene_id"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing scene_id".to_string()))?;
        let character_id = args["character_id"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing character_id".to_string()))?;
        let role = args["role"].as_str().unwrap_or("npc");
        let intro = args["intro"].as_str().unwrap_or("");

        let mut config = self.storage.load_scene(scene_id).await?;

        let entry = CharacterEntry {
            character_id: character_id.to_string(),
            role: match role {
                "primary" => CharacterRole::Primary,
                _ => CharacterRole::Npc,
            },
            intro: intro.to_string(),
        };
        config.characters.push(entry);
        self.storage.save_scene(&config).await?;

        Ok(serde_json::to_string_pretty(&config)?)
    }

    // ── M_MS Lorebook merge — pure algorithm, no AI ─────────────────────

    pub async fn handle_merge_lorebooks(&self, args: serde_json::Value) -> Result<String> {
        let character_ids: Vec<&str> = args["character_ids"]
            .as_array()
            .ok_or_else(|| AirpError::Validation("character_ids must be an array".to_string()))?
            .iter()
            .filter_map(|v| v.as_str())
            .collect();

        let strategy = args["strategy"].as_str().unwrap_or("union");

        let char_store = CharacterStore::new(&self.storage);
        let mut all_entries: Vec<LorebookEntry> = Vec::new();

        for cid in &character_ids {
            let id = CharacterId::new(*cid)?;
            if let Ok(lb) = char_store.get_lorebook(&id).await {
                all_entries.extend(lb.entries);
            }
        }

        if strategy == "primary_only" {
            if let Some(&first) = character_ids.first() {
                let id = CharacterId::new(first)?;
                let lb = char_store.get_lorebook(&id).await?;
                return Ok(serde_json::to_string_pretty(&lb)?);
            }
        }

        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut merged: Vec<&LorebookEntry> = Vec::new();
        for e in &all_entries {
            if e.enabled && seen.insert(&e.content) {
                merged.push(e);
            }
        }
        merged.sort_by_key(|e| std::cmp::Reverse(e.insertion_order));

        let mut out = String::new();
        out.push_str(&format!(
            "Merged {} lorebook entries from {} characters (strategy: {})\n\n",
            merged.len(),
            character_ids.len(),
            strategy
        ));

        for e in &merged {
            let name = e.name.as_deref().unwrap_or("unnamed");
            let keys = e.keys.join(", ");
            out.push_str(&format!(
                "## {}\n[keys: {}]\n{}\n\n---\n\n",
                name, keys, e.content
            ));
        }

        Ok(out)
    }

    pub async fn handle_build_scene_system_prompt(
        &self,
        args: serde_json::Value,
    ) -> Result<String> {
        let scene_id = args["scene_id"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing scene_id".to_string()))?;
        let user_name = args["user_name"].as_str().unwrap_or("User");
        let preset_id = args["preset_id"].as_str();
        // Opt-in style enhancement (non-mandatory, default off). Injects each
        // character's dialogue examples + the preset suffix as voice anchors.
        // Enhancement only: grows the prompt, may improve style fidelity, but
        // does NOT guarantee the final output style.
        let style_enhance = args["style_enhance"].as_bool().unwrap_or(false);

        let config = self.storage.load_scene(scene_id).await?;
        let char_store = CharacterStore::new(&self.storage);

        let mut prompt = String::new();

        if !config.description.is_empty() {
            prompt.push_str("[场景设定]\n");
            prompt.push_str(&config.description);
            prompt.push_str("\n\n");
        }

        prompt.push_str("[在场角色]\n");

        let mut char_ids: Vec<String> = Vec::new();
        for entry in &config.characters {
            char_ids.push(entry.character_id.clone());

            let role_label = match entry.role {
                CharacterRole::Primary => "（主视角）",
                CharacterRole::Npc => "（NPC）",
            };

            let id = CharacterId::new(&entry.character_id)?;
            let char_info = match char_store.get(&id).await {
                Ok(c) => {
                    let mut info = String::new();
                    if !c.card.personality.is_empty() {
                        info.push_str(&format!("[性格]: {}\n", c.card.personality));
                    }
                    if !c.card.description.is_empty() {
                        info.push_str(&format!("[描述]: {}\n", c.card.description));
                    }
                    if !c.card.scenario.is_empty() {
                        info.push_str(&format!("[场景背景]: {}\n", c.card.scenario));
                    }
                    // Few-shot voice samples anchor the character's prose style.
                    // Opt-in only (style_enhance) — single-char build always
                    // includes these, but here we leave it to the caller.
                    if style_enhance && !c.card.mes_example.is_empty() {
                        info.push_str(&format!("[对话范例]:\n{}\n", c.card.mes_example));
                    }
                    info
                }
                Err(_) => "(角色卡未导入)".to_string(),
            };

            prompt.push_str(&format!("## {}{}\n", entry.character_id, role_label));
            if !entry.intro.is_empty() {
                prompt.push_str(&format!("场景介绍: {}\n", entry.intro));
            }
            prompt.push_str(&char_info);
            prompt.push('\n');
        }

        let mut all_entries: Vec<LorebookEntry> = Vec::new();
        for cid in &char_ids {
            let id = CharacterId::new(cid.as_str())?;
            if let Ok(lb) = char_store.get_lorebook(&id).await {
                all_entries.extend(lb.entries);
            }
        }

        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut merged: Vec<&LorebookEntry> = Vec::new();
        for e in &all_entries {
            if e.enabled && seen.insert(&e.content) {
                merged.push(e);
            }
        }
        merged.sort_by_key(|e| std::cmp::Reverse(e.insertion_order));

        if !merged.is_empty() {
            prompt.push_str("[世界书信息]\n");
            for e in &merged {
                prompt.push_str(&format!(
                    "- {}: {}\n",
                    e.name.as_deref().unwrap_or("unnamed"),
                    e.content
                ));
            }
            prompt.push('\n');
        }

        if !config.format_hint.is_empty() {
            prompt.push_str("[格式规则]\n");
            prompt.push_str(&config.format_hint);
            prompt.push('\n');
        }
        prompt.push_str(&format!("用户扮演 {}，AI 不代写用户台词。\n", user_name));

        if let Some(pid) = preset_id {
            let preset_id_obj = PresetId::new(pid)?;
            let preset_store = PresetStore::new(&self.storage);
            if let Ok(preset) = preset_store.get(&preset_id_obj).await {
                if !preset.config.system_prompt_prefix.is_empty() {
                    prompt.push_str("\n---\n");
                    prompt.push_str(&preset.config.system_prompt_prefix);
                    prompt.push('\n');
                }
                // Suffix = post-history style anchor at the very end (e.g. "keep
                // voice vivid"). Opt-in (style_enhance); single-char
                // preset.build_system_prompt always honors it.
                if style_enhance && !preset.config.system_prompt_suffix.is_empty() {
                    prompt.push_str(&preset.config.system_prompt_suffix);
                    prompt.push('\n');
                }
            }
        }

        Ok(prompt)
    }

    // ── M_EXPORT: self-contained context bundle for subagent handoff ──────
    // Deterministic assembly (no LLM). Distinct from decompose_* (analysis
    // scaffold with TODO placeholders) and build_system_prompt (ephemeral
    // inline). Produces a persisted, placeholder-free, generic-Markdown bundle
    // meant to be fed to an ISOLATED subagent — where the RP persona dominates
    // a clean context instead of competing with the orchestrator's coding
    // register. Known RP fields are assembled into context.md; unknown bundled
    // content (raw preset prompts[], card.extensions) is passed through verbatim
    // to sidecars — AIRP never interprets it; the subagent applies it.

    pub async fn handle_export_context_bundle(&self, args: serde_json::Value) -> Result<String> {
        let character_id = args["character_id"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing character_id".to_string()))?;
        let preset_id = args["preset_id"].as_str();
        let out_dir = args["out_dir"].as_str().unwrap_or("./exports");
        let include_lorebook = args["include_lorebook"].as_bool().unwrap_or(false);
        // Optional thinking-mode directive (e.g. the "chen-guide" trick: control
        // the model's reasoning shape — immersive in-character monologue vs pure
        // analysis). The community treats this as the #1 RP-quality lever, set
        // first-turn. Verbatim passthrough — model-specific content stays the
        // caller's data; AIRP does not author or interpret it.
        let thinking_mode_text = args["thinking_mode_text"].as_str();

        let id = CharacterId::new(character_id)?;
        let char_store = CharacterStore::new(&self.storage);
        let character = char_store.get(&id).await?;

        // 1. Assembled clean prose (known fields; mes_example is included by
        //    card.build_system_prompt). Preset adds prefix/suffix only — its
        //    full prompts[] body goes to the sidecar, not interpreted here.
        let char_prompt = character.card.build_system_prompt();
        let mut prose = if let Some(pid) = preset_id {
            let preset_store = PresetStore::new(&self.storage);
            let preset = preset_store.get(&PresetId::new(pid)?).await?;
            preset.build_system_prompt(&char_prompt)
        } else {
            char_prompt
        };

        // Live state
        let state = char_store.get_live_state(&id).await?;
        if !state.values.is_empty() {
            prose.push_str("\n\n## Current State\n");
            prose.push_str(&state.format_for_prompt(None));
        }

        // Optional lorebook (all enabled entries, ordered by insertion_order)
        if include_lorebook {
            let lorebook = char_store.get_lorebook(&id).await?;
            let mut enabled: Vec<&LorebookEntry> =
                lorebook.entries.iter().filter(|e| e.enabled).collect();
            enabled.sort_by_key(|e| std::cmp::Reverse(e.insertion_order));
            if !enabled.is_empty() {
                prose.push_str("\n\n## World Knowledge (lorebook)\n");
                for e in enabled {
                    prose.push_str(&format!(
                        "\n### {}\n{}\n",
                        e.name.as_deref().unwrap_or(&e.id),
                        e.content
                    ));
                }
            }
        }

        // Thinking-mode block sits FIRST in the actual context (highest salience,
        // shapes the model's reasoning before the persona). Verbatim, uninterpreted.
        let thinking_block = match thinking_mode_text {
            Some(t) if !t.trim().is_empty() => format!(
                "## Thinking mode (keep active every turn — verbatim directive, AIRP does not interpret)\n{}\n\n---\n\n",
                t.trim()
            ),
            _ => String::new(),
        };

        let mut context_md = format!(
            "# RP Context Bundle: {}\n\n\
            > Feed this to an ISOLATED subagent as its system context. A fresh \
            subagent context lets this persona dominate, instead of competing with \
            the orchestrator's coding register. Generic Markdown — no host-specific \
            skill format; wrap it in your host's skill shape if needed.\n\n---\n\n{}{}\n",
            character.card.name, thinking_block, prose
        );

        // Write bundle dir + sidecars (raw passthrough, never interpreted)
        let dir = std::path::Path::new(out_dir).join(character_id);
        tokio::fs::create_dir_all(&dir).await?;
        let mut files: Vec<String> = Vec::new();

        if let Some(pid) = preset_id {
            let raw_path = self.storage.preset_json_path(pid);
            if raw_path.exists() {
                let raw = tokio::fs::read_to_string(&raw_path).await?;
                tokio::fs::write(dir.join("preset_raw.json"), &raw).await?;
                files.push("preset_raw.json".to_string());
                context_md.push_str(
                    "\n> Sidecar `preset_raw.json` — full preset incl. prompts[] \
                    (AIRP does not interpret; apply it for max style fidelity).\n",
                );
            }
        }

        if let Some(ext) = &character.card.extensions {
            let empty = ext.is_null() || ext.as_object().map(|o| o.is_empty()).unwrap_or(false);
            if !empty {
                tokio::fs::write(
                    dir.join("extensions.json"),
                    serde_json::to_string_pretty(ext)?,
                )
                .await?;
                files.push("extensions.json".to_string());
                context_md.push_str(
                    "\n> Sidecar `extensions.json` — raw bundled card extensions \
                    (character_book / depth_prompt / third-party), unparsed passthrough.\n",
                );
            }
        }

        tokio::fs::write(dir.join("context.md"), &context_md).await?;
        files.insert(0, "context.md".to_string());

        Ok(serde_json::json!({
            "character_id": character_id,
            "out_dir": dir.display().to_string(),
            "files": files,
            "context_bytes": context_md.len(),
            "note": "Self-contained context.md for subagent handoff; sidecars are raw passthrough (uninterpreted).",
        }).to_string())
    }

    // ── M_PR Preset tools ────────────────────────────────────────────────

    pub async fn handle_import_preset(&self, args: serde_json::Value) -> Result<String> {
        let preset_id = args["preset_id"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing preset_id".to_string()))?;
        let preset_json = args["preset_json"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing preset_json".to_string()))?;

        validate_id_segment(preset_id)?;

        let _: serde_json::Value = serde_json::from_str(preset_json)
            .map_err(|e| AirpError::Validation(format!("preset_json is not valid JSON: {}", e)))?;

        let preset_path = self.storage.preset_json_path(preset_id);
        if let Some(parent) = preset_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&preset_path, preset_json).await?;

        Ok(serde_json::json!({
            "preset_id": preset_id,
            "path": preset_path.to_string_lossy(),
            "bytes_written": preset_json.len(),
        })
        .to_string())
    }

    pub async fn handle_write_preset_artifact(&self, args: serde_json::Value) -> Result<String> {
        let preset_id = args["preset_id"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing preset_id".to_string()))?;
        let artifact_path = args["artifact_path"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing artifact_path".to_string()))?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing content".to_string()))?;

        validate_id_segment(preset_id)?;

        let preset_dir = self.storage.presets_dir().join(preset_id);
        tokio::fs::create_dir_all(&preset_dir).await?;

        let artifact_full = self
            .storage
            .safe_resolve_for_write(&preset_dir, artifact_path)?;
        if let Some(parent) = artifact_full.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&artifact_full, content).await?;

        Ok(serde_json::json!({
            "preset_id": preset_id,
            "artifact_path": artifact_path,
            "bytes_written": content.len(),
        })
        .to_string())
    }

    pub async fn handle_list_preset_regex_scripts(
        &self,
        args: serde_json::Value,
    ) -> Result<String> {
        let preset_id = args["preset_id"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing preset_id".to_string()))?;

        validate_id_segment(preset_id)?;

        let regex_dir = self.storage.preset_regex_dir(preset_id);
        if !regex_dir.exists() {
            return Ok("[]".to_string());
        }

        let mut scripts: Vec<serde_json::Value> = Vec::new();
        let mut read_dir = tokio::fs::read_dir(&regex_dir).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let filename = entry.file_name().to_string_lossy().into_owned();
            let raw = match tokio::fs::read_to_string(&path).await {
                Ok(r) => r,
                Err(_) => continue,
            };
            let cleaned = crate::storage::strip_utf8_bom(&raw);

            let mut v: serde_json::Value = match serde_json::from_str(cleaned) {
                Ok(v) => v,
                Err(_) => {
                    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(cleaned) {
                        for mut item in arr {
                            if let Some(obj) = item.as_object_mut() {
                                obj.insert("_filename".into(), serde_json::json!(filename));
                            }
                            scripts.push(item);
                        }
                    }
                    continue;
                }
            };
            if let Some(obj) = v.as_object_mut() {
                obj.insert("_filename".into(), serde_json::json!(filename));
            }
            scripts.push(v);
        }

        serde_json::to_string(&scripts).map_err(AirpError::Json)
    }

    pub async fn handle_remove_preset_regex_script(
        &self,
        args: serde_json::Value,
    ) -> Result<String> {
        let preset_id = args["preset_id"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing preset_id".to_string()))?;
        let filename = args["filename"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing filename".to_string()))?;

        validate_id_segment(preset_id)?;

        let preset_dir = self.storage.presets_dir().join(preset_id);
        let target = self
            .storage
            .safe_resolve_for_write(&preset_dir, &format!("regex/{}", filename))?;

        if !target.exists() {
            return Err(AirpError::Validation(format!(
                "Script file not found: {}",
                filename
            )));
        }

        tokio::fs::remove_file(&target).await?;

        Ok(serde_json::json!({
            "preset_id": preset_id,
            "filename": filename,
            "removed": true,
        })
        .to_string())
    }

    pub async fn handle_set_preset_regex_enabled(&self, args: serde_json::Value) -> Result<String> {
        let preset_id = args["preset_id"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing preset_id".to_string()))?;
        let filename = args["filename"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing filename".to_string()))?;
        let enabled = args["enabled"].as_bool().unwrap_or(true);

        validate_id_segment(preset_id)?;

        let preset_dir = self.storage.presets_dir().join(preset_id);
        let target = self
            .storage
            .safe_resolve_for_write(&preset_dir, &format!("regex/{}", filename))?;

        if !target.exists() {
            return Err(AirpError::Validation(format!(
                "Script file not found: {}",
                filename
            )));
        }

        let raw = tokio::fs::read_to_string(&target).await?;
        let cleaned = crate::storage::strip_utf8_bom(&raw).to_owned();
        let new_disabled = !enabled;

        let updated: String = if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&cleaned)
        {
            if v.is_object() {
                v["disabled"] = serde_json::json!(new_disabled);
            } else if let Some(arr) = v.as_array_mut() {
                for item in arr.iter_mut() {
                    item["disabled"] = serde_json::json!(new_disabled);
                }
            }
            serde_json::to_string_pretty(&v)?
        } else {
            return Err(AirpError::Validation(format!(
                "Script file is not valid JSON: {}",
                filename
            )));
        };

        tokio::fs::write(&target, updated).await?;

        Ok(serde_json::json!({
            "preset_id": preset_id,
            "filename": filename,
            "enabled": enabled,
            "disabled": new_disabled,
        })
        .to_string())
    }

    // ── M_PLUGIN_DATA: zero-schema plugin data primitives ─────────────────
    // Any third-party plugin (any language) stores its own data under
    // data/plugins/{plugin_name}/ with no manifest / registration / schema.
    // AIRP never parses, validates, or indexes the data's semantics.

    pub async fn handle_plugin_kv_get(&self, args: Value) -> Result<String> {
        let plugin_name = args["plugin_name"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing plugin_name".into()))?;
        let key = args["key"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing key".into()))?;
        validate_id_segment(plugin_name)?;
        validate_id_segment(key)?;

        let path = self
            .storage
            .plugin_dir(plugin_name)
            .join(format!("{}.json", key));
        let (present, value) = if path.exists() {
            let size = tokio::fs::metadata(&path)
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            if size > crate::mcp::max_read_bytes() as u64 {
                return Err(AirpError::Validation(format!(
                    "KV value {}.json is {} bytes, exceeds single-read cap {} bytes; use plugin_blob_read or read from filesystem directly",
                    key,
                    size,
                    crate::mcp::max_read_bytes()
                )));
            }
            let raw = tokio::fs::read_to_string(&path).await?;
            let v: serde_json::Value = serde_json::from_str(strip_utf8_bom(&raw)).map_err(|e| {
                AirpError::Validation(format!("KV file {}.json is not valid JSON: {}", key, e))
            })?;
            (true, v)
        } else {
            (false, serde_json::Value::Null)
        };
        Ok(serde_json::json!({
            "plugin_name": plugin_name,
            "key": key,
            "present": present,
            "value": value,
        })
        .to_string())
    }

    pub async fn handle_plugin_kv_set(&self, args: Value) -> Result<String> {
        let plugin_name = args["plugin_name"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing plugin_name".into()))?;
        let key = args["key"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing key".into()))?;
        let value_json = args["value_json"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing value_json".into()))?;
        validate_id_segment(plugin_name)?;
        validate_id_segment(key)?;
        let _: serde_json::Value = serde_json::from_str(value_json)
            .map_err(|e| AirpError::Validation(format!("value_json is not valid JSON: {}", e)))?;

        let dir = self.storage.plugin_dir(plugin_name);
        tokio::fs::create_dir_all(&dir).await?;
        let path = dir.join(format!("{}.json", key));
        tokio::fs::write(&path, value_json.as_bytes()).await?;
        Ok(serde_json::json!({
            "plugin_name": plugin_name,
            "key": key,
            "bytes_written": value_json.len(),
        })
        .to_string())
    }

    pub async fn handle_plugin_jsonl_append(&self, args: Value) -> Result<String> {
        let plugin_name = args["plugin_name"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing plugin_name".into()))?;
        let file = args["file"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing file".into()))?;
        let line_json = args["line_json"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing line_json".into()))?;
        validate_id_segment(plugin_name)?;
        let parsed: serde_json::Value = serde_json::from_str(line_json)
            .map_err(|e| AirpError::Validation(format!("line_json is not valid JSON: {}", e)))?;
        let compact = serde_json::to_string(&parsed)?;

        let dir = self.storage.plugin_dir(plugin_name);
        tokio::fs::create_dir_all(&dir).await?;
        let target = self.storage.safe_resolve_for_write(&dir, file)?;
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        use tokio::io::AsyncWriteExt;
        let mut f = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&target)
            .await?;
        f.write_all(compact.as_bytes()).await?;
        f.write_all(b"\n").await?;
        Ok(serde_json::json!({
            "plugin_name": plugin_name,
            "file": file,
            "bytes_appended": compact.len() + 1,
        })
        .to_string())
    }

    pub async fn handle_plugin_jsonl_read(&self, args: Value) -> Result<String> {
        let plugin_name = args["plugin_name"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing plugin_name".into()))?;
        let file = args["file"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing file".into()))?;
        validate_id_segment(plugin_name)?;
        let offset = args["offset"].as_u64().unwrap_or(0) as usize;
        let limit = (args["limit"].as_u64().unwrap_or(100) as usize).clamp(1, 1000);

        let dir = self.storage.plugin_dir(plugin_name);
        let empty = |total: usize| {
            serde_json::json!({
                "plugin_name": plugin_name,
                "file": file,
                "total_lines": total,
                "offset": offset,
                "returned": 0,
                "lines": [],
            })
            .to_string()
        };
        if !dir.exists() {
            return Ok(empty(0));
        }
        let target = self.storage.safe_resolve_for_write(&dir, file)?;
        if !target.exists() {
            return Ok(empty(0));
        }
        let raw = tokio::fs::read_to_string(&target).await?;
        let all: Vec<&str> = raw.lines().filter(|l| !l.trim().is_empty()).collect();
        let total = all.len();
        // Cap by cumulative bytes too, not just line count: 1000 huge lines would
        // still blow the token budget. Stop before exceeding max_read_bytes().
        let mut byte_budget = crate::mcp::max_read_bytes();
        let mut truncated = false;
        let mut lines: Vec<serde_json::Value> = Vec::new();
        for l in all.into_iter().skip(offset).take(limit) {
            if l.len() > byte_budget {
                truncated = true;
                break;
            }
            byte_budget -= l.len();
            lines.push(
                serde_json::from_str(l).unwrap_or_else(|_| serde_json::Value::String(l.to_owned())),
            );
        }
        let returned = lines.len();
        Ok(serde_json::json!({
            "plugin_name": plugin_name,
            "file": file,
            "total_lines": total,
            "offset": offset,
            "returned": returned,
            "truncated": truncated,
            "lines": lines,
        })
        .to_string())
    }

    pub async fn handle_plugin_blob_write(&self, args: Value) -> Result<String> {
        let plugin_name = args["plugin_name"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing plugin_name".into()))?;
        let rel_path = args["rel_path"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing rel_path".into()))?;
        validate_id_segment(plugin_name)?;
        let content_base64 = args["content_base64"].as_str();
        let content_text = args["content_text"].as_str();
        let (bytes, encoding) = match (content_base64, content_text) {
            (Some(b64), None) => (base64_decode(b64.trim())?, "base64"),
            (None, Some(text)) => (text.as_bytes().to_vec(), "text"),
            _ => {
                return Err(AirpError::Validation(
                    "exactly one of content_base64 / content_text must be provided".into(),
                ));
            }
        };

        let dir = self.storage.plugin_dir(plugin_name);
        tokio::fs::create_dir_all(&dir).await?;
        let target = self.storage.safe_resolve_for_write(&dir, rel_path)?;
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&target, &bytes).await?;
        Ok(serde_json::json!({
            "plugin_name": plugin_name,
            "rel_path": rel_path,
            "bytes_written": bytes.len(),
            "encoding": encoding,
        })
        .to_string())
    }

    pub async fn handle_plugin_blob_read(&self, args: Value) -> Result<String> {
        let max_blob_read: u64 = crate::mcp::max_read_bytes() as u64;
        let plugin_name = args["plugin_name"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing plugin_name".into()))?;
        let rel_path = args["rel_path"]
            .as_str()
            .ok_or_else(|| AirpError::Validation("Missing rel_path".into()))?;
        validate_id_segment(plugin_name)?;
        // encoding: "auto" (default) | "text" | "base64". `as_text` kept for
        // back-compat (true -> text, false -> base64).
        let encoding = match args["as_text"].as_bool() {
            Some(true) => "text",
            Some(false) => "base64",
            None => args["encoding"].as_str().unwrap_or("auto"),
        };

        let dir = self.storage.plugin_dir(plugin_name);
        if !dir.exists() {
            return Err(AirpError::Validation(format!(
                "plugin `{}` does not exist",
                plugin_name
            )));
        }
        let target = self.storage.safe_resolve_for_write(&dir, rel_path)?;
        if !target.is_file() {
            return Err(AirpError::Validation(format!(
                "file not found: plugins/{}/{}",
                plugin_name, rel_path
            )));
        }
        let size = tokio::fs::metadata(&target)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        // Oversized → never dump; return a cheap descriptor instead of erroring
        // so the caller still learns the size/path.
        if size > max_blob_read {
            return Ok(serde_json::json!({
                "plugin_name": plugin_name,
                "rel_path": rel_path,
                "size": size,
                "returned": false,
                "encoding": "too_large",
                "note": format!(
                    "{} bytes exceeds single-read cap {} bytes; not returned. Read plugins/{}/{} from the filesystem, or page.",
                    size, max_blob_read, plugin_name, rel_path
                ),
            })
            .to_string());
        }

        let bytes = tokio::fs::read(&target).await?;
        match encoding {
            "text" => {
                let text = String::from_utf8(bytes).map_err(|_| {
                    AirpError::Validation(
                        "file is not valid UTF-8; use encoding=base64 for raw bytes".into(),
                    )
                })?;
                Ok(serde_json::json!({
                    "plugin_name": plugin_name, "rel_path": rel_path,
                    "size": text.len(), "returned": true, "encoding": "text",
                    "content_text": text,
                })
                .to_string())
            }
            "base64" => {
                use base64::Engine;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                Ok(serde_json::json!({
                    "plugin_name": plugin_name, "rel_path": rel_path,
                    "size": bytes.len(), "returned": true, "encoding": "base64",
                    "content_base64": b64,
                })
                .to_string())
            }
            _ => {
                // auto: text for UTF-8; for binary, a cheap descriptor — never
                // auto-dump base64 (meaningless gibberish that burns tokens).
                match String::from_utf8(bytes) {
                    Ok(text) => Ok(serde_json::json!({
                        "plugin_name": plugin_name, "rel_path": rel_path,
                        "size": text.len(), "returned": true, "encoding": "text",
                        "content_text": text,
                    })
                    .to_string()),
                    Err(e) => {
                        let raw = e.into_bytes();
                        let head_hex: String =
                            raw.iter().take(16).map(|b| format!("{:02x}", b)).collect();
                        Ok(serde_json::json!({
                            "plugin_name": plugin_name, "rel_path": rel_path,
                            "size": raw.len(), "returned": false, "encoding": "binary",
                            "head_hex": head_hex,
                            "note": "binary content not returned (base64 would waste tokens on non-text). Read from the filesystem, or call again with encoding=base64 to force.",
                        })
                        .to_string())
                    }
                }
            }
        }
    }
}

fn base64_decode(input: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| crate::error::AirpError::Validation(format!("Base64 decode error: {}", e)))
}

fn class_reason(has_state: bool, has_lorebook: bool) -> &'static str {
    match (has_state, has_lorebook) {
        (true, true) => "Card contains state tracking fields and lorebook entries",
        (true, false) => "Card contains state tracking fields (HP/MP/etc.)",
        (false, true) => "Card has lorebook/world building entries",
        (false, false) => "Basic character card without complex state or world rules",
    }
}

fn class_categories(lorebook: &Lorebook) -> String {
    if lorebook.entries.is_empty() {
        return "None".to_string();
    }
    let categories: Vec<&str> = lorebook
        .entries
        .iter()
        .filter(|e| e.enabled)
        .filter_map(|e| e.name.as_deref())
        .take(5)
        .collect();
    if categories.is_empty() {
        "Uncategorized".to_string()
    } else {
        categories.join(", ")
    }
}

fn detect_tone(text: &str) -> &'static str {
    if text.is_empty() {
        return "Neutral";
    }
    let lower = text.to_lowercase();
    if lower.contains("!") || lower.contains("?!") {
        "Energetic/Excited"
    } else if lower.contains("...") || lower.contains("~") {
        "Gentle/Soft"
    } else if lower.contains("?") {
        "Inquisitive"
    } else {
        "Neutral/Calm"
    }
}

fn detect_style(text: &str) -> &'static str {
    if text.is_empty() {
        return "Plain";
    }
    let lower = text.to_lowercase();
    if lower.contains("*") || lower.contains("_") {
        "Descriptive/Action-heavy"
    } else if text.len() > 200 {
        "Detailed/Elaborate"
    } else {
        "Concise/Direct"
    }
}

fn detect_voice(text: &str) -> String {
    if text.is_empty() {
        return "```\n<!-- Agent: No example messages available for voice analysis -->\n```"
            .to_string();
    }
    let sample: String = text.chars().take(300).collect();
    format!(
        "```\n{}\n```\n<!-- Agent: Analyze speaking patterns from the above examples -->",
        sample
    )
}
