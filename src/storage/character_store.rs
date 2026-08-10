//! Character storage operations

use std::io::{Cursor, Read};

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

    /// Import character from PNG card data.
    ///
    /// Supports both V2 (`chara` chunk) and V3 (`ccv3` chunk) character cards
    /// per Issue #28 bug 3. V3 cards may carry a `character_book` (Lorebook)
    /// inside `data.character_book`; it is extracted and saved to the
    /// character's `world/lorebook.json` instead of being discarded.
    pub async fn import_from_png(&self, png_data: &[u8]) -> Result<Character> {
        // Parse PNG chara/ccv3 chunk
        let (card, extracted_lorebook) = self.parse_png_card(png_data).await?;
        let id = CharacterId::new(sanitize_id(&card.name))?;

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

        // Save lorebook — use the one extracted from the card (V3
        // character_book) if present, otherwise initialize empty.
        let lorebook = extracted_lorebook.unwrap_or_default();
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

    /// Parse PNG character card.
    ///
    /// Issue #28 bug 3: now supports V3 (`ccv3` tEXt chunk) in addition to
    /// V2 (`chara` zTXt/tEXt chunk). Per the V3 spec, if both `ccv3` and
    /// `chara` chunks are present, `ccv3` takes precedence. V3 cards may also
    /// carry a `character_book` (Lorebook) inside `data.character_book`; it is
    /// extracted and returned alongside the card.
    ///
    /// Returns `(CharacterCard, Option<Lorebook>)`.
    async fn parse_png_card(&self, png_data: &[u8]) -> Result<(CharacterCard, Option<Lorebook>)> {
        // `png::Reader::read_info` stops at the first IDAT chunk.  Metadata
        // after IDAT is therefore not guaranteed to be present in
        // `reader.info()`.  Scan the complete chunk stream ourselves (up to
        // IEND), then let the PNG decoder validate/decode the image.  The two
        // paths are intentionally separate: no decoder transformation setting
        // is used as a workaround for metadata ordering.
        let chunks = scan_png_card_chunks(png_data)?;
        validate_png_image(png_data)?;

        // V3 is authoritative whenever a ccv3 chunk exists, regardless of
        // where it appears in the PNG or whether a chara chunk appeared first.
        if let Some(text) = chunks.ccv3 {
            let decoded = decode_card_text(text)?;
            return parse_v3_json(&decoded);
        }

        // Prefer compressed chara when both V2 encodings are present.  This
        // preserves the historical parser behaviour while still supporting
        // post-IDAT chunks.
        if let Some(compressed) = chunks.chara_ztxt {
            let text = decompress_ztxt_bounded(compressed)?;
            let decoded = decode_card_text(&text)?;
            return parse_card_json(&decoded);
        }

        if let Some(text) = chunks.chara_text {
            let decoded = decode_card_text(text)?;
            return parse_card_json(&decoded);
        }

        Err(AirpError::PngParse(
            "No chara or ccv3 chunk found in PNG".to_string(),
        ))
    }
}

/// Sanitize string for use as ID
fn sanitize_id(name: &str) -> String {
    name.to_lowercase()
        .replace(" ", "_")
        .replace(|c: char| !c.is_alphanumeric() && c != '_' && c != '-', "")
}

/// Maximum amount of text accepted from a card metadata chunk.  This bounds
/// the input to base64 decoding and JSON parsing; zTXt is additionally capped
/// while it is being decompressed below.
const MAX_CARD_TEXT_BYTES: usize = 8 * 1024 * 1024;

/// Base64 decode helper with a pre-allocation guard.
fn base64_decode(input: &str) -> Result<Vec<u8>> {
    if input.len() > MAX_CARD_TEXT_BYTES {
        return Err(AirpError::PngParse(format!(
            "card metadata exceeds {} byte limit",
            MAX_CARD_TEXT_BYTES
        )));
    }

    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| AirpError::PngParse(format!("Base64 decode error: {}", e)))
}

/// Decode base64 text borrowed from a PNG chunk without first allocating a
/// `String`.  The text is ASCII by definition for card payloads, but the
/// explicit UTF-8 check gives a useful parse error for malformed chunks.
fn decode_card_text(input: &[u8]) -> Result<Vec<u8>> {
    if input.len() > MAX_CARD_TEXT_BYTES {
        return Err(AirpError::PngParse(format!(
            "card metadata exceeds {} byte limit",
            MAX_CARD_TEXT_BYTES
        )));
    }
    let text = std::str::from_utf8(input)
        .map_err(|e| AirpError::PngParse(format!("Card metadata is not UTF-8: {}", e)))?;
    base64_decode(text)
}

/// Decompress a zTXt payload with an explicit output cap.  `Read::take` lets
/// us detect an expansion beyond the cap while never allocating the full
/// decompressed stream.  This check runs before base64 decoding or JSON
/// parsing, so a high-compression-ratio payload is rejected early.
fn decompress_ztxt_bounded(input: &[u8]) -> Result<Vec<u8>> {
    let decoder = flate2::read::ZlibDecoder::new(input);
    let mut limited = decoder.take((MAX_CARD_TEXT_BYTES as u64) + 1);
    let mut output = Vec::new();
    limited
        .read_to_end(&mut output)
        .map_err(|e| AirpError::PngParse(format!("ZTXt decode error: {}", e)))?;

    if output.len() > MAX_CARD_TEXT_BYTES {
        return Err(AirpError::PngParse(format!(
            "ZTXt metadata exceeds {} byte decompressed limit",
            MAX_CARD_TEXT_BYTES
        )));
    }

    Ok(output)
}

/// References to card metadata discovered while scanning the complete PNG.
/// Slices borrow the caller's PNG buffer, avoiding copies of ignored chunks
/// (for example a chara fallback when ccv3 is present).
struct PngCardChunks<'a> {
    ccv3: Option<&'a [u8]>,
    chara_text: Option<&'a [u8]>,
    // For zTXt this is the compressed stream (the keyword and compression
    // method byte have already been removed).
    chara_ztxt: Option<&'a [u8]>,
}

/// Scan PNG chunks through IEND and collect card metadata.  The scanner is
/// deliberately independent of `png::Reader::info()`: ancillary chunks may
/// legally occur after IDAT, and `read_info` does not expose those chunks.
fn scan_png_card_chunks<'a>(png_data: &'a [u8]) -> Result<PngCardChunks<'a>> {
    const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

    if png_data.len() < PNG_SIGNATURE.len() || png_data[..8] != PNG_SIGNATURE {
        return Err(AirpError::PngParse("Invalid PNG signature".to_string()));
    }

    let mut offset = PNG_SIGNATURE.len();
    let mut ccv3 = None;
    let mut chara_text = None;
    let mut chara_ztxt = None;
    let mut saw_iend = false;

    while offset < png_data.len() {
        // length (4) + type (4) + CRC (4), before considering chunk data.
        if png_data.len() - offset < 12 {
            return Err(AirpError::PngParse(
                "Truncated PNG chunk header".to_string(),
            ));
        }

        let data_len = u32::from_be_bytes([
            png_data[offset],
            png_data[offset + 1],
            png_data[offset + 2],
            png_data[offset + 3],
        ]) as usize;
        let data_start = offset + 8;
        let data_end = data_start
            .checked_add(data_len)
            .ok_or_else(|| AirpError::PngParse("PNG chunk length overflow".to_string()))?;
        let crc_end = data_end
            .checked_add(4)
            .ok_or_else(|| AirpError::PngParse("PNG chunk length overflow".to_string()))?;
        if crc_end > png_data.len() {
            return Err(AirpError::PngParse("Truncated PNG chunk data".to_string()));
        }

        let chunk_type = &png_data[offset + 4..offset + 8];
        let chunk_data = &png_data[data_start..data_end];
        let chunk_crc = &png_data[data_end..crc_end];

        // `png::Decoder` skips ancillary chunks whose CRC is invalid by
        // default.  Validate the checksum here as well so metadata and any
        // other chunk encountered by the scanner cannot bypass PNG integrity
        // checks.
        verify_chunk_crc(chunk_type, chunk_data, chunk_crc)?;

        match chunk_type {
            b"tEXt" => {
                if let Some((keyword, text)) = split_text_chunk(chunk_data)? {
                    if keyword == b"ccv3" {
                        if text.len() > MAX_CARD_TEXT_BYTES {
                            return Err(AirpError::PngParse(format!(
                                "ccv3 metadata exceeds {} byte limit",
                                MAX_CARD_TEXT_BYTES
                            )));
                        }
                        if ccv3.is_none() {
                            ccv3 = Some(text);
                        }
                    } else if keyword == b"chara" {
                        if text.len() > MAX_CARD_TEXT_BYTES {
                            return Err(AirpError::PngParse(format!(
                                "chara metadata exceeds {} byte limit",
                                MAX_CARD_TEXT_BYTES
                            )));
                        }
                        if chara_text.is_none() {
                            chara_text = Some(text);
                        }
                    }
                }
            }
            b"zTXt" => {
                let (keyword, compressed) = split_ztxt_chunk(chunk_data)?;
                if keyword == b"chara" && chara_ztxt.is_none() {
                    chara_ztxt = Some(compressed);
                }
            }
            b"IEND" => {
                if !chunk_data.is_empty() {
                    return Err(AirpError::PngParse("IEND chunk must be empty".to_string()));
                }
                saw_iend = true;
                offset = crc_end;
                break;
            }
            _ => {}
        }

        offset = crc_end;
    }

    if !saw_iend {
        return Err(AirpError::PngParse("PNG missing IEND chunk".to_string()));
    }

    if offset != png_data.len() {
        return Err(AirpError::PngParse(
            "Trailing data after IEND chunk".to_string(),
        ));
    }

    Ok(PngCardChunks {
        ccv3,
        chara_text,
        chara_ztxt,
    })
}

/// Verify the CRC stored at the end of a PNG chunk.  The `png` crate's
/// high-level decoder intentionally skips ancillary CRC failures unless its
/// decode options are changed, so the scanner keeps a direct check for every
/// chunk it walks through.
fn verify_chunk_crc(chunk_type: &[u8], chunk_data: &[u8], crc_bytes: &[u8]) -> Result<()> {
    debug_assert_eq!(chunk_type.len(), 4);
    debug_assert_eq!(crc_bytes.len(), 4);

    let expected = u32::from_be_bytes([crc_bytes[0], crc_bytes[1], crc_bytes[2], crc_bytes[3]]);
    let mut crc = 0xFFFF_FFFFu32;

    for &byte in chunk_type.iter().chain(chunk_data.iter()) {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }

    let actual = !crc;
    if actual != expected {
        return Err(AirpError::PngParse(format!(
            "PNG chunk CRC mismatch for {}",
            String::from_utf8_lossy(chunk_type)
        )));
    }

    Ok(())
}

fn split_text_chunk(data: &[u8]) -> Result<Option<(&[u8], &[u8])>> {
    let Some(separator) = data.iter().position(|&byte| byte == 0) else {
        return Err(AirpError::PngParse(
            "Malformed tEXt chunk (missing keyword separator)".to_string(),
        ));
    };
    let keyword = &data[..separator];
    if keyword.is_empty() || keyword.len() > 79 {
        return Err(AirpError::PngParse("Invalid PNG text keyword".to_string()));
    }
    Ok(Some((keyword, &data[separator + 1..])))
}

fn split_ztxt_chunk(data: &[u8]) -> Result<(&[u8], &[u8])> {
    let Some(separator) = data.iter().position(|&byte| byte == 0) else {
        return Err(AirpError::PngParse(
            "Malformed zTXt chunk (missing keyword separator)".to_string(),
        ));
    };
    let keyword = &data[..separator];
    if keyword.is_empty() || keyword.len() > 79 {
        return Err(AirpError::PngParse("Invalid PNG text keyword".to_string()));
    }
    if data.len() <= separator + 1 {
        return Err(AirpError::PngParse(
            "Malformed zTXt chunk (missing compression method)".to_string(),
        ));
    }
    if data[separator + 1] != 0 {
        return Err(AirpError::PngParse(
            "Unsupported zTXt compression method".to_string(),
        ));
    }
    let compressed = &data[separator + 2..];
    if compressed.is_empty() {
        return Err(AirpError::PngParse("Empty zTXt payload".to_string()));
    }
    Ok((keyword, compressed))
}

/// Ask the PNG decoder to validate and consume the image stream.  The manual
/// scanner above guarantees that the complete chunk stream reaches IEND;
/// this call provides CRC/format validation and bounds decoder allocations.
fn validate_png_image(png_data: &[u8]) -> Result<()> {
    let mut options = png::DecodeOptions::default();
    // Ancillary CRC failures are skipped by default in png 0.17.  Character
    // metadata can legally follow IDAT, so force the decoder to validate all
    // chunks when it consumes the stream through IEND.
    options.set_skip_ancillary_crc_failures(false);
    let mut decoder = png::Decoder::new_with_options(Cursor::new(png_data), options);
    decoder.set_limits(png::Limits {
        bytes: 64 * 1024 * 1024,
    });
    let mut reader = decoder
        .read_info()
        .map_err(|e| AirpError::PngParse(e.to_string()))?;
    let mut frame = vec![0; reader.output_buffer_size()];
    reader
        .next_frame(&mut frame)
        .map_err(|e| AirpError::PngParse(e.to_string()))?;
    reader
        .finish()
        .map_err(|e| AirpError::PngParse(e.to_string()))?;
    Ok(())
}

/// Parse a character card JSON payload.
///
/// Handles three shapes transparently:
/// 1. V3 spec: `{ "spec": "chara_card_v3", "spec_version": "3.0", "data": { ... } }`
/// 2. V2 spec: `{ "spec": "chara_card_v2", "spec_version": "2.0", "data": { ... } }`
/// 3. Flat (legacy/SillyTavern export): `{ "name": "...", "description": "...", ... }`
///
/// For V3, also extracts `data.character_book` into a `Lorebook` if present.
/// For V2/V3 wrapped cards, `data.character_book` is also extracted (V2 spec
/// allows it too).
fn parse_card_json(json_bytes: &[u8]) -> Result<(CharacterCard, Option<Lorebook>)> {
    let value: serde_json::Value = serde_json::from_slice(json_bytes)
        .map_err(|e| AirpError::PngParse(format!("Invalid card JSON: {}", e)))?;

    // Detect wrapped spec format (V2 or V3): { spec, data: { ... } }
    if let Some(data_obj) = value.get("data").and_then(|d| d.as_object()) {
        let spec = value.get("spec").and_then(|s| s.as_str()).unwrap_or("");
        let is_v3 = spec == "chara_card_v3";
        return parse_wrapped_card(data_obj, is_v3);
    }

    // Flat format: deserialize directly into CharacterCard.
    let card: CharacterCard = serde_json::from_value(value)
        .map_err(|e| AirpError::PngParse(format!("Invalid card fields: {}", e)))?;
    Ok((card, None))
}

/// Parse a V3 character card JSON payload (from the `ccv3` chunk).
/// V3 always uses the wrapped `{ spec, spec_version, data }` shape.
fn parse_v3_json(json_bytes: &[u8]) -> Result<(CharacterCard, Option<Lorebook>)> {
    let value: serde_json::Value = serde_json::from_slice(json_bytes)
        .map_err(|e| AirpError::PngParse(format!("Invalid ccv3 JSON: {}", e)))?;

    let data_obj = value
        .get("data")
        .and_then(|d| d.as_object())
        .ok_or_else(|| AirpError::PngParse("V3 card missing 'data' object".to_string()))?;

    parse_wrapped_card(data_obj, true)
}

/// Extract CharacterCard + optional Lorebook from a V2/V3 `data` object.
///
/// `is_v3` only affects the error message context. Both V2 and V3 may carry
/// `character_book`; we extract it for either.
fn parse_wrapped_card(
    data: &serde_json::Map<String, serde_json::Value>,
    is_v3: bool,
) -> Result<(CharacterCard, Option<Lorebook>)> {
    let version_label = if is_v3 { "V3" } else { "V2" };

    let card: CharacterCard = serde_json::from_value(serde_json::Value::Object(data.clone()))
        .map_err(|e| {
            AirpError::PngParse(format!("Invalid {} card data fields: {}", version_label, e))
        })?;

    // Extract character_book (Lorebook) if present. V3 spec: data.character_book
    // is a Lorebook object with `entries`. V2 spec also allows it.
    let lorebook = data
        .get("character_book")
        .and_then(|lb| {
            if lb.is_null() {
                None
            } else {
                Some(extract_lorebook(lb))
            }
        })
        .transpose()?;

    Ok((card, lorebook))
}

/// Convert a V2/V3 `character_book` JSON value into a `Lorebook`.
///
/// The V2/V3 lorebook entry format uses different field names than AIRP's
/// internal `LorebookEntry` (e.g. `comment` vs `name`, `constant` flag,
/// `selective`/`secondary_keys`). We map the known fields and round-trip
/// the rest through `serde_json::Value` deserialization (with `#[serde(default)]`
/// on new fields handling missing keys gracefully).
pub fn extract_lorebook(value: &serde_json::Value) -> Result<Lorebook> {
    // The V2/V3 lorebook has `entries` as either an array or an object map
    // (SillyTavern uses an object keyed by entry ID in some versions).
    let lorebook: Lorebook =
        if let Some(entries_array) = value.get("entries").and_then(|e| e.as_array()) {
            // Array form: parse each entry directly.
            let mut entries = Vec::with_capacity(entries_array.len());
            for entry_val in entries_array {
                entries.push(normalize_lorebook_entry(entry_val)?);
            }
            Lorebook { entries }
        } else if let Some(entries_obj) = value.get("entries").and_then(|e| e.as_object()) {
            // Object form (SillyTavern): keys are entry IDs; merge ID into each entry.
            let mut entries = Vec::with_capacity(entries_obj.len());
            for (id, entry_val) in entries_obj {
                let mut entry = normalize_lorebook_entry(entry_val)?;
                // Ensure the entry has an ID — use the map key if the entry lacks one.
                if entry.id.is_empty() {
                    entry.id = id.clone();
                }
                entries.push(entry);
            }
            Lorebook { entries }
        } else {
            // Try direct deserialization as a fallback.
            serde_json::from_value(value.clone())
                .map_err(|e| AirpError::PngParse(format!("Invalid character_book format: {}", e)))?
        };

    Ok(lorebook)
}

/// Normalize a V2/V3 lorebook entry JSON into AIRP's `LorebookEntry`.
///
/// SillyTavern V2/V3 entries use different field names than AIRP's
/// `LorebookEntry`:
///   - `comment` → `name` (display name; AIRP's `build_context` uses `name`)
///   - `disable` → `enabled` (inverted: `disable: true` means `enabled: false`)
///   - `key`, `keysecondary`, `order`, `caseSensitive` are handled via
///     `#[serde(alias = ...)]` on the struct.
///
/// We also backfill a missing `id` (the ID is the map key in object-form
/// entries).
fn normalize_lorebook_entry(value: &serde_json::Value) -> Result<LorebookEntry> {
    let mut val = value.clone();

    if let Some(obj) = val.as_object_mut() {
        // SillyTavern entries often omit `id` (the ID is the map key in
        // object-form entries). Insert an empty string so serde doesn't fail
        // on the missing required field; the caller fills it from the map key.
        if !obj.contains_key("id") {
            obj.insert("id".to_string(), serde_json::Value::String(String::new()));
        }

        // Map `disable` → `enabled` (inverted). Only applies when `enabled`
        // is absent — if both are present, `enabled` takes precedence.
        if !obj.contains_key("enabled") {
            if let Some(disable) = obj.get("disable").and_then(|d| d.as_bool()) {
                obj.insert("enabled".to_string(), serde_json::Value::Bool(!disable));
            }
        }

        // If the entry has a `comment` but no `name`, copy comment → name so the
        // display name is preserved in AIRP's output (build_context uses `name`).
        let has_name = obj
            .get("name")
            .map(|n| n.as_str().map(|s| !s.is_empty()).unwrap_or(false))
            .unwrap_or(false);
        if !has_name {
            if let Some(comment) = obj.get("comment").cloned() {
                obj.insert("name".to_string(), comment);
            }
        }
    }

    serde_json::from_value(val)
        .map_err(|e| AirpError::PngParse(format!("Invalid lorebook entry: {}", e)))
}
