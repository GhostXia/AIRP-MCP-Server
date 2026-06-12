//! Preset models

use serde::{Deserialize, Serialize};

/// AI configuration preset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub id: super::PresetId,
    pub name: String,
    pub config: PresetConfig,
}

/// Preset configuration — Agent-reference LLM parameters.
///
/// AIRP stores these for the MCP Client (Agent) to read.
/// **AIRP itself never calls any AI LLM API.**
/// It is the Agent's responsibility to apply these values
/// when sending requests to its own LLM backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetConfig {
    pub system_prompt_prefix: String,
    pub system_prompt_suffix: String,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: i32,
    pub repetition_penalty: f32,
    pub max_tokens: i32,
    pub stop_sequences: Vec<String>,
    pub regex_scripts: Vec<RegexScript>,
}

impl Default for PresetConfig {
    fn default() -> Self {
        Self {
            system_prompt_prefix: String::new(),
            system_prompt_suffix: String::new(),
            temperature: 0.7,
            top_p: 0.9,
            top_k: 40,
            repetition_penalty: 1.0,
            max_tokens: 2048,
            stop_sequences: vec![],
            regex_scripts: vec![],
        }
    }
}

/// Regex script for text filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegexScript {
    pub id: String,
    pub name: String,
    pub find: String,
    pub replace: String,
    pub enabled: bool,
}

impl Preset {
    /// Build complete system prompt with prefix/suffix
    pub fn build_system_prompt(&self, character_prompt: &str) -> String {
        let mut parts = vec![];

        if !self.config.system_prompt_prefix.is_empty() {
            parts.push(self.config.system_prompt_prefix.clone());
        }

        parts.push(character_prompt.to_string());

        if !self.config.system_prompt_suffix.is_empty() {
            parts.push(self.config.system_prompt_suffix.clone());
        }

        parts.join("\n\n")
    }

    /// Apply regex scripts to text
    pub fn apply_filters(&self, text: &str) -> String {
        let mut result = text.to_string();

        for script in &self.config.regex_scripts {
            if script.enabled {
                if let Ok(re) = regex::Regex::new(&script.find) {
                    result = re.replace_all(&result, &script.replace).to_string();
                }
            }
        }

        result
    }
}
