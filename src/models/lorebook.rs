//! Lorebook models

use serde::{Deserialize, Serialize};

/// Lorebook (world knowledge)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Lorebook {
    pub entries: Vec<LorebookEntry>,
}

/// Single lorebook entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LorebookEntry {
    pub id: String,
    pub keys: Vec<String>,
    pub content: String,
    pub enabled: bool,
    pub insertion_order: i32,
    pub case_sensitive: bool,
    pub name: Option<String>,
    pub comment: Option<String>,
}

impl Lorebook {
    /// Find entries matching the given text
    pub fn find_matches(&self, text: &str) -> Vec<&LorebookEntry> {
        self.entries
            .iter()
            .filter(|e| e.enabled && e.matches(text))
            .collect()
    }

    /// Build context string from matched entries
    pub fn build_context(&self, text: &str) -> String {
        let matches = self.find_matches(text);
        if matches.is_empty() {
            return String::new();
        }

        let mut parts = vec!["[World Information]".to_string()];
        for entry in matches {
            parts.push(format!(
                "- {}: {}",
                entry.name.as_deref().unwrap_or(&entry.id),
                entry.content
            ));
        }
        parts.join("\n")
    }
}

impl LorebookEntry {
    /// Check if this entry matches the given text
    pub fn matches(&self, text: &str) -> bool {
        let text_to_check = if self.case_sensitive {
            text.to_string()
        } else {
            text.to_lowercase()
        };

        self.keys.iter().any(|key| {
            let key_to_check = if self.case_sensitive {
                key.clone()
            } else {
                key.to_lowercase()
            };
            text_to_check.contains(&key_to_check)
        })
    }
}
