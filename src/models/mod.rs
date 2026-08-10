//! Data models for AIRP

pub mod character;
pub mod gating;
pub mod lorebook;
pub mod message;
pub mod preset;
pub mod scene;
pub mod session;
pub mod state;

pub use character::{AnalysisTier, Character, CharacterCard, CharacterData};
pub use lorebook::{Lorebook, LorebookEntry};
pub use message::{Message, MessageRole};
pub use preset::{Preset, PresetConfig};
pub use scene::{CharacterEntry, CharacterRole, LorebookMerge, SceneConfig};
pub use session::{Session, SessionMeta};
pub use state::{LiveState, StateSchema};

use serde::{Deserialize, Serialize};

/// Unique identifier for characters
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CharacterId(pub String);

impl CharacterId {
    pub fn new(id: impl Into<String>) -> crate::error::Result<Self> {
        let id = id.into();
        // Validate: only alphanumeric, hyphen, underscore
        if id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            Ok(Self(id))
        } else {
            Err(crate::error::AirpError::InvalidId(format!(
                "Character ID must be alphanumeric with hyphens/underscores: {}",
                id
            )))
        }
    }
}

impl AsRef<str> for CharacterId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Unique identifier for sessions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new(id: impl Into<String>) -> crate::error::Result<Self> {
        let id = id.into();
        if id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            Ok(Self(id))
        } else {
            Err(crate::error::AirpError::InvalidId(format!(
                "Session ID must be alphanumeric with hyphens/underscores: {}",
                id
            )))
        }
    }

    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl AsRef<str> for SessionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Unique identifier for presets
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PresetId(pub String);

impl PresetId {
    pub fn new(id: impl Into<String>) -> crate::error::Result<Self> {
        let id = id.into();
        // Preset IDs become directory names and resource URI segments.  Keep
        // the accepted alphabet deliberately narrow so whitespace/control
        // characters, Unicode normalization surprises, separators and path
        // semantics cannot reach the filesystem.  Dots are allowed inside an
        // ID for common SillyTavern names such as `lunareclipse_2.0.1`, but
        // leading/trailing dots and `..` are rejected (not safe on Windows).
        let valid = !id.is_empty()
            && !id.starts_with('.')
            && !id.ends_with('.')
            && !id.contains("..")
            && id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'));

        if valid {
            Ok(Self(id))
        } else {
            Err(crate::error::AirpError::InvalidId(format!(
                "Preset ID must use ASCII letters, digits, hyphens, underscores, and internal dots: {}",
                id
            )))
        }
    }
}

impl AsRef<str> for PresetId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
