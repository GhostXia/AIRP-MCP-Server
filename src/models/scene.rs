//! M_MS: Multi-character Scene models

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CharacterRole {
    Primary,
    #[default]
    Npc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterEntry {
    pub character_id: String,
    #[serde(default)]
    pub role: CharacterRole,
    #[serde(default)]
    pub intro: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LorebookMerge {
    #[default]
    Union,
    PrimaryOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneConfig {
    pub scene_id: String,
    #[serde(default)]
    pub description: String,
    pub characters: Vec<CharacterEntry>,
    #[serde(default)]
    pub narrator_style: String,
    #[serde(default)]
    pub lorebook_merge: LorebookMerge,
    #[serde(default)]
    pub format_hint: String,
}

impl SceneConfig {
    pub fn primary(&self) -> Option<&CharacterEntry> {
        self.characters
            .iter()
            .find(|c| c.role == CharacterRole::Primary)
    }
}
