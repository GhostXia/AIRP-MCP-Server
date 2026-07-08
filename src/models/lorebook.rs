//! Lorebook models

use serde::{Deserialize, Serialize};

/// Lorebook (world knowledge)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Lorebook {
    pub entries: Vec<LorebookEntry>,
}

/// Single lorebook entry
///
/// `constant` follows the SillyTavern V2/V3 lorebook spec: when true, the
/// entry is ALWAYS included in context regardless of keyword matching — it
/// does not need to be triggered via `apply_lorebook`. `selective`,
/// `secondary_keys`, and `position` are stored for round-trip fidelity but
/// not yet used by AIRP's keyword-matching algorithm.
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
    /// If true, this entry is always included in context (no keyword match
    /// required). Defaults to false for backward compatibility with existing
    /// lorebook JSON files that predate the field.
    #[serde(default)]
    pub constant: bool,
    /// Secondary keys used when `selective` is true (SillyTavern V2+).
    /// Stored for round-trip fidelity; not yet used by AIRP matching.
    #[serde(default)]
    pub secondary_keys: Vec<String>,
    /// If true, `secondary_keys` are used for matching instead of `keys`.
    /// Stored for round-trip fidelity; not yet used by AIRP matching.
    #[serde(default)]
    pub selective: bool,
    /// Insertion position (SillyTavern V2+). Stored for round-trip fidelity.
    #[serde(default)]
    pub position: Option<String>,
}

impl Lorebook {
    /// Find entries matching the given text.
    ///
    /// Constant entries (`constant: true`) are ALWAYS included regardless of
    /// keyword matching — this is the core fix for Issue #28 bug 1, where
    /// 常驻 entries required `apply_lorebook` to be read. Non-constant entries
    /// are included only if their keys appear in `text`.
    pub fn find_matches(&self, text: &str) -> Vec<&LorebookEntry> {
        self.entries
            .iter()
            .filter(|e| e.enabled && (e.constant || e.matches(text)))
            .collect()
    }

    /// Return all constant (常驻) enabled entries, regardless of text.
    /// Used to inject always-on context into system prompts without requiring
    /// a separate `apply_lorebook` call.
    pub fn constant_entries(&self) -> Vec<&LorebookEntry> {
        self.entries
            .iter()
            .filter(|e| e.enabled && e.constant)
            .collect()
    }

    /// Build context string from matched entries (includes constant entries)
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
    /// Check if this entry's keys match the given text.
    /// Does NOT consider the `constant` flag — callers should check
    /// `e.constant || e.matches(text)` for full inclusion logic.
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
