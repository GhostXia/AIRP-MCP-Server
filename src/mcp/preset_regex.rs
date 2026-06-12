//! M_PR PR-4: SillyTavern regex script parser.
//!
//! Parses `presets/{id}/regex/*.json` files in SillyTavern regex script format.
//! Filters scripts with placement containing AI Output (2) and not disabled,
//! converts them to regex patterns for filtering.

use serde::Deserialize;
use std::path::Path;

/// SillyTavern regex script rule. Fields match community export format (camelCase).
#[derive(Debug, Clone, Deserialize)]
pub struct SillyTavernRegexScript {
    #[serde(rename = "scriptName", default)]
    pub script_name: String,
    #[serde(rename = "findRegex", default)]
    pub find_regex: String,
    #[serde(rename = "replaceString", default)]
    pub replace_string: String,
    #[serde(default)]
    pub placement: Vec<i32>,
    #[serde(default)]
    pub disabled: bool,
}

const PLACEMENT_AI_OUTPUT: i32 = 2;

/// Load all regex scripts for a preset from `presets/{preset_id}/regex/*.json`.
///
/// Each JSON file can be a single script object or an array of scripts.
/// Per-file parse failure only logs a warning, won't block other scripts.
pub fn load_preset_regex_scripts(regex_dir: &Path) -> Vec<SillyTavernRegexScript> {
    if !regex_dir.exists() {
        return vec![];
    }

    let mut out = Vec::new();
    let entries = match std::fs::read_dir(regex_dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let raw = match std::fs::read_to_string(&path) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let cleaned = crate::storage::strip_utf8_bom(&raw);

        if let Ok(arr) = serde_json::from_str::<Vec<SillyTavernRegexScript>>(cleaned) {
            out.extend(arr);
        } else if let Ok(single) = serde_json::from_str::<SillyTavernRegexScript>(cleaned) {
            out.push(single);
        }
    }
    out
}

/// Filter scripts to only those applicable for pure removal (hide):
/// - not disabled
/// - placement includes AI Output (2)
/// - replace_string is empty
pub fn scripts_to_patterns(scripts: &[SillyTavernRegexScript]) -> Vec<String> {
    scripts
        .iter()
        .filter(|s| !s.disabled)
        .filter(|s| s.placement.contains(&PLACEMENT_AI_OUTPUT))
        .filter(|s| s.replace_string.is_empty())
        .map(|s| strip_regex_delimiters(&s.find_regex))
        .collect()
}

fn strip_regex_delimiters(s: &str) -> String {
    let trimmed = s.trim();
    if let Some(stripped) = trimmed.strip_prefix('/') {
        if let Some(last_slash) = stripped.rfind('/') {
            return stripped[..last_slash].to_string();
        }
    }
    trimmed.to_string()
}
