//! Storage layer for AIRP data

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{info, warn};

use crate::error::{AirpError, Result};
use crate::models::*;

pub mod character_store;
pub mod session_store;
pub mod preset_store;

pub use character_store::CharacterStore;
pub use session_store::SessionStore;
pub use preset_store::PresetStore;

/// Data storage manager
#[derive(Debug, Clone)]
pub struct Storage {
    data_root: PathBuf,
}

impl Storage {
    pub fn new(data_root: impl AsRef<Path>) -> Result<Self> {
        let data_root = data_root.as_ref().to_path_buf();
        Ok(Self { data_root })
    }

    pub async fn init(&self) -> Result<()> {
        let dirs = [
            self.characters_dir(),
            self.presets_dir(),
            self.scenes_dir(),
        ];

        for dir in &dirs {
            fs::create_dir_all(dir).await?;
        }

        if let Err(e) = self.migrate_legacy_presets().await {
            warn!(err = %e, "M_PR: preset migration partially failed");
        }

        info!("Storage initialized at {:?}", self.data_root);
        Ok(())
    }

    pub fn characters_dir(&self) -> PathBuf {
        self.data_root.join("characters")
    }

    pub fn character_dir(&self, id: &CharacterId) -> PathBuf {
        self.characters_dir().join(&id.0)
    }

    pub fn presets_dir(&self) -> PathBuf {
        self.data_root.join("presets")
    }

    pub fn scenes_dir(&self) -> PathBuf {
        self.data_root.join("scenes")
    }

    pub fn scene_dir(&self, scene_id: &str) -> PathBuf {
        self.scenes_dir().join(scene_id)
    }

    pub fn scene_json_path(&self, scene_id: &str) -> PathBuf {
        self.scene_dir(scene_id).join("scene.json")
    }

    pub async fn list_scenes(&self) -> Result<Vec<String>> {
        let scenes_dir = self.scenes_dir();
        if !scenes_dir.exists() {
            return Ok(vec![]);
        }
        let mut result = Vec::new();
        let mut entries = fs::read_dir(&scenes_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await.map(|ft| ft.is_dir()).unwrap_or(false) {
                let p = entry.path();
                if p.join("scene.json").exists() {
                    if let Some(name) = entry.file_name().to_str() {
                        result.push(name.to_string());
                    }
                }
            }
        }
        result.sort();
        Ok(result)
    }

    pub async fn load_scene(&self, scene_id: &str) -> Result<SceneConfig> {
        let path = self.scene_json_path(scene_id);
        let json = fs::read_to_string(&path).await?;
        Ok(serde_json::from_str(&json)?)
    }

    pub async fn save_scene(&self, config: &SceneConfig) -> Result<()> {
        let scene_dir = self.scene_dir(&config.scene_id);
        fs::create_dir_all(&scene_dir).await?;
        let path = scene_dir.join("scene.json");
        let json = serde_json::to_string_pretty(config)?;
        fs::write(path, json).await?;
        Ok(())
    }

    pub fn preset_json_path(&self, preset_id: &str) -> PathBuf {
        self.data_root.join("presets").join(preset_id).join("preset.json")
    }

    pub fn preset_regex_dir(&self, preset_id: &str) -> PathBuf {
        self.data_root.join("presets").join(preset_id).join("regex")
    }

    pub async fn list_presets(&self) -> Result<Vec<String>> {
        let presets_dir = self.presets_dir();
        if !presets_dir.exists() {
            return Ok(vec![]);
        }

        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut entries = fs::read_dir(&presets_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let ft = entry.file_type().await?;
            let name = entry.file_name().to_string_lossy().into_owned();

            if ft.is_dir() {
                let p = entry.path();
                if p.join("preset.json").exists() || p.join("preset.md").exists() {
                    seen.insert(name);
                }
            } else if ft.is_file() {
                if let Some(stem) = name.strip_suffix(".json").or_else(|| name.strip_suffix(".md")) {
                    seen.insert(stem.to_string());
                }
            }
        }

        Ok(seen.into_iter().collect())
    }

    pub async fn migrate_legacy_presets(&self) -> Result<()> {
        let presets = self.presets_dir();
        if !presets.exists() {
            return Ok(());
        }

        let mut flat_files: Vec<PathBuf> = Vec::new();
        let mut entries = fs::read_dir(&presets).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                flat_files.push(path);
            }
        }

        for path in flat_files {
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            let target_name = match ext {
                "json" => "preset.json",
                "md" => "preset.md",
                _ => continue,
            };
            let new_dir = presets.join(&stem);
            if let Err(e) = fs::create_dir_all(&new_dir).await {
                warn!(path = ?new_dir, err = %e, "M_PR: create preset dir failed");
                continue;
            }
            let new_path = new_dir.join(target_name);
            if new_path.exists() {
                continue;
            }
            if let Err(e) = fs::rename(&path, &new_path).await {
                warn!(old = ?path, new = ?new_path, err = %e, "M_PR: migrate preset failed");
                continue;
            }
            info!(new = ?new_path, "M_PR: migrated flat preset to dir");
        }
        Ok(())
    }

    pub fn safe_resolve(&self, path: &str) -> Result<PathBuf> {
        let resolved = self.data_root.join(path);
        let canonical = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());
        let root_canonical = self.data_root.canonicalize().unwrap_or_else(|_| self.data_root.clone());

        if !canonical.starts_with(&root_canonical) {
            return Err(AirpError::Validation(format!("Path traversal attempt: {:?}", path)));
        }

        Ok(canonical)
    }

    pub fn safe_resolve_for_write(&self, base_dir: &Path, user_path: &str) -> Result<PathBuf> {
        let trimmed = user_path.trim();
        if trimmed.is_empty() {
            return Err(AirpError::Validation("path is empty".into()));
        }

        if trimmed.starts_with('/') || trimmed.starts_with('\\')
            || (trimmed.len() >= 2 && trimmed.as_bytes()[1] == b':')
        {
            return Err(AirpError::Validation(format!("absolute path rejected: {}", user_path)));
        }

        if trimmed.contains('\0') {
            return Err(AirpError::Validation("path contains null byte".into()));
        }

        let canon_base = base_dir.canonicalize().unwrap_or_else(|_| base_dir.to_path_buf());
        let mut stack: Vec<std::ffi::OsString> = Vec::new();
        for comp in Path::new(trimmed).components() {
            match comp {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    if stack.pop().is_none() {
                        return Err(AirpError::Validation(format!("path escape attempt: {}", user_path)));
                    }
                }
                std::path::Component::Normal(s) => stack.push(s.to_owned()),
                _ => return Err(AirpError::Validation(format!("illegal path component: {}", user_path))),
            }
        }

        if stack.is_empty() {
            return Err(AirpError::Validation("resolved path is empty".into()));
        }

        let resolved = stack.iter().fold(canon_base.clone(), |acc, c| acc.join(c));
        if !resolved.starts_with(&canon_base) {
            return Err(AirpError::Validation(format!("path escape attempt: {}", user_path)));
        }

        Ok(resolved)
    }
}

pub fn validate_id_segment(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(AirpError::InvalidId("ID is empty".into()));
    }
    if id == "." || id == ".." {
        return Err(AirpError::InvalidId(format!("illegal ID: {}", id)));
    }
    if id.starts_with('.') {
        return Err(AirpError::InvalidId(format!("ID must not start with dot: {}", id)));
    }
    for c in id.chars() {
        match c {
            '/' | '\\' | '\0' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => {
                return Err(AirpError::InvalidId(format!("ID contains illegal char {:?}: {}", c, id)));
            }
            _ => {}
        }
    }
    if id.contains("..") {
        return Err(AirpError::InvalidId(format!("ID contains ..: {}", id)));
    }
    Ok(())
}

pub fn strip_utf8_bom(s: &str) -> &str {
    s.strip_prefix('\u{FEFF}').unwrap_or(s)
}
