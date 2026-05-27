//! Session models

use serde::{Deserialize, Serialize};
use super::{CharacterId, SessionId, Message};

/// Chat session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub character_id: CharacterId,
    pub meta: SessionMeta,
    pub messages: Vec<Message>,
}

/// Session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub message_count: usize,
    pub current_volume: u32,
    pub preset_id: Option<String>,
    pub tags: Vec<String>,
}

impl SessionMeta {
    pub fn new() -> Self {
        let now = chrono::Utc::now();
        Self {
            created_at: now,
            updated_at: now,
            message_count: 0,
            current_volume: 0,
            preset_id: None,
            tags: vec![],
        }
    }
}

impl Default for SessionMeta {
    fn default() -> Self {
        Self::new()
    }
}
