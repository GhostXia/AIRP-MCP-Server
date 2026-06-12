//! Session storage operations

use std::path::PathBuf;
use tokio::fs;
use tracing::info;

use super::Storage;
use crate::error::{AirpError, Result};
use crate::models::*;

/// Session storage operations
pub struct SessionStore<'a> {
    storage: &'a Storage,
}

impl<'a> SessionStore<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    /// Create new session
    pub async fn create(
        &self,
        character_id: &CharacterId,
        session_id: Option<SessionId>,
    ) -> Result<Session> {
        let id = session_id.unwrap_or_else(SessionId::generate);

        // Create session directory
        let session_dir = self.session_dir(character_id, &id);
        fs::create_dir_all(&session_dir).await?;

        let meta = SessionMeta::new();
        let session = Session {
            id: id.clone(),
            character_id: character_id.clone(),
            meta: meta.clone(),
            messages: vec![],
        };

        // Save metadata
        let meta_path = session_dir.join("meta.json");
        let meta_json = serde_json::to_string_pretty(&meta)?;
        fs::write(&meta_path, meta_json).await?;

        // Create empty chat log (JSONL)
        let chat_path = session_dir.join("chat.jsonl");
        fs::write(&chat_path, "").await?;

        info!(
            "Created session: {} for character: {}",
            id.as_ref(),
            character_id.as_ref()
        );

        Ok(session)
    }

    /// Get session
    pub async fn get(&self, character_id: &CharacterId, session_id: &SessionId) -> Result<Session> {
        let session_dir = self.session_dir(character_id, session_id);

        if !session_dir.exists() {
            return Err(AirpError::SessionNotFound(session_id.as_ref().to_string()));
        }

        // Load metadata
        let meta_path = session_dir.join("meta.json");
        let meta_json = fs::read_to_string(&meta_path).await?;
        let meta: SessionMeta = serde_json::from_str(&meta_json)?;

        // Load messages (JSONL)
        let chat_path = session_dir.join("chat.jsonl");
        let messages = if chat_path.exists() {
            self.load_messages(&chat_path).await?
        } else {
            vec![]
        };

        Ok(Session {
            id: session_id.clone(),
            character_id: character_id.clone(),
            meta,
            messages,
        })
    }

    /// List sessions for a character, as (session_id, meta) pairs.
    /// The id is the session directory name — it is NOT stored inside meta.json,
    /// so callers that need the id must get it from here.
    pub async fn list(&self, character_id: &CharacterId) -> Result<Vec<(String, SessionMeta)>> {
        let char_dir = self.storage.character_dir(character_id);
        let sessions_dir = char_dir.join("sessions");

        if !sessions_dir.exists() {
            return Ok(vec![]);
        }

        let mut entries = fs::read_dir(&sessions_dir).await?;
        let mut sessions = vec![];

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                let meta_path = path.join("meta.json");
                if meta_path.exists() {
                    if let Ok(json) = fs::read_to_string(&meta_path).await {
                        if let Ok(meta) = serde_json::from_str::<SessionMeta>(&json) {
                            let sid = entry.file_name().to_string_lossy().into_owned();
                            sessions.push((sid, meta));
                        }
                    }
                }
            }
        }

        Ok(sessions)
    }

    /// Append message to session
    pub async fn append_message(
        &self,
        character_id: &CharacterId,
        session_id: &SessionId,
        message: &Message,
    ) -> Result<()> {
        let session_dir = self.session_dir(character_id, session_id);

        if !session_dir.exists() {
            return Err(AirpError::SessionNotFound(session_id.as_ref().to_string()));
        }

        // Append to JSONL
        let chat_path = session_dir.join("chat.jsonl");
        let line = serde_json::to_string(message)?;

        use tokio::io::AsyncWriteExt;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&chat_path)
            .await?;

        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;

        // Update metadata
        let meta_path = session_dir.join("meta.json");
        let meta_json = fs::read_to_string(&meta_path).await?;
        let mut meta: SessionMeta = serde_json::from_str(&meta_json)?;
        meta.message_count += 1;
        meta.updated_at = chrono::Utc::now();

        let updated_meta = serde_json::to_string_pretty(&meta)?;
        fs::write(&meta_path, updated_meta).await?;

        Ok(())
    }

    /// Get recent context (last N messages)
    pub async fn get_recent_context(
        &self,
        character_id: &CharacterId,
        session_id: &SessionId,
        n: usize,
    ) -> Result<Vec<Message>> {
        let session = self.get(character_id, session_id).await?;

        // Get last N messages
        let start = session.messages.len().saturating_sub(n);
        Ok(session.messages[start..].to_vec())
    }

    /// Delete session
    pub async fn delete(&self, character_id: &CharacterId, session_id: &SessionId) -> Result<()> {
        let session_dir = self.session_dir(character_id, session_id);

        if !session_dir.exists() {
            return Err(AirpError::SessionNotFound(session_id.as_ref().to_string()));
        }

        fs::remove_dir_all(&session_dir).await?;
        info!(
            "Deleted session: {} for character: {}",
            session_id.as_ref(),
            character_id.as_ref()
        );

        Ok(())
    }

    // Helper methods

    fn session_dir(&self, character_id: &CharacterId, session_id: &SessionId) -> PathBuf {
        self.storage
            .character_dir(character_id)
            .join("sessions")
            .join(&session_id.0)
    }

    async fn load_messages(&self, path: &PathBuf) -> Result<Vec<Message>> {
        let content = fs::read_to_string(path).await?;
        let mut messages = vec![];

        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let message: Message = serde_json::from_str(line)?;
            messages.push(message);
        }

        Ok(messages)
    }
}
