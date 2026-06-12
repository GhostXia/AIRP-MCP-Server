//! Character storage operations

use tokio::fs;
use tracing::info;

use super::Storage;
use crate::error::{AirpError, Result};
use crate::models::*;

/// Character storage operations
pub struct CharacterStore<'a> {
    storage: &'a Storage,
}

impl<'a> CharacterStore<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    /// Import character from PNG card data
    pub async fn import_from_png(&self, png_data: &[u8]) -> Result<Character> {
        // Parse PNG chara chunk
        let card = self.parse_png_card(png_data).await?;
        let id = CharacterId::new(&sanitize_id(&card.name))?;

        // Create character directory
        let char_dir = self.storage.character_dir(&id);
        fs::create_dir_all(&char_dir).await?;

        // Save card data
        let card_path = char_dir.join("card.json");
        let card_json = serde_json::to_string_pretty(&card)?;
        fs::write(&card_path, card_json).await?;

        // Save raw PNG
        let png_path = char_dir.join("card.png");
        fs::write(&png_path, png_data).await?;

        // Create metadata
        let data = CharacterData {
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            import_source: Some("png_import".to_string()),
            analysis_tier: None,
            has_state_tracking: false,
        };

        let data_path = char_dir.join("data.json");
        let data_json = serde_json::to_string_pretty(&data)?;
        fs::write(&data_path, data_json).await?;

        // Initialize empty lorebook
        let lorebook = Lorebook::default();
        let lorebook_path = char_dir.join("world").join("lorebook.json");
        fs::create_dir_all(lorebook_path.parent().unwrap()).await?;
        let lorebook_json = serde_json::to_string_pretty(&lorebook)?;
        fs::write(&lorebook_path, lorebook_json).await?;

        // Initialize empty state
        let state = LiveState::new();
        let state_path = char_dir.join("state").join("live.json");
        fs::create_dir_all(state_path.parent().unwrap()).await?;
        let state_json = serde_json::to_string_pretty(&state)?;
        fs::write(&state_path, state_json).await?;

        info!("Imported character: {} ({})", card.name, id.as_ref());

        Ok(Character { id, data, card })
    }

    /// Get character by ID
    pub async fn get(&self, id: &CharacterId) -> Result<Character> {
        let char_dir = self.storage.character_dir(id);

        if !char_dir.exists() {
            return Err(AirpError::CharacterNotFound(id.as_ref().to_string()));
        }

        // Load card
        let card_path = char_dir.join("card.json");
        let card_json = fs::read_to_string(&card_path).await?;
        let card: CharacterCard = serde_json::from_str(&card_json)?;

        // Load data
        let data_path = char_dir.join("data.json");
        let data_json = fs::read_to_string(&data_path).await?;
        let data: CharacterData = serde_json::from_str(&data_json)?;

        Ok(Character {
            id: id.clone(),
            data,
            card,
        })
    }

    /// List all characters
    pub async fn list(&self) -> Result<Vec<Character>> {
        let chars_dir = self.storage.characters_dir();

        if !chars_dir.exists() {
            return Ok(vec![]);
        }

        let mut entries = fs::read_dir(&chars_dir).await?;
        let mut characters = vec![];

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                let id_str = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                if let Ok(id) = CharacterId::new(id_str) {
                    if let Ok(character) = self.get(&id).await {
                        characters.push(character);
                    }
                }
            }
        }

        Ok(characters)
    }

    /// Delete character
    pub async fn delete(&self, id: &CharacterId) -> Result<()> {
        let char_dir = self.storage.character_dir(id);

        if !char_dir.exists() {
            return Err(AirpError::CharacterNotFound(id.as_ref().to_string()));
        }

        fs::remove_dir_all(&char_dir).await?;
        info!("Deleted character: {}", id.as_ref());

        Ok(())
    }

    /// Get lorebook for character
    pub async fn get_lorebook(&self, id: &CharacterId) -> Result<Lorebook> {
        let lorebook_path = self
            .storage
            .character_dir(id)
            .join("world")
            .join("lorebook.json");

        if !lorebook_path.exists() {
            return Ok(Lorebook::default());
        }

        let json = fs::read_to_string(&lorebook_path).await?;
        let lorebook: Lorebook = serde_json::from_str(&json)?;
        Ok(lorebook)
    }

    /// Save lorebook
    pub async fn save_lorebook(&self, id: &CharacterId, lorebook: &Lorebook) -> Result<()> {
        let lorebook_path = self
            .storage
            .character_dir(id)
            .join("world")
            .join("lorebook.json");

        fs::create_dir_all(lorebook_path.parent().unwrap()).await?;

        let json = serde_json::to_string_pretty(lorebook)?;
        fs::write(&lorebook_path, json).await?;

        Ok(())
    }

    /// Get live state
    pub async fn get_live_state(&self, id: &CharacterId) -> Result<LiveState> {
        let state_path = self
            .storage
            .character_dir(id)
            .join("state")
            .join("live.json");

        if !state_path.exists() {
            return Ok(LiveState::new());
        }

        let json = fs::read_to_string(&state_path).await?;
        let state: LiveState = serde_json::from_str(&json)?;
        Ok(state)
    }

    /// Save live state
    pub async fn save_live_state(&self, id: &CharacterId, state: &LiveState) -> Result<()> {
        let state_path = self
            .storage
            .character_dir(id)
            .join("state")
            .join("live.json");

        fs::create_dir_all(state_path.parent().unwrap()).await?;

        let json = serde_json::to_string_pretty(state)?;
        fs::write(&state_path, json).await?;

        Ok(())
    }

    /// Parse PNG character card
    async fn parse_png_card(&self, png_data: &[u8]) -> Result<CharacterCard> {
        use std::io::Cursor;

        let mut decoder = png::Decoder::new(Cursor::new(png_data));
        // Bound decoder allocation to limit zlib decompression-bomb expansion.
        let mut limits = png::Limits::default();
        limits.bytes = 64 * 1024 * 1024;
        decoder.set_limits(limits);
        let reader = decoder
            .read_info()
            .map_err(|e| AirpError::PngParse(e.to_string()))?;

        // Look for chara chunk
        for chunk in reader.info().compressed_latin1_text.iter() {
            if chunk.keyword == "chara" {
                let text = chunk
                    .get_text()
                    .map_err(|e| AirpError::PngParse(format!("ZTXt decode error: {}", e)))?;
                let decoded = base64_decode(&text)?;
                let card: CharacterCard = serde_json::from_slice(&decoded)
                    .map_err(|e| AirpError::PngParse(format!("Invalid chara JSON: {}", e)))?;
                return Ok(card);
            }
        }

        Err(AirpError::PngParse(
            "No chara chunk found in PNG".to_string(),
        ))
    }
}

/// Sanitize string for use as ID
fn sanitize_id(name: &str) -> String {
    name.to_lowercase()
        .replace(" ", "_")
        .replace(|c: char| !c.is_alphanumeric() && c != '_' && c != '-', "")
}

/// Base64 decode helper
fn base64_decode(input: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| AirpError::PngParse(format!("Base64 decode error: {}", e)))
}
