//! Preset storage operations

use tokio::fs;
use tracing::info;

use super::Storage;
use crate::error::{AirpError, Result};
use crate::models::*;
use crate::models::preset::RegexScript;

/// Preset storage operations
pub struct PresetStore<'a> {
    storage: &'a Storage,
}

impl<'a> PresetStore<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    /// Create or update preset
    pub async fn save(&self, preset: &Preset) -> Result<()> {
        let preset_path = self.preset_path(&preset.id);
        fs::create_dir_all(preset_path.parent().unwrap()).await?;

        let json = serde_json::to_string_pretty(preset)?;
        fs::write(&preset_path, json).await?;

        info!("Saved preset: {} ({})", preset.name, preset.id.as_ref());
        Ok(())
    }

    /// Get preset by ID
    pub async fn get(&self, id: &PresetId) -> Result<Preset> {
        let preset_path = self.preset_path(id);

        if !preset_path.exists() {
            return Err(AirpError::PresetNotFound(id.as_ref().to_string()));
        }

        let json = fs::read_to_string(&preset_path).await?;
        // preset.json stores the raw SillyTavern JSON (see handle_import_preset).
        // Parse it and extract the subset that maps to AIRP's Preset struct.
        let raw: serde_json::Value = serde_json::from_str(&json)?;

        let name = raw.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let config = PresetConfig {
            system_prompt_prefix: String::new(),
            system_prompt_suffix: String::new(),
            temperature: raw.get("temperature").and_then(|v| v.as_f64()).unwrap_or(0.7) as f32,
            top_p: raw.get("top_p").and_then(|v| v.as_f64()).unwrap_or(0.9) as f32,
            top_k: raw.get("top_k").and_then(|v| v.as_i64()).unwrap_or(40) as i32,
            repetition_penalty: raw.get("repetition_penalty").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
            max_tokens: raw.get("openai_max_tokens")
                .or_else(|| raw.get("max_tokens"))
                .and_then(|v| v.as_i64())
                .unwrap_or(2048) as i32,
            stop_sequences: raw.get("stop_sequences")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()
                })
                .unwrap_or_default(),
            regex_scripts: raw
                .get("extensions")
                .and_then(|ext| {
                    ext.get("SPreset")
                        .and_then(|s| s.get("RegexBinding"))
                        .or_else(|| ext.get("RegexBinding"))
                })
                .and_then(|rb| rb.get("regexes"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter().filter_map(|r| {
                        Some(RegexScript {
                            id: r.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            name: r.get("scriptName").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            find: r.get("findRegex").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            replace: r.get("replaceString").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            enabled: !r.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false),
                        })
                    }).collect()
                })
                .unwrap_or_default(),
        };

        Ok(Preset {
            id: id.clone(),
            name,
            config,
        })
    }

    /// List all presets
    pub async fn list(&self) -> Result<Vec<Preset>> {
        let presets_dir = self.storage.presets_dir();

        if !presets_dir.exists() {
            return Ok(vec![]);
        }

        let mut entries = fs::read_dir(&presets_dir).await?;
        let mut presets = vec![];

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                let preset_json = path.join("preset.json");
                if preset_json.exists() {
                    if let Ok(json) = fs::read_to_string(&preset_json).await {
                        // Parse raw SillyTavern JSON; skip entries that don't parse.
                        if let Ok(raw) = serde_json::from_str::<serde_json::Value>(&json) {
                            if let Some(id_str) = path.file_name().and_then(|n| n.to_str()) {
                                if let Ok(id) = PresetId::new(id_str) {
                                    let name = raw.get("name")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or(id_str)
                                        .to_string();
                                    let config = PresetConfig {
                                        system_prompt_prefix: String::new(),
                                        system_prompt_suffix: String::new(),
                                        temperature: raw.get("temperature").and_then(|v| v.as_f64()).unwrap_or(0.7) as f32,
                                        top_p: raw.get("top_p").and_then(|v| v.as_f64()).unwrap_or(0.9) as f32,
                                        top_k: raw.get("top_k").and_then(|v| v.as_i64()).unwrap_or(40) as i32,
                                        repetition_penalty: raw.get("repetition_penalty").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                                        max_tokens: raw.get("openai_max_tokens")
                                            .or_else(|| raw.get("max_tokens"))
                                            .and_then(|v| v.as_i64())
                                            .unwrap_or(2048) as i32,
                                        stop_sequences: raw.get("stop_sequences")
                                            .and_then(|v| v.as_array())
                                            .map(|arr| {
                                                arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()
                                            })
                                            .unwrap_or_default(),
                                        regex_scripts: raw
                                            .get("extensions")
                                            .and_then(|ext| {
                                                ext.get("SPreset")
                                                    .and_then(|s| s.get("RegexBinding"))
                                                    .or_else(|| ext.get("RegexBinding"))
                                            })
                                            .and_then(|rb| rb.get("regexes"))
                                            .and_then(|v| v.as_array())
                                            .map(|arr| {
                                                arr.iter().filter_map(|r| {
                                                    Some(RegexScript {
                                                        id: r.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                                        name: r.get("scriptName").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                                        find: r.get("findRegex").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                                        replace: r.get("replaceString").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                                        enabled: !r.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false),
                                                    })
                                                }).collect()
                                            })
                                            .unwrap_or_default(),
                                    };
                                    presets.push(Preset { id, name, config });
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(presets)
    }

    /// Delete preset (removes the directory tree)
    pub async fn delete(&self, id: &PresetId) -> Result<()> {
        let preset_dir = self
            .storage
            .presets_dir()
            .join(id.as_ref());

        if !preset_dir.exists() {
            return Err(AirpError::PresetNotFound(id.as_ref().to_string()));
        }

        fs::remove_dir_all(&preset_dir).await?;
        info!("Deleted preset: {}", id.as_ref());

        Ok(())
    }

    fn preset_path(&self, id: &PresetId) -> std::path::PathBuf {
        self.storage
            .presets_dir()
            .join(id.as_ref())
            .join("preset.json")
    }
}
