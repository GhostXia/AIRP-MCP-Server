//! Preset storage operations

use tokio::fs;
use tracing::info;

use crate::error::{AirpError, Result};
use crate::models::*;
use super::Storage;

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
        let preset: Preset = serde_json::from_str(&json)?;
        Ok(preset)
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
            if path.extension().map_or(false, |e| e == "json") {
                if let Ok(json) = fs::read_to_string(&path).await {
                    if let Ok(preset) = serde_json::from_str::<Preset>(&json) {
                        presets.push(preset);
                    }
                }
            }
        }
        
        Ok(presets)
    }
    
    /// Delete preset
    pub async fn delete(&self, id: &PresetId) -> Result<()> {
        let preset_path = self.preset_path(id);
        
        if !preset_path.exists() {
            return Err(AirpError::PresetNotFound(id.as_ref().to_string()));
        }
        
        fs::remove_file(&preset_path).await?;
        info!("Deleted preset: {}", id.as_ref());
        
        Ok(())
    }
    
    fn preset_path(&self, id: &PresetId) -> std::path::PathBuf {
        self.storage.presets_dir()
            .join(format!("{}.json", id.as_ref()))
    }
}
