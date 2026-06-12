//! Character card models

use serde::{Deserialize, Serialize};

/// Complete character information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub id: super::CharacterId,
    pub data: CharacterData,
    pub card: CharacterCard,
}

/// Character card data (PNG chara chunk format).
///
/// SillyTavern V2 cards use snake_case keys (first_mes, mes_example,
/// character_version), so the field names are used as-is — NO rename_all.
/// (A previous `rename_all = "camelCase"` made real-card and test import fail
/// with "missing field firstMes".)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CharacterCard {
    pub name: String,
    pub description: String,
    pub personality: String,
    pub scenario: String,
    pub first_mes: String,
    pub mes_example: String,
    pub creatorcomment: Option<String>,
    pub tags: Vec<String>,
    pub creator: Option<String>,
    pub character_version: Option<String>,
    pub extensions: Option<serde_json::Value>,
}

/// Internal character data managed by AIRP
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CharacterData {
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub import_source: Option<String>,
    pub analysis_tier: Option<AnalysisTier>,
    pub has_state_tracking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisTier {
    Tier0Basic,
    Tier1Greeting,
    Tier2Lorebook,
    Tier3Advanced,
}

impl CharacterCard {
    /// Extract system prompt components
    pub fn build_system_prompt(&self) -> String {
        let mut parts = vec![];

        if !self.description.is_empty() {
            parts.push(format!("[Character Description]\n{}", self.description));
        }

        if !self.personality.is_empty() {
            parts.push(format!("[Personality]\n{}", self.personality));
        }

        if !self.scenario.is_empty() {
            parts.push(format!("[Scenario]\n{}", self.scenario));
        }

        if !self.mes_example.is_empty() {
            parts.push(format!("[Example Messages]\n{}", self.mes_example));
        }

        parts.join("\n\n")
    }

    /// Get first greeting message
    pub fn get_greeting(&self) -> String {
        self.first_mes.clone()
    }
}
