//! MCP Resource handlers

use crate::error::Result;
use crate::models::*;
use crate::storage::*;
use super::AirpMcpServer;

impl AirpMcpServer {
    pub async fn dispatch_resource(&self, uri: &str) -> Result<String> {
        // airp://scenes (list)
        if uri == "airp://scenes" {
            return self.read_scenes_list().await;
        }

        if let Some(rest) = uri.strip_prefix("airp://scenes/") {
            return self.read_scene(rest).await;
        }

        // airp://characters (list)
        if uri == "airp://characters" {
            return self.read_characters_list().await;
        }
        
        if let Some(rest) = uri.strip_prefix("airp://characters/") {
            let parts: Vec<&str> = rest.split('/').collect();
            if parts.len() >= 2 {
                let character_id = parts[0];
                let resource_type = parts[1];
                
                return match resource_type {
                    "card" => self.read_character_card(character_id).await,
                    "greetings" => self.read_character_greetings(character_id).await,
                    "world" if parts.len() >= 3 && parts[2] == "lorebook" => {
                        self.read_character_lorebook(character_id).await
                    }
                    "state" if parts.len() >= 3 && parts[2] == "live" => {
                        self.read_character_live_state(character_id).await
                    }
                    "memory" if parts.len() >= 3 => {
                        match parts[2] {
                            "current" => self.read_character_memory(character_id).await,
                            "index" => self.read_memory_index(character_id).await,
                            "volumes" if parts.len() >= 4 => {
                                self.read_volume(character_id, parts[3]).await
                            }
                            _ => Err(crate::error::AirpError::Validation(
                                format!("Unknown memory resource: {}", uri)
                            )),
                        }
                    }
                    _ => Err(crate::error::AirpError::Validation(
                        format!("Unknown resource: {}", uri)
                    )),
                };
            }
        }
        
        // airp://presets (list)
        if uri == "airp://presets" {
            return self.read_presets_list().await;
        }

        if let Some(rest) = uri.strip_prefix("airp://presets/") {
            let mut split = rest.splitn(2, '/');
            let pid = split.next().unwrap_or("");
            let sub = split.next().unwrap_or("");

            match sub {
                "" => {
                    return self.read_preset(pid).await;
                }
                "raw" => {
                    return self.read_preset_raw(pid).await;
                }
                "artifacts" => {
                    return self.read_preset_artifacts(pid).await;
                }
                "regex" => {
                    return self.read_preset_regex(pid).await;
                }
                _ => {}
            }
        }

        if let Some(rest) = uri.strip_prefix("airp://gating/") {
            let parts: Vec<&str> = rest.split('/').collect();
            if parts.len() >= 2 && parts[1] == "checkpoints" {
                return self.read_gating_checkpoints(parts[0]).await;
            }
        }

        // M_PLUGIN_DATA: airp://plugins (list)
        if uri == "airp://plugins" {
            return self.read_plugins_list().await;
        }

        if let Some(rest) = uri.strip_prefix("airp://plugins/") {
            let mut split = rest.splitn(2, '/');
            let pname = split.next().unwrap_or("");
            let sub = split.next().unwrap_or("");
            crate::storage::validate_id_segment(pname)?;
            if sub == "files" {
                return self.read_plugin_files(pname).await;
            }
            if let Some(rel) = sub.strip_prefix("data/") {
                return self.read_plugin_data(pname, rel).await;
            }
            return Err(crate::error::AirpError::Validation(
                format!("Unknown plugin sub-resource: {}", uri)
            ));
        }

        Err(crate::error::AirpError::Validation(
            format!("Invalid resource URI: {}", uri)
        ))
    }

    // ── M_PLUGIN_DATA resource readers ────────────────────────────────────

    async fn read_plugins_list(&self) -> Result<String> {
        let plugins_dir = self.storage.plugins_dir();
        let mut names: Vec<String> = Vec::new();
        if plugins_dir.exists() {
            let mut entries = tokio::fs::read_dir(&plugins_dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                if entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                    names.push(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
        names.sort();
        serde_json::to_string_pretty(&names).map_err(Into::into)
    }

    async fn read_plugin_files(&self, plugin_name: &str) -> Result<String> {
        crate::storage::validate_id_segment(plugin_name)?;
        let dir = self.storage.plugin_dir(plugin_name);
        let mut files: Vec<String> = Vec::new();
        if dir.exists() {
            let mut stack = vec![dir.clone()];
            while let Some(cur) = stack.pop() {
                let mut entries = tokio::fs::read_dir(&cur).await?;
                while let Some(entry) = entries.next_entry().await? {
                    let path = entry.path();
                    let ft = entry.file_type().await?;
                    if ft.is_dir() {
                        stack.push(path);
                    } else if ft.is_file() {
                        if let Ok(rel) = path.strip_prefix(&dir) {
                            files.push(rel.to_string_lossy().replace('\\', "/"));
                        }
                    }
                }
            }
        }
        files.sort();
        serde_json::to_string_pretty(&files).map_err(Into::into)
    }

    async fn read_plugin_data(&self, plugin_name: &str, rel_path: &str) -> Result<String> {
        crate::storage::validate_id_segment(plugin_name)?;
        let dir = self.storage.plugin_dir(plugin_name);
        if !dir.exists() {
            return Err(crate::error::AirpError::Validation(
                format!("plugin `{}` does not exist", plugin_name)));
        }
        let target = self.storage.safe_resolve_for_write(&dir, rel_path)?;
        if !target.is_file() {
            return Err(crate::error::AirpError::Validation(
                format!("file not found: plugins/{}/{}", plugin_name, rel_path)));
        }
        let content = tokio::fs::read_to_string(&target).await.map_err(|_| {
            crate::error::AirpError::Validation(
                "file is not valid UTF-8; use plugin_blob_read tool for binary".into())
        })?;
        // Cap content returned into the model context. Mirrors read_preset_raw:
        // truncate oversized files with a [PARTIAL: ...] marker instead of
        // dumping the whole file and burning the token budget.
        let max_len = crate::mcp::MAX_READ_BYTES;
        if content.len() > max_len {
            let mut end = max_len;
            while end > 0 && !content.is_char_boundary(end) {
                end -= 1;
            }
            Ok(format!(
                "[PARTIAL: total={}, offset=0, limit={} — file exceeds single-read cap; use plugin_blob_read or read from filesystem directly for full content]\n{}",
                content.len(), end, &content[..end]))
        } else {
            Ok(content)
        }
    }
    
    async fn read_characters_list(&self) -> Result<String> {
        let store = CharacterStore::new(&self.storage);
        let characters = store.list().await?;
        
        let list: Vec<serde_json::Value> = characters.iter()
            .map(|c| serde_json::json!({
                "id": c.id.as_ref(),
                "name": c.card.name,
                "description": c.card.description.chars().take(100).collect::<String>(),
            }))
            .collect();
        
        serde_json::to_string_pretty(&list).map_err(Into::into)
    }
    
    async fn read_character_card(&self, character_id: &str) -> Result<String> {
        let id = CharacterId::new(character_id)?;
        let store = CharacterStore::new(&self.storage);
        let character = store.get(&id).await?;
        
        serde_json::to_string_pretty(&character.card).map_err(Into::into)
    }
    
    async fn read_character_greetings(&self, character_id: &str) -> Result<String> {
        let id = CharacterId::new(character_id)?;
        let store = CharacterStore::new(&self.storage);
        let character = store.get(&id).await?;
        
        let greetings = serde_json::json!({
            "first_mes": character.card.first_mes,
            "alternate_greetings": character.card.extensions
                .as_ref()
                .and_then(|e| e.get("alternate_greetings"))
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![])),
        });
        
        serde_json::to_string_pretty(&greetings).map_err(Into::into)
    }
    
    async fn read_character_lorebook(&self, character_id: &str) -> Result<String> {
        let id = CharacterId::new(character_id)?;
        let store = CharacterStore::new(&self.storage);
        let lorebook = store.get_lorebook(&id).await?;
        
        serde_json::to_string_pretty(&lorebook).map_err(Into::into)
    }
    
    async fn read_character_live_state(&self, character_id: &str) -> Result<String> {
        let id = CharacterId::new(character_id)?;
        let store = CharacterStore::new(&self.storage);
        let state = store.get_live_state(&id).await?;
        
        serde_json::to_string_pretty(&state).map_err(Into::into)
    }
    
    async fn read_character_memory(&self, character_id: &str) -> Result<String> {
        let id = CharacterId::new(character_id)?;
        let _store = CharacterStore::new(&self.storage);

        // Get all sessions and recent context
        let sessions = SessionStore::new(&self.storage).list(&id).await?;
        
        let memory = serde_json::json!({
            "character_id": character_id,
            "session_count": sessions.len(),
            "note": "Use get_recent_context tool to retrieve actual conversation history",
        });
        
        serde_json::to_string_pretty(&memory).map_err(Into::into)
    }
    
    async fn read_preset(&self, preset_id: &str) -> Result<String> {
        let id = PresetId::new(preset_id)?;
        let store = PresetStore::new(&self.storage);
        let preset = store.get(&id).await?;
        
        serde_json::to_string_pretty(&preset).map_err(Into::into)
    }
    
    async fn read_memory_index(&self, character_id: &str) -> Result<String> {
        let id = CharacterId::new(character_id)?;
        let char_dir = self.storage.character_dir(&id);
        let index_path = char_dir.join("memory").join("index.md");
        
        if !index_path.exists() {
            return Ok("# Memory Index\n\nNo volumes archived yet.".to_string());
        }
        
        let content = tokio::fs::read_to_string(&index_path).await?;
        Ok(content)
    }
    
    async fn read_volume(&self, character_id: &str, volume_id: &str) -> Result<String> {
        let id = CharacterId::new(character_id)?;
        let char_dir = self.storage.character_dir(&id);
        
        // volume_id can be "latest" or a specific filename
        let volume_path = if volume_id == "latest" {
            let volumes_dir = char_dir.join("memory").join("volumes");
            if !volumes_dir.exists() {
                return Err(crate::error::AirpError::Validation(
                    "No volumes found".to_string()
                ));
            }
            
            // Find the latest volume by filename (vol_YYYYMMDD_HHMMSS.md)
            let mut entries = tokio::fs::read_dir(&volumes_dir).await?;
            let mut latest: Option<std::path::PathBuf> = None;
            
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "md") {
                    if latest.is_none() || path > latest.clone().unwrap() {
                        latest = Some(path);
                    }
                }
            }
            
            latest.ok_or_else(|| crate::error::AirpError::Validation(
                "No volumes found".to_string()
            ))?
        } else {
            char_dir.join("memory").join("volumes").join(volume_id)
        };
        
        if !volume_path.exists() {
            return Err(crate::error::AirpError::Validation(
                format!("Volume not found: {}", volume_id)
            ));
        }
        
        let content = tokio::fs::read_to_string(&volume_path).await?;
        Ok(content)
    }

    async fn read_gating_checkpoints(&self, character_id: &str) -> Result<String> {
        let id = CharacterId::new(character_id)?;
        let char_dir = self.storage.character_dir(&id);
        let gating_path = char_dir.join("gating").join("checkpoints.json");

        if !gating_path.exists() {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "character_id": character_id,
                "status": "no_gating_configured",
                "checkpoints": []
            }))?);
        }

        let content = tokio::fs::read_to_string(&gating_path).await?;
        Ok(content)
    }

    // ── Scene resources ─────────────────────────────────────────────────

    async fn read_scenes_list(&self) -> Result<String> {
        let list = self.storage.list_scenes().await?;
        serde_json::to_string(&list).map_err(Into::into)
    }

    async fn read_scene(&self, scene_id: &str) -> Result<String> {
        let config = self.storage.load_scene(scene_id).await?;
        serde_json::to_string_pretty(&config).map_err(Into::into)
    }

    // ── Preset resources ────────────────────────────────────────────────

    async fn read_presets_list(&self) -> Result<String> {
        let list = self.storage.list_presets().await?;
        serde_json::to_string(&list).map_err(Into::into)
    }

    async fn read_preset_raw(&self, preset_id: &str) -> Result<String> {
        crate::storage::validate_id_segment(preset_id)?;

        let path = self.storage.preset_json_path(preset_id);
        if !path.exists() {
            return Err(crate::error::AirpError::PresetNotFound(preset_id.to_string()));
        }

        let raw = tokio::fs::read_to_string(&path).await?;
        let cleaned = crate::storage::strip_utf8_bom(&raw);

        let max_len = 100_000;
        if cleaned.len() > max_len {
            let truncated = &cleaned[..max_len];
            Ok(format!(
                "[PARTIAL: total={}, offset=0, limit={} — increase limit query param for more]\n{}",
                cleaned.len(),
                max_len,
                truncated
            ))
        } else {
            Ok(cleaned.to_string())
        }
    }

    async fn read_preset_artifacts(&self, preset_id: &str) -> Result<String> {
        crate::storage::validate_id_segment(preset_id)?;

        let preset_dir = self.storage.presets_dir().join(preset_id);
        if !preset_dir.exists() {
            return Ok("[]".to_string());
        }

        let mut files = vec![];
        let mut stack = vec![preset_dir.clone()];

        while let Some(dir) = stack.pop() {
            let mut entries = tokio::fs::read_dir(&dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let rel = path.strip_prefix(&preset_dir).unwrap_or(&path);
                    if rel.file_name().and_then(|n| n.to_str()) != Some("preset.json") {
                        files.push(rel.to_string_lossy().replace('\\', "/"));
                    }
                }
            }
        }

        files.sort();
        serde_json::to_string(&files).map_err(Into::into)
    }

    async fn read_preset_regex(&self, preset_id: &str) -> Result<String> {
        crate::storage::validate_id_segment(preset_id)?;

        let regex_dir = self.storage.preset_regex_dir(preset_id);
        if !regex_dir.exists() {
            return Ok("[]".to_string());
        }

        let mut scripts: Vec<serde_json::Value> = Vec::new();
        let mut entries = tokio::fs::read_dir(&regex_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
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

        serde_json::to_string(&scripts).map_err(Into::into)
    }
}
